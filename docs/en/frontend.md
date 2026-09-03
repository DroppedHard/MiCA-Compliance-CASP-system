# CASP frontend

The React application contains the customer portal and administration panel. Demonstration accounts can be switched without authentication, and MetaMask is not required.

```powershell
npm.cmd install
npm.cmd run dev
npm.cmd test
npm.cmd run build
```

The portal is served at `http://127.0.0.1:5174/`, administration at `/admin`, and a printable statement at `/statement?client=alice`. Customers can buy, sell, transfer internally, and request an Ethereum withdrawal. Administrators manage inventory, wallets, reconciliation, fees, restrictions, reporting, and manual rebalancing.

