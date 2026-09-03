CREATE TABLE IF NOT EXISTS address_blacklist (
    normalized_address TEXT PRIMARY KEY,
    original_address TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS blocked_transfer_attempts (
    attempt_id TEXT PRIMARY KEY,
    transfer_kind TEXT NOT NULL,
    source_address TEXT NOT NULL,
    destination_address TEXT NOT NULL,
    transaction_reference TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL
);

-- Public address of an unused deterministic Hardhat demo account. This is not
-- a private key and has no meaning outside the disposable local chain.
INSERT OR IGNORE INTO address_blacklist(normalized_address, original_address, reason, created_at_unix_ms)
VALUES('0xa0ee7a142d267c1f36714e4a8f75612f20a79720', '0xa0Ee7A142d267C1f36714E4a8F75612F20a79720', 'Przykładowa blokada demonstracyjna', 0);
