# CASP P0 API scenarios

`scripts/run-p0-api-tests.py` executes representative CA-01--CA-06 flows
against the running CASP deployment. It covers bootstrap, replenishment and
rebalancing, customer purchase, fee-bearing internal transfer, concurrent
balance protection, sale, and issuer-backed redemption.

Run it after starting both Compose projects:

```powershell
python .\scripts\run-p0-api-tests.py
```

This is a mutating local-demo test. It leaves auditable SQLite records and
small balance changes, while unique identifiers make repeated runs safe from
idempotency-key collisions. Results are written to
`test-results/api-p0-casp-*.json`.
