CREATE TABLE IF NOT EXISTS casp_exchange_rate (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    usd_minor_per_rusd INTEGER NOT NULL CHECK(usd_minor_per_rusd BETWEEN 1 AND 10000),
    updated_at_unix_ms INTEGER NOT NULL
);
INSERT OR IGNORE INTO casp_exchange_rate VALUES(1, 100, 0);
