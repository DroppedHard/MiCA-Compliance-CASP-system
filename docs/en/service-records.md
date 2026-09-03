# CASP service-record model

The position ledger is authoritative for customer balances. A separate append-only service register reconstructs how an instruction was accepted and executed but never changes a balance itself. Its fields are a research subset inspired by Commission Delegated Regulation (EU) 2025/1140.

A record contains stable record and operation identifiers, customer and parties, service classification, asset and network, lifecycle timestamps, gross/net/fee amounts, fiat and price-method data, an on-chain transaction hash when relevant, processing actors, policy version, and rejection reason.

Corrections are appended to `service_record_amendments` and reference the original record. They do not overwrite history or reverse ledger postings; financial reversal requires a compensating operation.

- `GET /api/v1/admin/service-records` exports records;
- `GET /api/v1/admin/service-record-amendments` returns correction history;
- `POST /api/v1/admin/service-record-amendments` appends an annotation.

The demo assigns a five-year retention deadline but does not implement automatic archival or formal regulator submission.
