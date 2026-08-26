use crate::{
    retail::{ClientAccount, FeePosition, InternalTransfer, RetailOrder, ServiceRecord},
    retail_application::{RetailError, RetailStore, TransferPosting},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{fs, path::Path, sync::Mutex};
use uuid::Uuid;

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
            .execute_batch(include_str!("../../migrations/0002_retail.sql"))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!("../../migrations/0004_internal_transfers.sql"))
            .map_err(storage)?;
        migrate_sale_order_type(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl RetailStore for SqliteRetailStore {
    fn activate_inventory(&self, amount: u64) -> Result<(), RetailError> {
        let connection = self.connection.lock().map_err(storage)?;
        connection
            .execute(
                "UPDATE inventory_state SET available_raw=?1,activated_at_unix_ms=?2 WHERE singleton=1 AND activated_at_unix_ms IS NULL",
                params![as_i64(amount)?, now() as i64],
            )
            .map_err(storage)?;
        Ok(())
    }

    fn account(&self, client: &str) -> Result<ClientAccount, RetailError> {
        let connection = self.connection.lock().map_err(storage)?;
        connection
            .query_row(
                "SELECT p.available_raw,p.locked_raw,i.available_raw FROM client_positions p CROSS JOIN inventory_state i WHERE p.client_id=?1 AND i.singleton=1",
                [client],
                |row| Ok(ClientAccount {
                    client_id: client.to_owned(),
                    available_raw: row.get::<_, i64>(0)?.to_string(),
                    locked_raw: row.get::<_, i64>(1)?.to_string(),
                    inventory_available_raw: row.get::<_, i64>(2)?.to_string(),
                }),
            )
            .map_err(storage)
    }

    fn accounts(&self) -> Result<Vec<ClientAccount>, RetailError> {
        let connection = self.connection.lock().map_err(storage)?;
        let inventory: i64 = connection
            .query_row(
                "SELECT available_raw FROM inventory_state WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        let mut statement=connection.prepare("SELECT client_id,available_raw,locked_raw FROM client_positions WHERE client_id IN ('alice','bob','carol') ORDER BY client_id").map_err(storage)?;
        statement
            .query_map([], |row| {
                Ok(ClientAccount {
                    client_id: row.get(0)?,
                    available_raw: row.get::<_, i64>(1)?.to_string(),
                    locked_raw: row.get::<_, i64>(2)?.to_string(),
                    inventory_available_raw: inventory.to_string(),
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)
    }

    fn purchase(
        &self,
        id: &str,
        client: &str,
        cents: u64,
        contract: &str,
        chain: u64,
    ) -> Result<RetailOrder, RetailError> {
        let raw = cents
            .checked_mul(UNITS_PER_CENT)
            .ok_or_else(|| RetailError::Invalid("amount is too large".into()))?;
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        if let Some(existing) = order(&tx, id)? {
            verify_same(&existing, "purchase", client, raw, cents)?;
            return Ok(existing);
        }
        let changed = tx.execute("UPDATE inventory_state SET available_raw=available_raw-?1 WHERE singleton=1 AND available_raw>=?1", [as_i64(raw)?]).map_err(storage)?;
        if changed == 0 {
            return Err(RetailError::InsufficientInventory);
        }
        tx.execute("UPDATE client_positions SET available_raw=available_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3", params![as_i64(raw)?, now() as i64, client]).map_err(storage)?;
        insert_order(&tx, id, client, "purchase", raw, cents, "completed", None)?;
        ledger(&tx, id, "inventory", "casp-inventory", "debit", raw)?;
        ledger(&tx, id, "client", client, "credit", raw)?;
        record(
            &tx,
            id,
            client,
            "purchase",
            raw,
            cents,
            "completed",
            Some("casp-inventory"),
            Some(client),
            None,
            contract,
            chain,
        )?;
        let result =
            order(&tx, id)?.ok_or_else(|| RetailError::Storage("purchase disappeared".into()))?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    fn sale(
        &self,
        id: &str,
        client: &str,
        raw: u64,
        contract: &str,
        chain: u64,
    ) -> Result<RetailOrder, RetailError> {
        let cents = raw / UNITS_PER_CENT;
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        if let Some(existing) = order(&tx, id)? {
            verify_same(&existing, "sale", client, raw, cents)?;
            return Ok(existing);
        }
        let changed=tx.execute("UPDATE client_positions SET available_raw=available_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND available_raw>=?1",params![as_i64(raw)?,now() as i64,client]).map_err(storage)?;
        if changed == 0 {
            return Err(RetailError::InsufficientBalance);
        }
        tx.execute(
            "UPDATE inventory_state SET available_raw=available_raw+?1 WHERE singleton=1",
            [as_i64(raw)?],
        )
        .map_err(storage)?;
        insert_order(&tx, id, client, "sale", raw, cents, "completed", None)?;
        ledger(&tx, id, "client", client, "debit", raw)?;
        ledger(&tx, id, "inventory", "casp-inventory", "credit", raw)?;
        record(
            &tx,
            id,
            client,
            "sale",
            raw,
            cents,
            "completed",
            Some(client),
            Some("casp-inventory"),
            None,
            contract,
            chain,
        )?;
        let result =
            order(&tx, id)?.ok_or_else(|| RetailError::Storage("sale disappeared".into()))?;
        tx.commit().map_err(storage)?;
        Ok(result)
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
        let cents = raw / UNITS_PER_CENT;
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        if let Some(existing) = order(&tx, id)? {
            verify_same(&existing, "redemption", client, raw, cents)?;
            return Ok(existing);
        }
        let changed=tx.execute("UPDATE client_positions SET available_raw=available_raw-?1,locked_raw=locked_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND available_raw>=?1",params![as_i64(raw)?,now() as i64,client]).map_err(storage)?;
        if changed == 0 {
            return Err(RetailError::InsufficientBalance);
        }
        let issuer_id = format!("issuer-{id}");
        insert_order(
            &tx,
            id,
            client,
            "redemption",
            raw,
            cents,
            "pending_issuer",
            Some(&issuer_id),
        )?;
        ledger(&tx, id, "client", client, "debit", raw)?;
        ledger(&tx, id, "client_locked", client, "lock", raw)?;
        record(
            &tx,
            id,
            client,
            "redemption",
            raw,
            cents,
            "pending_issuer",
            Some(client),
            Some(hot),
            None,
            contract,
            chain,
        )?;
        let result =
            order(&tx, id)?.ok_or_else(|| RetailError::Storage("redemption disappeared".into()))?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    fn complete_redemption(
        &self,
        id: &str,
        hash: Option<&str>,
    ) -> Result<RetailOrder, RetailError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        let current =
            order(&tx, id)?.ok_or_else(|| RetailError::Storage("redemption not found".into()))?;
        if current.status == "completed" {
            return Ok(current);
        }
        tx.execute("UPDATE client_positions SET locked_raw=locked_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND locked_raw>=?1",params![as_i64(current.quantity_raw.parse().map_err(storage)?)?,now() as i64,current.client_id]).map_err(storage)?;
        tx.execute("UPDATE retail_orders SET status='completed',blockchain_transaction_hash=?1,last_error=NULL,updated_at_unix_ms=?2 WHERE operation_id=?3",params![hash,now() as i64,id]).map_err(storage)?;
        ledger(
            &tx,
            id,
            "client_locked",
            &current.client_id,
            "debit",
            current.quantity_raw.parse().map_err(storage)?,
        )?;
        let (contract, chain): (String, i64) = tx
            .query_row(
                "SELECT contract_address,chain_id FROM service_records WHERE operation_id=?1 ORDER BY created_at_unix_ms LIMIT 1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage)?;
        record(
            &tx,
            id,
            &current.client_id,
            "redemption",
            current.quantity_raw.parse().map_err(storage)?,
            current.fiat_amount_minor.parse().map_err(storage)?,
            "completed",
            Some(&current.client_id),
            Some("issuer"),
            hash,
            &contract,
            chain as u64,
        )?;
        let result = order(&tx, id)?.unwrap();
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    fn fail_redemption(&self, id: &str, message: &str) -> Result<(), RetailError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE retail_orders SET status='issuer_retry_required',last_error=?1,updated_at_unix_ms=?2 WHERE operation_id=?3 AND status<>'completed'",params![message,now() as i64,id]).map_err(storage)?;
        Ok(())
    }

    fn records(&self, client: &str) -> Result<Vec<ServiceRecord>, RetailError> {
        let connection = self.connection.lock().map_err(storage)?;
        let mut statement=connection.prepare("SELECT record_id,operation_id,client_id,service_type,order_type,asset_symbol,contract_address,chain_id,quantity_raw,fiat_currency,gross_fiat_minor,fee_minor,status,source_account,destination_account,blockchain_transaction_hash,decision_actor,created_at_unix_ms FROM service_records WHERE client_id=?1 OR source_account=?1 OR destination_account=?1 ORDER BY created_at_unix_ms DESC,record_id DESC LIMIT 200").map_err(storage)?;
        statement
            .query_map([client], |r| {
                Ok(ServiceRecord {
                    record_id: r.get(0)?,
                    operation_id: r.get(1)?,
                    client_id: r.get(2)?,
                    service_type: r.get(3)?,
                    order_type: r.get(4)?,
                    asset_symbol: r.get(5)?,
                    contract_address: r.get(6)?,
                    chain_id: r.get::<_, i64>(7)? as u64,
                    quantity_raw: r.get::<_, i64>(8)?.to_string(),
                    fiat_currency: r.get(9)?,
                    gross_fiat_minor: r.get::<_, i64>(10)?.to_string(),
                    fee_minor: r.get::<_, i64>(11)?.to_string(),
                    status: r.get(12)?,
                    source_account: r.get(13)?,
                    destination_account: r.get(14)?,
                    blockchain_transaction_hash: r.get(15)?,
                    decision_actor: r.get(16)?,
                    created_at_unix_ms: r.get::<_, i64>(17)? as u64,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)
    }

    fn transfer(&self, command: TransferPosting<'_>) -> Result<InternalTransfer, RetailError> {
        let TransferPosting {
            id,
            sender,
            recipient,
            gross_raw: gross,
            purpose,
            contract,
            chain,
        } = command;
        let fee = gross / 1_000;
        let net = gross
            .checked_sub(fee)
            .ok_or_else(|| RetailError::Invalid("transfer fee exceeds amount".into()))?;
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        if let Some(existing) = internal_transfer(&tx, id)? {
            if existing.sender_client_id == sender
                && existing.recipient_client_id == recipient
                && existing.gross_raw == gross.to_string()
                && existing.purpose_classification == purpose
            {
                return Ok(existing);
            }
            return Err(RetailError::IdempotencyConflict);
        }
        let changed = tx
            .execute(
                "UPDATE client_positions SET available_raw=available_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND available_raw>=?1",
                params![as_i64(gross)?, now() as i64, sender],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(RetailError::InsufficientBalance);
        }
        tx.execute(
            "UPDATE client_positions SET available_raw=available_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3",
            params![as_i64(net)?, now() as i64, recipient],
        )
        .map_err(storage)?;
        tx.execute(
            "UPDATE fee_position SET pending_raw=pending_raw+?1 WHERE singleton=1",
            [as_i64(fee)?],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO internal_transfers(operation_id,sender_client_id,recipient_client_id,gross_raw,fee_raw,net_raw,purpose_classification,status,created_at_unix_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,'completed',?8)",
            params![id, sender, recipient, as_i64(gross)?, as_i64(fee)?, as_i64(net)?, purpose, now() as i64],
        )
        .map_err(storage)?;
        ledger(&tx, id, "client", sender, "debit", gross)?;
        ledger(&tx, id, "client", recipient, "credit", net)?;
        ledger(&tx, id, "casp_fee_pending", "casp-fees", "credit", fee)?;
        record(
            &tx,
            id,
            sender,
            "internal_transfer",
            gross,
            0,
            "completed",
            Some(sender),
            Some(recipient),
            None,
            contract,
            chain,
        )?;
        let result = internal_transfer(&tx, id)?
            .ok_or_else(|| RetailError::Storage("transfer disappeared".into()))?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    fn fee_position(&self) -> Result<FeePosition, RetailError> {
        let connection = self.connection.lock().map_err(storage)?;
        let pending: i64 = connection
            .query_row(
                "SELECT pending_raw FROM fee_position WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        Ok(FeePosition {
            pending_raw: pending.to_string(),
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_order(
    tx: &Transaction,
    id: &str,
    client: &str,
    kind: &str,
    raw: u64,
    cents: u64,
    status: &str,
    issuer: Option<&str>,
) -> Result<(), RetailError> {
    let n = now();
    tx.execute(
        "INSERT INTO retail_orders VALUES(?1,?2,?3,?4,'USD',?5,?6,?7,NULL,NULL,?8,?8)",
        params![
            id,
            client,
            kind,
            as_i64(raw)?,
            as_i64(cents)?,
            status,
            issuer,
            n as i64
        ],
    )
    .map_err(storage)?;
    Ok(())
}
fn ledger(
    tx: &Transaction,
    id: &str,
    account_type: &str,
    account: &str,
    direction: &str,
    raw: u64,
) -> Result<(), RetailError> {
    tx.execute(
        "INSERT INTO ledger_entries VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            Uuid::now_v7().to_string(),
            id,
            account_type,
            account,
            direction,
            as_i64(raw)?,
            now() as i64
        ],
    )
    .map_err(storage)?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn record(
    tx: &Transaction,
    id: &str,
    client: &str,
    kind: &str,
    raw: u64,
    cents: u64,
    status: &str,
    source: Option<&str>,
    destination: Option<&str>,
    hash: Option<&str>,
    contract: &str,
    chain: u64,
) -> Result<(), RetailError> {
    let service_type = if kind == "internal_transfer" {
        "transfer_service"
    } else {
        "exchange_of_crypto_assets_for_funds"
    };
    tx.execute("INSERT INTO service_records(record_id,operation_id,client_id,service_type,order_type,asset_symbol,contract_address,chain_id,quantity_raw,fiat_currency,gross_fiat_minor,fee_minor,status,source_account,destination_account,blockchain_transaction_hash,decision_actor,created_at_unix_ms) VALUES(?1,?2,?3,?4,?5,'rUSD',?6,?7,?8,'USD',?9,0,?10,?11,?12,?13,'casp-retail-demo-v1',?14)",params![Uuid::now_v7().to_string(),id,client,service_type,kind,contract,as_i64(chain)?,as_i64(raw)?,as_i64(cents)?,status,source,destination,hash,now() as i64]).map_err(storage)?;
    Ok(())
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
