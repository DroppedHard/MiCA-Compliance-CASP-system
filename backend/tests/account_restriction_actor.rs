//! Integracyjny test administracyjnej blokady konta klienta CASP.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    account_restrictions::SqliteAccountRestrictions,
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
    infrastructure::{SqliteReconciliationStore, SqliteRetailStore},
    reconciliation::ReconciliationService,
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

#[derive(Default)]
struct TestIssuer(AtomicUsize);

#[async_trait]
impl RetailIssuerGateway for TestIssuer {
    async fn create_redemption(&self, _: &str, _: Address, _: u64) -> Result<(), RetailError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn settle_redemption(&self, _: &str) -> Result<IssuerRedemption, RetailError> {
        Ok(IssuerRedemption {
            transaction_hash: None,
        })
    }
}

struct TestWallet;

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(&self, _: Address, _: u64) -> Result<Option<String>, BootstrapError> {
        unreachable!()
    }

    async fn balances(
        &self,
        _: Address,
        _: Address,
        _: Address,
    ) -> Result<WalletBalances, BootstrapError> {
        Ok(WalletBalances {
            corporate_raw: "0".into(),
            hot_raw: "2000000000".into(),
            cold_raw: "8000000000".into(),
            evidence_block: Some(1),
        })
    }
}

#[tokio::test]
async fn persisted_account_block_rejects_all_value_operations_and_unblock_restores_access() {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-restriction-{}-{}-{}",
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
    store.activate_inventory(10_000_000_000).unwrap();
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&database).unwrap()),
        store.clone(),
        Arc::new(TestWallet),
        CORPORATE,
        HOT,
        COLD,
    ));
    let issuer = Arc::new(TestIssuer::default());
    let unrestricted = RetailService::new(
        store.clone(),
        issuer.clone(),
        HOT,
        "0xtoken".into(),
        31337,
        reconciliation.clone(),
    );
    unrestricted
        .purchase("alice", "seed-alice", 200)
        .await
        .unwrap();
    let restrictions = Arc::new(SqliteAccountRestrictions::open(&database).unwrap());
    restrictions.block("alice", "polecenie organu").unwrap();
    let restricted = RetailService::new(
        store.clone(),
        issuer.clone(),
        HOT,
        "0xtoken".into(),
        31337,
        reconciliation,
    )
    .with_account_restrictions(restrictions.clone());

    let purchase = restricted.purchase("alice", "blocked-purchase", 100).await;
    let sale = restricted.sale("alice", "blocked-sale", 1_000_000).await;
    let transfer = restricted
        .transfer(
            "alice",
            "bob",
            "blocked-transfer",
            1_000_000,
            "private_transfer",
        )
        .await;
    let redemption = restricted
        .redeem("alice", "blocked-redemption", 1_000_000)
        .await;

    for result in [purchase, sale, redemption] {
        assert!(matches!(result, Err(RetailError::AccountRestricted(_))));
    }
    assert!(matches!(transfer, Err(RetailError::AccountRestricted(_))));
    assert_eq!(store.account("alice").unwrap().available_raw, "2000000");
    assert_eq!(store.account("bob").unwrap().available_raw, "0");
    assert_eq!(issuer.0.load(Ordering::SeqCst), 0);

    assert!(restrictions.unblock("alice").unwrap());
    let restored = restricted
        .sale("alice", "restored-sale", 1_000_000)
        .await
        .unwrap();
    assert_eq!(restored.status, "completed");
    assert_eq!(store.account("alice").unwrap().available_raw, "1000000");
}
