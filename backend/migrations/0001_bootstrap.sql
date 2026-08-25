CREATE TABLE IF NOT EXISTS inventory_bootstrap (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    operation_id TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    amount_usd_minor INTEGER NOT NULL,
    token_amount_raw INTEGER NOT NULL,
    corporate_address TEXT NOT NULL,
    hot_address TEXT NOT NULL,
    cold_address TEXT NOT NULL,
    hot_target_raw INTEGER NOT NULL,
    cold_target_raw INTEGER NOT NULL,
    issuer_transaction_hash TEXT,
    cold_transaction_hash TEXT,
    hot_transaction_hash TEXT,
    last_error TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
