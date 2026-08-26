# CASP backend

## Custody reconciliation

The backend persists custody evidence under policy `casp-custody-reconciliation-v1`. It compares on-chain hot and cold wallet balances with the sum of customer available positions, customer locked positions, unallocated inventory and CASP fees accrued but not yet swept on-chain. The corporate wallet is reported separately and never counted as coverage for client entitlements.

Reconciliation runs immediately after the startup bootstrap, every five minutes, and before and after current purchase, sale, internal-transfer and redemption operations. New customer purchases fail closed when evidence is unavailable or totals differ. Sales and redemptions remain available because they do not create a new customer entitlement. Allocation drift away from the demo 20/80 target produces `warning` without claiming a custody shortfall.

Read the latest persisted snapshot:

```powershell
Invoke-RestMethod http://127.0.0.1:3200/api/v1/admin/reconciliation
```

Rust service representing the CASP boundary. It has its own SQLite database and never accesses the issuer database directly.

## First implemented scenario

The explicit bootstrap operation purchases `10,000 rUSD` from the issuer and ends with:

- cold client-custody wallet: `8,000 rUSD`;
- hot client-custody wallet: `2,000 rUSD`;
- corporate CASP wallet: no fixed absolute target; it may contain separate CASP-owned inventory or revenue.

The corporate wallet is the CASP-private wallet discussed in the project model. The issuer first mints the full purchase to this wallet. The CASP then transfers 8,000 to cold custody and 2,000 to hot custody. No client positions are created yet.

The workflow is an idempotent saga:

```text
CASP persists operation_id
-> POST issuer issuance order for the corporate wallet
-> POST matching USD deposit to mockBank
-> POST issuer settlement request
-> issuer mints 10,000 rUSD to corporate
-> CASP moves 8,000 to cold and 2,000 to hot
-> CASP reconciles the 2,000 / 8,000 hot/cold bootstrap targets
```

SQLite persists every completed boundary. A retry resumes the same operation. Issuer and bank calls reuse the same operation ID. Wallet distribution calculates the difference between the current balance and the fixed target rather than blindly repeating a transfer.

This target-balance technique is safe for the isolated initial bootstrap only. Later inventory purchases need a general append-only custody ledger and operation-correlated transfer design because hot and cold wallets will already contain changing client assets.

## Configuration

Use three different local Hardhat accounts. The corporate private key must correspond to `CASP_CORPORATE_ADDRESS` and needs local ETH for gas.

The service loads `.env` from this directory at startup. Existing shell environment variables take precedence. `.env` is gitignored, while `.env.example` contains the reproducible fresh-Hardhat template. This workspace already contains a ready local `.env` using accounts 1, 2 and 3.

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `TOKEN_ADDRESS` | yes | none | deployed `ResearchUsdEMT` |
| `CASP_CORPORATE_PRIVATE_KEY` | yes | none | disposable local signer for the corporate wallet |
| `CASP_CORPORATE_ADDRESS` | yes | none | wallet receiving the issuer mint |
| `CASP_HOT_ADDRESS` | yes | none | target hot custody wallet |
| `CASP_COLD_ADDRESS` | yes | none | target cold custody wallet |
| `RPC_URL` | no | `http://127.0.0.1:8545` | local Ethereum JSON-RPC |
| `ISSUER_URL` | no | `http://127.0.0.1:3000` | issuer backend |
| `MOCK_BANK_URL` | no | `http://127.0.0.1:3100` | issuer mockBank |
| `CASP_HTTP_ADDRESS` | no | `127.0.0.1:3200` | CASP API listen address |
| `CASP_DATABASE_PATH` | no | `data/casp.sqlite` | independent CASP SQLite database |

## Local tutorial

Start a fresh Hardhat node, deploy the current contract, start mockBank and the issuer backend first. Use account 0 as the issuer. A simple CASP mapping is:

- Hardhat account 1: CASP corporate;
- Hardhat account 2: CASP hot;
- Hardhat account 3: CASP cold.

In a new PowerShell terminal no manual variable assignment is needed:

```powershell
cd D:\repos\Magisterka\kod\casp\backend
cargo run
```

Never paste a real or funded private key into this demo.

At startup the backend automatically resumes the idempotent initial purchase and does not open its HTTP port until the 10,000 rUSD pool is ready. The issuer backend, mockBank and Hardhat node must therefore already be running. The following endpoint remains available as a manual retry/inspection tool:

```powershell
Invoke-RestMethod -Method Post http://127.0.0.1:3200/api/v1/admin/bootstrap-inventory
```

Expected final status: `distributed`.

Inspect the persisted operation and live wallet balances:

```powershell
Invoke-RestMethod http://127.0.0.1:3200/api/v1/admin/bootstrap-inventory
Invoke-RestMethod http://127.0.0.1:3200/api/v1/admin/wallets
```

Expected bootstrap custody targets in smallest units:

```text
hotRaw       = 2000000000
coldRaw      = 8000000000
```

Calling the POST endpoint again returns the existing completed operation and does not purchase or transfer another 10,000 rUSD.

## API

- `GET /health`
- `POST /api/v1/admin/bootstrap-inventory`
- `GET /api/v1/admin/bootstrap-inventory`
- `GET /api/v1/admin/wallets`
- `GET /api/v1/admin/reconciliation`
- `GET /api/v1/admin/fees`
- `GET /api/v1/clients`
- `GET /api/v1/clients/{clientId}/account`
- `GET /api/v1/clients/{clientId}/records`
- `POST /api/v1/clients/{clientId}/purchases`
- `POST /api/v1/clients/{clientId}/sales`
- `POST /api/v1/clients/{clientId}/transfers`
- `POST /api/v1/clients/{clientId}/redemptions` (issuer-redemption integration; not used by the current customer screen)

Retail request bodies contain a caller-generated `operationId`. Reusing it with the same payload returns the same operation without posting balances twice; reusing it with different parameters returns a conflict. The demo exposes three deterministic customers: `alice`, `bob` and `carol`.

Customer purchase and sale are CASP-internal SQLite postings. They move value between the unallocated-inventory position and the selected customer's position, while the total rUSD held in CASP hot/cold custody wallets remains unchanged. They do not call Ethereum or the issuer. The separate `redemptions` endpoint represents redemption at the issuer and is intentionally not wired to the current buy/sell screen.

Example purchase of 25 rUSD:

```powershell
$body = @{ operationId = [guid]::NewGuid().ToString(); amountUsdMinor = 2500 } | ConvertTo-Json
Invoke-RestMethod -Method Post -ContentType application/json -Body $body http://127.0.0.1:3200/api/v1/clients/alice/purchases
```

Example CASP-internal sale of 10 rUSD (the token has six decimals):

```powershell
$body = @{ operationId = [guid]::NewGuid().ToString(); tokenAmountRaw = 10000000 } | ConvertTo-Json
Invoke-RestMethod -Method Post -ContentType application/json -Body $body http://127.0.0.1:3200/api/v1/clients/alice/sales
```

Example internal transfer of gross 10 rUSD from Alice to Bob:

```powershell
$body = @{
  operationId = [guid]::NewGuid().ToString()
  recipientClientId = "bob"
  tokenAmountRaw = 10000000
  purposeClassification = "private_transfer"
} | ConvertTo-Json
Invoke-RestMethod -Method Post -ContentType application/json -Body $body http://127.0.0.1:3200/api/v1/clients/alice/transfers
```

The sender is debited by the gross amount. The recipient receives 99.9%, while the 0.1% demo transaction fee is posted to `fee_position.pending_raw`. The three postings and the audit records share one SQLite transaction. No Ethereum transaction or gas fee is involved. Until a future on-chain sweep moves accrued fees to the corporate wallet, pending fees remain included in hot/cold custody obligations.

## Verification

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The current tests harden operation idempotency, lifecycle persistence and the final 0/2,000/8,000 reconciliation. HTTP adapters and the real local-chain path are verified by running the tutorial against the issuer stack.
