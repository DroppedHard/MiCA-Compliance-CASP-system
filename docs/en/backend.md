# CASP backend

The Rust service maintains an independent customer-entitlement ledger and communicates with the issuer over HTTP. SQLite stores customer positions, inventory, fees, and operation history; blockchain provides custody-wallet balances.

Hot custody serves external operations, cold custody holds most customer assets, and the corporate wallet contains CASP-owned assets such as swept fees. Bootstrap buys 10,000 rUSD directly into hot custody and rebalances 8,000 rUSD to cold custody. Customer allocation is an atomic SQLite posting, not a blockchain transfer.

For a non-Docker start, run the contract, mockBank, and issuer backend first, copy `.env.example` to `.env`, and execute `cargo run`. Configuration includes `TOKEN_ADDRESS`, the three wallet addresses and keys, `RPC_URL`, `ISSUER_URL`, `MOCK_BANK_URL`, and `CASP_DATABASE_PATH`.

The main API areas cover `/api/v1/clients`, administrative wallets and reconciliation, rebalancing, inventory replenishment, address and account restrictions, and daily reports.

```powershell
cargo fmt --all --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

