//! Integracyjny test rozbieżności custody CASP jako sygnału diagnostycznego.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
    infrastructure::{SqliteReconciliationStore, SqliteRetailStore},
    reconciliation::{ReconciliationService, ReconciliationStatus},
    retail_application::{
        IssuerRedemption, RetailError, RetailIssuerGateway, RetailService, RetailStore,
    },
};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const CORPORATE: Address = Address::with_last_byte(1);
const HOT: Address = Address::with_last_byte(2);
const COLD: Address = Address::with_last_byte(3);
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestWallet {
    hot: AtomicU64,
    cold: AtomicU64,
    fail_reads: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(&self, _: Address, _: u64) -> Result<Option<String>, BootstrapError> {
        unreachable!("sprzedaż i wykup nie wykonują rebalansu")
    }

    async fn balances(
        &self,
        _: Address,
        _: Address,
        _: Address,
    ) -> Result<WalletBalances, BootstrapError> {
        if self.fail_reads.load(Ordering::SeqCst) {
            return Err(BootstrapError::Wallet(
                "symulowana niedostępność odczytu custody".into(),
            ));
        }
        Ok(WalletBalances {
            corporate_raw: "0".into(),
            hot_raw: self.hot.load(Ordering::SeqCst).to_string(),
            cold_raw: self.cold.load(Ordering::SeqCst).to_string(),
            evidence_block: Some(14),
        })
    }
}

struct TestIssuer {
    wallet: Arc<TestWallet>,
    pending_burn: AtomicU64,
    create_calls: AtomicUsize,
    settle_calls: AtomicUsize,
}

#[async_trait]
impl RetailIssuerGateway for TestIssuer {
    async fn create_redemption(
        &self,
        _: &str,
        holder: Address,
        amount_raw: u64,
    ) -> Result<(), RetailError> {
        assert_eq!(holder, HOT);
        self.pending_burn.store(amount_raw, Ordering::SeqCst);
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn settle_redemption(&self, _: &str) -> Result<IssuerRedemption, RetailError> {
        self.hot_burn();
        self.settle_calls.fetch_add(1, Ordering::SeqCst);
        Ok(IssuerRedemption {
            transaction_hash: Some("0xissuer-burn".into()),
        })
    }
}

impl TestIssuer {
    fn hot_burn(&self) {
        let amount = self.pending_burn.load(Ordering::SeqCst);
        assert!(self.wallet.hot.load(Ordering::SeqCst) >= amount);
        self.wallet.hot.fetch_sub(amount, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn custody_drift_is_persisted_but_sale_and_redemption_remain_available() {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-reconciliation-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        sequence
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite").to_string_lossy().to_string();

    let store = Arc::new(SqliteRetailStore::open(&database).unwrap());
    store.activate_inventory(10_000_000).unwrap();
    store
        .purchase("seed-alice", "alice", 200, "0xtoken", 31_337)
        .unwrap();
    let wallet = Arc::new(TestWallet {
        hot: AtomicU64::new(2_000_000),
        cold: AtomicU64::new(7_999_999),
        fail_reads: std::sync::atomic::AtomicBool::new(false),
    });
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&database).unwrap()),
        store.clone(),
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let issuer = Arc::new(TestIssuer {
        wallet: wallet.clone(),
        pending_burn: AtomicU64::new(0),
        create_calls: AtomicUsize::new(0),
        settle_calls: AtomicUsize::new(0),
    });
    let service = RetailService::new(
        store.clone(),
        issuer.clone(),
        HOT,
        "0xtoken".into(),
        31_337,
        reconciliation.clone(),
    );

    let drift = reconciliation.check().await.unwrap();
    assert_eq!(drift.status, ReconciliationStatus::Mismatch);
    assert_eq!(drift.difference_raw.as_deref(), Some("-1"));

    let sale = service
        .sale("alice", "sale-during-drift", 1_000_000)
        .await
        .unwrap();
    let redemption = service
        .redeem("alice", "redeem-during-drift", 1_000_000)
        .await
        .unwrap();

    assert_eq!(sale.status, "completed");
    assert_eq!(redemption.status, "completed");
    assert_eq!(issuer.create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(issuer.settle_calls.load(Ordering::SeqCst), 1);
    assert_eq!(store.account("alice").unwrap().available_raw, "0");
    assert_eq!(
        reconciliation.current().unwrap().status,
        ReconciliationStatus::Mismatch,
        "rozbieżność pozostaje widoczna administratorowi, ale nie jest automatyczną blokadą"
    );
}

#[tokio::test]
async fn unavailable_custody_evidence_does_not_block_retail_ledger_and_recovers_on_next_check() {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-reconciliation-recovery-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        sequence
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite").to_string_lossy().to_string();
    let store = Arc::new(SqliteRetailStore::open(&database).unwrap());
    store.activate_inventory(10_000_000).unwrap();
    let wallet = Arc::new(TestWallet {
        hot: AtomicU64::new(2_000_000),
        cold: AtomicU64::new(8_000_000),
        fail_reads: std::sync::atomic::AtomicBool::new(true),
    });
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&database).unwrap()),
        store.clone(),
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let issuer = Arc::new(TestIssuer {
        wallet: wallet.clone(),
        pending_burn: AtomicU64::new(0),
        create_calls: AtomicUsize::new(0),
        settle_calls: AtomicUsize::new(0),
    });
    let service = RetailService::new(
        store.clone(),
        issuer,
        HOT,
        "0xtoken".into(),
        31_337,
        reconciliation.clone(),
    );

    assert_eq!(
        reconciliation.check().await.unwrap().status,
        ReconciliationStatus::Unavailable
    );
    let purchase = service
        .purchase("alice", "purchase-during-unavailable", 100)
        .await
        .unwrap();
    assert_eq!(purchase.status, "completed");
    assert_eq!(store.account("alice").unwrap().available_raw, "1000000");

    wallet.fail_reads.store(false, Ordering::SeqCst);
    assert_eq!(
        reconciliation.check().await.unwrap().status,
        ReconciliationStatus::Balanced
    );
}
