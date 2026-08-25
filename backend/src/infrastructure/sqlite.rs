use crate::{
    application::{
        BootstrapError, BootstrapStore, COLD_TARGET_RAW, HOT_TARGET_RAW, PURCHASE_TOKEN_RAW,
        PURCHASE_USD_MINOR,
    },
    domain::{BootstrapOperation, BootstrapStatus},
};
use alloy::primitives::Address;
use rusqlite::{Connection, OptionalExtension, params};
use std::{fs, path::Path, sync::Mutex};
use uuid::Uuid;
pub struct SqliteBootstrapStore {
    connection: Mutex<Connection>,
}
impl SqliteBootstrapStore {
    pub fn open(path: &str) -> Result<Self, BootstrapError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?
        }
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .execute_batch(include_str!("../../migrations/0001_bootstrap.sql"))
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}
impl BootstrapStore for SqliteBootstrapStore {
    fn get(&self) -> Result<Option<BootstrapOperation>, BootstrapError> {
        let connection = self.connection.lock().map_err(storage)?;
        query(&connection)
    }
    fn create(
        &self,
        c: Address,
        h: Address,
        d: Address,
    ) -> Result<BootstrapOperation, BootstrapError> {
        let connection = self.connection.lock().map_err(storage)?;
        if let Some(value) = query(&connection)? {
            return Ok(value);
        }
        let now = unix_ms();
        let id = format!("casp-bootstrap-{}", Uuid::now_v7());
        connection.execute("INSERT INTO inventory_bootstrap VALUES(1,?1,'created',?2,?3,?4,?5,?6,?7,?8,NULL,NULL,NULL,NULL,?9,?9)",params![id,PURCHASE_USD_MINOR as i64,PURCHASE_TOKEN_RAW as i64,c.to_checksum(None),h.to_checksum(None),d.to_checksum(None),HOT_TARGET_RAW as i64,COLD_TARGET_RAW as i64,now as i64]).map_err(storage)?;
        query(&connection)?
            .ok_or_else(|| BootstrapError::Storage("bootstrap insert disappeared".into()))
    }
    fn advance(
        &self,
        status: BootstrapStatus,
        issuer: Option<&str>,
        cold: Option<&str>,
        hot: Option<&str>,
    ) -> Result<BootstrapOperation, BootstrapError> {
        let connection = self.connection.lock().map_err(storage)?;
        connection.execute("UPDATE inventory_bootstrap SET status=?1,issuer_transaction_hash=COALESCE(?2,issuer_transaction_hash),cold_transaction_hash=COALESCE(?3,cold_transaction_hash),hot_transaction_hash=COALESCE(?4,hot_transaction_hash),last_error=NULL,updated_at_unix_ms=?5 WHERE singleton=1",params![status_text(status),issuer,cold,hot,unix_ms() as i64]).map_err(storage)?;
        query(&connection)?.ok_or(BootstrapError::NotStarted)
    }
    fn fail(&self, message: &str) -> Result<(), BootstrapError> {
        self.connection.lock().map_err(storage)?.execute("UPDATE inventory_bootstrap SET status='failed',last_error=?1,updated_at_unix_ms=?2 WHERE singleton=1",params![message,unix_ms() as i64]).map_err(storage)?;
        Ok(())
    }
}
fn query(c: &Connection) -> Result<Option<BootstrapOperation>, BootstrapError> {
    c.query_row("SELECT operation_id,status,amount_usd_minor,token_amount_raw,corporate_address,hot_address,cold_address,hot_target_raw,cold_target_raw,issuer_transaction_hash,cold_transaction_hash,hot_transaction_hash,last_error,created_at_unix_ms,updated_at_unix_ms FROM inventory_bootstrap WHERE singleton=1",[],|r|{let s:String=r.get(1)?;Ok(BootstrapOperation{operation_id:r.get(0)?,status:parse_status(&s),amount_usd_minor:r.get::<_,i64>(2)?.to_string(),token_amount_raw:r.get::<_,i64>(3)?.to_string(),corporate_address:r.get(4)?,hot_address:r.get(5)?,cold_address:r.get(6)?,hot_target_raw:r.get::<_,i64>(7)?.to_string(),cold_target_raw:r.get::<_,i64>(8)?.to_string(),issuer_transaction_hash:r.get(9)?,cold_transaction_hash:r.get(10)?,hot_transaction_hash:r.get(11)?,last_error:r.get(12)?,created_at_unix_ms:r.get::<_,i64>(13)? as u64,updated_at_unix_ms:r.get::<_,i64>(14)? as u64})}).optional().map_err(storage)
}
fn status_text(s: BootstrapStatus) -> &'static str {
    match s {
        BootstrapStatus::Created => "created",
        BootstrapStatus::IssuerOrderCreated => "issuer_order_created",
        BootstrapStatus::FiatSent => "fiat_sent",
        BootstrapStatus::TokensIssued => "tokens_issued",
        BootstrapStatus::Distributed => "distributed",
        BootstrapStatus::Failed => "failed",
    }
}
fn parse_status(s: &str) -> BootstrapStatus {
    match s {
        "created" => BootstrapStatus::Created,
        "issuer_order_created" => BootstrapStatus::IssuerOrderCreated,
        "fiat_sent" => BootstrapStatus::FiatSent,
        "tokens_issued" => BootstrapStatus::TokensIssued,
        "distributed" => BootstrapStatus::Distributed,
        _ => BootstrapStatus::Failed,
    }
}
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn storage(e: impl std::fmt::Display) -> BootstrapError {
    BootstrapError::Storage(e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_one_idempotent_bootstrap_and_progress() {
        let s = SqliteBootstrapStore::open(":memory:").unwrap();
        let a = s
            .create(
                Address::with_last_byte(1),
                Address::with_last_byte(2),
                Address::with_last_byte(3),
            )
            .unwrap();
        let b = s
            .create(
                Address::with_last_byte(9),
                Address::with_last_byte(8),
                Address::with_last_byte(7),
            )
            .unwrap();
        assert_eq!(a.operation_id, b.operation_id);
        assert_eq!(
            s.advance(BootstrapStatus::FiatSent, None, None, None)
                .unwrap()
                .status,
            BootstrapStatus::FiatSent
        );
    }
}
