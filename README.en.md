# rUSD CASP

[Wersja polska](README.md)

This repository contains a demonstration crypto-asset service provider platform dedicated to rUSD. The CASP maintains its own client-entitlement ledger, controls omnibus custody wallets, and purchases token inventory from the issuer. It is a research environment, not a production service.

## Components

- `backend/` — the Rust CASP API and ledger, SQLite, custody, fees, and reporting;
- `frontend/` — one React application with customer and administrator views;
- `docs/` — detailed Polish and English documentation;
- `compose.yaml` — a local CASP deployment connected to a separately running issuer.

The CASP never reads the issuer database. It communicates through configured HTTP, mockBank, and blockchain endpoints.

## Quick start

Start the issuer repository first, then run:

```powershell
docker compose up --build --detach --wait
```

Docker Desktop connects to the issuer through `host.docker.internal` by default. Available endpoints:

- customer portal: `http://127.0.0.1:5174`;
- administration panel: `http://127.0.0.1:5174/admin`;
- CASP API: `http://127.0.0.1:3200`.

Integration addresses may be overridden before startup:

```powershell
$env:ISSUER_URL="http://host.docker.internal:3000"
$env:RPC_URL="http://host.docker.internal:8545"
$env:MOCK_BANK_URL="http://host.docker.internal:3100"
docker compose up --build --detach --wait
```

Use `docker compose down` to stop the deployment or `docker compose down --volumes` to remove demo data. Compose keys belong to public deterministic Hardhat accounts and must never be used outside the local test network.

## Operating model

On first startup the CASP buys 10,000 rUSD from the issuer. Issuance goes directly to hot custody, after which rebalancing moves 80% to cold custody. Customer purchases, sales, and internal transfers update SQLite entitlements without an on-chain transfer. External operations use the hot wallet.

## API tests of the running CASP

Create an isolated Python environment and install the repository manifest:

```powershell
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r .\scripts\lib.txt
```

No third-party package is currently required. Start the issuer and CASP Compose
projects, then run:

```powershell
.\.venv\Scripts\python.exe .\scripts\run-p0-api-tests.py
```

The mutating suite covers representative CA-01--CA-06 flows and writes
`test-results/api-p0-casp-*.json`. Unique operation IDs allow repeated local
runs, while records remain visible for audit demonstrations. See
[CASP P0 API scenarios](docs/en/p0-api-tests.md) for details.

## Verification and further documentation

- [CASP backend and endpoints](docs/en/backend.md);
- [customer and administrator frontend](docs/en/frontend.md);
- [CASP documentation index](docs/en/README.md);
- [CASP P0 API scenarios](docs/en/p0-api-tests.md);
- [service-record model](docs/en/service-records.md).

The basic verification set is `cargo test`, `cargo clippy`, `npm.cmd test`, and `npm.cmd run build`. Cross-institution system tests will be designed later. GitHub Actions are intentionally omitted.
