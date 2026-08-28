use crate::external_deposits::{ExternalDepositError, ExternalDepositEvent, ExternalDepositStore};
use rusqlite::{Connection, params};
use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub struct SqliteExternalDepositStore {
    connection: Mutex<Connection>,
}
impl SqliteExternalDepositStore {
    pub fn open(path: &str) -> Result<Self, ExternalDepositError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let connection = Connection::open(path).map_err(storage)?;
        for migration in [
            include_str!("../../migrations/0002_retail.sql"),
            include_str!("../../migrations/0006_extended_service_records.sql"),
            include_str!("../../migrations/0010_client_wallets.sql"),
            include_str!("../../migrations/0011_external_deposits.sql"),
        ] {
            connection.execute_batch(migration).map_err(storage)?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}
impl ExternalDepositStore for SqliteExternalDepositStore {
    fn checkpoint(&self) -> Result<u64, ExternalDepositError> {
        self.connection
            .lock()
            .map_err(storage)?
            .query_row(
                "SELECT last_confirmed_block FROM external_deposit_checkpoint WHERE singleton=1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|v| v as u64)
            .map_err(storage)
    }
    fn apply(
        &self,
        chain_id: u64,
        event: &ExternalDepositEvent,
        client_id: Option<&str>,
    ) -> Result<(), ExternalDepositError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        let inserted=tx.execute("INSERT OR IGNORE INTO external_deposits(chain_id,transaction_hash,log_index,block_number,sender_address,client_reference,client_id,amount_raw,status,credited_at_unix_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![chain_id as i64,event.transaction_hash,event.log_index as i64,event.block_number as i64,event.sender.to_checksum(None),event.client_reference.to_string(),client_id,event.amount_raw as i64,if client_id.is_some(){"credited"}else{"unknown_reference"},client_id.map(|_|now() as i64)]).map_err(storage)?;
        if inserted == 1
            && let Some(client) = client_id
        {
            tx.execute("UPDATE client_positions SET available_raw=available_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3",params![event.amount_raw as i64,now() as i64,client]).map_err(storage)?;
            let operation_id = format!("external:{}:{}", event.transaction_hash, event.log_index);
            tx.execute("INSERT INTO ledger_entries(entry_id,operation_id,account_type,account_id,direction,quantity_raw,created_at_unix_ms) VALUES(?1,?2,'client',?3,'credit',?4,?5)",params![Uuid::now_v7().to_string(),operation_id,client,event.amount_raw as i64,now() as i64]).map_err(storage)?;
            tx.execute("INSERT INTO service_records(record_id,operation_id,client_id,service_type,order_type,asset_symbol,contract_address,chain_id,quantity_raw,fiat_currency,gross_fiat_minor,fee_minor,status,source_account,destination_account,blockchain_transaction_hash,decision_actor,created_at_unix_ms) VALUES(?1,?2,?3,'transfer_service','external_deposit','rUSD','deposit-router',?4,?5,'USD',0,0,'completed',?6,?3,?7,'casp-chain-observer',?8)",params![Uuid::now_v7().to_string(),operation_id,client,chain_id as i64,event.amount_raw as i64,event.sender.to_checksum(None),event.transaction_hash,now() as i64]).map_err(storage)?;
        }
        tx.commit().map_err(storage)
    }
    fn advance(&self, block: u64) -> Result<(), ExternalDepositError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE external_deposit_checkpoint SET last_confirmed_block=max(last_confirmed_block,?1) WHERE singleton=1",[block as i64]).map_err(storage)?;
        Ok(())
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn storage(error: impl std::fmt::Display) -> ExternalDepositError {
    ExternalDepositError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{Address, keccak256};

    fn event(hash: &str, reference: &str) -> ExternalDepositEvent {
        ExternalDepositEvent {
            transaction_hash: hash.into(),
            log_index: 0,
            block_number: 10,
            sender: Address::with_last_byte(9),
            client_reference: keccak256(reference.as_bytes()),
            amount_raw: 25_000_000,
        }
    }

    #[test]
    fn credits_a_known_deposit_exactly_once_and_keeps_unknown_references_uncredited() {
        let store = SqliteExternalDepositStore::open(":memory:").unwrap();
        let known = event("0xknown", "rusd:casp:alice");
        store.apply(31337, &known, Some("alice")).unwrap();
        store.apply(31337, &known, Some("alice")).unwrap();
        let unknown = event("0xunknown", "unknown");
        store.apply(31337, &unknown, None).unwrap();
        let connection = store.connection.lock().unwrap();
        let alice: i64 = connection
            .query_row(
                "SELECT available_raw FROM client_positions WHERE client_id='alice'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let records: i64 = connection
            .query_row("SELECT count(*) FROM service_records", [], |row| row.get(0))
            .unwrap();
        let unknown_status: String = connection
            .query_row(
                "SELECT status FROM external_deposits WHERE transaction_hash='0xunknown'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(alice, 25_000_000);
        assert_eq!(records, 1);
        assert_eq!(unknown_status, "unknown_reference");
    }
}
