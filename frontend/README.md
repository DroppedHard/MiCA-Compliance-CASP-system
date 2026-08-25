# CASP frontend

Polish client-facing demo for buying and selling rUSD through the CASP backend. It offers three switchable demo accounts. Both actions are internal CASP ledger postings; no blockchain wallet is changed. It deliberately does not require MetaMask.

## Run locally

Start the CASP backend on port `3200`, then:

```powershell
cd D:\repos\Magisterka\kod\casp\frontend
npm.cmd install
npm.cmd run dev
```

Open `http://127.0.0.1:5174`. Before the first retail purchase, initialize CASP inventory with the admin bootstrap endpoint described in `../backend/README.md`.

## Verify

```powershell
npm.cmd test
npm.cmd run build
```

Implementation placeholder. One React application will contain separate customer and administrator routes.

The existing customer mock remains temporarily at `/client` in `issuer/frontend` so the working demo is not duplicated during the folder move. It will be migrated here when the CASP routing and backend boundary are implemented.
