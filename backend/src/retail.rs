use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientAccount {
    pub client_id: String,
    pub available_raw: String,
    pub locked_raw: String,
    pub inventory_available_raw: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetailOrder {
    pub operation_id: String,
    pub client_id: String,
    pub order_type: String,
    pub quantity_raw: String,
    pub fiat_currency: String,
    pub fiat_amount_minor: String,
    pub status: String,
    pub issuer_operation_id: Option<String>,
    pub blockchain_transaction_hash: Option<String>,
    pub last_error: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRecord {
    pub record_id: String,
    pub operation_id: String,
    pub client_id: String,
    pub service_type: String,
    pub order_type: String,
    pub asset_symbol: String,
    pub contract_address: String,
    pub chain_id: u64,
    pub quantity_raw: String,
    pub fiat_currency: String,
    pub gross_fiat_minor: String,
    pub fee_minor: String,
    pub status: String,
    pub source_account: Option<String>,
    pub destination_account: Option<String>,
    pub blockchain_transaction_hash: Option<String>,
    pub decision_actor: String,
    pub created_at_unix_ms: u64,
}
