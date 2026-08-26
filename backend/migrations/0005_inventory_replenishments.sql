CREATE TABLE IF NOT EXISTS inventory_replenishments(
  operation_id TEXT PRIMARY KEY,
  status TEXT NOT NULL,
  amount_usd_minor INTEGER NOT NULL CHECK(amount_usd_minor > 0),
  token_amount_raw INTEGER NOT NULL CHECK(token_amount_raw > 0),
  hot_increment_raw INTEGER NOT NULL CHECK(hot_increment_raw > 0),
  cold_increment_raw INTEGER NOT NULL CHECK(cold_increment_raw > 0),
  hot_target_raw INTEGER,
  cold_target_raw INTEGER,
  issuer_transaction_hash TEXT,
  cold_transaction_hash TEXT,
  hot_transaction_hash TEXT,
  last_error TEXT,
  created_at_unix_ms INTEGER NOT NULL,
  updated_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS inventory_replenishment_postings(
  operation_id TEXT NOT NULL,
  wallet_role TEXT NOT NULL CHECK(wallet_role IN ('hot','cold')),
  amount_raw INTEGER NOT NULL CHECK(amount_raw > 0),
  created_at_unix_ms INTEGER NOT NULL,
  PRIMARY KEY(operation_id, wallet_role)
);
