use alloy::primitives::{Address, B256, keccak256};
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tracing::error;

use crate::reconciliation::ReconciliationService;

#[derive(Debug, Clone)]
pub struct ExternalDepositEvent {
    pub transaction_hash: String,
    pub log_index: u64,
    pub block_number: u64,
    pub sender: Address,
    pub client_reference: B256,
    pub amount_raw: u64,
}

#[async_trait]
pub trait ExternalDepositGateway: Send + Sync {
    async fn confirmed_block(&self) -> Result<u64, ExternalDepositError>;
    async fn events(
        &self,
        from: u64,
        to: u64,
    ) -> Result<Vec<ExternalDepositEvent>, ExternalDepositError>;
}

pub trait ExternalDepositStore: Send + Sync {
    fn checkpoint(&self) -> Result<u64, ExternalDepositError>;
    fn apply(
        &self,
        chain_id: u64,
        event: &ExternalDepositEvent,
        client_id: Option<&str>,
    ) -> Result<(), ExternalDepositError>;
    fn advance(&self, block: u64) -> Result<(), ExternalDepositError>;
    fn reset_checkpoint(&self) -> Result<(), ExternalDepositError>;
}

#[derive(Clone)]
pub struct ExternalDepositObserver {
    gateway: Arc<dyn ExternalDepositGateway>,
    store: Arc<dyn ExternalDepositStore>,
    reconciliation: Arc<ReconciliationService>,
    chain_id: u64,
}

impl ExternalDepositObserver {
    pub fn new(
        gateway: Arc<dyn ExternalDepositGateway>,
        store: Arc<dyn ExternalDepositStore>,
        reconciliation: Arc<ReconciliationService>,
        chain_id: u64,
    ) -> Self {
        Self {
            gateway,
            store,
            reconciliation,
            chain_id,
        }
    }

    pub async fn poll_once(&self) -> Result<(), ExternalDepositError> {
        let checkpoint = self.store.checkpoint()?;
        let to = self.gateway.confirmed_block().await?;
        // A persisted Compose volume may outlive the disposable Hardhat chain.
        // A lower confirmed height therefore identifies a fresh local chain.
        let from = if checkpoint > to {
            self.store.reset_checkpoint()?;
            1
        } else {
            checkpoint.saturating_add(1)
        };
        if from > to {
            return Ok(());
        }
        for event in self.gateway.events(from, to).await? {
            let client = client_for_reference(event.client_reference);
            self.store.apply(self.chain_id, &event, client)?;
        }
        self.store.advance(to)?;
        self.reconciliation
            .check()
            .await
            .map_err(|error| ExternalDepositError::Reconciliation(error.to_string()))?;
        Ok(())
    }

    pub async fn run(self, interval: Duration) {
        loop {
            if let Err(error) = self.poll_once().await {
                error!(%error, "external deposit polling failed");
            }
            tokio::time::sleep(interval).await;
        }
    }
}

fn client_for_reference(reference: B256) -> Option<&'static str> {
    [
        ("alice", "rusd:casp:alice"),
        ("bob", "rusd:casp:bob"),
        ("carol", "rusd:casp:carol"),
    ]
    .into_iter()
    .find_map(|(client, logical)| (keccak256(logical.as_bytes()) == reference).then_some(client))
}

#[derive(Debug, Error)]
pub enum ExternalDepositError {
    #[error("deposit chain read failed: {0}")]
    Rpc(String),
    #[error("external deposit storage failed: {0}")]
    Storage(String),
    #[error("external deposit amount exceeds demo numeric range")]
    AmountOverflow,
    #[error("custody reconciliation after deposit failed: {0}")]
    Reconciliation(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_only_known_logical_wallet_references() {
        assert_eq!(
            client_for_reference(keccak256(b"rusd:casp:alice")),
            Some("alice")
        );
        assert_eq!(client_for_reference(keccak256(b"unknown")), None);
    }
}
