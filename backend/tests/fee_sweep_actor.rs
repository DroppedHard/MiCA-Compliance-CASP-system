//! Integracyjny test przeniesienia prowizji CASP do portfela korporacyjnego.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
    fee_sweep::{FeeSweepError, FeeSweepGateway, FeeSweepService},
    infrastructure::{SqliteFeeSweepStore, SqliteReconciliationStore, SqliteRetailStore},
    reconciliation::ReconciliationService,
    retail_application::{RetailStore, TransferPosting},
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
    corporate: AtomicU64,
    hot: AtomicU64,
    cold: AtomicU64,
    sweep_calls: AtomicUsize,
    fail_next_sweep: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(&self, _: Address, _: u64) -> Result<Option<String>, BootstrapError> {
        unreachable!("przeniesienie prowizji używa dedykowanej bramki")
    }

    async fn balances(
        &self,
        _: Address,
        _: Address,
        _: Address,
    ) -> Result<WalletBalances, BootstrapError> {
        Ok(WalletBalances {
            corporate_raw: self.corporate.load(Ordering::SeqCst).to_string(),
            hot_raw: self.hot.load(Ordering::SeqCst).to_string(),
            cold_raw: self.cold.load(Ordering::SeqCst).to_string(),
            evidence_block: Some(12),
        })
    }
}

#[async_trait]
impl FeeSweepGateway for TestWallet {
    async fn transfer_to_corporate(
        &self,
        corporate: Address,
        amount_raw: u64,
    ) -> Result<String, FeeSweepError> {
        assert_eq!(corporate, CORPORATE);
        self.sweep_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_next_sweep.swap(false, Ordering::SeqCst) {
            return Err(FeeSweepError::Wallet(
                "symulowane odrzucenie przed wysłaniem transferu".into(),
            ));
        }
        assert!(self.hot.load(Ordering::SeqCst) >= amount_raw);
        self.hot.fetch_sub(amount_raw, Ordering::SeqCst);
        self.corporate.fetch_add(amount_raw, Ordering::SeqCst);
        Ok("0xfee-sweep".into())
    }
}

#[tokio::test]
async fn fee_sweep_moves_pending_fee_once_to_corporate_and_preserves_custody_reconciliation() {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-fee-sweep-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        sequence
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite").to_string_lossy().to_string();

    let retail = Arc::new(SqliteRetailStore::open(&database).unwrap());
    retail.activate_inventory(10_000_000).unwrap();
    retail
        .purchase("seed-alice", "alice", 100, "0xtoken", 31_337)
        .unwrap();
    let transfer = retail
        .transfer(TransferPosting {
            id: "fee-generating-transfer",
            sender: "alice",
            recipient: "bob",
            gross_raw: 1_000_000,
            purpose: "transfer_prywatny",
            contract: "0xtoken",
            chain: 31_337,
        })
        .unwrap();
    assert_eq!(transfer.fee_raw, "1000");

    let wallet = Arc::new(TestWallet {
        corporate: AtomicU64::new(0),
        hot: AtomicU64::new(2_000_000),
        cold: AtomicU64::new(8_000_000),
        sweep_calls: AtomicUsize::new(0),
        fail_next_sweep: std::sync::atomic::AtomicBool::new(false),
    });
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&database).unwrap()),
        retail.clone(),
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let service = FeeSweepService::new(
        Arc::new(SqliteFeeSweepStore::open(&database).unwrap()),
        wallet.clone(),
        reconciliation.clone(),
        CORPORATE,
    );

    let first = service.execute("fee-sweep-1").await.unwrap();
    let replay = service.execute("fee-sweep-1").await.unwrap();

    assert_eq!(first.status, "completed");
    assert_eq!(first.transaction_hash.as_deref(), Some("0xfee-sweep"));
    assert_eq!(first, replay);
    assert_eq!(wallet.sweep_calls.load(Ordering::SeqCst), 1);
    assert_eq!(retail.fee_position().unwrap().pending_raw, "0");
    assert_eq!(wallet.corporate.load(Ordering::SeqCst), 1_000);
    assert_eq!(wallet.hot.load(Ordering::SeqCst), 1_999_000);
    assert_eq!(
        reconciliation.current().unwrap().difference_raw.as_deref(),
        Some("0")
    );
}

#[tokio::test]
async fn definite_fee_transfer_failure_keeps_pending_fee_and_retry_moves_it_once() {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-fee-retry-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        sequence
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite").to_string_lossy().to_string();
    let retail = Arc::new(SqliteRetailStore::open(&database).unwrap());
    retail.activate_inventory(10_000_000).unwrap();
    retail
        .purchase("seed-alice", "alice", 100, "0xtoken", 31_337)
        .unwrap();
    retail
        .transfer(TransferPosting {
            id: "fee-retry-transfer",
            sender: "alice",
            recipient: "bob",
            gross_raw: 1_000_000,
            purpose: "transfer_prywatny",
            contract: "0xtoken",
            chain: 31_337,
        })
        .unwrap();

    let wallet = Arc::new(TestWallet {
        corporate: AtomicU64::new(0),
        hot: AtomicU64::new(2_000_000),
        cold: AtomicU64::new(8_000_000),
        sweep_calls: AtomicUsize::new(0),
        fail_next_sweep: std::sync::atomic::AtomicBool::new(true),
    });
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&database).unwrap()),
        retail.clone(),
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let service = FeeSweepService::new(
        Arc::new(SqliteFeeSweepStore::open(&database).unwrap()),
        wallet.clone(),
        reconciliation,
        CORPORATE,
    );

    assert!(matches!(
        service.execute("fee-sweep-retry").await,
        Err(FeeSweepError::Wallet(_))
    ));
    assert_eq!(retail.fee_position().unwrap().pending_raw, "1000");
    assert_eq!(wallet.corporate.load(Ordering::SeqCst), 0);
    assert_eq!(wallet.hot.load(Ordering::SeqCst), 2_000_000);

    let completed = service.execute("fee-sweep-retry").await.unwrap();
    assert_eq!(completed.status, "completed");
    assert_eq!(wallet.sweep_calls.load(Ordering::SeqCst), 2);
    assert_eq!(retail.fee_position().unwrap().pending_raw, "0");
    assert_eq!(wallet.corporate.load(Ordering::SeqCst), 1_000);
    assert_eq!(wallet.hot.load(Ordering::SeqCst), 1_999_000);
}
