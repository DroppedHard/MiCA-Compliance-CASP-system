use crate::reconciliation::{
    ReconciliationError, ReconciliationSnapshot, ReconciliationStatus, ReconciliationStore,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{fs, path::Path, sync::Mutex};

pub struct SqliteReconciliationStore {
    connection: Mutex<Connection>,
}

impl SqliteReconciliationStore {
    pub fn open(path: &str) -> Result<Self, ReconciliationError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .execute_batch(include_str!(
                "../../migrations/0003_custody_reconciliation.sql"
            ))
            .map_err(storage)?;
        if !has_column(
            &connection,
            "custody_reconciliation_snapshots",
            "pending_fee_raw",
        )? {
            connection
                .execute(
                    "ALTER TABLE custody_reconciliation_snapshots ADD COLUMN pending_fee_raw INTEGER",
                    [],
                )
                .map_err(storage)?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl ReconciliationStore for SqliteReconciliationStore {
    fn append(&self, value: &ReconciliationSnapshot) -> Result<(), ReconciliationError> {
        self.connection.lock().map_err(storage)?.execute("INSERT INTO custody_reconciliation_snapshots(status,hot_raw,cold_raw,corporate_raw,customer_available_raw,customer_locked_raw,inventory_available_raw,pending_fee_raw,custody_total_raw,obligation_total_raw,difference_raw,evidence_block,reason,checked_at_unix_ms,policy_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)", params![status(value.status),number(&value.hot_raw)?,number(&value.cold_raw)?,number(&value.corporate_raw)?,number(&value.customer_available_raw)?,number(&value.customer_locked_raw)?,number(&value.inventory_available_raw)?,number(&value.pending_fee_raw)?,number(&value.custody_total_raw)?,number(&value.obligation_total_raw)?,signed_number(&value.difference_raw)?,value.evidence_block.map(|v|v as i64),value.reason,value.checked_at_unix_ms as i64,value.policy_version]).map_err(storage)?;
        Ok(())
    }

    fn latest(&self) -> Result<Option<ReconciliationSnapshot>, ReconciliationError> {
        self.connection.lock().map_err(storage)?.query_row("SELECT status,hot_raw,cold_raw,corporate_raw,customer_available_raw,customer_locked_raw,inventory_available_raw,pending_fee_raw,custody_total_raw,obligation_total_raw,difference_raw,evidence_block,reason,checked_at_unix_ms,policy_version FROM custody_reconciliation_snapshots ORDER BY id DESC LIMIT 1",[],|row|Ok(ReconciliationSnapshot{status:parse_status(&row.get::<_,String>(0)?),hot_raw:optional_string(row.get(1)?),cold_raw:optional_string(row.get(2)?),corporate_raw:optional_string(row.get(3)?),customer_available_raw:optional_string(row.get(4)?),customer_locked_raw:optional_string(row.get(5)?),inventory_available_raw:optional_string(row.get(6)?),pending_fee_raw:optional_string(row.get(7)?),custody_total_raw:optional_string(row.get(8)?),obligation_total_raw:optional_string(row.get(9)?),difference_raw:row.get::<_,Option<i64>>(10)?.map(|v|v.to_string()),evidence_block:row.get::<_,Option<i64>>(11)?.map(|v|v as u64),reason:row.get(12)?,checked_at_unix_ms:row.get::<_,i64>(13)? as u64,policy_version:row.get(14)?})).optional().map_err(storage)
    }
}

fn status(value: ReconciliationStatus) -> &'static str {
    match value {
        ReconciliationStatus::Balanced => "balanced",
        ReconciliationStatus::Warning => "warning",
        ReconciliationStatus::Blocked => "blocked",
        ReconciliationStatus::Unavailable => "unavailable",
    }
}
fn parse_status(value: &str) -> ReconciliationStatus {
    match value {
        "balanced" => ReconciliationStatus::Balanced,
        "warning" => ReconciliationStatus::Warning,
        "blocked" => ReconciliationStatus::Blocked,
        _ => ReconciliationStatus::Unavailable,
    }
}
fn number(value: &Option<String>) -> Result<Option<i64>, ReconciliationError> {
    value
        .as_ref()
        .map(|v| v.parse::<i64>().map_err(storage))
        .transpose()
}
fn signed_number(value: &Option<String>) -> Result<Option<i64>, ReconciliationError> {
    number(value)
}
fn optional_string(value: Option<i64>) -> Option<String> {
    value.map(|v| v.to_string())
}
fn storage(error: impl std::fmt::Display) -> ReconciliationError {
    ReconciliationError::Storage(error.to_string())
}
fn has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, ReconciliationError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(storage)?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    Ok(names.iter().any(|name| name == column))
}
