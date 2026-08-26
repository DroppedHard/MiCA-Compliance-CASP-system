use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInformation {
    pub symbol: String,
    pub name: String,
    pub reference_currency: String,
    pub parity_statement: String,
    pub contract_address: String,
    pub chain_id: u64,
    pub decimals: u8,
    pub total_supply_raw: String,
    pub asset_state: String,
    pub asset_state_reason: String,
    pub reserve_coverage_percent: Option<f64>,
    pub esg_methodology_version: String,
    pub esg_source_name: String,
    pub esg_source_url: String,
    pub esg_note: String,
    pub white_paper_url: String,
    pub issuer_observed_at_unix_ms: u64,
    pub disclaimer: String,
}

#[async_trait]
pub trait IssuerPublicGateway: Send + Sync {
    async fn information(&self) -> Result<TokenInformation, PublicInfoError>;
}

#[derive(Clone)]
pub struct PublicInfoService {
    gateway: Arc<dyn IssuerPublicGateway>,
}
impl PublicInfoService {
    pub fn new(gateway: Arc<dyn IssuerPublicGateway>) -> Self {
        Self { gateway }
    }
    pub async fn information(&self) -> Result<TokenInformation, PublicInfoError> {
        self.gateway.information().await
    }
}

#[derive(Debug, Error)]
pub enum PublicInfoError {
    #[error("issuer public information is unavailable: {0}")]
    Unavailable(String),
}
