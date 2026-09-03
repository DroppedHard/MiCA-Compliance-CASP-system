# CASP backend

The Rust service maintains an independent customer-entitlement ledger and communicates with the issuer over HTTP. SQLite stores customer positions, inventory, fees, and operation history; blockchain provides custody-wallet balances.

## Source layout

`src/api` is deliberately thin: `router` composes the routes, `routes` declares HTTP paths, `requests` and `validators` define and validate input, and `responses` maps domain failures to stable HTTP responses. Handlers are separated into `public`, `administration`, and `customer`; they call services only through `AppState`.

Use cases live in `src/services`, business models and rules in `src/domain`, and port implementations in `src/infrastructure/sqlite`, `src/infrastructure/blockchain`, and `src/infrastructure/issuer`. Endpoints therefore do not contain SQL or blockchain/issuer transport details.

Hot custody serves external operations, cold custody holds most customer assets, and the corporate wallet contains CASP-owned assets such as swept fees. Bootstrap buys 10,000 rUSD directly into hot custody and rebalances 8,000 rUSD to cold custody. Customer allocation is an atomic SQLite posting, not a blockchain transfer.

For a non-Docker start, run the contract, mockBank, and issuer backend first, copy `.env.example` to `.env`, and execute `cargo run`. Configuration includes `TOKEN_ADDRESS`, the three wallet addresses and keys, `RPC_URL`, `ISSUER_URL`, `MOCK_BANK_URL`, and `CASP_DATABASE_PATH`.

The main API areas cover `/api/v1/clients`, administrative wallets and reconciliation, rebalancing, inventory replenishment, address and account restrictions, and daily reports.

```powershell
cargo fmt --all --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```
# Planned OpenAPI support

HTTP contracts are currently maintained manually. A planned extension is to generate an OpenAPI specification directly from routes, request and response models, and error types. This should simplify CASP frontend integration and CASP-to-issuer communication, but it is not part of the current source-structure refactoring.
