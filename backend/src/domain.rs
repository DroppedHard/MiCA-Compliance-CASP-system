use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStatus {
    Created,
    IssuerOrderCreated,
    FiatSent,
    TokensIssued,
    Distributed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapOperation {
    pub operation_id: String,
    pub status: BootstrapStatus,
    pub amount_usd_minor: String,
    pub token_amount_raw: String,
    pub corporate_address: String,
    pub hot_address: String,
    pub cold_address: String,
    pub hot_target_raw: String,
    pub cold_target_raw: String,
    pub issuer_transaction_hash: Option<String>,
    pub cold_transaction_hash: Option<String>,
    pub hot_transaction_hash: Option<String>,
    pub last_error: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletBalances {
    pub corporate_raw: String,
    pub hot_raw: String,
    pub cold_raw: String,
}
