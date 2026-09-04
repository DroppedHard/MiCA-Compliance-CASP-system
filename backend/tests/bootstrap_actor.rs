//! Integracyjne testy początkowego zakupu puli CASP.
//!
//! Testy składają produkcyjny `BootstrapService` i adapter SQLite z
//! kontrolowanymi portami emitenta, banku i custody. Weryfikują trwały stan
//! procesu, idempotencję oraz docelowy podział 20/80.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    application::{
        BankGateway, BootstrapError, BootstrapService, BootstrapStore, COLD_TARGET_RAW,
        HOT_TARGET_RAW, IssuerGateway, IssuerOrder, WalletGateway,
    },
    domain::{BootstrapStatus, WalletBalances},
    infrastructure::SqliteBootstrapStore,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

const CORPORATE: Address = Address::with_last_byte(1);
const HOT: Address = Address::with_last_byte(2);
const COLD: Address = Address::with_last_byte(3);

#[derive(Default)]
struct TestIssuer {
    create_calls: AtomicUsize,
    settle_calls: AtomicUsize,
}

#[async_trait]
impl IssuerGateway for TestIssuer {
    async fn create_order(&self, _: &str, _: Address, _: u64) -> Result<(), BootstrapError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn settle_order(&self, _: &str) -> Result<IssuerOrder, BootstrapError> {
        self.settle_calls.fetch_add(1, Ordering::SeqCst);
        Ok(IssuerOrder {
            transaction_hash: Some("0xissuer-mint".into()),
        })
    }
}

#[derive(Default)]
struct TestBank(AtomicUsize);

#[async_trait]
impl BankGateway for TestBank {
    async fn send_usd(&self, _: &str, _: u64) -> Result<(), BootstrapError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct TestWallet {
    hot: AtomicU64,
    cold: AtomicU64,
    distribution_calls: AtomicUsize,
    inconsistent_after_distribution: bool,
}

impl TestWallet {
    fn issued_pool(inconsistent_after_distribution: bool) -> Self {
        Self {
            hot: AtomicU64::new(HOT_TARGET_RAW + COLD_TARGET_RAW),
            cold: AtomicU64::new(0),
            distribution_calls: AtomicUsize::new(0),
            inconsistent_after_distribution,
        }
    }
}

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(
        &self,
        destination: Address,
        target_raw: u64,
    ) -> Result<Option<String>, BootstrapError> {
        assert_eq!(
            destination, COLD,
            "bootstrap may only distribute hot -> cold"
        );
        assert_eq!(target_raw, COLD_TARGET_RAW);
        self.distribution_calls.fetch_add(1, Ordering::SeqCst);
        self.cold.store(target_raw, Ordering::SeqCst);
        if !self.inconsistent_after_distribution {
            self.hot.store(HOT_TARGET_RAW, Ordering::SeqCst);
        }
        Ok(Some("0xhot-to-cold".into()))
    }

    async fn balances(
        &self,
        _: Address,
        _: Address,
        _: Address,
    ) -> Result<WalletBalances, BootstrapError> {
        Ok(WalletBalances {
            corporate_raw: "0".into(),
            hot_raw: self.hot.load(Ordering::SeqCst).to_string(),
            cold_raw: self.cold.load(Ordering::SeqCst).to_string(),
            evidence_block: Some(1),
        })
    }
}

fn service(
    wallet: Arc<TestWallet>,
) -> (
    BootstrapService,
    Arc<TestIssuer>,
    Arc<TestBank>,
    Arc<SqliteBootstrapStore>,
) {
    let store = Arc::new(SqliteBootstrapStore::open(":memory:").unwrap());
    let issuer = Arc::new(TestIssuer::default());
    let bank = Arc::new(TestBank::default());
    let service = BootstrapService::new(
        store.clone(),
        issuer.clone(),
        bank.clone(),
        wallet,
        CORPORATE,
        HOT,
        COLD,
    );
    (service, issuer, bank, store)
}

#[tokio::test]
async fn bootstrap_is_durable_idempotent_and_leaves_tokens_only_in_hot_and_cold_wallets() {
    let wallet = Arc::new(TestWallet::issued_pool(false));
    let (service, issuer, bank, store) = service(wallet.clone());

    let completed = service.execute().await.unwrap();
    let replay = service.execute().await.unwrap();

    assert_eq!(completed.status, BootstrapStatus::Distributed);
    assert_eq!(completed.operation_id, replay.operation_id);
    assert_eq!(
        completed.issuer_transaction_hash.as_deref(),
        Some("0xissuer-mint")
    );
    assert_eq!(
        completed.cold_transaction_hash.as_deref(),
        Some("0xhot-to-cold")
    );
    assert_eq!(issuer.create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(issuer.settle_calls.load(Ordering::SeqCst), 1);
    assert_eq!(bank.0.load(Ordering::SeqCst), 1);
    assert_eq!(wallet.distribution_calls.load(Ordering::SeqCst), 1);
    assert_eq!(wallet.hot.load(Ordering::SeqCst), HOT_TARGET_RAW);
    assert_eq!(wallet.cold.load(Ordering::SeqCst), COLD_TARGET_RAW);
    assert_eq!(
        store.get().unwrap().unwrap().status,
        BootstrapStatus::Distributed
    );
}

#[tokio::test]
async fn bootstrap_marks_operation_failed_when_20_80_reconciliation_fails() {
    let wallet = Arc::new(TestWallet::issued_pool(true));
    let (service, _, _, store) = service(wallet);

    let error = service.execute().await.unwrap_err();

    assert!(matches!(error, BootstrapError::Reconciliation(_)));
    let failed = store.get().unwrap().unwrap();
    assert_eq!(failed.status, BootstrapStatus::Failed);
    assert!(
        failed
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("wallet balances do not match")
    );
}
