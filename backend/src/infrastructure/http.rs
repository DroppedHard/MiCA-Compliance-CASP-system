use crate::application::{BankGateway, BootstrapError, IssuerGateway, IssuerOrder};
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
