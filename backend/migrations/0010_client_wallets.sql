CREATE TABLE IF NOT EXISTS client_wallets (
    client_id TEXT PRIMARY KEY REFERENCES client_positions(client_id),
    wallet_address TEXT NOT NULL UNIQUE
);

INSERT OR IGNORE INTO client_wallets VALUES ('alice', 'rusd:casp:alice');
INSERT OR IGNORE INTO client_wallets VALUES ('bob', 'rusd:casp:bob');
INSERT OR IGNORE INTO client_wallets VALUES ('carol', 'rusd:casp:carol');
