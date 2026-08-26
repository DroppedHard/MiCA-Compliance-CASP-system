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
            .execute_batch(include_str!("../../migrations/0002_retail.sql"))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!("../../migrations/0004_internal_transfers.sql"))
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl ReportingStore for SqliteReportingStore {
    fn events(&self, from: &str, to: &str) -> Result<Vec<ReportingEvent>, ReportingError> {
        let connection = self.connection.lock().map_err(storage)?;
        let mut statement = connection
            .prepare(
                "SELECT date(created_at_unix_ms / 1000, 'unixepoch'), operation_id,
                 CASE order_type WHEN 'purchase' THEN 'exchange_for_funds' WHEN 'sale' THEN 'exchange_for_funds' WHEN 'redemption' THEN 'exchange_for_funds' ELSE 'unknown' END,
                 quantity_raw, 0, CASE WHEN order_type='redemption' AND blockchain_transaction_hash IS NOT NULL THEN 1 ELSE 0 END
                 FROM retail_orders WHERE status='completed' AND date(created_at_unix_ms / 1000, 'unixepoch') BETWEEN ?1 AND ?2
                 UNION ALL
                 SELECT date(created_at_unix_ms / 1000, 'unixepoch'), operation_id, purpose_classification, gross_raw, fee_raw, 0
                 FROM internal_transfers WHERE status='completed' AND date(created_at_unix_ms / 1000, 'unixepoch') BETWEEN ?1 AND ?2
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
                    fee_raw: row.get::<_, i64>(4)? as u64,
                    known_onchain_overlap: row.get::<_, i64>(5)? != 0,
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
}
