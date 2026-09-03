use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientAccount {
    pub client_id: String,
    pub wallet_address: String,
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
    pub record_status: String,
    pub received_at_unix_ms: u64,
    pub accepted_at_unix_ms: Option<u64>,
    pub executed_at_unix_ms: Option<u64>,
    pub settled_at_unix_ms: Option<u64>,
    pub failed_at_unix_ms: Option<u64>,
    pub price_method: String,
    pub unit_price_minor: Option<String>,
    pub gross_quantity_raw: String,
    pub net_quantity_raw: String,
    pub fee_quantity_raw: String,
    pub instruction_channel: String,
    pub execution_actor: String,
    pub policy_version: String,
    pub rejection_reason: Option<String>,
    pub retention_until_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRecordAmendment {
    pub amendment_id: String,
    pub original_record_id: String,
    pub amendment_type: String,
    pub reason: String,
    pub actor: String,
    pub created_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InternalTransfer {
    pub operation_id: String,
    pub sender_client_id: String,
    pub recipient_client_id: String,
    pub gross_raw: String,
    pub fee_raw: String,
    pub net_raw: String,
    pub purpose_classification: String,
    pub status: String,
    pub created_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeePosition {
    pub pending_raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRate {
    pub usd_minor_per_rusd: u64,
    pub updated_at_unix_ms: u64,
    pub methodology: String,
}
