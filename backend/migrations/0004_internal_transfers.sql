CREATE TABLE IF NOT EXISTS internal_transfers (
    operation_id TEXT PRIMARY KEY,
    sender_client_id TEXT NOT NULL,
    recipient_client_id TEXT NOT NULL,
    gross_raw INTEGER NOT NULL CHECK(gross_raw > 0),
    fee_raw INTEGER NOT NULL CHECK(fee_raw > 0),
    net_raw INTEGER NOT NULL CHECK(net_raw > 0),
    purpose_classification TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS fee_position (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    pending_raw INTEGER NOT NULL CHECK(pending_raw >= 0)
);

INSERT OR IGNORE INTO fee_position(singleton, pending_raw) VALUES(1, 0);
