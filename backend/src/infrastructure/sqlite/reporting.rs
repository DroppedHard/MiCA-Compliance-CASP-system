use crate::reporting::{ReportingError, ReportingEvent, ReportingStore};
use rusqlite::{Connection, params};
use std::{fs, path::Path, sync::Mutex};

pub struct SqliteReportingStore {
    connection: Mutex<Connection>,
}

impl SqliteReportingStore {
    pub fn open(path: &str) -> Result<Self, ReportingError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .execute_batch(include_str!("../../../migrations/0002_retail.sql"))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!(
                "../../../migrations/0004_internal_transfers.sql"
            ))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!(
                "../../../migrations/0014_demo_reporting_events.sql"
            ))
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Adds a rolling six-day presentation fixture without changing customer balances.
    /// Stable operation IDs make repeated startup calls idempotent for one database.
    pub fn seed_demo_history(&self) -> Result<(), ReportingError> {
        self.connection
            .lock()
            .map_err(storage)?
            .execute_batch(
                "INSERT OR IGNORE INTO demo_reporting_events VALUES
                 ('demo-report-d6-goods', date('now','-6 day'), 'goods_or_services', 124500000, 124500, 0),
                 ('demo-report-d6-private', date('now','-6 day'), 'private_transfer', 48000000, 48000, 0),
                 ('demo-report-d5-goods-a', date('now','-5 day'), 'goods_or_services', 81500000, 81500, 0),
                 ('demo-report-d5-goods-b', date('now','-5 day'), 'goods_or_services', 36750000, 36750, 0),
                 ('demo-report-d5-exchange', date('now','-5 day'), 'exchange_for_funds', 250000000, 0, 0),
                 ('demo-report-d4-private', date('now','-4 day'), 'private_transfer', 92000000, 92000, 0),
                 ('demo-report-d4-goods', date('now','-4 day'), 'goods_or_services', 157250000, 157250, 0),
                 ('demo-report-d3-goods-a', date('now','-3 day'), 'goods_or_services', 44300000, 44300, 0),
                 ('demo-report-d3-goods-b', date('now','-3 day'), 'goods_or_services', 189900000, 189900, 0),
                 ('demo-report-d3-same-owner', date('now','-3 day'), 'same_owner_transfer', 60000000, 0, 0),
                 ('demo-report-d2-exchange', date('now','-2 day'), 'exchange_for_funds', 310000000, 0, 0),
                 ('demo-report-d2-goods', date('now','-2 day'), 'goods_or_services', 138400000, 138400, 0),
                 ('demo-report-d1-goods-a', date('now','-1 day'), 'goods_or_services', 74250000, 74250, 0),
                 ('demo-report-d1-goods-b', date('now','-1 day'), 'goods_or_services', 216800000, 216800, 0),
                 ('demo-report-d1-private', date('now','-1 day'), 'private_transfer', 53500000, 53500, 0);",
            )
            .map_err(storage)?;
        Ok(())
    }
}

impl ReportingStore for SqliteReportingStore {
    fn events(&self, from: &str, to: &str) -> Result<Vec<ReportingEvent>, ReportingError> {
        let connection = self.connection.lock().map_err(storage)?;
        let mut statement = connection
            .prepare(
                "SELECT date(created_at_unix_ms / 1000, 'unixepoch'), operation_id,
                 CASE order_type WHEN 'purchase' THEN 'exchange_for_funds' WHEN 'sale' THEN 'exchange_for_funds' WHEN 'redemption' THEN 'exchange_for_funds' ELSE 'unknown' END,
                 quantity_raw, fiat_amount_minor, 0, CASE WHEN order_type='redemption' AND blockchain_transaction_hash IS NOT NULL THEN 1 ELSE 0 END
                 FROM retail_orders WHERE status='completed' AND date(created_at_unix_ms / 1000, 'unixepoch') BETWEEN ?1 AND ?2
                 UNION ALL
                 SELECT date(created_at_unix_ms / 1000, 'unixepoch'), operation_id, purpose_classification, gross_raw, NULL, fee_raw, 0
                 FROM internal_transfers WHERE status='completed' AND date(created_at_unix_ms / 1000, 'unixepoch') BETWEEN ?1 AND ?2
                 UNION ALL
                 SELECT date_utc, operation_id, classification, value_raw, NULL, fee_raw, known_onchain_overlap
                 FROM demo_reporting_events WHERE date_utc BETWEEN ?1 AND ?2
                 ORDER BY 1,2",
            )
            .map_err(storage)?;
        statement
            .query_map(params![from, to], |row| {
                Ok(ReportingEvent {
                    date_utc: row.get(0)?,
                    operation_id: row.get(1)?,
                    classification: row.get(2)?,
                    value_raw: row.get::<_, i64>(3)? as u64,
                    value_usd_minor: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                    fee_raw: row.get::<_, i64>(5)? as u64,
                    known_onchain_overlap: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)
    }
}

fn storage(error: impl std::fmt::Display) -> ReportingError {
    ReportingError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        infrastructure::SqliteRetailStore,
        reporting::ReportingService,
        retail_application::{RetailStore, TransferPosting},
    };
    use std::sync::Arc;

    #[test]
    fn projects_completed_raw_operations_without_counting_fiat_exchange_as_means_of_exchange() {
        let path =
            std::env::temp_dir().join(format!("casp-reporting-{}.sqlite", uuid::Uuid::now_v7()));
        let path_text = path.to_string_lossy().to_string();
        let retail = SqliteRetailStore::open(&path_text).unwrap();
        retail.activate_inventory(20_000_000).unwrap();
        retail.purchase("purchase", "alice", 1000, "x", 1).unwrap();
        retail
            .transfer(TransferPosting {
                id: "goods",
                sender: "alice",
                recipient: "bob",
                gross_raw: 5_000_000,
                purpose: "goods_or_services",
                contract: "x",
                chain: 1,
            })
            .unwrap();
        let service =
            ReportingService::new(Arc::new(SqliteReportingStore::open(&path_text).unwrap()));
        let report = service.daily("1970-01-01", "9999-12-31").unwrap();
        assert_eq!(report.days.len(), 1);
        assert_eq!(report.days[0].total_operation_count, 2);
        assert_eq!(report.days[0].means_of_exchange_count, 1);
        assert_eq!(report.days[0].means_of_exchange_value_usd_minor, "500");
        drop(service);
        drop(retail);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reports_exact_fiat_value_when_exchange_rate_creates_fractional_token_cents() {
        let path = std::env::temp_dir().join(format!(
            "casp-reporting-rate-{}.sqlite",
            uuid::Uuid::now_v7()
        ));
        let path_text = path.to_string_lossy().to_string();
        let retail = SqliteRetailStore::open(&path_text).unwrap();
        retail.activate_inventory(200_000_000).unwrap();
        retail.set_exchange_rate(110).unwrap();
        let order = retail
            .purchase("purchase-at-1-10", "alice", 10_000, "x", 1)
            .unwrap();
        assert_eq!(order.quantity_raw, "90909090");

        let service =
            ReportingService::new(Arc::new(SqliteReportingStore::open(&path_text).unwrap()));
        let report = service.daily("1970-01-01", "9999-12-31").unwrap();

        assert_eq!(report.days.len(), 1);
        assert_eq!(report.days[0].total_value_usd_minor, "10000");
        drop(service);
        drop(retail);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn demo_history_is_idempotent_and_populates_only_previous_days() {
        let store = SqliteReportingStore::open(":memory:").unwrap();
        store.seed_demo_history().unwrap();
        store.seed_demo_history().unwrap();

        let events = store.events("1970-01-01", "9999-12-31").unwrap();
        assert_eq!(events.len(), 15);
        assert!(
            events
                .iter()
                .all(|event| event.date_utc < current_utc_date())
        );
        assert!(
            events
                .iter()
                .any(|event| event.classification == "goods_or_services")
        );
    }

    fn current_utc_date() -> String {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .query_row("SELECT date('now')", [], |row| row.get(0))
            .unwrap()
    }
}
