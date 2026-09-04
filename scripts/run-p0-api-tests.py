#!/usr/bin/env python3
"""Scenariusze API P0 CASP wykonywane na lokalnym wdrozeniu Docker."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import sys
import time
import unittest
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class Response:
    status: int
    body: Any
    raw: str


class Client:
    def __init__(self, base: str, timeout: float) -> None:
        self.base = base.rstrip("/")
        self.timeout = timeout

    def request(self, method: str, path: str, body: dict[str, Any] | None = None) -> Response:
        data = json.dumps(body).encode() if body is not None else None
        headers = {"Accept": "application/json", "Connection": "close"}
        if data is not None:
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(self.base + path, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as result:
                status, raw = result.status, result.read().decode()
        except urllib.error.HTTPError as error:
            status, raw = error.code, error.read().decode()
        except urllib.error.URLError as error:
            raise AssertionError(f"Brak polaczenia z {self.base}: {error.reason}") from error
        try:
            parsed = json.loads(raw) if raw else None
        except json.JSONDecodeError:
            parsed = None
        return Response(status, parsed, raw)

    def get(self, path: str) -> Response:
        return self.request("GET", path)

    def post(self, path: str, body: dict[str, Any] | None = None) -> Response:
        return self.request("POST", path, body or {})


def require(response: Response, status: int) -> dict[str, Any]:
    if response.status != status or not isinstance(response.body, dict):
        raise AssertionError(f"Oczekiwano HTTP {status} z JSON, otrzymano {response.status}: {response.raw}")
    return response.body


def amount(account: dict[str, Any]) -> int:
    return int(account["availableRaw"])


def build_tests(casp: Client, prefix: str) -> type[unittest.TestCase]:
    class CaspP0ApiTests(unittest.TestCase):
        def account(self, client: str) -> dict[str, Any]:
            return require(casp.get(f"/api/v1/clients/{client}/account"), 200)

        def purchase(self, client: str, suffix: str, cents: int) -> dict[str, Any]:
            return require(
                casp.post(
                    f"/api/v1/clients/{client}/purchases",
                    {"operationId": f"{prefix}-{suffix}", "amountUsdMinor": cents},
                ),
                200,
            )

        def test_01_ca01_bootstrap_is_idempotent_and_targets_hot_cold(self) -> None:
            first = require(casp.post("/api/v1/admin/bootstrap-inventory"), 200)
            replay = require(casp.post("/api/v1/admin/bootstrap-inventory"), 200)
            self.assertEqual("distributed", first["status"])
            self.assertEqual(first["operationId"], replay["operationId"])
            self.assertEqual("2000000000", first["hotTargetRaw"])
            self.assertEqual("8000000000", first["coldTargetRaw"])
            wallets = require(casp.get("/api/v1/admin/wallets"), 200)
            self.assertEqual("0", wallets["corporateRaw"])

        def test_02_ca02_replenishment_is_idempotent_and_keeps_20_80_target(self) -> None:
            operation = f"{prefix}-ca02"
            # The scenario reserves enough unassigned inventory for all later
            # customer operations in this run (20 rUSD in total).
            payload = {"operationId": operation, "amountUsdMinor": 2000}
            first = require(casp.post("/api/v1/admin/inventory-replenishments", payload), 200)
            replay = require(casp.post("/api/v1/admin/inventory-replenishments", payload), 200)
            self.assertEqual("completed", first["status"])
            self.assertEqual(first["operationId"], replay["operationId"])
            self.assertEqual("4000000", first["hotIncrementRaw"])
            self.assertEqual("16000000", first["coldIncrementRaw"])
            balanced = require(casp.post("/api/v1/admin/rebalancing"), 200)
            self.assertIn(balanced["direction"], ("none", "hot_to_cold", "cold_to_hot"))
            resulting_plan = require(casp.get("/api/v1/admin/rebalancing-plan"), 200)
            self.assertEqual(0, resulting_plan["hotDeltaRaw"])
            self.assertEqual(0, resulting_plan["coldDeltaRaw"])

        def test_03_ca03_purchase_changes_only_the_customer_ledger_once(self) -> None:
            before = amount(self.account("alice"))
            first = self.purchase("alice", "ca03", 200)
            replay = require(
                casp.post(
                    "/api/v1/clients/alice/purchases",
                    {"operationId": f"{prefix}-ca03", "amountUsdMinor": 200},
                ),
                200,
            )
            after = amount(self.account("alice"))
            self.assertEqual("completed", first["status"])
            self.assertEqual(first["operationId"], replay["operationId"])
            self.assertEqual(2_000_000, after - before)

        def test_04_ca04_internal_transfer_applies_fee_and_is_idempotent(self) -> None:
            self.purchase("alice", "ca04-funding", 200)
            alice_before = amount(self.account("alice"))
            bob_before = amount(self.account("bob"))
            payload = {
                "operationId": f"{prefix}-ca04-transfer",
                "recipientClientId": "bob",
                "tokenAmountRaw": 1_000_000,
                "purposeClassification": "private_transfer",
            }
            first = require(casp.post("/api/v1/clients/alice/transfers", payload), 200)
            replay = require(casp.post("/api/v1/clients/alice/transfers", payload), 200)
            self.assertEqual("1000", first["feeRaw"])
            self.assertEqual("999000", first["netRaw"])
            self.assertEqual(first["operationId"], replay["operationId"])
            self.assertEqual(1_000_000, alice_before - amount(self.account("alice")))
            self.assertEqual(999_000, amount(self.account("bob")) - bob_before)

        def test_05_ca05_concurrent_requests_cannot_double_spend_balance(self) -> None:
            self.purchase("alice", "ca05-funding", 200)
            available = amount(self.account("alice"))
            # Reporting accepts whole USD cents, therefore the competing
            # amount must remain a multiple of 10,000 token units.
            spend = (available // 2 // 10_000 + 1) * 10_000
            payloads = [
                {
                    "operationId": f"{prefix}-ca05-{index}",
                    "recipientClientId": "bob",
                    "tokenAmountRaw": spend,
                    "purposeClassification": "private_transfer",
                }
                for index in (1, 2)
            ]
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                responses = list(
                    pool.map(lambda payload: casp.post("/api/v1/clients/alice/transfers", payload), payloads)
                )
            self.assertEqual([200, 409], sorted(response.status for response in responses))
            self.assertEqual(available - spend, amount(self.account("alice")))

        def test_06_ca06_sale_and_redemption_are_idempotent(self) -> None:
            self.purchase("alice", "ca06-funding", 300)
            sale_payload = {"operationId": f"{prefix}-ca06-sale", "tokenAmountRaw": 1_000_000}
            sale = require(casp.post("/api/v1/clients/alice/sales", sale_payload), 200)
            sale_replay = require(casp.post("/api/v1/clients/alice/sales", sale_payload), 200)
            self.assertEqual("completed", sale["status"])
            self.assertEqual(sale["operationId"], sale_replay["operationId"])

            redemption_payload = {
                "operationId": f"{prefix}-ca06-redemption",
                "tokenAmountRaw": 1_000_000,
            }
            redemption = require(casp.post("/api/v1/clients/alice/redemptions", redemption_payload), 200)
            replay = require(casp.post("/api/v1/clients/alice/redemptions", redemption_payload), 200)
            self.assertEqual("completed", redemption["status"])
            self.assertEqual(redemption["operationId"], replay["operationId"])
            self.assertTrue(redemption.get("blockchainTransactionHash"))

    CaspP0ApiTests.__name__ = "CaspP0ApiTests"
    CaspP0ApiTests.__qualname__ = "CaspP0ApiTests"
    return CaspP0ApiTests


def main() -> int:
    parser = argparse.ArgumentParser(description="Mutujace scenariusze API P0 CASP (CA-01--CA-06).")
    parser.add_argument("--casp-url", default="http://127.0.0.1:3200")
    parser.add_argument("--timeout", type=float, default=20.0)
    args = parser.parse_args()
    prefix = "api-p0-casp-" + uuid.uuid4().hex[:12]
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(
        build_tests(Client(args.casp_url, args.timeout), prefix)
    )
    started = time.perf_counter()
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    report = {
        "schemaVersion": "rusd-casp-p0-api-v1",
        "runId": prefix,
        "status": "PASS" if result.wasSuccessful() else "FAIL",
        "testsRun": result.testsRun,
        "failures": len(result.failures),
        "errors": len(result.errors),
        "skipped": len(result.skipped),
        "durationSeconds": round(time.perf_counter() - started, 3),
        "coverage": ["CA-01", "CA-02", "CA-03", "CA-04", "CA-05", "CA-06"],
    }
    output = Path(__file__).resolve().parents[1] / "test-results" / f"{prefix}.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"\nRaport: {output}")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
