# CASP frontend

Polish client-facing demo for buying, selling and transferring rUSD through the CASP backend. It offers three switchable demo accounts. These actions use the internal CASP ledger; no blockchain wallet is changed. It deliberately does not require MetaMask.

## Run locally

Start the CASP backend on port `3200`, then:

```powershell
cd D:\repos\Magisterka\kod\casp\frontend
npm.cmd install
npm.cmd run dev
```

Open one of the routes:

- `http://127.0.0.1:5174/` — customer purchase, sale and internal-transfer demo;
- `http://127.0.0.1:5174/admin` — unauthenticated, read-only administrator demo.
- `http://127.0.0.1:5174/statement?client=alice` — client/date statement view suitable for browser print-to-PDF.

Before the first retail purchase, initialize CASP inventory with the admin bootstrap endpoint described in `../backend/README.md`.

The administrator route polls the CASP backend every 10 seconds. It presents custody reconciliation, hot/cold/corporate wallet balances, customer liabilities, pending CASP fees, unallocated inventory, issuer bootstrap state, recent service records and the last seven days of transaction-reporting aggregates.

The same route contains the demo-only manual inventory form. It lets an administrator choose a USD amount, follows the persisted issuer purchase operation and refreshes custody, inventory and the advisory 20/80 rebalancing plan after completion. Automatic inventory replenishment is not enabled.

The customer purchase/sale area presents the issuer-published rUSD state, reserve coverage, contract identity and ESG methodology. It calls the CASP facade rather than the issuer directly and links to the issuer-owned `http://127.0.0.1:5173/white-paper` page.

The account header links to the statement route for the currently selected demo client. The page defaults to the current UTC month, supports another inclusive date range and shows opening/closing available and locked balances, purchases, sales, transfers, fees and issuer redemptions.

An internal transfer debits the gross amount from the selected sender, assigns the net amount to another demo customer and accrues a 0.1% demo transaction fee. It is an atomic, gas-free SQLite operation; it does not move tokens between blockchain wallets.

## Verify

```powershell
npm.cmd test
npm.cmd run build
```
