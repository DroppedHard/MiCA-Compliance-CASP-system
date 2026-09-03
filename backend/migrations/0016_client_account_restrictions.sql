CREATE TABLE IF NOT EXISTS client_account_restrictions (
    client_id TEXT PRIMARY KEY,
    reason TEXT NOT NULL,
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    updated_at_unix_ms INTEGER NOT NULL,
    FOREIGN KEY (client_id) REFERENCES client_positions(client_id)
);

