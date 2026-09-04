use crate::{
    account_restrictions::AccountRestrictionReader, blacklist::AddressBlacklist,
    reconciliation::ReconciliationService,
};
use alloy::primitives::Address;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

const UNITS_PER_CENT: u64 = 10_000;
const EXTERNAL_FEE_DIVISOR: u64 = 100; // Demonstration policy: 1% in rUSD.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalWithdrawal {
    pub operation_id: String,
    pub client_id: String,
    pub destination_address: String,
    pub amount_raw: String,
    pub fee_raw: String,
    pub total_debit_raw: String,
    pub status: String,
    pub transaction_hash: Option<String>,
    pub last_error: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

pub trait ExternalWithdrawalStore: Send + Sync {
    fn client_wallet(&self, client: &str) -> Result<String, ExternalWithdrawalError>;
    fn begin(
        &self,
        operation: &str,
        client: &str,
        destination: Address,
        amount: u64,
        fee: u64,
    ) -> Result<ExternalWithdrawal, ExternalWithdrawalError>;
    fn chain_confirmed(&self, operation: &str, hash: &str) -> Result<(), ExternalWithdrawalError>;
    fn mark_submission_uncertain(
        &self,
        operation: &str,
        message: &str,
    ) -> Result<(), ExternalWithdrawalError>;
    fn complete(
        &self,
        operation: &str,
        contract: &str,
        chain: u64,
    ) -> Result<ExternalWithdrawal, ExternalWithdrawalError>;
    fn fail(&self, operation: &str, message: &str) -> Result<(), ExternalWithdrawalError>;
}

#[async_trait]
pub trait ExternalWithdrawalGateway: Send + Sync {
    async fn transfer(
        &self,
        destination: Address,
        amount_raw: u64,
    ) -> Result<String, ExternalWithdrawalError>;
}

#[derive(Clone)]
pub struct ExternalWithdrawalService {
    store: Arc<dyn ExternalWithdrawalStore>,
    gateway: Arc<dyn ExternalWithdrawalGateway>,
    blacklist: Arc<dyn AddressBlacklist>,
    account_restrictions: Arc<dyn AccountRestrictionReader>,
    reconciliation: Arc<ReconciliationService>,
    contract: String,
    chain: u64,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl ExternalWithdrawalService {
    pub fn new(
        store: Arc<dyn ExternalWithdrawalStore>,
        gateway: Arc<dyn ExternalWithdrawalGateway>,
        blacklist: Arc<dyn AddressBlacklist>,
        account_restrictions: Arc<dyn AccountRestrictionReader>,
        reconciliation: Arc<ReconciliationService>,
        contract: String,
        chain: u64,
    ) -> Self {
        Self {
            store,
            gateway,
            blacklist,
            account_restrictions,
            reconciliation,
            contract,
            chain,
            lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub async fn execute(
        &self,
        client: &str,
        operation: &str,
        destination: &str,
        amount: u64,
    ) -> Result<ExternalWithdrawal, ExternalWithdrawalError> {
        if self
            .account_restrictions
            .is_restricted(client)
            .map_err(|error| ExternalWithdrawalError::Storage(error.to_string()))?
        {
            return Err(ExternalWithdrawalError::AccountRestricted(
                client_display_name(client).to_owned(),
            ));
        }
        if !matches!(client, "alice" | "bob" | "carol") {
            return Err(ExternalWithdrawalError::Invalid(
                "unknown demo client".into(),
            ));
        }
        if operation.trim().is_empty() || operation.len() > 128 {
            return Err(ExternalWithdrawalError::Invalid(
                "operationId must contain 1-128 characters".into(),
            ));
        }
        if amount == 0 || !amount.is_multiple_of(UNITS_PER_CENT) {
            return Err(ExternalWithdrawalError::Invalid(
                "tokenAmountRaw must represent positive whole USD cents".into(),
            ));
        }
        let destination: Address = destination.parse().map_err(|_| {
            ExternalWithdrawalError::Invalid(
                "destinationAddress must be a valid Ethereum address".into(),
            )
        })?;
        let source = self.store.client_wallet(client)?;
        for address in [&source, &destination.to_checksum(None)] {
            if self
                .blacklist
                .is_blocked(address)
                .map_err(|error| ExternalWithdrawalError::Storage(error.to_string()))?
            {
                return Err(ExternalWithdrawalError::BlacklistedAddress(address.clone()));
            }
        }
        let fee = amount / EXTERNAL_FEE_DIVISOR;
        let _guard = self.lock.lock().await;
        self.reconciliation
            .check()
            .await
            .map_err(|error| ExternalWithdrawalError::Reconciliation(error.to_string()))?;
        let withdrawal = self
            .store
            .begin(operation, client, destination, amount, fee)?;
        if withdrawal.status == "completed" {
            return Ok(withdrawal);
        }
        if withdrawal.status == "chain_confirmed" {
            let completed = self.store.complete(operation, &self.contract, self.chain)?;
            self.reconciliation
                .check()
                .await
                .map_err(|error| ExternalWithdrawalError::Reconciliation(error.to_string()))?;
            return Ok(completed);
        }
        if withdrawal.status == "submission_uncertain" {
            return Err(ExternalWithdrawalError::SubmissionUncertain(
                withdrawal
                    .last_error
                    .unwrap_or_else(|| "wymaga ręcznego potwierdzenia transakcji".into()),
            ));
        }
        if withdrawal.status != "pending_chain" {
            return Err(ExternalWithdrawalError::IdempotencyConflict);
        }
        match self.gateway.transfer(destination, amount).await {
            Ok(hash) => {
                self.store.chain_confirmed(operation, &hash)?;
                let completed = self.store.complete(operation, &self.contract, self.chain)?;
                self.reconciliation
                    .check()
                    .await
                    .map_err(|error| ExternalWithdrawalError::Reconciliation(error.to_string()))?;
                Ok(completed)
            }
            Err(error @ ExternalWithdrawalError::SubmissionUncertain(_)) => {
                self.store
                    .mark_submission_uncertain(operation, &error.to_string())?;
                Err(error)
            }
            Err(error) => {
                self.store.fail(operation, &error.to_string())?;
                Err(error)
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ExternalWithdrawalError {
    #[error("invalid external withdrawal: {0}")]
    Invalid(String),
    #[error("external withdrawal conflicts with an existing operation")]
    IdempotencyConflict,
    #[error("insufficient client balance for amount and 1% fee")]
    InsufficientBalance,
    #[error("insufficient rUSD liquidity in the CASP hot wallet")]
    InsufficientHotWalletBalance,
    #[error("external withdrawal rejected because address is blacklisted: {0}")]
    BlacklistedAddress(String),
    #[error("konto klienta jest zablokowane: {0}")]
    AccountRestricted(String),
    #[error("external withdrawal persistence failed: {0}")]
    Storage(String),
    #[error("hot-wallet transfer failed: {0}")]
    Wallet(String),
    #[error("wynik wysłania transakcji z portfela gorącego jest niejednoznaczny: {0}")]
    SubmissionUncertain(String),
    #[error("custody reconciliation failed: {0}")]
    Reconciliation(String),
}

fn client_display_name(client: &str) -> &str {
    match client {
        "alice" => "Alicja",
        "bob" => "Bartosz",
        "carol" => "Karolina",
        other => other,
    }
}
