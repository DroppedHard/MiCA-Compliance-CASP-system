use crate::statements::{StatementError, StatementMovement, StatementSource, StatementStore};
use rusqlite::{Connection, params};
use std::{fs, path::Path, sync::Mutex};

pub struct SqliteStatementStore {
    connection: Mutex<Connection>,
}
impl SqliteStatementStore {
    pub fn open(path: &str) -> Result<Self, StatementError> {
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

impl StatementStore for SqliteStatementStore {
    fn source(
        &self,
        client: &str,
        from: &str,
        to: &str,
    ) -> Result<StatementSource, StatementError> {
        let connection = self.connection.lock().map_err(storage)?;
        let (opening_available_raw, opening_locked_raw) = balances(&connection, client, "<", from)?;
        let (closing_available_raw, closing_locked_raw) = balances(&connection, client, "<=", to)?;
        let mut statement=connection.prepare(
            "SELECT l.operation_id,MIN(l.created_at_unix_ms),
             COALESCE(SUM(CASE WHEN l.account_type='client' AND l.direction='credit' THEN l.quantity_raw WHEN l.account_type='client' AND l.direction='debit' THEN -l.quantity_raw ELSE 0 END),0),
             COALESCE(SUM(CASE WHEN l.account_type='client_locked' AND l.direction='lock' THEN l.quantity_raw WHEN l.account_type='client_locked' AND l.direction='debit' THEN -l.quantity_raw ELSE 0 END),0),
             CASE WHEN t.operation_id IS NOT NULL AND t.sender_client_id=?1 THEN 'internal_transfer_sent' WHEN t.operation_id IS NOT NULL THEN 'internal_transfer_received' ELSE r.order_type END,
             COALESCE(t.status,r.status,'completed'),COALESCE(t.gross_raw,r.quantity_raw,0),
             CASE WHEN t.operation_id IS NOT NULL THEN t.net_raw ELSE COALESCE(r.quantity_raw,0) END,
             CASE WHEN t.sender_client_id=?1 THEN COALESCE(t.fee_raw,0) ELSE 0 END,
             CASE WHEN t.operation_id IS NOT NULL AND t.sender_client_id=?1 THEN t.recipient_client_id WHEN t.operation_id IS NOT NULL THEN t.sender_client_id WHEN r.order_type='purchase' OR r.order_type='sale' THEN 'casp-inventory' WHEN r.order_type='redemption' THEN 'issuer' ELSE NULL END
             FROM ledger_entries l LEFT JOIN retail_orders r ON r.operation_id=l.operation_id LEFT JOIN internal_transfers t ON t.operation_id=l.operation_id
             WHERE l.account_id=?1 AND l.account_type IN('client','client_locked') AND date(l.created_at_unix_ms/1000,'unixepoch') BETWEEN ?2 AND ?3
             GROUP BY l.operation_id ORDER BY MIN(l.created_at_unix_ms),l.operation_id"
        ).map_err(storage)?;
        let movements = statement
            .query_map(params![client, from, to], |row| {
                Ok(StatementMovement {
                    operation_id: row.get(0)?,
                    occurred_at_unix_ms: row.get::<_, i64>(1)? as u64,
                    available_delta_raw: row.get::<_, i64>(2)?.to_string(),
                    locked_delta_raw: row.get::<_, i64>(3)?.to_string(),
                    operation_type: row.get(4)?,
                    status: row.get(5)?,
                    gross_raw: row.get::<_, i64>(6)?.to_string(),
                    net_raw: row.get::<_, i64>(7)?.to_string(),
                    fee_raw: row.get::<_, i64>(8)?.to_string(),
                    counterparty: row.get(9)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(StatementSource {
            opening_available_raw,
            opening_locked_raw,
            closing_available_raw,
            closing_locked_raw,
            movements,
        })
    }
}

fn balances(
    connection: &Connection,
    client: &str,
    operator: &str,
    date: &str,
) -> Result<(i64, i64), StatementError> {
    let sql = format!(
        "SELECT COALESCE(SUM(CASE WHEN account_type='client' AND direction='credit' THEN quantity_raw WHEN account_type='client' AND direction='debit' THEN -quantity_raw ELSE 0 END),0),COALESCE(SUM(CASE WHEN account_type='client_locked' AND direction='lock' THEN quantity_raw WHEN account_type='client_locked' AND direction='debit' THEN -quantity_raw ELSE 0 END),0) FROM ledger_entries WHERE account_id=?1 AND date(created_at_unix_ms/1000,'unixepoch') {operator} ?2"
    );
    connection
        .query_row(&sql, params![client, date], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(storage)
}
fn storage(error: impl std::fmt::Display) -> StatementError {
    StatementError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        infrastructure::SqliteRetailStore,
        retail_application::{RetailStore, TransferPosting},
    };
    #[test]
    fn statement_balances_match_ledger_movements() {
        let path =
            std::env::temp_dir().join(format!("casp-statement-{}.sqlite", uuid::Uuid::now_v7()));
        let text = path.to_string_lossy().to_string();
        let retail = SqliteRetailStore::open(&text).unwrap();
        retail.activate_inventory(20_000_000).unwrap();
        retail.purchase("p", "alice", 1000, "x", 1).unwrap();
        retail
            .transfer(TransferPosting {
                id: "t",
                sender: "alice",
                recipient: "bob",
                gross_raw: 5_000_000,
                purpose: "private_transfer",
                contract: "x",
                chain: 1,
            })
            .unwrap();
        retail
            .begin_redemption("r", "alice", 1_000_000, "x", 1, "hot")
            .unwrap();
        let source = SqliteStatementStore::open(&text)
            .unwrap()
            .source("alice", "1970-01-01", "9999-12-31")
            .unwrap();
        assert_eq!(source.opening_available_raw, 0);
        assert_eq!(source.closing_available_raw, 4_000_000);
        assert_eq!(source.closing_locked_raw, 1_000_000);
        assert_eq!(source.movements.len(), 3);
        assert_eq!(
            source
                .movements
                .iter()
                .map(|m| m.available_delta_raw.parse::<i64>().unwrap())
                .sum::<i64>(),
            source.closing_available_raw - source.opening_available_raw
        );
        let transfer = source
            .movements
            .iter()
            .find(|m| m.operation_id == "t")
            .unwrap();
        assert_eq!(transfer.fee_raw, "5000");
        drop(retail);
        let _ = std::fs::remove_file(path);
    }
}
