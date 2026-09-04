use crate::external_withdrawals::{
    ExternalWithdrawal, ExternalWithdrawalError, ExternalWithdrawalStore,
};
use alloy::primitives::Address;
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub struct SqliteExternalWithdrawalStore {
    connection: Mutex<Connection>,
}
impl SqliteExternalWithdrawalStore {
    pub fn open(path: &str) -> Result<Self, ExternalWithdrawalError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let connection = Connection::open(path).map_err(storage)?;
        for migration in [
            include_str!("../../../migrations/0002_retail.sql"),
            include_str!("../../../migrations/0004_internal_transfers.sql"),
            include_str!("../../../migrations/0006_extended_service_records.sql"),
            include_str!("../../../migrations/0010_client_wallets.sql"),
            include_str!("../../../migrations/0013_external_withdrawals.sql"),
            include_str!("../../../migrations/0016_client_account_restrictions.sql"),
        ] {
            connection.execute_batch(migration).map_err(storage)?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}
impl ExternalWithdrawalStore for SqliteExternalWithdrawalStore {
    fn client_wallet(&self, client: &str) -> Result<String, ExternalWithdrawalError> {
        self.connection
            .lock()
            .map_err(storage)?
            .query_row(
                "SELECT wallet_address FROM client_wallets WHERE client_id=?1",
                [client],
                |r| r.get(0),
            )
            .map_err(storage)
    }
    fn begin(
        &self,
        id: &str,
        client: &str,
        destination: Address,
        amount: u64,
        fee: u64,
    ) -> Result<ExternalWithdrawal, ExternalWithdrawalError> {
        let total = amount
            .checked_add(fee)
            .ok_or_else(|| ExternalWithdrawalError::Invalid("amount overflow".into()))?;
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        if let Some(existing) = withdrawal(&tx, id)? {
            if existing.client_id == client
                && existing
                    .destination_address
                    .eq_ignore_ascii_case(&destination.to_checksum(None))
                && existing.amount_raw == amount.to_string()
            {
                return Ok(existing);
            }
            return Err(ExternalWithdrawalError::IdempotencyConflict);
        }
        let timestamp = now();
        let changed=tx.execute("UPDATE client_positions SET available_raw=available_raw-?1,locked_raw=locked_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND available_raw>=?1",params![as_i64(total)?,timestamp as i64,client]).map_err(storage)?;
        if changed == 0 {
            return Err(ExternalWithdrawalError::InsufficientBalance);
        }
        tx.execute("INSERT INTO external_withdrawals VALUES(?1,?2,?3,?4,?5,?6,'pending_chain',NULL,NULL,?7,?7)",params![id,client,destination.to_checksum(None),as_i64(amount)?,as_i64(fee)?,as_i64(total)?,timestamp as i64]).map_err(storage)?;
        ledger(&tx, id, "client", client, "debit", total)?;
        ledger(&tx, id, "client_locked", client, "lock", total)?;
        let result = withdrawal(&tx, id)?.unwrap();
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    fn chain_confirmed(&self, id: &str, hash: &str) -> Result<(), ExternalWithdrawalError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE external_withdrawals SET status='chain_confirmed',transaction_hash=?1,updated_at_unix_ms=?2 WHERE operation_id=?3 AND status='pending_chain'",params![hash,now() as i64,id]).map_err(storage)?;
        Ok(())
    }
    fn mark_submission_uncertain(
        &self,
        id: &str,
        message: &str,
    ) -> Result<(), ExternalWithdrawalError> {
        self.connection
            .lock()
            .map_err(storage)?
            .execute(
                "UPDATE external_withdrawals SET status='submission_uncertain',last_error=?1,updated_at_unix_ms=?2 WHERE operation_id=?3 AND status='pending_chain'",
                params![message, now() as i64, id],
            )
            .map_err(storage)?;
        Ok(())
    }
    fn complete(
        &self,
        id: &str,
        contract: &str,
        chain: u64,
    ) -> Result<ExternalWithdrawal, ExternalWithdrawalError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        let current = withdrawal(&tx, id)?
            .ok_or_else(|| ExternalWithdrawalError::Storage("withdrawal not found".into()))?;
        if current.status == "completed" {
            return Ok(current);
        }
        if current.status != "chain_confirmed" {
            return Err(ExternalWithdrawalError::Storage(
                "withdrawal has no confirmed transaction".into(),
            ));
        }
        let total: u64 = current.total_debit_raw.parse().map_err(storage)?;
        let fee: u64 = current.fee_raw.parse().map_err(storage)?;
        let amount: u64 = current.amount_raw.parse().map_err(storage)?;
        let timestamp = now();
        tx.execute("UPDATE client_positions SET locked_raw=locked_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND locked_raw>=?1",params![as_i64(total)?,timestamp as i64,current.client_id]).map_err(storage)?;
        tx.execute(
            "UPDATE fee_position SET pending_raw=pending_raw+?1 WHERE singleton=1",
            [as_i64(fee)?],
        )
        .map_err(storage)?;
        tx.execute("UPDATE external_withdrawals SET status='completed',updated_at_unix_ms=?1 WHERE operation_id=?2",params![timestamp as i64,id]).map_err(storage)?;
        ledger(&tx, id, "client_locked", &current.client_id, "debit", total)?;
        ledger(&tx, id, "casp_fee_pending", "casp-fees", "credit", fee)?;
        let record_id = Uuid::now_v7().to_string();
        tx.execute("INSERT INTO service_records VALUES(?1,?2,?3,'transfer_service','external_withdrawal','rUSD',?4,?5,?6,'USD',0,0,'completed',?3,?7,?8,'casp-external-withdrawal-v1',?9)",params![record_id,id,current.client_id,contract,chain as i64,as_i64(amount)?,current.destination_address,current.transaction_hash,timestamp as i64]).map_err(storage)?;
        let retention = timestamp.saturating_add(5 * 365 * 24 * 60 * 60 * 1_000);
        tx.execute("INSERT INTO service_record_details VALUES(?1,'new',?2,?2,?2,?2,NULL,'not_applicable',NULL,?3,?4,?5,'demo_web','casp-withdrawal-engine','casp-external-withdrawal-v1',NULL,?6)",params![record_id,timestamp as i64,total as i64,amount as i64,fee as i64,retention as i64]).map_err(storage)?;
        let result = withdrawal(&tx, id)?.unwrap();
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    fn fail(&self, id: &str, message: &str) -> Result<(), ExternalWithdrawalError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        let current = withdrawal(&tx, id)?
            .ok_or_else(|| ExternalWithdrawalError::Storage("withdrawal not found".into()))?;
        if current.status == "pending_chain" {
            let total: u64 = current.total_debit_raw.parse().map_err(storage)?;
            tx.execute("UPDATE client_positions SET available_raw=available_raw+?1,locked_raw=locked_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3",params![as_i64(total)?,now() as i64,current.client_id]).map_err(storage)?;
            ledger(
                &tx,
                id,
                "client_locked",
                &current.client_id,
                "release",
                total,
            )?;
            ledger(&tx, id, "client", &current.client_id, "credit", total)?;
        }
        tx.execute("UPDATE external_withdrawals SET status='failed',last_error=?1,updated_at_unix_ms=?2 WHERE operation_id=?3",params![message,now() as i64,id]).map_err(storage)?;
        tx.commit().map_err(storage)
    }
}
fn withdrawal(
    c: &Connection,
    id: &str,
) -> Result<Option<ExternalWithdrawal>, ExternalWithdrawalError> {
    c.query_row("SELECT operation_id,client_id,destination_address,amount_raw,fee_raw,total_debit_raw,status,transaction_hash,last_error,created_at_unix_ms,updated_at_unix_ms FROM external_withdrawals WHERE operation_id=?1",[id],|r|Ok(ExternalWithdrawal{operation_id:r.get(0)?,client_id:r.get(1)?,destination_address:r.get(2)?,amount_raw:r.get::<_,i64>(3)?.to_string(),fee_raw:r.get::<_,i64>(4)?.to_string(),total_debit_raw:r.get::<_,i64>(5)?.to_string(),status:r.get(6)?,transaction_hash:r.get(7)?,last_error:r.get(8)?,created_at_unix_ms:r.get::<_,i64>(9)? as u64,updated_at_unix_ms:r.get::<_,i64>(10)? as u64})).optional().map_err(storage)
}
fn ledger(
    tx: &rusqlite::Transaction,
    id: &str,
    kind: &str,
    account: &str,
    direction: &str,
    amount: u64,
) -> Result<(), ExternalWithdrawalError> {
    tx.execute(
        "INSERT INTO ledger_entries VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            Uuid::now_v7().to_string(),
            id,
            kind,
            account,
            direction,
            as_i64(amount)?,
            now() as i64
        ],
    )
    .map_err(storage)?;
    Ok(())
}
fn as_i64(v: u64) -> Result<i64, ExternalWithdrawalError> {
    i64::try_from(v)
        .map_err(|_| ExternalWithdrawalError::Invalid("amount exceeds demo range".into()))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn storage(e: impl std::fmt::Display) -> ExternalWithdrawalError {
    ExternalWithdrawalError::Storage(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_debits_amount_and_fee_exactly_once() {
        let store = SqliteExternalWithdrawalStore::open(":memory:").unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE client_positions SET available_raw=2000000 WHERE client_id='alice'",
                [],
            )
            .unwrap();
        store
            .begin(
                "withdraw",
                "alice",
                Address::with_last_byte(7),
                1_000_000,
                10_000,
            )
            .unwrap();
        store.chain_confirmed("withdraw", "0xtx").unwrap();
        let first = store.complete("withdraw", "0xtoken", 31337).unwrap();
        let second = store.complete("withdraw", "0xtoken", 31337).unwrap();
        assert_eq!(first, second);
        let connection = store.connection.lock().unwrap();
        let positions: (i64, i64) = connection
            .query_row(
                "SELECT available_raw,locked_raw FROM client_positions WHERE client_id='alice'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let fees: i64 = connection
            .query_row(
                "SELECT pending_raw FROM fee_position WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let records: i64 = connection
            .query_row(
                "SELECT count(*) FROM service_records WHERE order_type='external_withdrawal'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(positions, (990_000, 0));
        assert_eq!(fees, 10_000);
        assert_eq!(records, 1);
    }

    #[test]
    fn definite_submission_failure_releases_locked_balance() {
        let store = SqliteExternalWithdrawalStore::open(":memory:").unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE client_positions SET available_raw=2000000 WHERE client_id='alice'",
                [],
            )
            .unwrap();
        store
            .begin(
                "failed",
                "alice",
                Address::with_last_byte(7),
                1_000_000,
                10_000,
            )
            .unwrap();
        store.fail("failed", "submission rejected").unwrap();
        let positions: (i64, i64) = store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT available_raw,locked_raw FROM client_positions WHERE client_id='alice'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(positions, (2_000_000, 0));
    }
}
