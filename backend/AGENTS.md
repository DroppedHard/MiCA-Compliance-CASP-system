# CASP backend agent context

This is a separate Rust service and Git repository from the issuer backend.

Architecture direction:

- `domain.rs` contains serialized CASP-owned state;
- `application.rs` owns the bootstrap saga and infrastructure ports;
- `infrastructure/` contains SQLite, issuer/mockBank HTTP and Alloy adapters;
- `api.rs` maps HTTP to application use cases;
- `main.rs` is only the composition root.

The issuer API, mockBank API and ERC-20 ABI are trust boundaries. Do not couple this service to issuer Rust types or its SQLite database. Preserve idempotency keys and persisted intermediate states when changing purchase logic.

The initial fixed-balance distribution is deliberately limited to an isolated, fresh-chain bootstrap. Do not reuse it as the general custody algorithm after client balances exist.
