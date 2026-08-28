CREATE TABLE IF NOT EXISTS external_deposits (
    chain_id INTEGER NOT NULL,
    transaction_hash TEXT NOT NULL,
    log_index INTEGER NOT NULL,
    block_number INTEGER NOT NULL,
    sender_address TEXT NOT NULL,
    client_reference TEXT NOT NULL,
    client_id TEXT,
    amount_raw INTEGER NOT NULL CHECK(amount_raw > 0),
    status TEXT NOT NULL CHECK(status IN ('credited', 'unknown_reference')),
    credited_at_unix_ms INTEGER,
    PRIMARY KEY(chain_id, transaction_hash, log_index)
);

CREATE TABLE IF NOT EXISTS external_deposit_checkpoint (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    last_confirmed_block INTEGER NOT NULL
);
INSERT OR IGNORE INTO external_deposit_checkpoint VALUES (1, 0);
