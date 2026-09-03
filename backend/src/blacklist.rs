use rusqlite::{Connection, params};
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
pub struct BlacklistEntry {
    pub address: String,
    pub reason: String,
    pub created_at_unix_ms: u64,
}

pub trait AddressBlacklist: Send + Sync {
    fn is_blocked(&self, address: &str) -> Result<bool, BlacklistError>;
}

pub struct SqliteAddressBlacklist {
    connection: Mutex<Connection>,
}

impl SqliteAddressBlacklist {
    pub fn open(path: &str) -> Result<Self, BlacklistError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .execute_batch(include_str!("../migrations/0012_address_blacklist.sql"))
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn add(&self, address: &str, reason: &str) -> Result<BlacklistEntry, BlacklistError> {
        let normalized = normalize(address)?;
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 500 {
            return Err(BlacklistError::Invalid(
                "reason must contain 1-500 characters".into(),
            ));
        }
        let created = now();
        self.connection.lock().map_err(storage)?.execute(
            "INSERT INTO address_blacklist(normalized_address,original_address,reason,created_at_unix_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(normalized_address) DO UPDATE SET original_address=excluded.original_address,reason=excluded.reason",
            params![normalized, address.trim(), reason, created as i64],
        ).map_err(storage)?;
        Ok(BlacklistEntry {
            address: address.trim().into(),
            reason: reason.into(),
            created_at_unix_ms: created,
        })
    }

    pub fn remove(&self, address: &str) -> Result<bool, BlacklistError> {
        let normalized = normalize(address)?;
        self.connection
            .lock()
            .map_err(storage)?
            .execute(
                "DELETE FROM address_blacklist WHERE normalized_address=?1",
                [normalized],
            )
            .map(|count| count > 0)
            .map_err(storage)
    }

    pub fn list(&self) -> Result<Vec<BlacklistEntry>, BlacklistError> {
        let connection = self.connection.lock().map_err(storage)?;
        let mut statement = connection.prepare("SELECT original_address,reason,created_at_unix_ms FROM address_blacklist ORDER BY created_at_unix_ms DESC").map_err(storage)?;
        statement
            .query_map([], |row| {
                Ok(BlacklistEntry {
                    address: row.get(0)?,
                    reason: row.get(1)?,
                    created_at_unix_ms: row.get::<_, i64>(2)? as u64,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)
    }
}

impl AddressBlacklist for SqliteAddressBlacklist {
    fn is_blocked(&self, address: &str) -> Result<bool, BlacklistError> {
        let normalized = normalize(address)?;
        self.connection
            .lock()
            .map_err(storage)?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM address_blacklist WHERE normalized_address=?1)",
                [normalized],
                |row| row.get(0),
            )
            .map_err(storage)
    }
}

fn normalize(address: &str) -> Result<String, BlacklistError> {
    let value = address.trim();
    if value.is_empty() || value.len() > 256 {
        Err(BlacklistError::Invalid(
            "address must contain 1-256 characters".into(),
        ))
    } else {
        Ok(value.to_ascii_lowercase())
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn storage(error: impl std::fmt::Display) -> BlacklistError {
    BlacklistError::Storage(error.to_string())
}

#[derive(Debug, Error)]
pub enum BlacklistError {
    #[error("invalid blacklist entry: {0}")]
    Invalid(String),
    #[error("blacklist persistence failed: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_is_case_insensitive_and_removal_restores_access() {
        let list = SqliteAddressBlacklist::open(":memory:").unwrap();
        list.add("0xAbC", "test").unwrap();
        assert!(list.is_blocked("0xaBc").unwrap());
        assert!(list.remove("0xABC").unwrap());
        assert!(!list.is_blocked("0xabc").unwrap());
    }
}
