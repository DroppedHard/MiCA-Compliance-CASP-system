CREATE TABLE IF NOT EXISTS demo_reporting_events (
    operation_id TEXT PRIMARY KEY,
    date_utc TEXT NOT NULL,
    classification TEXT NOT NULL,
    value_raw INTEGER NOT NULL CHECK(value_raw > 0),
    fee_raw INTEGER NOT NULL CHECK(fee_raw >= 0),
    known_onchain_overlap INTEGER NOT NULL CHECK(known_onchain_overlap IN (0, 1))
);

CREATE INDEX IF NOT EXISTS idx_demo_reporting_events_date
ON demo_reporting_events(date_utc);
