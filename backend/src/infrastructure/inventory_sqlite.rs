use crate::inventory::{
    InventoryError, InventoryOperation, InventoryStatus, InventoryStore, allocation,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{fs, path::Path, sync::Mutex};

pub struct SqliteInventoryStore {
    connection: Mutex<Connection>,
}

impl SqliteInventoryStore {
    pub fn open(path: &str) -> Result<Self, InventoryError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .execute_batch(include_str!(
                "../../migrations/0005_inventory_replenishments.sql"
            ))
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl InventoryStore for SqliteInventoryStore {
    fn create(&self, id: &str, amount_minor: u64) -> Result<InventoryOperation, InventoryError> {
        let connection = self.connection.lock().map_err(storage)?;
        if let Some(existing) = query_one(&connection, id)? {
            if existing.amount_usd_minor != amount_minor.to_string() {
                return Err(InventoryError::IdempotencyConflict);
            }
            return Ok(existing);
        }
        let (total, hot, cold) = allocation(amount_minor)?;
        let now = now();
        connection.execute("INSERT INTO inventory_replenishments(operation_id,status,amount_usd_minor,token_amount_raw,hot_increment_raw,cold_increment_raw,created_at_unix_ms,updated_at_unix_ms) VALUES(?1,'created',?2,?3,?4,?5,?6,?6)", params![id, i64_of(amount_minor)?, i64_of(total)?, i64_of(hot)?, i64_of(cold)?, now as i64]).map_err(storage)?;
        query_one(&connection, id)?
            .ok_or_else(|| InventoryError::Storage("inserted operation disappeared".into()))
    }
    fn get(&self, id: &str) -> Result<Option<InventoryOperation>, InventoryError> {
        let connection = self.connection.lock().map_err(storage)?;
        query_one(&connection, id)
    }
    fn list(&self) -> Result<Vec<InventoryOperation>, InventoryError> {
        let connection = self.connection.lock().map_err(storage)?;
        let mut statement = connection
            .prepare(&format!("{} ORDER BY created_at_unix_ms DESC", SELECT))
            .map_err(storage)?;
        statement
            .query_map([], map)
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)
    }
    fn targets(&self, id: &str, hot: u64, cold: u64) -> Result<InventoryOperation, InventoryError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE inventory_replenishments SET status='targets_recorded',hot_target_raw=?1,cold_target_raw=?2,last_error=NULL,updated_at_unix_ms=?3 WHERE operation_id=?4", params![i64_of(hot)?,i64_of(cold)?,now() as i64,id]).map_err(storage)?;
        self.get(id)?
            .ok_or_else(|| InventoryError::Storage("operation not found".into()))
    }
    fn advance(
        &self,
        id: &str,
        status: InventoryStatus,
        issuer: Option<&str>,
        cold: Option<&str>,
        hot: Option<&str>,
    ) -> Result<InventoryOperation, InventoryError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE inventory_replenishments SET status=?1,issuer_transaction_hash=COALESCE(?2,issuer_transaction_hash),cold_transaction_hash=COALESCE(?3,cold_transaction_hash),hot_transaction_hash=COALESCE(?4,hot_transaction_hash),last_error=NULL,updated_at_unix_ms=?5 WHERE operation_id=?6", params![status_text(status),issuer,cold,hot,now() as i64,id]).map_err(storage)?;
        self.get(id)?
            .ok_or_else(|| InventoryError::Storage("operation not found".into()))
    }
    fn fail(&self, id: &str, message: &str) -> Result<(), InventoryError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE inventory_replenishments SET last_error=?1,updated_at_unix_ms=?2 WHERE operation_id=?3",params![message,now() as i64,id]).map_err(storage)?;
        Ok(())
    }
}

const SELECT: &str = "SELECT operation_id,status,amount_usd_minor,token_amount_raw,hot_increment_raw,cold_increment_raw,hot_target_raw,cold_target_raw,issuer_transaction_hash,cold_transaction_hash,hot_transaction_hash,last_error,created_at_unix_ms,updated_at_unix_ms FROM inventory_replenishments";
fn query_one(c: &Connection, id: &str) -> Result<Option<InventoryOperation>, InventoryError> {
    c.query_row(&format!("{} WHERE operation_id=?1", SELECT), [id], map)
        .optional()
        .map_err(storage)
}
fn map(row: &rusqlite::Row<'_>) -> rusqlite::Result<InventoryOperation> {
    let status: String = row.get(1)?;
    Ok(InventoryOperation {
        operation_id: row.get(0)?,
        status: parse_status(&status),
        amount_usd_minor: row.get::<_, i64>(2)?.to_string(),
        token_amount_raw: row.get::<_, i64>(3)?.to_string(),
        hot_increment_raw: row.get::<_, i64>(4)?.to_string(),
        cold_increment_raw: row.get::<_, i64>(5)?.to_string(),
        hot_target_raw: row.get::<_, Option<i64>>(6)?.map(|v| v.to_string()),
        cold_target_raw: row.get::<_, Option<i64>>(7)?.map(|v| v.to_string()),
        issuer_transaction_hash: row.get(8)?,
        cold_transaction_hash: row.get(9)?,
        hot_transaction_hash: row.get(10)?,
        last_error: row.get(11)?,
        created_at_unix_ms: row.get::<_, i64>(12)? as u64,
        updated_at_unix_ms: row.get::<_, i64>(13)? as u64,
    })
}
fn status_text(status: InventoryStatus) -> &'static str {
    match status {
        InventoryStatus::Created => "created",
        InventoryStatus::IssuerOrderCreated => "issuer_order_created",
        InventoryStatus::FiatSent => "fiat_sent",
        InventoryStatus::TokensIssued => "tokens_issued",
        InventoryStatus::TargetsRecorded => "targets_recorded",
        InventoryStatus::ColdDistributed => "cold_distributed",
        InventoryStatus::Completed => "completed",
    }
}
fn parse_status(value: &str) -> InventoryStatus {
    match value {
        "issuer_order_created" => InventoryStatus::IssuerOrderCreated,
        "fiat_sent" => InventoryStatus::FiatSent,
        "tokens_issued" => InventoryStatus::TokensIssued,
        "targets_recorded" => InventoryStatus::TargetsRecorded,
        "cold_distributed" => InventoryStatus::ColdDistributed,
        "completed" => InventoryStatus::Completed,
        _ => InventoryStatus::Created,
    }
}
fn i64_of(value: u64) -> Result<i64, InventoryError> {
    i64::try_from(value).map_err(|_| InventoryError::Overflow)
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn storage(error: impl std::fmt::Display) -> InventoryError {
    InventoryError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_idempotent_request_and_rejects_changed_amount() {
        let store = SqliteInventoryStore::open(":memory:").unwrap();
        let one = store.create("op", 100).unwrap();
        assert_eq!(one.hot_increment_raw, "200000");
        assert_eq!(store.create("op", 100).unwrap().operation_id, "op");
        assert!(matches!(
            store.create("op", 101),
            Err(InventoryError::IdempotencyConflict)
        ));
    }
}
