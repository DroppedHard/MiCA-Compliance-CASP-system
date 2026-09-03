use crate::{
    retail::{
        ClientAccount, ExchangeRate, FeePosition, InternalTransfer, RetailOrder, ServiceRecord,
        ServiceRecordAmendment,
    },
    retail_application::{RetailError, RetailStore, TransferPosting},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{fs, path::Path, sync::Mutex};
use uuid::Uuid;

mod accounts;
mod inventory;
mod pricing;
mod records;
mod redemptions;
mod shared_utils;
mod trades;
mod transfers;
use shared_utils::{insert_order, ledger, record};

const UNITS_PER_CENT: u64 = 10_000;

pub struct SqliteRetailStore {
    connection: Mutex<Connection>,
}

impl SqliteRetailStore {
    pub fn open(path: &str) -> Result<Self, RetailError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let mut connection = Connection::open(path).map_err(storage)?;
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
                "../../../migrations/0005_inventory_replenishments.sql"
            ))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!(
                "../../../migrations/0006_extended_service_records.sql"
            ))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!("../../../migrations/0010_client_wallets.sql"))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!("../../../migrations/0015_exchange_rate.sql"))
            .map_err(storage)?;
        migrate_sale_order_type(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl RetailStore for SqliteRetailStore {
    fn activate_inventory(&self, amount: u64) -> Result<(), RetailError> {
        inventory::activate(&self.connection, amount)
    }

    fn add_inventory_once(
        &self,
        operation: &str,
        wallet: &str,
        amount: u64,
    ) -> Result<(), RetailError> {
        inventory::add_once(&self.connection, operation, wallet, amount)
    }

    fn account(&self, client: &str) -> Result<ClientAccount, RetailError> {
        accounts::account(&self.connection, client)
    }

    fn accounts(&self) -> Result<Vec<ClientAccount>, RetailError> {
        accounts::accounts(&self.connection)
    }

    fn client_id_by_wallet(&self, wallet_address: &str) -> Result<Option<String>, RetailError> {
        accounts::client_id_by_wallet(&self.connection, wallet_address)
    }

    fn purchase(
        &self,
        id: &str,
        client: &str,
        cents: u64,
        contract: &str,
        chain: u64,
    ) -> Result<RetailOrder, RetailError> {
        trades::purchase(&self.connection, id, client, cents, contract, chain)
    }

    fn sale(
        &self,
        id: &str,
        client: &str,
        raw: u64,
        contract: &str,
        chain: u64,
    ) -> Result<RetailOrder, RetailError> {
        trades::sale(&self.connection, id, client, raw, contract, chain)
    }

    fn begin_redemption(
        &self,
        id: &str,
        client: &str,
        raw: u64,
        contract: &str,
        chain: u64,
        hot: &str,
    ) -> Result<RetailOrder, RetailError> {
        redemptions::begin(&self.connection, id, client, raw, contract, chain, hot)
    }

    fn complete_redemption(
        &self,
        id: &str,
        hash: Option<&str>,
    ) -> Result<RetailOrder, RetailError> {
        redemptions::complete(&self.connection, id, hash)
    }

    fn fail_redemption(&self, id: &str, message: &str) -> Result<(), RetailError> {
        redemptions::fail(&self.connection, id, message)
    }

    fn records(&self, client: &str) -> Result<Vec<ServiceRecord>, RetailError> {
        records::for_client(&self.connection, client)
    }

    fn all_records(&self) -> Result<Vec<ServiceRecord>, RetailError> {
        records::all(&self.connection)
    }

    fn amend_record(
        &self,
        original: &str,
        amendment_type: &str,
        reason: &str,
    ) -> Result<ServiceRecordAmendment, RetailError> {
        records::amend(&self.connection, original, amendment_type, reason)
    }

    fn amendments(&self) -> Result<Vec<ServiceRecordAmendment>, RetailError> {
        records::amendments(&self.connection)
    }

    fn transfer(&self, command: TransferPosting<'_>) -> Result<InternalTransfer, RetailError> {
        transfers::post(&self.connection, command)
    }

    fn fee_position(&self) -> Result<FeePosition, RetailError> {
        transfers::fee_position(&self.connection)
    }

    fn exchange_rate(&self) -> Result<ExchangeRate, RetailError> {
        pricing::get(&self.connection)
    }

    fn set_exchange_rate(&self, usd_minor_per_rusd: u64) -> Result<ExchangeRate, RetailError> {
        pricing::set(&self.connection, usd_minor_per_rusd)
    }
}

fn exchange_rate_minor(connection: &Connection) -> Result<u64, RetailError> {
    let value: i64 = connection
        .query_row(
            "SELECT usd_minor_per_rusd FROM casp_exchange_rate WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(storage)?;
    u64::try_from(value).map_err(storage)
}

fn read_exchange_rate(connection: &Connection) -> Result<ExchangeRate, RetailError> {
    connection
        .query_row(
            "SELECT usd_minor_per_rusd,updated_at_unix_ms FROM casp_exchange_rate WHERE singleton=1",
            [],
            |row| {
                Ok(ExchangeRate {
                    usd_minor_per_rusd: row.get::<_, i64>(0)? as u64,
                    updated_at_unix_ms: row.get::<_, i64>(1)? as u64,
                    methodology: "casp-admin-configured-rate-v1".into(),
                })
            },
        )
        .map_err(storage)
}

const RECORD_SELECT: &str = "SELECT s.record_id,s.operation_id,s.client_id,s.service_type,s.order_type,s.asset_symbol,s.contract_address,s.chain_id,s.quantity_raw,s.fiat_currency,s.gross_fiat_minor,s.fee_minor,s.status,s.source_account,s.destination_account,s.blockchain_transaction_hash,s.decision_actor,s.created_at_unix_ms,COALESCE(d.record_status,'new'),COALESCE(d.received_at_unix_ms,s.created_at_unix_ms),d.accepted_at_unix_ms,d.executed_at_unix_ms,d.settled_at_unix_ms,d.failed_at_unix_ms,COALESCE(d.price_method,'legacy_unspecified'),d.unit_price_minor,COALESCE(d.gross_quantity_raw,s.quantity_raw),COALESCE(d.net_quantity_raw,s.quantity_raw),COALESCE(d.fee_quantity_raw,0),COALESCE(d.instruction_channel,'legacy_unspecified'),COALESCE(d.execution_actor,s.decision_actor),COALESCE(d.policy_version,'casp-service-record-v1'),d.rejection_reason,COALESCE(d.retention_until_unix_ms,s.created_at_unix_ms) FROM service_records s LEFT JOIN service_record_details d ON d.record_id=s.record_id";

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServiceRecord> {
    Ok(ServiceRecord {
        record_id: row.get(0)?,
        operation_id: row.get(1)?,
        client_id: row.get(2)?,
        service_type: row.get(3)?,
        order_type: row.get(4)?,
        asset_symbol: row.get(5)?,
        contract_address: row.get(6)?,
        chain_id: row.get::<_, i64>(7)? as u64,
        quantity_raw: row.get::<_, i64>(8)?.to_string(),
        fiat_currency: row.get(9)?,
        gross_fiat_minor: row.get::<_, i64>(10)?.to_string(),
        fee_minor: row.get::<_, i64>(11)?.to_string(),
        status: row.get(12)?,
        source_account: row.get(13)?,
        destination_account: row.get(14)?,
        blockchain_transaction_hash: row.get(15)?,
        decision_actor: row.get(16)?,
        created_at_unix_ms: row.get::<_, i64>(17)? as u64,
        record_status: row.get(18)?,
        received_at_unix_ms: row.get::<_, i64>(19)? as u64,
        accepted_at_unix_ms: row.get::<_, Option<i64>>(20)?.map(|v| v as u64),
        executed_at_unix_ms: row.get::<_, Option<i64>>(21)?.map(|v| v as u64),
        settled_at_unix_ms: row.get::<_, Option<i64>>(22)?.map(|v| v as u64),
        failed_at_unix_ms: row.get::<_, Option<i64>>(23)?.map(|v| v as u64),
        price_method: row.get(24)?,
        unit_price_minor: row.get::<_, Option<i64>>(25)?.map(|v| v.to_string()),
        gross_quantity_raw: row.get::<_, i64>(26)?.to_string(),
        net_quantity_raw: row.get::<_, i64>(27)?.to_string(),
        fee_quantity_raw: row.get::<_, i64>(28)?.to_string(),
        instruction_channel: row.get(29)?,
        execution_actor: row.get(30)?,
        policy_version: row.get(31)?,
        rejection_reason: row.get(32)?,
        retention_until_unix_ms: row.get::<_, i64>(33)? as u64,
    })
}
fn order(connection: &Connection, id: &str) -> Result<Option<RetailOrder>, RetailError> {
    connection.query_row("SELECT operation_id,client_id,order_type,quantity_raw,fiat_currency,fiat_amount_minor,status,issuer_operation_id,blockchain_transaction_hash,last_error,created_at_unix_ms,updated_at_unix_ms FROM retail_orders WHERE operation_id=?1",[id],|r|Ok(RetailOrder{operation_id:r.get(0)?,client_id:r.get(1)?,order_type:r.get(2)?,quantity_raw:r.get::<_,i64>(3)?.to_string(),fiat_currency:r.get(4)?,fiat_amount_minor:r.get::<_,i64>(5)?.to_string(),status:r.get(6)?,issuer_operation_id:r.get(7)?,blockchain_transaction_hash:r.get(8)?,last_error:r.get(9)?,created_at_unix_ms:r.get::<_,i64>(10)? as u64,updated_at_unix_ms:r.get::<_,i64>(11)? as u64})).optional().map_err(storage)
}
fn internal_transfer(
    connection: &Connection,
    id: &str,
) -> Result<Option<InternalTransfer>, RetailError> {
    connection
        .query_row(
            "SELECT operation_id,sender_client_id,recipient_client_id,gross_raw,fee_raw,net_raw,purpose_classification,status,created_at_unix_ms FROM internal_transfers WHERE operation_id=?1",
            [id],
            |row| {
                Ok(InternalTransfer {
                    operation_id: row.get(0)?,
                    sender_client_id: row.get(1)?,
                    recipient_client_id: row.get(2)?,
                    gross_raw: row.get::<_, i64>(3)?.to_string(),
                    fee_raw: row.get::<_, i64>(4)?.to_string(),
                    net_raw: row.get::<_, i64>(5)?.to_string(),
                    purpose_classification: row.get(6)?,
                    status: row.get(7)?,
                    created_at_unix_ms: row.get::<_, i64>(8)? as u64,
                })
            },
        )
        .optional()
        .map_err(storage)
}
fn verify_same(
    o: &RetailOrder,
    kind: &str,
    client: &str,
    raw: u64,
    cents: u64,
) -> Result<(), RetailError> {
    if o.order_type == kind
        && o.client_id == client
        && o.quantity_raw == raw.to_string()
        && o.fiat_amount_minor == cents.to_string()
    {
        Ok(())
    } else {
        Err(RetailError::IdempotencyConflict)
    }
}
fn as_i64(value: u64) -> Result<i64, RetailError> {
    i64::try_from(value)
        .map_err(|_| RetailError::Invalid("amount exceeds demo database range".into()))
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn storage(error: impl std::fmt::Display) -> RetailError {
    RetailError::Storage(error.to_string())
}
fn migrate_sale_order_type(connection: &mut Connection) -> Result<(), RetailError> {
    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='retail_orders'",
            [],
            |row| row.get(0),
        )
        .map_err(storage)?;
    if sql.contains("'sale'") {
        return Ok(());
    }
    let tx = connection.transaction().map_err(storage)?;
    tx.execute_batch("ALTER TABLE retail_orders RENAME TO retail_orders_legacy; CREATE TABLE retail_orders(operation_id TEXT PRIMARY KEY,client_id TEXT NOT NULL,order_type TEXT NOT NULL CHECK(order_type IN('purchase','sale','redemption')),quantity_raw INTEGER NOT NULL CHECK(quantity_raw>0),fiat_currency TEXT NOT NULL,fiat_amount_minor INTEGER NOT NULL CHECK(fiat_amount_minor>0),status TEXT NOT NULL,issuer_operation_id TEXT,blockchain_transaction_hash TEXT,last_error TEXT,created_at_unix_ms INTEGER NOT NULL,updated_at_unix_ms INTEGER NOT NULL); INSERT INTO retail_orders SELECT * FROM retail_orders_legacy; DROP TABLE retail_orders_legacy;").map_err(storage)?;
    tx.commit().map_err(storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    const CLIENT: &str = "alice";
    fn posting<'a>(
        id: &'a str,
        sender: &'a str,
        recipient: &'a str,
        gross_raw: u64,
    ) -> TransferPosting<'a> {
        TransferPosting {
            id,
            sender,
            recipient,
            gross_raw,
            purpose: "private_transfer",
            contract: "x",
            chain: 1,
        }
    }
    #[test]
    fn purchase_is_atomic_and_idempotent() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(10_000_000).unwrap();
        let a = s.purchase("p1", CLIENT, 100, "0x1", 31337).unwrap();
        let b = s.purchase("p1", CLIENT, 100, "0x1", 31337).unwrap();
        assert_eq!(a.operation_id, b.operation_id);
        let account = s.account(CLIENT).unwrap();
        assert_eq!(account.available_raw, "1000000");
        assert_eq!(account.inventory_available_raw, "9000000");
        assert_eq!(s.records(CLIENT).unwrap().len(), 1)
    }
    #[test]
    fn configured_exchange_rate_is_used_by_purchase_sale_and_record() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(10_000_000).unwrap();
        let rate = s.set_exchange_rate(125).unwrap();
        assert_eq!(rate.usd_minor_per_rusd, 125);

        let purchase = s.purchase("rate-p", CLIENT, 250, "0x1", 31337).unwrap();
        assert_eq!(purchase.quantity_raw, "2000000");
        let sale = s.sale("rate-s", CLIENT, 1_000_000, "0x1", 31337).unwrap();
        assert_eq!(sale.fiat_amount_minor, "125");

        let records = s.records(CLIENT).unwrap();
        assert!(records.iter().all(|record| {
            record.price_method == "casp_admin_configured_rate"
                && record.unit_price_minor.as_deref() == Some("125")
        }));
    }
    #[test]
    fn redemption_locks_then_removes_position() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(10_000_000).unwrap();
        s.purchase("p1", CLIENT, 100, "0x1", 31337).unwrap();
        s.begin_redemption("r1", CLIENT, 500_000, "0x1", 31337, "hot")
            .unwrap();
        assert_eq!(s.account(CLIENT).unwrap().locked_raw, "500000");
        s.complete_redemption("r1", Some("0xtx")).unwrap();
        let a = s.account(CLIENT).unwrap();
        assert_eq!(a.available_raw, "500000");
        assert_eq!(a.locked_raw, "0");
        assert_eq!(s.records(CLIENT).unwrap().len(), 3)
    }
    #[test]
    fn sale_changes_only_client_and_unallocated_inventory() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(2_000_000).unwrap();
        s.purchase("p", CLIENT, 100, "x", 1).unwrap();
        s.sale("s", CLIENT, 400_000, "x", 1).unwrap();
        let account = s.account(CLIENT).unwrap();
        assert_eq!(account.available_raw, "600000");
        assert_eq!(account.inventory_available_raw, "1400000");
        assert_eq!(s.account("bob").unwrap().available_raw, "0");
        assert_eq!(s.accounts().unwrap().len(), 3);
        assert_eq!(s.records(CLIENT).unwrap().len(), 2)
    }
    #[test]
    fn rejects_purchase_above_inventory() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(1).unwrap();
        assert!(matches!(
            s.purchase("p", CLIENT, 1, "x", 1),
            Err(RetailError::InsufficientInventory)
        ))
    }

    #[test]
    fn replenishment_posting_increases_inventory_exactly_once() {
        let store = SqliteRetailStore::open(":memory:").unwrap();
        store.activate_inventory(1_000_000).unwrap();
        store
            .add_inventory_once("inventory-1", "cold", 800_000)
            .unwrap();
        store
            .add_inventory_once("inventory-1", "cold", 800_000)
            .unwrap();
        store
            .add_inventory_once("inventory-1", "hot", 200_000)
            .unwrap();
        assert_eq!(
            store.account("alice").unwrap().inventory_available_raw,
            "2000000"
        );
        assert!(matches!(
            store.add_inventory_once("inventory-1", "hot", 200_001),
            Err(RetailError::IdempotencyConflict)
        ));
    }

    #[test]
    fn extended_record_keeps_execution_metadata_and_append_only_amendment() {
        let store = SqliteRetailStore::open(":memory:").unwrap();
        store.activate_inventory(10_000_000).unwrap();
        store
            .purchase("purchase-record", CLIENT, 100, "0xasset", 31337)
            .unwrap();
        let record = store.records(CLIENT).unwrap().remove(0);
        assert_eq!(record.price_method, "casp_admin_configured_rate");
        assert_eq!(record.unit_price_minor.as_deref(), Some("100"));
        assert_eq!(record.gross_quantity_raw, "1000000");
        assert_eq!(record.net_quantity_raw, "1000000");
        assert_eq!(record.instruction_channel, "demo_web");
        assert!(record.settled_at_unix_ms.is_some());

        let amendment = store
            .amend_record(
                &record.record_id,
                "correction",
                "demo classification correction",
            )
            .unwrap();
        assert_eq!(amendment.original_record_id, record.record_id);
        assert_eq!(store.records(CLIENT).unwrap().len(), 1);
        assert_eq!(store.amendments().unwrap(), vec![amendment]);
    }

    #[test]
    fn transfer_is_atomic_idempotent_and_uses_exact_point_one_percent_fee() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(10_000_000).unwrap();
        s.purchase("purchase", CLIENT, 100, "x", 1).unwrap();

        let first = s
            .transfer(posting("transfer", CLIENT, "bob", 1_000_000))
            .unwrap();
        let replay = s
            .transfer(posting("transfer", CLIENT, "bob", 1_000_000))
            .unwrap();

        assert_eq!(first, replay);
        assert_eq!(first.fee_raw, "1000");
        assert_eq!(first.net_raw, "999000");
        assert_eq!(s.account(CLIENT).unwrap().available_raw, "0");
        assert_eq!(s.account("bob").unwrap().available_raw, "999000");
        assert_eq!(s.fee_position().unwrap().pending_raw, "1000");
        assert_eq!(s.records(CLIENT).unwrap().len(), 2);
        assert_eq!(s.records("bob").unwrap().len(), 1);
    }

    #[test]
    fn failed_transfer_does_not_write_partial_balances_or_fee() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(10_000_000).unwrap();
        assert!(matches!(
            s.transfer(posting("transfer", CLIENT, "bob", 1_000_000)),
            Err(RetailError::InsufficientBalance)
        ));
        assert_eq!(s.account(CLIENT).unwrap().available_raw, "0");
        assert_eq!(s.account("bob").unwrap().available_raw, "0");
        assert_eq!(s.fee_position().unwrap().pending_raw, "0");
        assert!(s.records(CLIENT).unwrap().is_empty());
    }

    #[test]
    fn transfer_id_cannot_be_reused_with_different_parameters() {
        let s = SqliteRetailStore::open(":memory:").unwrap();
        s.activate_inventory(10_000_000).unwrap();
        s.purchase("purchase", CLIENT, 100, "x", 1).unwrap();
        s.transfer(posting("transfer", CLIENT, "bob", 500_000))
            .unwrap();
        assert!(matches!(
            s.transfer(posting("transfer", CLIENT, "carol", 500_000)),
            Err(RetailError::IdempotencyConflict)
        ));
    }
}
