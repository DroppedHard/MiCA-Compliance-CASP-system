use alloy::primitives::Address;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

use crate::reconciliation::ReconciliationService;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeeSweep {
    pub operation_id: String,
    pub amount_raw: String,
    pub status: String,
    pub transaction_hash: Option<String>,
    pub last_error: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

pub trait FeeSweepStore: Send + Sync {
    fn begin(&self, operation_id: &str) -> Result<FeeSweep, FeeSweepError>;
    fn chain_confirmed(
        &self,
        operation_id: &str,
        transaction_hash: &str,
    ) -> Result<FeeSweep, FeeSweepError>;
    fn complete(&self, operation_id: &str) -> Result<FeeSweep, FeeSweepError>;
    fn fail(&self, operation_id: &str, error: &str) -> Result<(), FeeSweepError>;
    fn get(&self, operation_id: &str) -> Result<Option<FeeSweep>, FeeSweepError>;
}

#[async_trait]
pub trait FeeSweepGateway: Send + Sync {
    async fn transfer_to_corporate(
        &self,
        corporate: Address,
        amount_raw: u64,
    ) -> Result<String, FeeSweepError>;
}

#[derive(Clone)]
pub struct FeeSweepService {
    store: Arc<dyn FeeSweepStore>,
    gateway: Arc<dyn FeeSweepGateway>,
    reconciliation: Arc<ReconciliationService>,
    corporate: Address,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl FeeSweepService {
    pub fn new(
        store: Arc<dyn FeeSweepStore>,
        gateway: Arc<dyn FeeSweepGateway>,
        reconciliation: Arc<ReconciliationService>,
        corporate: Address,
    ) -> Self {
        Self {
            store,
            gateway,
            reconciliation,
            corporate,
            lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub async fn execute(&self, operation_id: &str) -> Result<FeeSweep, FeeSweepError> {
        if operation_id.trim().is_empty() || operation_id.len() > 128 {
            return Err(FeeSweepError::InvalidOperationId);
        }
        let _guard = self.lock.lock().await;
        self.reconciliation
            .check()
            .await
            .map_err(|error| FeeSweepError::Reconciliation(error.to_string()))?;

        let mut operation = self.store.begin(operation_id)?;
        if operation.status == "completed" {
            return Ok(operation);
        }
        let amount = operation
            .amount_raw
            .parse::<u64>()
            .map_err(|_| FeeSweepError::Storage("invalid persisted sweep amount".into()))?;
        if operation.transaction_hash.is_none() {
            match self
                .gateway
                .transfer_to_corporate(self.corporate, amount)
                .await
            {
                Ok(hash) => {
                    self.store.chain_confirmed(operation_id, &hash)?;
                }
                Err(error) => {
                    self.store.fail(operation_id, &error.to_string())?;
                    return Err(error);
                }
            }
        }
        operation = self.store.complete(operation_id)?;
        self.reconciliation
            .check()
            .await
            .map_err(|error| FeeSweepError::Reconciliation(error.to_string()))?;
        Ok(operation)
    }
}

#[derive(Debug, Error)]
pub enum FeeSweepError {
    #[error("operationId is required and must not exceed 128 characters")]
    InvalidOperationId,
    #[error("there are no pending CASP fees to transfer")]
    NoPendingFees,
    #[error("fee sweep operation conflicts with an existing request")]
    IdempotencyConflict,
    #[error("hot wallet does not contain enough rUSD for this fee transfer")]
    InsufficientHotBalance,
    #[error("fee transfer failed: {0}")]
    Wallet(String),
    #[error("custody reconciliation failed: {0}")]
    Reconciliation(String),
    #[error("fee sweep storage failed: {0}")]
    Storage(String),
}
