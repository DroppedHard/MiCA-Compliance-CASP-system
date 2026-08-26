# CASP frontend

Polish client-facing demo for buying and selling rUSD through the CASP backend. It offers three switchable demo accounts. Both actions are internal CASP ledger postings; no blockchain wallet is changed. It deliberately does not require MetaMask.

## Run locally

Start the CASP backend on port `3200`, then:

```powershell
cd D:\repos\Magisterka\kod\casp\frontend
npm.cmd install
npm.cmd run dev
```

Open one of the routes:

- `http://127.0.0.1:5174/` — customer purchase and sale demo;
- `http://127.0.0.1:5174/admin` — unauthenticated, read-only administrator demo.

Before the first retail purchase, initialize CASP inventory with the admin bootstrap endpoint described in `../backend/README.md`.

The administrator route polls the CASP backend every 10 seconds. It presents custody reconciliation, hot/cold/corporate wallet balances, customer liabilities, unallocated inventory, issuer bootstrap state and recent service records. Daily regulatory aggregates remain intentionally absent until the reporting integration described in roadmap item 8 is implemented.

## Verify

```powershell
npm.cmd test
npm.cmd run build
```
