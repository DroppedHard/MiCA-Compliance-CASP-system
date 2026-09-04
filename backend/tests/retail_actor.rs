//! Integracyjne testy CASP dla wewnętrznego ledgeru klienta.
//!
//! Testy składają produkcyjny `RetailService`, `ReconciliationService` i
//! adaptery SQLite. Porty emitenta oraz custody pozostają deterministyczne,
//! ponieważ transfer wewnętrzny CASP nie wykonuje transakcji blockchainowej.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
    infrastructure::{SqliteReconciliationStore, SqliteRetailStore},
    reconciliation::ReconciliationService,
    retail_application::{
        IssuerRedemption, RetailError, RetailIssuerGateway, RetailService, RetailStore,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const HOT: Address = Address::with_last_byte(2);
const COLD: Address = Address::with_last_byte(3);
const CORPORATE: Address = Address::with_last_byte(1);
const INITIAL_RAW: u64 = 10_000_000_000;

#[derive(Default)]
struct TestIssuer {
    create_calls: AtomicUsize,
    settle_calls: AtomicUsize,
}

#[async_trait]
impl RetailIssuerGateway for TestIssuer {
    async fn create_redemption(&self, _: &str, _: Address, _: u64) -> Result<(), RetailError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn settle_redemption(&self, _: &str) -> Result<IssuerRedemption, RetailError> {
        self.settle_calls.fetch_add(1, Ordering::SeqCst);
        Ok(IssuerRedemption {
            transaction_hash: Some("0xburn".into()),
        })
    }
}

struct TestWallet {
    balance_reads: AtomicUsize,
}

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(&self, _: Address, _: u64) -> Result<Option<String>, BootstrapError> {
        panic!("retail operations must not move custody tokens on-chain")
    }

    async fn balances(
        &self,
        _: Address,
        _: Address,
        _: Address,
    ) -> Result<WalletBalances, BootstrapError> {
        self.balance_reads.fetch_add(1, Ordering::SeqCst);
        Ok(WalletBalances {
            corporate_raw: "0".into(),
            hot_raw: "2000000000".into(),
            cold_raw: "8000000000".into(),
            evidence_block: Some(1),
        })
    }
}

struct Fixture {
    service: Arc<RetailService>,
    store: Arc<SqliteRetailStore>,
    wallet: Arc<TestWallet>,
    issuer: Arc<TestIssuer>,
}

fn fixture() -> Fixture {
    let store = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
    store.activate_inventory(INITIAL_RAW).unwrap();
    let wallet = Arc::new(TestWallet {
        balance_reads: AtomicUsize::new(0),
    });
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
        store.clone(),
        wallet.clone(),
        CORPORATE,
        HOT,
        COLD,
    ));
    let issuer = Arc::new(TestIssuer::default());
    let service = Arc::new(RetailService::new(
        store.clone(),
        issuer.clone(),
        HOT,
        "0x0000000000000000000000000000000000000009".into(),
        31337,
        reconciliation,
    ));
    Fixture {
        service,
        store,
        wallet,
        issuer,
    }
}

#[tokio::test]
async fn purchase_transfer_and_replay_keep_ledger_fee_and_custody_consistent() {
    let fixture = fixture();
    fixture
        .service
        .purchase("alice", "purchase-alice", 100)
        .await
        .unwrap();
    let transfer = fixture
        .service
        .transfer(
            "alice",
            "bob",
            "transfer-alice-bob",
            1_000_000,
            "private_transfer",
        )
        .await
        .unwrap();
    let replay = fixture
        .service
        .transfer(
            "alice",
            "bob",
            "transfer-alice-bob",
            1_000_000,
            "private_transfer",
        )
        .await
        .unwrap();

    assert_eq!(transfer, replay);
    assert_eq!(transfer.gross_raw, "1000000");
    assert_eq!(transfer.net_raw, "999000");
    assert_eq!(transfer.fee_raw, "1000");
    assert_eq!(fixture.store.account("alice").unwrap().available_raw, "0");
    assert_eq!(
        fixture.store.account("bob").unwrap().available_raw,
        "999000"
    );
    assert_eq!(fixture.store.fee_position().unwrap().pending_raw, "1000");
    assert_eq!(
        fixture
            .store
            .account("alice")
            .unwrap()
            .inventory_available_raw,
        // Wartość fiat w API jest podawana w centach: 100 = 1,00 USD = 1 rUSD.
        "9999000000"
    );
    assert!(fixture.wallet.balance_reads.load(Ordering::SeqCst) >= 6);
}

#[tokio::test]
async fn concurrent_transfers_cannot_spend_one_client_balance_twice() {
    let fixture = fixture();
    fixture
        .service
        .purchase("alice", "purchase-alice", 100)
        .await
        .unwrap();

    let first = fixture.service.clone();
    let second = fixture.service.clone();
    let (left, right) = tokio::join!(
        async move {
            first
                .transfer("alice", "bob", "transfer-one", 600_000, "private_transfer")
                .await
        },
        async move {
            second
                .transfer(
                    "alice",
                    "carol",
                    "transfer-two",
                    600_000,
                    "private_transfer",
                )
                .await
        },
    );

    assert!(left.is_ok() ^ right.is_ok());
    let error = left.err().or(right.err()).unwrap();
    assert!(matches!(error, RetailError::InsufficientBalance));
    assert_eq!(
        fixture.store.account("alice").unwrap().available_raw,
        "400000"
    );
    assert_eq!(fixture.store.fee_position().unwrap().pending_raw, "600");
    let credited = ["bob", "carol"]
        .into_iter()
        .map(|client| {
            fixture
                .store
                .account(client)
                .unwrap()
                .available_raw
                .parse::<u64>()
                .unwrap()
        })
        .sum::<u64>();
    assert_eq!(credited, 599_400);
}

#[tokio::test]
async fn sale_returns_inventory_and_redemption_calls_issuer_only_once() {
    let fixture = fixture();
    fixture
        .service
        .purchase("alice", "purchase-alice", 200)
        .await
        .unwrap();

    let sale = fixture
        .service
        .sale("alice", "sale-alice", 1_000_000)
        .await
        .unwrap();
    assert_eq!(sale.status, "completed");
    assert_eq!(
        fixture.store.account("alice").unwrap().available_raw,
        "1000000"
    );

    let redemption = fixture
        .service
        .redeem("alice", "redeem-alice", 1_000_000)
        .await
        .unwrap();
    let replay = fixture
        .service
        .redeem("alice", "redeem-alice", 1_000_000)
        .await
        .unwrap();

    assert_eq!(redemption.status, "completed");
    assert_eq!(redemption.operation_id, replay.operation_id);
    assert_eq!(replay.status, "completed");
    assert_eq!(fixture.store.account("alice").unwrap().available_raw, "0");
    assert_eq!(fixture.issuer.create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.issuer.settle_calls.load(Ordering::SeqCst), 1);
}
