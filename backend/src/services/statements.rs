use crate::reporting::validate_date;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementSource {
    pub opening_available_raw: i64,
    pub opening_locked_raw: i64,
    pub closing_available_raw: i64,
    pub closing_locked_raw: i64,
    pub movements: Vec<StatementMovement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatementMovement {
    pub operation_id: String,
    pub occurred_at_unix_ms: u64,
    pub operation_type: String,
    pub status: String,
    pub available_delta_raw: String,
    pub locked_delta_raw: String,
    pub gross_raw: String,
    pub net_raw: String,
    pub fee_raw: String,
    pub counterparty: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatement {
    pub statement_version: String,
    pub client_id: String,
    pub asset_symbol: String,
    pub from_date_utc: String,
    pub to_date_utc: String,
    pub opening_available_raw: String,
    pub opening_locked_raw: String,
    pub closing_available_raw: String,
    pub closing_locked_raw: String,
    pub total_purchases_raw: String,
    pub total_sales_raw: String,
    pub total_transfers_sent_raw: String,
    pub total_transfers_received_raw: String,
    pub total_fees_raw: String,
    pub total_redemptions_raw: String,
    pub movements: Vec<StatementMovement>,
    pub generated_at_unix_ms: u64,
    pub disclaimer: String,
}

pub trait StatementStore: Send + Sync {
    fn source(&self, client: &str, from: &str, to: &str)
    -> Result<StatementSource, StatementError>;
}
#[derive(Clone)]
pub struct StatementService {
    store: Arc<dyn StatementStore>,
}
impl StatementService {
    pub fn new(store: Arc<dyn StatementStore>) -> Self {
        Self { store }
    }
    pub fn generate(
        &self,
        client: &str,
        from: &str,
        to: &str,
    ) -> Result<ClientStatement, StatementError> {
        if !matches!(client, "alice" | "bob" | "carol") {
            return Err(StatementError::InvalidClient);
        }
        validate_date(from).map_err(|_| StatementError::InvalidRange)?;
        validate_date(to).map_err(|_| StatementError::InvalidRange)?;
        if from > to {
            return Err(StatementError::InvalidRange);
        }
        let source = self.store.source(client, from, to)?;
        let mut purchase = 0i64;
        let mut sale = 0i64;
        let mut sent = 0i64;
        let mut received = 0i64;
        let mut fees = 0i64;
        let mut redemptions = 0i64;
        for movement in &source.movements {
            let gross = parse(&movement.gross_raw)?;
            let fee = parse(&movement.fee_raw)?;
            match movement.operation_type.as_str() {
                "purchase" => purchase = add(purchase, gross)?,
                "sale" => sale = add(sale, gross)?,
                "internal_transfer_sent" => {
                    sent = add(sent, gross)?;
                    fees = add(fees, fee)?
                }
                "internal_transfer_received" => {
                    received = add(received, parse(&movement.net_raw)?)?
                }
                "redemption" if parse(&movement.available_delta_raw)? < 0 => {
                    redemptions = add(redemptions, gross)?
                }
                _ => {}
            }
        }
        Ok(ClientStatement{statement_version:"casp-client-statement-v1".into(),client_id:client.into(),asset_symbol:"rUSD".into(),from_date_utc:from.into(),to_date_utc:to.into(),opening_available_raw:source.opening_available_raw.to_string(),opening_locked_raw:source.opening_locked_raw.to_string(),closing_available_raw:source.closing_available_raw.to_string(),closing_locked_raw:source.closing_locked_raw.to_string(),total_purchases_raw:purchase.to_string(),total_sales_raw:sale.to_string(),total_transfers_sent_raw:sent.to_string(),total_transfers_received_raw:received.to_string(),total_fees_raw:fees.to_string(),total_redemptions_raw:redemptions.to_string(),movements:source.movements,generated_at_unix_ms:now(),disclaimer:"Research demo statement generated from the CASP ledger; not a regulated account statement.".into()})
    }
}
fn parse(value: &str) -> Result<i64, StatementError> {
    value
        .parse()
        .map_err(|_| StatementError::Storage("invalid ledger amount".into()))
}
fn add(left: i64, right: i64) -> Result<i64, StatementError> {
    left.checked_add(right).ok_or(StatementError::Overflow)
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Debug, Error)]
pub enum StatementError {
    #[error("unknown demo client")]
    InvalidClient,
    #[error("from/to must form an ordered YYYY-MM-DD UTC date range")]
    InvalidRange,
    #[error("statement arithmetic overflow")]
    Overflow,
    #[error("statement persistence failed: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Store;
    impl StatementStore for Store {
        fn source(&self, _: &str, _: &str, _: &str) -> Result<StatementSource, StatementError> {
            Ok(StatementSource {
                opening_available_raw: 1_000_000,
                opening_locked_raw: 0,
                closing_available_raw: 1_499_000,
                closing_locked_raw: 0,
                movements: vec![
                    StatementMovement {
                        operation_id: "purchase".into(),
                        occurred_at_unix_ms: 1,
                        operation_type: "purchase".into(),
                        status: "completed".into(),
                        available_delta_raw: "1000000".into(),
                        locked_delta_raw: "0".into(),
                        gross_raw: "1000000".into(),
                        net_raw: "1000000".into(),
                        fee_raw: "0".into(),
                        counterparty: Some("casp-inventory".into()),
                    },
                    StatementMovement {
                        operation_id: "transfer".into(),
                        occurred_at_unix_ms: 2,
                        operation_type: "internal_transfer_sent".into(),
                        status: "completed".into(),
                        available_delta_raw: "-501000".into(),
                        locked_delta_raw: "0".into(),
                        gross_raw: "501000".into(),
                        net_raw: "500499".into(),
                        fee_raw: "501".into(),
                        counterparty: Some("bob".into()),
                    },
                ],
            })
        }
    }
    #[test]
    fn aggregates_deterministically() {
        let report = StatementService::new(Arc::new(Store))
            .generate("alice", "2026-08-01", "2026-08-31")
            .unwrap();
        assert_eq!(report.total_purchases_raw, "1000000");
        assert_eq!(report.total_transfers_sent_raw, "501000");
        assert_eq!(report.total_fees_raw, "501");
        assert_eq!(report.closing_available_raw, "1499000");
    }
}
