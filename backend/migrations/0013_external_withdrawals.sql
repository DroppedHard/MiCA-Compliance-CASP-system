CREATE TABLE IF NOT EXISTS external_withdrawals (
    operation_id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    destination_address TEXT NOT NULL,
    amount_raw INTEGER NOT NULL CHECK(amount_raw > 0),
    fee_raw INTEGER NOT NULL CHECK(fee_raw > 0),
    total_debit_raw INTEGER NOT NULL CHECK(total_debit_raw > amount_raw),
    status TEXT NOT NULL,
    transaction_hash TEXT,
    last_error TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
