use crate::application::{BankGateway, BootstrapError, IssuerGateway, IssuerOrder};
use crate::public_info::{IssuerPublicGateway, PublicInfoError, TokenInformation};
use crate::retail_application::{IssuerRedemption, RetailError, RetailIssuerGateway};
use alloy::primitives::Address;
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

pub struct HttpIssuerGateway {
    client: reqwest::Client,
    base: String,
}
impl HttpIssuerGateway {
    pub fn new(base: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_owned(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateOrder<'a> {
    operation_id: &'a str,
    recipient_address: String,
    amount_usd_minor: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderResponse {
    transaction_hash: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRedemption<'a> {
    operation_id: &'a str,
    holder_address: String,
    token_amount_raw: String,
}
#[async_trait]
impl IssuerGateway for HttpIssuerGateway {
    async fn create_order(
        &self,
        id: &str,
        recipient: Address,
        amount: u64,
    ) -> Result<(), BootstrapError> {
        self.client
            .post(format!("{}/api/v1/issuance-orders", self.base))
            .json(&CreateOrder {
                operation_id: id,
                recipient_address: recipient.to_checksum(None),
                amount_usd_minor: amount.to_string(),
            })
            .send()
            .await
            .map_err(issuer)?
            .error_for_status()
            .map_err(issuer)?;
        Ok(())
    }
    async fn settle_order(&self, id: &str) -> Result<IssuerOrder, BootstrapError> {
        let value = self
            .client
            .post(format!("{}/api/v1/issuance-orders/{id}/settle", self.base))
            .send()
            .await
            .map_err(issuer)?
            .error_for_status()
            .map_err(issuer)?
            .json::<OrderResponse>()
            .await
            .map_err(issuer)?;
        Ok(IssuerOrder {
            transaction_hash: value.transaction_hash,
        })
    }
}

#[async_trait]
impl RetailIssuerGateway for HttpIssuerGateway {
    async fn create_redemption(
        &self,
        id: &str,
        holder: Address,
        amount: u64,
    ) -> Result<(), RetailError> {
        self.client
            .post(format!("{}/api/v1/redemption-orders", self.base))
            .json(&CreateRedemption {
                operation_id: id,
                holder_address: holder.to_checksum(None),
                token_amount_raw: amount.to_string(),
            })
            .send()
            .await
            .map_err(retail_issuer)?
            .error_for_status()
            .map_err(retail_issuer)?;
        Ok(())
    }
    async fn settle_redemption(&self, id: &str) -> Result<IssuerRedemption, RetailError> {
        let value = self
            .client
            .post(format!(
                "{}/api/v1/redemption-orders/{id}/settle",
                self.base
            ))
            .send()
            .await
            .map_err(retail_issuer)?
            .error_for_status()
            .map_err(retail_issuer)?
            .json::<OrderResponse>()
            .await
            .map_err(retail_issuer)?;
        Ok(IssuerRedemption {
            transaction_hash: value.transaction_hash,
        })
    }
}

pub struct HttpBankGateway {
    client: reqwest::Client,
    base: String,
}
impl HttpBankGateway {
    pub fn new(base: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_owned(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Deposit<'a> {
    amount_minor: String,
    reference: &'a str,
    idempotency_key: String,
}
#[async_trait]
impl BankGateway for HttpBankGateway {
    async fn send_usd(&self, id: &str, amount: u64) -> Result<(), BootstrapError> {
        let response = self
            .client
            .post(format!(
                "{}/api/v1/reserve-accounts/reserve-rusd/deposits",
                self.base
            ))
            .json(&Deposit {
                amount_minor: amount.to_string(),
                reference: id,
                idempotency_key: format!("issuance-{id}"),
            })
            .send()
            .await
            .map_err(bank)?;
        if response.status() == StatusCode::CONFLICT {
            return Err(BootstrapError::Bank(
                "mockBank rejected the idempotency key or deposit".to_owned(),
            ));
        }
        response.error_for_status().map_err(bank)?;
        Ok(())
    }
}
fn issuer(e: impl std::fmt::Display) -> BootstrapError {
    BootstrapError::Issuer(e.to_string())
}
fn bank(e: impl std::fmt::Display) -> BootstrapError {
    BootstrapError::Bank(e.to_string())
}
fn retail_issuer(e: impl std::fmt::Display) -> RetailError {
    RetailError::Issuer(e.to_string())
}

pub struct HttpIssuerPublicGateway {
    client: reqwest::Client,
    api_base: String,
    public_base: String,
}
impl HttpIssuerPublicGateway {
    pub fn new(api_base: &str, public_base: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base: api_base.trim_end_matches('/').to_owned(),
            public_base: public_base.trim_end_matches('/').to_owned(),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicTokenObservation {
    observed_at_unix_ms: u64,
    snapshot: PublicTokenSnapshot,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicTokenSnapshot {
    chain_id: u64,
    contract_address: String,
    name: String,
    symbol: String,
    decimals: u8,
    total_supply_raw: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicAssetState {
    state: String,
    reason: String,
    reserve_coverage_percent: Option<f64>,
    updated_at_unix_ms: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicEsgObservation {
    observed_at_unix_ms: u64,
    methodology: PublicEsgMethodology,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicEsgMethodology {
    version: String,
    source_name: String,
    source_url: String,
    note: String,
}

#[async_trait]
impl IssuerPublicGateway for HttpIssuerPublicGateway {
    async fn information(&self) -> Result<TokenInformation, PublicInfoError> {
        let token = self
            .client
            .get(format!("{}/api/v1/token", self.api_base))
            .send();
        let state = self
            .client
            .get(format!("{}/api/v1/asset-state", self.api_base))
            .send();
        let esg = self
            .client
            .get(format!("{}/api/v1/esg", self.api_base))
            .send();
        let (token, state, esg) = tokio::join!(token, state, esg);
        let token = decode::<PublicTokenObservation>(token).await?;
        let state = decode::<PublicAssetState>(state).await?;
        let esg = decode::<PublicEsgObservation>(esg).await?;
        Ok(compose_information(token, state, esg, &self.public_base))
    }
}
fn compose_information(
    token: PublicTokenObservation,
    state: PublicAssetState,
    esg: PublicEsgObservation,
    public_base: &str,
) -> TokenInformation {
    TokenInformation {
        symbol: token.snapshot.symbol,
        name: token.snapshot.name,
        reference_currency: "USD".into(),
        parity_statement: "Demonstration assumption: 1 rUSD = 1 USD".into(),
        contract_address: token.snapshot.contract_address,
        chain_id: token.snapshot.chain_id,
        decimals: token.snapshot.decimals,
        total_supply_raw: token.snapshot.total_supply_raw,
        asset_state: state.state,
        asset_state_reason: state.reason,
        reserve_coverage_percent: state.reserve_coverage_percent,
        esg_methodology_version: esg.methodology.version,
        esg_source_name: esg.methodology.source_name,
        esg_source_url: esg.methodology.source_url,
        esg_note: esg.methodology.note,
        white_paper_url: format!("{public_base}/white-paper"),
        issuer_observed_at_unix_ms: token
            .observed_at_unix_ms
            .max(state.updated_at_unix_ms)
            .max(esg.observed_at_unix_ms),
        disclaimer:
            "Academic local demo; not an offer, regulated service, or proof of MiCA compliance."
                .into(),
    }
}
async fn decode<T: for<'de> Deserialize<'de>>(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<T, PublicInfoError> {
    response
        .map_err(public_info)?
        .error_for_status()
        .map_err(public_info)?
        .json::<T>()
        .await
        .map_err(public_info)
}
fn public_info(error: impl std::fmt::Display) -> PublicInfoError {
    PublicInfoError::Unavailable(error.to_string())
}

#[cfg(test)]
mod public_info_tests {
    use super::*;

    #[test]
    fn combines_only_issuer_values_and_the_configured_public_link() {
        let info = compose_information(
            PublicTokenObservation {
                observed_at_unix_ms: 10,
                snapshot: PublicTokenSnapshot {
                    chain_id: 31337,
                    contract_address: "0xasset".into(),
                    name: "Research USD EMT".into(),
                    symbol: "rUSD".into(),
                    decimals: 6,
                    total_supply_raw: "12000000".into(),
                },
            },
            PublicAssetState {
                state: "warning".into(),
                reason: "reserve margin".into(),
                reserve_coverage_percent: Some(104.0),
                updated_at_unix_ms: 12,
            },
            PublicEsgObservation {
                observed_at_unix_ms: 11,
                methodology: PublicEsgMethodology {
                    version: "esg-v1".into(),
                    source_name: "Cambridge".into(),
                    source_url: "https://example.test".into(),
                    note: "estimate".into(),
                },
            },
            "http://issuer-ui",
        );
        assert_eq!(info.total_supply_raw, "12000000");
        assert_eq!(info.asset_state, "warning");
        assert_eq!(info.reserve_coverage_percent, Some(104.0));
        assert_eq!(info.issuer_observed_at_unix_ms, 12);
        assert_eq!(info.white_paper_url, "http://issuer-ui/white-paper");
    }
}
