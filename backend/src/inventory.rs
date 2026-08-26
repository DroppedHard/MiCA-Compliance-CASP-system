use crate::{
    application::{BankGateway, BootstrapError, IssuerGateway, WalletGateway},
    domain::WalletBalances,
    reconciliation::ReconciliationService,
    retail_application::RetailStore,
};
use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

const TOKEN_UNITS_PER_CENT: u64 = 10_000;
pub const POLICY_VERSION: &str = "casp-manual-inventory-20-80-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryStatus {
    Created,
    IssuerOrderCreated,
    FiatSent,
    TokensIssued,
    TargetsRecorded,
    ColdDistributed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOperation {
    pub operation_id: String,
    pub status: InventoryStatus,
    pub amount_usd_minor: String,
    pub token_amount_raw: String,
    pub hot_increment_raw: String,
    pub cold_increment_raw: String,
    pub hot_target_raw: Option<String>,
    pub cold_target_raw: Option<String>,
    pub issuer_transaction_hash: Option<String>,
    pub cold_transaction_hash: Option<String>,
    pub hot_transaction_hash: Option<String>,
    pub last_error: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

pub trait InventoryStore: Send + Sync {
    fn create(&self, id: &str, amount_minor: u64) -> Result<InventoryOperation, InventoryError>;
    fn get(&self, id: &str) -> Result<Option<InventoryOperation>, InventoryError>;
    fn list(&self) -> Result<Vec<InventoryOperation>, InventoryError>;
    fn targets(&self, id: &str, hot: u64, cold: u64) -> Result<InventoryOperation, InventoryError>;
    fn advance(
        &self,
        id: &str,
        status: InventoryStatus,
        issuer: Option<&str>,
        cold: Option<&str>,
        hot: Option<&str>,
    ) -> Result<InventoryOperation, InventoryError>;
    fn fail(&self, id: &str, message: &str) -> Result<(), InventoryError>;
}

#[derive(Clone)]
pub struct InventoryService {
    store: Arc<dyn InventoryStore>,
    issuer: Arc<dyn IssuerGateway>,
    bank: Arc<dyn BankGateway>,
    wallet: Arc<dyn WalletGateway>,
    ledger: Arc<dyn RetailStore>,
    reconciliation: Arc<ReconciliationService>,
    corporate: Address,
    hot: Address,
    cold: Address,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl InventoryService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: Arc<dyn InventoryStore>,
        issuer: Arc<dyn IssuerGateway>,
        bank: Arc<dyn BankGateway>,
        wallet: Arc<dyn WalletGateway>,
        ledger: Arc<dyn RetailStore>,
        reconciliation: Arc<ReconciliationService>,
        corporate: Address,
        hot: Address,
        cold: Address,
    ) -> Self {
        Self {
            store,
            issuer,
            bank,
            wallet,
            ledger,
            reconciliation,
            corporate,
            hot,
            cold,
            lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn list(&self) -> Result<Vec<InventoryOperation>, InventoryError> {
        self.store.list()
    }

    pub async fn execute(
        &self,
        id: &str,
        amount_minor: u64,
    ) -> Result<InventoryOperation, InventoryError> {
        if id.trim().is_empty() || amount_minor == 0 {
            return Err(InventoryError::Invalid(
                "operationId and a positive amountUsdMinor are required".into(),
            ));
        }
        let _guard = self.lock.lock().await;
        let mut operation = self.store.create(id, amount_minor)?;
        let result = self.resume(&mut operation).await;
        if let Err(error) = &result {
            let _ = self.store.fail(id, &error.to_string());
        }
        result
    }

    async fn resume(
        &self,
        operation: &mut InventoryOperation,
    ) -> Result<InventoryOperation, InventoryError> {
        let amount_minor = parse(&operation.amount_usd_minor)?;
        if operation.status == InventoryStatus::Created {
            self.issuer
                .create_order(&operation.operation_id, self.corporate, amount_minor)
                .await?;
            *operation = self.store.advance(
                &operation.operation_id,
                InventoryStatus::IssuerOrderCreated,
                None,
                None,
                None,
            )?;
        }
        if operation.status == InventoryStatus::IssuerOrderCreated {
            self.bank
                .send_usd(&operation.operation_id, amount_minor)
                .await?;
            *operation = self.store.advance(
                &operation.operation_id,
                InventoryStatus::FiatSent,
                None,
                None,
                None,
            )?;
        }
        if operation.status == InventoryStatus::FiatSent {
            let issued = self.issuer.settle_order(&operation.operation_id).await?;
            *operation = self.store.advance(
                &operation.operation_id,
                InventoryStatus::TokensIssued,
                issued.transaction_hash.as_deref(),
                None,
                None,
            )?;
        }
        if operation.status == InventoryStatus::TokensIssued {
            let balances = self
                .wallet
                .balances(self.corporate, self.hot, self.cold)
                .await?;
            let hot_target = parse(&balances.hot_raw)?
                .checked_add(parse(&operation.hot_increment_raw)?)
                .ok_or(InventoryError::Overflow)?;
            let cold_target = parse(&balances.cold_raw)?
                .checked_add(parse(&operation.cold_increment_raw)?)
                .ok_or(InventoryError::Overflow)?;
            *operation = self
                .store
                .targets(&operation.operation_id, hot_target, cold_target)?;
        }
        if operation.status == InventoryStatus::TargetsRecorded {
            let target = required(&operation.cold_target_raw)?;
            let hash = self.wallet.ensure_balance(self.cold, target).await?;
            self.ledger.add_inventory_once(
                &operation.operation_id,
                "cold",
                parse(&operation.cold_increment_raw)?,
            )?;
            *operation = self.store.advance(
                &operation.operation_id,
                InventoryStatus::ColdDistributed,
                None,
                hash.as_deref(),
                None,
            )?;
            self.reconciliation.check().await?;
        }
        if operation.status == InventoryStatus::ColdDistributed {
            let target = required(&operation.hot_target_raw)?;
            let hash = self.wallet.ensure_balance(self.hot, target).await?;
            self.ledger.add_inventory_once(
                &operation.operation_id,
                "hot",
                parse(&operation.hot_increment_raw)?,
            )?;
            *operation = self.store.advance(
                &operation.operation_id,
                InventoryStatus::Completed,
                None,
                None,
                hash.as_deref(),
            )?;
            self.reconciliation.check().await?;
        }
        Ok(operation.clone())
    }
}

pub fn allocation(amount_minor: u64) -> Result<(u64, u64, u64), InventoryError> {
    let total = amount_minor
        .checked_mul(TOKEN_UNITS_PER_CENT)
        .ok_or(InventoryError::Overflow)?;
    let hot = total / 5;
    Ok((total, hot, total - hot))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebalancingPlan {
    pub hot_delta_raw: i128,
    pub cold_delta_raw: i128,
    pub target_hot_raw: String,
    pub target_cold_raw: String,
    pub executable_from_corporate_only: bool,
    pub policy_version: &'static str,
}

pub fn rebalancing_plan(balances: &WalletBalances) -> Result<RebalancingPlan, InventoryError> {
    let hot = parse(&balances.hot_raw)?;
    let cold = parse(&balances.cold_raw)?;
    let total = hot.checked_add(cold).ok_or(InventoryError::Overflow)?;
    let target_hot = total / 5;
    let target_cold = total - target_hot;
    let hot_delta = target_hot as i128 - hot as i128;
    let cold_delta = target_cold as i128 - cold as i128;
    Ok(RebalancingPlan {
        hot_delta_raw: hot_delta,
        cold_delta_raw: cold_delta,
        target_hot_raw: target_hot.to_string(),
        target_cold_raw: target_cold.to_string(),
        executable_from_corporate_only: hot_delta >= 0 && cold_delta >= 0,
        policy_version: POLICY_VERSION,
    })
}

fn parse(value: &str) -> Result<u64, InventoryError> {
    value
        .parse()
        .map_err(|_| InventoryError::Storage("invalid persisted amount".into()))
}
fn required(value: &Option<String>) -> Result<u64, InventoryError> {
    value
        .as_deref()
        .ok_or_else(|| InventoryError::Storage("distribution target is missing".into()))
        .and_then(parse)
}

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("invalid inventory request: {0}")]
    Invalid(String),
    #[error("inventory operation conflicts with the persisted request")]
    IdempotencyConflict,
    #[error("inventory amount overflow")]
    Overflow,
    #[error("inventory persistence failed: {0}")]
    Storage(String),
    #[error(transparent)]
    Bootstrap(#[from] BootstrapError),
    #[error("inventory ledger update failed: {0}")]
    Ledger(String),
    #[error("inventory reconciliation failed: {0}")]
    Reconciliation(String),
}

impl From<crate::retail_application::RetailError> for InventoryError {
    fn from(error: crate::retail_application::RetailError) -> Self {
        Self::Ledger(error.to_string())
    }
}
impl From<crate::reconciliation::ReconciliationError> for InventoryError {
    fn from(error: crate::reconciliation::ReconciliationError) -> Self {
        Self::Reconciliation(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_is_exact_20_80() {
        assert_eq!(
            allocation(12_345).unwrap(),
            (123_450_000, 24_690_000, 98_760_000)
        );
    }
    #[test]
    fn calculator_exposes_required_direction() {
        let balances = WalletBalances {
            corporate_raw: "0".into(),
            hot_raw: "300".into(),
            cold_raw: "700".into(),
            evidence_block: Some(1),
        };
        let plan = rebalancing_plan(&balances).unwrap();
        assert_eq!(plan.hot_delta_raw, -100);
        assert_eq!(plan.cold_delta_raw, 100);
        assert!(!plan.executable_from_corporate_only);
    }
}
