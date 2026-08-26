CREATE TABLE IF NOT EXISTS custody_reconciliation_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    status TEXT NOT NULL CHECK(status IN ('balanced','warning','blocked','unavailable')),
    hot_raw INTEGER,
    cold_raw INTEGER,
    corporate_raw INTEGER,
    customer_available_raw INTEGER,
    customer_locked_raw INTEGER,
    inventory_available_raw INTEGER,
    custody_total_raw INTEGER,
    obligation_total_raw INTEGER,
    difference_raw INTEGER,
    evidence_block INTEGER,
    reason TEXT NOT NULL,
    checked_at_unix_ms INTEGER NOT NULL,
    policy_version TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_custody_reconciliation_time
    ON custody_reconciliation_snapshots(checked_at_unix_ms DESC);
