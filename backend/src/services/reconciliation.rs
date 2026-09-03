use crate::{application::WalletGateway, domain::WalletBalances, retail_application::RetailStore};
use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tracing::{error, info};

pub const RECONCILIATION_POLICY_VERSION: &str = "casp-custody-reconciliation-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Balanced,
    Warning,
    Mismatch,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationSnapshot {
    pub status: ReconciliationStatus,
    pub hot_raw: Option<String>,
    pub cold_raw: Option<String>,
    pub corporate_raw: Option<String>,
    pub customer_available_raw: Option<String>,
    pub customer_locked_raw: Option<String>,
    pub inventory_available_raw: Option<String>,
    pub pending_fee_raw: Option<String>,
    pub custody_total_raw: Option<String>,
    pub obligation_total_raw: Option<String>,
    pub difference_raw: Option<String>,
    pub evidence_block: Option<u64>,
    pub reason: String,
    pub checked_at_unix_ms: u64,
    pub policy_version: String,
}

pub trait ReconciliationStore: Send + Sync {
    fn append(&self, snapshot: &ReconciliationSnapshot) -> Result<(), ReconciliationError>;
    fn latest(&self) -> Result<Option<ReconciliationSnapshot>, ReconciliationError>;
}

#[derive(Clone)]
pub struct ReconciliationService {
    store: Arc<dyn ReconciliationStore>,
    ledger: Arc<dyn RetailStore>,
    wallet: Arc<dyn WalletGateway>,
    corporate: Address,
    hot: Address,
    cold: Address,
}

impl ReconciliationService {
    pub fn new(
        store: Arc<dyn ReconciliationStore>,
        ledger: Arc<dyn RetailStore>,
        wallet: Arc<dyn WalletGateway>,
        corporate: Address,
        hot: Address,
        cold: Address,
    ) -> Self {
        Self {
            store,
            ledger,
            wallet,
            corporate,
            hot,
            cold,
        }
    }

    pub fn current(&self) -> Result<ReconciliationSnapshot, ReconciliationError> {
        self.store.latest()?.ok_or(ReconciliationError::Unavailable(
            "custody reconciliation has not run yet".into(),
        ))
    }

    pub async fn check(&self) -> Result<ReconciliationSnapshot, ReconciliationError> {
        let result = self.collect_and_evaluate().await;
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => ReconciliationSnapshot::unavailable(error.to_string()),
        };
        self.store.append(&snapshot)?;
        Ok(snapshot)
    }

    pub async fn run(self, interval: Duration) {
        loop {
            match self.check().await {
                Ok(snapshot) => {
                    info!(status=?snapshot.status, difference=?snapshot.difference_raw, "CASP custody reconciliation completed")
                }
                Err(error) => error!(%error, "CASP custody reconciliation persistence failed"),
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn collect_and_evaluate(&self) -> Result<ReconciliationSnapshot, ReconciliationError> {
        let balances = self
            .wallet
            .balances(self.corporate, self.hot, self.cold)
            .await
            .map_err(|error| ReconciliationError::Unavailable(error.to_string()))?;
        let accounts = self
            .ledger
            .accounts()
            .map_err(|error| ReconciliationError::Unavailable(error.to_string()))?;
        let inventory = accounts
            .first()
            .ok_or_else(|| ReconciliationError::Unavailable("no CASP client positions".into()))?
            .inventory_available_raw
            .parse::<u64>()
            .map_err(numeric)?;
        let available = accounts.iter().try_fold(0_u64, |sum, account| {
            sum.checked_add(account.available_raw.parse::<u64>().map_err(numeric)?)
                .ok_or_else(|| ReconciliationError::Unavailable("available total overflow".into()))
        })?;
        let locked = accounts.iter().try_fold(0_u64, |sum, account| {
            sum.checked_add(account.locked_raw.parse::<u64>().map_err(numeric)?)
                .ok_or_else(|| ReconciliationError::Unavailable("locked total overflow".into()))
        })?;
        let pending_fee = self
            .ledger
            .fee_position()
            .map_err(|error| ReconciliationError::Unavailable(error.to_string()))?
            .pending_raw
            .parse::<u64>()
            .map_err(numeric)?;
        evaluate(&balances, available, locked, inventory, pending_fee)
    }
}

pub fn evaluate(
    balances: &WalletBalances,
    customer_available: u64,
    customer_locked: u64,
    inventory: u64,
    pending_fee: u64,
) -> Result<ReconciliationSnapshot, ReconciliationError> {
    let hot = balances.hot_raw.parse::<u64>().map_err(numeric)?;
    let cold = balances.cold_raw.parse::<u64>().map_err(numeric)?;
    let corporate = balances.corporate_raw.parse::<u64>().map_err(numeric)?;
    let custody = hot
        .checked_add(cold)
        .ok_or_else(|| ReconciliationError::Unavailable("custody total overflow".into()))?;
    let obligations = customer_available
        .checked_add(customer_locked)
        .and_then(|value| value.checked_add(inventory))
        .and_then(|value| value.checked_add(pending_fee))
        .ok_or_else(|| ReconciliationError::Unavailable("obligation total overflow".into()))?;
    let difference = i128::from(custody) - i128::from(obligations);
    let (status, reason) = if difference != 0 {
        (
            ReconciliationStatus::Mismatch,
            "hot and cold custody do not equal customer positions, locks, unallocated inventory and pending CASP fees",
        )
    } else if custody > 0 && u128::from(hot) * 100 != u128::from(custody) * 20 {
        (
            ReconciliationStatus::Warning,
            "custody is covered, but the hot/cold allocation differs from the 20/80 demo target",
        )
    } else {
        (
            ReconciliationStatus::Balanced,
            "custody and CASP obligations reconcile at the 20/80 demo target",
        )
    };
    Ok(ReconciliationSnapshot {
        status,
        hot_raw: Some(hot.to_string()),
        cold_raw: Some(cold.to_string()),
        corporate_raw: Some(corporate.to_string()),
        customer_available_raw: Some(customer_available.to_string()),
        customer_locked_raw: Some(customer_locked.to_string()),
        inventory_available_raw: Some(inventory.to_string()),
        pending_fee_raw: Some(pending_fee.to_string()),
        custody_total_raw: Some(custody.to_string()),
        obligation_total_raw: Some(obligations.to_string()),
        difference_raw: Some(difference.to_string()),
        evidence_block: balances.evidence_block,
        reason: reason.into(),
        checked_at_unix_ms: unix_ms(),
        policy_version: RECONCILIATION_POLICY_VERSION.into(),
    })
}

impl ReconciliationSnapshot {
    fn unavailable(reason: String) -> Self {
        Self {
            status: ReconciliationStatus::Unavailable,
            hot_raw: None,
            cold_raw: None,
            corporate_raw: None,
            customer_available_raw: None,
            customer_locked_raw: None,
            inventory_available_raw: None,
            pending_fee_raw: None,
            custody_total_raw: None,
            obligation_total_raw: None,
            difference_raw: None,
            evidence_block: None,
            reason,
            checked_at_unix_ms: unix_ms(),
            policy_version: RECONCILIATION_POLICY_VERSION.into(),
        }
    }
}

fn numeric(error: impl std::fmt::Display) -> ReconciliationError {
    ReconciliationError::Unavailable(format!("invalid numeric reconciliation input: {error}"))
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Error)]
pub enum ReconciliationError {
    #[error("custody reconciliation is unavailable: {0}")]
    Unavailable(String),
    #[error("custody reconciliation persistence failed: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::BootstrapError,
        infrastructure::{SqliteReconciliationStore, SqliteRetailStore},
    };
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    fn balances(hot: u64, cold: u64) -> WalletBalances {
        WalletBalances {
            corporate_raw: "500".into(),
            hot_raw: hot.to_string(),
            cold_raw: cold.to_string(),
            evidence_block: Some(1),
        }
    }

    #[test]
    fn classifies_balanced_allocation_warning_and_non_blocking_mismatch() {
        assert_eq!(
            evaluate(&balances(2_000, 8_000), 3_000, 1_000, 6_000, 0)
                .unwrap()
                .status,
            ReconciliationStatus::Balanced
        );
        assert_eq!(
            evaluate(&balances(1_000, 9_000), 3_000, 1_000, 6_000, 0)
                .unwrap()
                .status,
            ReconciliationStatus::Warning
        );
        let mismatch = evaluate(&balances(2_000, 7_999), 3_000, 1_000, 6_000, 0).unwrap();
        assert_eq!(mismatch.status, ReconciliationStatus::Mismatch);
        assert_eq!(mismatch.difference_raw.as_deref(), Some("-1"));
    }

    struct MutableWallet {
        hot: AtomicU64,
        cold: AtomicU64,
        fail: AtomicBool,
    }
    #[async_trait::async_trait]
    impl WalletGateway for MutableWallet {
        async fn ensure_balance(
            &self,
            _: Address,
            _: u64,
        ) -> Result<Option<String>, BootstrapError> {
            unreachable!()
        }
        async fn balances(
            &self,
            _: Address,
            _: Address,
            _: Address,
        ) -> Result<WalletBalances, BootstrapError> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(BootstrapError::Wallet("RPC unavailable".into()));
            }
            Ok(WalletBalances {
                corporate_raw: "0".into(),
                hot_raw: self.hot.load(Ordering::SeqCst).to_string(),
                cold_raw: self.cold.load(Ordering::SeqCst).to_string(),
                evidence_block: Some(7),
            })
        }
    }

    fn service(wallet: Arc<MutableWallet>) -> ReconciliationService {
        let ledger = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
        ledger.activate_inventory(10_000_000).unwrap();
        ReconciliationService::new(
            Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
            ledger,
            wallet,
            Address::with_last_byte(1),
            Address::with_last_byte(2),
            Address::with_last_byte(3),
        )
    }

    #[tokio::test]
    async fn persists_unavailable_evidence_when_chain_read_fails() {
        let service = service(Arc::new(MutableWallet {
            hot: AtomicU64::new(2_000_000),
            cold: AtomicU64::new(8_000_000),
            fail: AtomicBool::new(true),
        }));
        assert_eq!(
            service.check().await.unwrap().status,
            ReconciliationStatus::Unavailable
        );
        assert_eq!(
            service.current().unwrap().status,
            ReconciliationStatus::Unavailable
        );
    }

    #[tokio::test]
    async fn records_drift_and_recovery_after_wallet_correction() {
        let wallet = Arc::new(MutableWallet {
            hot: AtomicU64::new(2_000_000),
            cold: AtomicU64::new(7_999_999),
            fail: AtomicBool::new(false),
        });
        let service = service(wallet.clone());
        assert_eq!(
            service.check().await.unwrap().status,
            ReconciliationStatus::Mismatch
        );
        wallet.cold.store(8_000_000, Ordering::SeqCst);
        assert_eq!(
            service.check().await.unwrap().status,
            ReconciliationStatus::Balanced
        );
        assert_eq!(service.current().unwrap().evidence_block, Some(7));
    }
}
