//! Integracyjny test ręcznego powiększenia puli CASP od emitenta.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    application::{BankGateway, BootstrapError, IssuerGateway, IssuerOrder, WalletGateway},
    domain::WalletBalances,
    infrastructure::{SqliteInventoryStore, SqliteReconciliationStore, SqliteRetailStore},
    inventory::{InventoryService, InventoryStatus, InventoryStore},
    reconciliation::ReconciliationService,
    retail_application::RetailStore,
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

#[derive(Default)]
struct TestWallet {
    hot: AtomicU64,
    cold: AtomicU64,
    cold_transfers: AtomicUsize,
}

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(
        &self,
        destination: Address,
        target_raw: u64,
    ) -> Result<Option<String>, BootstrapError> {
        if destination == COLD {
            let current_cold = self.cold.load(Ordering::SeqCst);
            assert!(target_raw >= current_cold);
            let moved = target_raw - current_cold;
            assert!(self.hot.load(Ordering::SeqCst) >= moved);
            self.hot.fetch_sub(moved, Ordering::SeqCst);
            self.cold.store(target_raw, Ordering::SeqCst);
            self.cold_transfers.fetch_add(1, Ordering::SeqCst);
            return Ok(Some("0xhot-to-cold".into()));
        }

        assert_eq!(destination, HOT);
        assert_eq!(self.hot.load(Ordering::SeqCst), target_raw);
        Ok(None)
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
            evidence_block: Some(10),
        })
    }
}

struct TestIssuer {
    wallet: Arc<TestWallet>,
    create_calls: AtomicUsize,
    settle_calls: AtomicUsize,
}

#[async_trait]
impl IssuerGateway for TestIssuer {
    async fn create_order(
        &self,
        _: &str,
        destination: Address,
        _: u64,
    ) -> Result<(), BootstrapError> {
        assert_eq!(
            destination, HOT,
            "emisja CASP musi trafić najpierw do hot walletu"
        );
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn settle_order(&self, _: &str) -> Result<IssuerOrder, BootstrapError> {
        self.settle_calls.fetch_add(1, Ordering::SeqCst);
        // 100 centów USD = 1 rUSD = 1_000_000 jednostek tokena.
        self.wallet.hot.fetch_add(1_000_000, Ordering::SeqCst);
        Ok(IssuerOrder {
            transaction_hash: Some("0xissuer-mint".into()),
        })
    }
}

#[derive(Default)]
struct TestBank(AtomicUsize);

#[async_trait]
impl BankGateway for TestBank {
    async fn send_usd(&self, _: &str, amount_minor: u64) -> Result<(), BootstrapError> {
        assert_eq!(amount_minor, 100);
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn manual_inventory_purchase_mints_to_hot_then_persists_20_80_distribution_once() {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-inventory-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        sequence
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite").to_string_lossy().to_string();

    let ledger = Arc::new(SqliteRetailStore::open(&database).unwrap());
    // Ręczne zasilenie jest dostępne po bootstrapie CASP; aktywujemy pusty
    // ledger, aby odtworzyć ten warunek początkowy bez dodawania tokenów.
    ledger.activate_inventory(0).unwrap();
    let wallet = Arc::new(TestWallet::default());
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&database).unwrap()),
        ledger.clone(),
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let issuer = Arc::new(TestIssuer {
        wallet: wallet.clone(),
        create_calls: AtomicUsize::new(0),
        settle_calls: AtomicUsize::new(0),
    });
    let bank = Arc::new(TestBank::default());
    let inventory_store = Arc::new(SqliteInventoryStore::open(&database).unwrap());
    let service = InventoryService::new(
        inventory_store.clone(),
        issuer.clone(),
        bank.clone(),
        wallet.clone(),
        ledger.clone(),
        reconciliation.clone(),
        CORPORATE,
        HOT,
        COLD,
    );

    let first = service.execute("manual-inventory-1", 100).await.unwrap();
    let replay = service.execute("manual-inventory-1", 100).await.unwrap();

    assert_eq!(first.status, InventoryStatus::Completed);
    assert_eq!(replay.status, InventoryStatus::Completed);
    assert_eq!(
        first.issuer_transaction_hash.as_deref(),
        Some("0xissuer-mint")
    );
    assert_eq!(
        first.cold_transaction_hash.as_deref(),
        Some("0xhot-to-cold")
    );
    assert_eq!(issuer.create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(issuer.settle_calls.load(Ordering::SeqCst), 1);
    assert_eq!(bank.0.load(Ordering::SeqCst), 1);
    assert_eq!(wallet.cold_transfers.load(Ordering::SeqCst), 1);
    assert_eq!(wallet.hot.load(Ordering::SeqCst), 200_000);
    assert_eq!(wallet.cold.load(Ordering::SeqCst), 800_000);
    assert_eq!(
        ledger.account("alice").unwrap().inventory_available_raw,
        "1000000"
    );
    assert_eq!(
        inventory_store
            .get("manual-inventory-1")
            .unwrap()
            .unwrap()
            .status,
        InventoryStatus::Completed
    );
    assert_eq!(
        reconciliation.current().unwrap().difference_raw.as_deref(),
        Some("0")
    );

    // Katalog tymczasowy żyje do końca testu; pliki SQLite nie są częścią danych demo.
    drop(directory);
}
