CREATE TABLE IF NOT EXISTS service_record_details(
  record_id TEXT PRIMARY KEY,
  record_status TEXT NOT NULL CHECK(record_status IN ('new','cancellation')),
  received_at_unix_ms INTEGER NOT NULL,
  accepted_at_unix_ms INTEGER,
  executed_at_unix_ms INTEGER,
  settled_at_unix_ms INTEGER,
  failed_at_unix_ms INTEGER,
  price_method TEXT NOT NULL,
  unit_price_minor INTEGER,
  gross_quantity_raw INTEGER NOT NULL,
  net_quantity_raw INTEGER NOT NULL,
  fee_quantity_raw INTEGER NOT NULL,
  instruction_channel TEXT NOT NULL,
  execution_actor TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  rejection_reason TEXT,
  retention_until_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS service_record_amendments(
  amendment_id TEXT PRIMARY KEY,
  original_record_id TEXT NOT NULL,
  amendment_type TEXT NOT NULL CHECK(amendment_type IN ('correction','reversal')),
  reason TEXT NOT NULL,
  actor TEXT NOT NULL,
  created_at_unix_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_service_record_amendments_original
ON service_record_amendments(original_record_id, created_at_unix_ms);
