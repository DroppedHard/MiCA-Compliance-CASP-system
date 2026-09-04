//! Integracyjne testy ręcznego rebalansu custody CASP.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
    infrastructure::{SqliteReconciliationStore, SqliteRetailStore},
    inventory::{CustodyTransferGateway, InventoryError, RebalancingService},
    reconciliation::ReconciliationService,
    retail_application::RetailStore,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

const CORPORATE: Address = Address::with_last_byte(1);
const HOT: Address = Address::with_last_byte(2);
const COLD: Address = Address::with_last_byte(3);
const TOTAL_RAW: u64 = 10_000_000_000;

struct CustodyWallet {
    hot: AtomicU64,
    cold: AtomicU64,
}

#[async_trait]
impl WalletGateway for CustodyWallet {
    async fn ensure_balance(&self, _: Address, _: u64) -> Result<Option<String>, BootstrapError> {
        unreachable!("rebalans wysyła transfer przez dedykowaną bramkę custody")
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

struct TransferGateway {
    wallet: Arc<CustodyWallet>,
    source: Address,
    calls: AtomicUsize,
    fail_next_transfer: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl CustodyTransferGateway for TransferGateway {
    async fn transfer_custody(
        &self,
        destination: Address,
        amount_raw: u64,
    ) -> Result<String, InventoryError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_next_transfer.swap(false, Ordering::SeqCst) {
            return Err(InventoryError::Storage(
                "symulowane odrzucenie przed wysłaniem transferu custody".into(),
            ));
        }
        match (self.source, destination) {
            (HOT, COLD) => {
                self.wallet.hot.fetch_sub(amount_raw, Ordering::SeqCst);
                self.wallet.cold.fetch_add(amount_raw, Ordering::SeqCst);
            }
            (COLD, HOT) => {
                self.wallet.cold.fetch_sub(amount_raw, Ordering::SeqCst);
                self.wallet.hot.fetch_add(amount_raw, Ordering::SeqCst);
            }
            _ => return Err(InventoryError::Storage("unexpected custody route".into())),
        }
        Ok("0xrebalance".into())
    }
}

#[tokio::test]
async fn rebalancing_moves_only_hot_excess_to_cold_and_is_a_noop_at_20_80_target() {
    let ledger = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
    ledger.activate_inventory(TOTAL_RAW).unwrap();
    let wallet = Arc::new(CustodyWallet {
        hot: AtomicU64::new(TOTAL_RAW),
        cold: AtomicU64::new(0),
    });
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
        ledger,
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let hot_gateway = Arc::new(TransferGateway {
        wallet: wallet.clone(),
        source: HOT,
        calls: AtomicUsize::new(0),
        fail_next_transfer: std::sync::atomic::AtomicBool::new(false),
    });
    let cold_gateway = Arc::new(TransferGateway {
        wallet: wallet.clone(),
        source: COLD,
        calls: AtomicUsize::new(0),
        fail_next_transfer: std::sync::atomic::AtomicBool::new(false),
    });
    let service = RebalancingService::new(
        wallet.clone(),
        hot_gateway.clone(),
        cold_gateway.clone(),
        reconciliation,
        CORPORATE,
        HOT,
        COLD,
    );

    let first = service.execute().await.unwrap();
    let replay = service.execute().await.unwrap();

    assert_eq!(first.direction, "hot_to_cold");
    assert_eq!(first.amount_raw, "8000000000");
    assert_eq!(first.transaction_hash.as_deref(), Some("0xrebalance"));
    assert_eq!(replay.direction, "none");
    assert_eq!(hot_gateway.calls.load(Ordering::SeqCst), 1);
    assert_eq!(cold_gateway.calls.load(Ordering::SeqCst), 0);
    assert_eq!(wallet.hot.load(Ordering::SeqCst), 2_000_000_000);
    assert_eq!(wallet.cold.load(Ordering::SeqCst), 8_000_000_000);
}

#[tokio::test]
async fn failed_rebalance_keeps_wallet_balances_and_retry_moves_custody_once() {
    let ledger = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
    ledger.activate_inventory(TOTAL_RAW).unwrap();
    let wallet = Arc::new(CustodyWallet {
        hot: AtomicU64::new(TOTAL_RAW),
        cold: AtomicU64::new(0),
    });
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
        ledger,
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let hot_gateway = Arc::new(TransferGateway {
        wallet: wallet.clone(),
        source: HOT,
        calls: AtomicUsize::new(0),
        fail_next_transfer: std::sync::atomic::AtomicBool::new(true),
    });
    let cold_gateway = Arc::new(TransferGateway {
        wallet: wallet.clone(),
        source: COLD,
        calls: AtomicUsize::new(0),
        fail_next_transfer: std::sync::atomic::AtomicBool::new(false),
    });
    let service = RebalancingService::new(
        wallet.clone(),
        hot_gateway.clone(),
        cold_gateway,
        reconciliation,
        CORPORATE,
        HOT,
        COLD,
    );

    assert!(service.execute().await.is_err());
    assert_eq!(wallet.hot.load(Ordering::SeqCst), TOTAL_RAW);
    assert_eq!(wallet.cold.load(Ordering::SeqCst), 0);

    let completed = service.execute().await.unwrap();
    assert_eq!(completed.direction, "hot_to_cold");
    assert_eq!(hot_gateway.calls.load(Ordering::SeqCst), 2);
    assert_eq!(wallet.hot.load(Ordering::SeqCst), 2_000_000_000);
    assert_eq!(wallet.cold.load(Ordering::SeqCst), 8_000_000_000);
}
