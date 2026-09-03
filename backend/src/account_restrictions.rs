use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientAccountRestriction {
    pub client_id: String,
    pub reason: String,
    pub active: bool,
    pub updated_at_unix_ms: u64,
}

pub trait AccountRestrictionReader: Send + Sync {
    fn is_restricted(&self, client_id: &str) -> Result<bool, AccountRestrictionError>;
}

pub struct SqliteAccountRestrictions {
    connection: Mutex<Connection>,
}

impl SqliteAccountRestrictions {
    pub fn open(path: &str) -> Result<Self, AccountRestrictionError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .execute_batch(include_str!("../migrations/0002_retail.sql"))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!(
                "../migrations/0016_client_account_restrictions.sql"
            ))
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn block(
        &self,
        client_id: &str,
        reason: &str,
    ) -> Result<ClientAccountRestriction, AccountRestrictionError> {
        validate_client(client_id)?;
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 500 {
            return Err(AccountRestrictionError::Invalid(
                "uzasadnienie musi mieć od 1 do 500 znaków".into(),
            ));
        }
        let timestamp = now();
        let connection = self.connection.lock().map_err(storage)?;
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM client_positions WHERE client_id=?1)",
                [client_id],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if !exists {
            return Err(AccountRestrictionError::Invalid(
                "nieznany klient demonstracyjny".into(),
            ));
        }
        connection.execute("INSERT INTO client_account_restrictions(client_id,reason,active,updated_at_unix_ms) VALUES(?1,?2,1,?3) ON CONFLICT(client_id) DO UPDATE SET reason=excluded.reason,active=1,updated_at_unix_ms=excluded.updated_at_unix_ms", params![client_id, reason, timestamp as i64]).map_err(storage)?;
        Ok(ClientAccountRestriction {
            client_id: client_id.into(),
            reason: reason.into(),
            active: true,
            updated_at_unix_ms: timestamp,
        })
    }

    pub fn unblock(&self, client_id: &str) -> Result<bool, AccountRestrictionError> {
        validate_client(client_id)?;
        self.connection.lock().map_err(storage)?.execute("UPDATE client_account_restrictions SET active=0,updated_at_unix_ms=?1 WHERE client_id=?2 AND active=1", params![now() as i64, client_id]).map(|count| count > 0).map_err(storage)
    }

    pub fn list(&self) -> Result<Vec<ClientAccountRestriction>, AccountRestrictionError> {
        let connection = self.connection.lock().map_err(storage)?;
        let mut statement = connection.prepare("SELECT client_id,reason,active,updated_at_unix_ms FROM client_account_restrictions WHERE active=1 ORDER BY updated_at_unix_ms DESC").map_err(storage)?;
        statement
            .query_map([], |row| {
                Ok(ClientAccountRestriction {
                    client_id: row.get(0)?,
                    reason: row.get(1)?,
                    active: row.get::<_, i64>(2)? == 1,
                    updated_at_unix_ms: row.get::<_, i64>(3)? as u64,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)
    }
}

impl AccountRestrictionReader for SqliteAccountRestrictions {
    fn is_restricted(&self, client_id: &str) -> Result<bool, AccountRestrictionError> {
        self.connection
            .lock()
            .map_err(storage)?
            .query_row(
                "SELECT active FROM client_account_restrictions WHERE client_id=?1",
                [client_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|value| value == Some(1))
            .map_err(storage)
    }
}

fn validate_client(value: &str) -> Result<(), AccountRestrictionError> {
    if value.trim().is_empty() || value.len() > 128 {
        Err(AccountRestrictionError::Invalid(
            "identyfikator klienta jest nieprawidłowy".into(),
        ))
    } else {
        Ok(())
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn storage(error: impl std::fmt::Display) -> AccountRestrictionError {
    AccountRestrictionError::Storage(error.to_string())
}

#[derive(Debug, Error)]
pub enum AccountRestrictionError {
    #[error("nieprawidłowa blokada konta: {0}")]
    Invalid(String),
    #[error("nie udało się zapisać blokady konta: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_is_persistent_and_unblock_restores_access() {
        let store = SqliteAccountRestrictions::open(":memory:").unwrap();
        store.block("alice", "polecenie organu").unwrap();
        assert!(store.is_restricted("alice").unwrap());
        assert_eq!(store.list().unwrap()[0].reason, "polecenie organu");
        assert!(store.unblock("alice").unwrap());
        assert!(!store.is_restricted("alice").unwrap());
    }
}
