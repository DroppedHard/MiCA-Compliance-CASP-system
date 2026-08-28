use crate::fee_sweep::{FeeSweep, FeeSweepError, FeeSweepStore};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct SqliteFeeSweepStore {
    connection: Mutex<Connection>,
}

impl SqliteFeeSweepStore {
    pub fn open(path: &str) -> Result<Self, FeeSweepError> {
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
        connection
            .execute_batch(include_str!("../../migrations/0009_fee_sweeps.sql"))
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl FeeSweepStore for SqliteFeeSweepStore {
    fn begin(&self, operation_id: &str) -> Result<FeeSweep, FeeSweepError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        if let Some(existing) = read(&tx, operation_id)? {
            if existing.status == "failed" {
                tx.execute("UPDATE fee_sweeps SET status='pending',last_error=NULL,updated_at_unix_ms=?2 WHERE operation_id=?1", params![operation_id, now()?]).map_err(storage)?;
            }
            tx.commit().map_err(storage)?;
            drop(connection);
            return self
                .get(operation_id)?
                .ok_or_else(|| FeeSweepError::Storage("fee sweep disappeared".into()));
        }
        let pending: i64 = tx
            .query_row(
                "SELECT pending_raw FROM fee_position WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if pending <= 0 {
            return Err(FeeSweepError::NoPendingFees);
        }
        let timestamp = now()?;
        tx.execute("INSERT INTO fee_sweeps(operation_id,amount_raw,status,created_at_unix_ms,updated_at_unix_ms) VALUES(?1,?2,'pending',?3,?3)", params![operation_id,pending,timestamp]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        drop(connection);
        self.get(operation_id)?
            .ok_or_else(|| FeeSweepError::Storage("fee sweep disappeared".into()))
    }

    fn chain_confirmed(
        &self,
        operation_id: &str,
        transaction_hash: &str,
    ) -> Result<FeeSweep, FeeSweepError> {
        let connection = self.connection.lock().map_err(storage)?;
        connection.execute("UPDATE fee_sweeps SET status='chain_confirmed',transaction_hash=?2,last_error=NULL,updated_at_unix_ms=?3 WHERE operation_id=?1 AND transaction_hash IS NULL", params![operation_id,transaction_hash,now()?]).map_err(storage)?;
        drop(connection);
        self.get(operation_id)?
            .ok_or_else(|| FeeSweepError::Storage("fee sweep disappeared".into()))
    }

    fn complete(&self, operation_id: &str) -> Result<FeeSweep, FeeSweepError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        let operation = read(&tx, operation_id)?
            .ok_or_else(|| FeeSweepError::Storage("fee sweep does not exist".into()))?;
        if operation.status == "completed" {
            return Ok(operation);
        }
        if operation.status != "chain_confirmed" {
            return Err(FeeSweepError::Storage(
                "on-chain transfer is not confirmed".into(),
            ));
        }
        let amount = operation.amount_raw.parse::<i64>().map_err(storage)?;
        let changed = tx.execute("UPDATE fee_position SET pending_raw=pending_raw-?1 WHERE singleton=1 AND pending_raw>=?1", [amount]).map_err(storage)?;
        if changed != 1 {
            return Err(FeeSweepError::Storage(
                "pending fee position is lower than confirmed sweep".into(),
            ));
        }
        tx.execute("INSERT INTO ledger_entries(entry_id,operation_id,account_type,account_id,direction,quantity_raw,created_at_unix_ms) VALUES(lower(hex(randomblob(16))),?1,'casp_fee_pending','casp-fees','debit',?2,?3)", params![operation_id,amount,now()?]).map_err(storage)?;
        tx.execute(
            "UPDATE fee_sweeps SET status='completed',updated_at_unix_ms=?2 WHERE operation_id=?1",
            params![operation_id, now()?],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        drop(connection);
        self.get(operation_id)?
            .ok_or_else(|| FeeSweepError::Storage("fee sweep disappeared".into()))
    }

    fn fail(&self, operation_id: &str, error: &str) -> Result<(), FeeSweepError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE fee_sweeps SET status='failed',last_error=?2,updated_at_unix_ms=?3 WHERE operation_id=?1 AND transaction_hash IS NULL", params![operation_id,error,now()?]).map_err(storage)?;
        Ok(())
    }

    fn get(&self, operation_id: &str) -> Result<Option<FeeSweep>, FeeSweepError> {
        read(&*self.connection.lock().map_err(storage)?, operation_id)
    }
}

fn read(connection: &Connection, operation_id: &str) -> Result<Option<FeeSweep>, FeeSweepError> {
    connection.query_row("SELECT operation_id,amount_raw,status,transaction_hash,last_error,created_at_unix_ms,updated_at_unix_ms FROM fee_sweeps WHERE operation_id=?1", [operation_id], |row| Ok(FeeSweep { operation_id: row.get(0)?, amount_raw: row.get::<_,i64>(1)?.to_string(), status: row.get(2)?, transaction_hash: row.get(3)?, last_error: row.get(4)?, created_at_unix_ms: row.get::<_,i64>(5)? as u64, updated_at_unix_ms: row.get::<_,i64>(6)? as u64 })).optional().map_err(storage)
}
fn now() -> Result<i64, FeeSweepError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(storage)?
        .as_millis() as i64)
}
fn storage(error: impl std::fmt::Display) -> FeeSweepError {
    FeeSweepError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completing_a_sweep_moves_the_exact_pending_position_once() {
        let store = SqliteFeeSweepStore::open(":memory:").unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE fee_position SET pending_raw=1000 WHERE singleton=1",
                [],
            )
            .unwrap();
        let pending = store.begin("sweep-1").unwrap();
        assert_eq!(pending.amount_raw, "1000");
        store.chain_confirmed("sweep-1", "0xabc").unwrap();
        let completed = store.complete("sweep-1").unwrap();
        let replay = store.complete("sweep-1").unwrap();
        assert_eq!(completed, replay);
        assert_eq!(completed.status, "completed");
        let remaining: i64 = store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT pending_raw FROM fee_position WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn refuses_to_create_a_sweep_without_pending_fees() {
        let store = SqliteFeeSweepStore::open(":memory:").unwrap();
        assert!(matches!(
            store.begin("empty"),
            Err(FeeSweepError::NoPendingFees)
        ));
    }
}
