CREATE TABLE IF NOT EXISTS fee_sweeps (
    operation_id TEXT PRIMARY KEY,
    amount_raw INTEGER NOT NULL CHECK(amount_raw > 0),
    status TEXT NOT NULL CHECK(status IN ('pending', 'chain_confirmed', 'completed', 'failed')),
    transaction_hash TEXT,
    last_error TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
