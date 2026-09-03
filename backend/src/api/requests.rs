use super::validators::{Validate, positive, required};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PurchaseRequest {
    pub operation_id: String,
    pub amount_usd_minor: u64,
}
impl Validate for PurchaseRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.operation_id, "operationId")?;
        positive(self.amount_usd_minor, "amountUsdMinor")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TokenAmountRequest {
    pub operation_id: String,
    pub token_amount_raw: u64,
}
impl Validate for TokenAmountRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.operation_id, "operationId")?;
        positive(self.token_amount_raw, "tokenAmountRaw")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TransferRequest {
    pub operation_id: String,
    pub recipient_client_id: String,
    pub token_amount_raw: u64,
    pub purpose_classification: String,
}
impl Validate for TransferRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.operation_id, "operationId")?;
        required(&self.recipient_client_id, "recipientClientId")?;
        required(&self.purpose_classification, "purposeClassification")?;
        positive(self.token_amount_raw, "tokenAmountRaw")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExternalWithdrawalRequest {
    pub operation_id: String,
    pub destination_address: String,
    pub token_amount_raw: u64,
}
impl Validate for ExternalWithdrawalRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.operation_id, "operationId")?;
        required(&self.destination_address, "destinationAddress")?;
        if !(self.destination_address.starts_with("0x") && self.destination_address.len() == 42) {
            return Err("destinationAddress musi być adresem Ethereum".into());
        }
        positive(self.token_amount_raw, "tokenAmountRaw")
    }
}

#[derive(Deserialize)]
pub(super) struct BlacklistRequest {
    pub address: String,
    pub reason: String,
}
impl Validate for BlacklistRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.address, "address")?;
        required(&self.reason, "reason")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AccountRestrictionRequest {
    pub client_id: String,
    pub reason: String,
}
impl Validate for AccountRestrictionRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.client_id, "clientId")?;
        required(&self.reason, "reason")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FeeSweepRequest {
    pub operation_id: String,
}
impl Validate for FeeSweepRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.operation_id, "operationId")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReplenishmentRequest {
    pub operation_id: String,
    pub amount_usd_minor: u64,
}
impl Validate for ReplenishmentRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.operation_id, "operationId")?;
        positive(self.amount_usd_minor, "amountUsdMinor")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExchangeRateRequest {
    pub usd_minor_per_rusd: u64,
}
impl Validate for ExchangeRateRequest {
    fn validate(&self) -> Result<(), String> {
        positive(self.usd_minor_per_rusd, "usdMinorPerRusd")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AmendmentRequest {
    pub original_record_id: String,
    pub amendment_type: String,
    pub reason: String,
}
impl Validate for AmendmentRequest {
    fn validate(&self) -> Result<(), String> {
        required(&self.original_record_id, "originalRecordId")?;
        required(&self.amendment_type, "amendmentType")?;
        required(&self.reason, "reason")
    }
}
