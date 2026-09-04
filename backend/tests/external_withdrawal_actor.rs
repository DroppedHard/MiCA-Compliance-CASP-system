//! Integracyjne testy wypłaty rUSD z custody CASP na adres Ethereum.
//!
//! Produkcyjne adaptery SQLite współdzielą jedną tymczasową bazę. Bramka
//! portfela gorącego jest kontrolowana, aby można było policzyć faktyczne
//! polecenia on-chain bez uruchamiania węzła blockchaina.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    account_restrictions::SqliteAccountRestrictions,
    application::{BootstrapError, WalletGateway},
    blacklist::SqliteAddressBlacklist,
    domain::WalletBalances,
    external_withdrawals::{
        ExternalWithdrawalError, ExternalWithdrawalGateway, ExternalWithdrawalService,
    },
    infrastructure::{SqliteExternalWithdrawalStore, SqliteReconciliationStore, SqliteRetailStore},
    reconciliation::ReconciliationService,
    retail_application::RetailStore,
};
use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const CORPORATE: Address = Address::with_last_byte(1);
const HOT: Address = Address::with_last_byte(2);
const COLD: Address = Address::with_last_byte(3);
const INITIAL_RAW: u64 = 10_000_000_000;
const DESTINATION: &str = "0x0000000000000000000000000000000000000007";
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestWallet;

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(&self, _: Address, _: u64) -> Result<Option<String>, BootstrapError> {
        unreachable!("wypłata korzysta z ExternalWithdrawalGateway, nie z rebalansu")
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

#[derive(Default)]
struct TestHotWallet {
    calls: AtomicUsize,
    transfers: Mutex<Vec<(Address, u64)>>,
    insufficient_balance: bool,
    submission_uncertain: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl ExternalWithdrawalGateway for TestHotWallet {
    async fn transfer(
        &self,
        destination: Address,
        amount_raw: u64,
    ) -> Result<String, ExternalWithdrawalError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.transfers
            .lock()
            .unwrap()
            .push((destination, amount_raw));
        if self.insufficient_balance {
            return Err(ExternalWithdrawalError::InsufficientHotWalletBalance);
        }
        if self.submission_uncertain.load(Ordering::SeqCst) {
            return Err(ExternalWithdrawalError::SubmissionUncertain(
                "połączenie zerwane po przekazaniu zlecenia do dostawcy RPC".into(),
            ));
        }
        Ok("0xexternal-withdrawal".into())
    }
}

struct Fixture {
    service: ExternalWithdrawalService,
    retail: Arc<SqliteRetailStore>,
    hot_wallet: Arc<TestHotWallet>,
    blacklist: Arc<SqliteAddressBlacklist>,
    _database_directory: std::path::PathBuf,
}

fn fixture() -> Fixture {
    fixture_with_hot_wallet_failure(false)
}

fn fixture_with_hot_wallet_failure(insufficient_balance: bool) -> Fixture {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-withdrawal-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        sequence
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite");
    let path = database.to_string_lossy().to_string();
    let retail = Arc::new(SqliteRetailStore::open(&path).unwrap());
    retail.activate_inventory(INITIAL_RAW).unwrap();
    // Wartości fiat w zakupie są centami: 200 = 2,00 rUSD na saldo Alicji.
    retail
        .purchase("seed-alice", "alice", 200, "0xtoken", 31337)
        .unwrap();
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&path).unwrap()),
        retail.clone(),
        Arc::new(TestWallet),
        CORPORATE,
        HOT,
        COLD,
    ));
    let blacklist = Arc::new(SqliteAddressBlacklist::open(&path).unwrap());
    let hot_wallet = Arc::new(TestHotWallet {
        calls: AtomicUsize::new(0),
        transfers: Mutex::new(Vec::new()),
        insufficient_balance,
        submission_uncertain: std::sync::atomic::AtomicBool::new(false),
    });
    let service = ExternalWithdrawalService::new(
        Arc::new(SqliteExternalWithdrawalStore::open(&path).unwrap()),
        hot_wallet.clone(),
        blacklist.clone(),
        Arc::new(SqliteAccountRestrictions::open(&path).unwrap()),
        reconciliation,
        "0xtoken".into(),
        31337,
    );
    Fixture {
        service,
        retail,
        hot_wallet,
        blacklist,
        _database_directory: directory,
    }
}

fn fixture_with_uncertain_submission() -> Fixture {
    let fixture = fixture_with_hot_wallet_failure(false);
    // Fixture ma własną kontrolowaną bramkę; zmiana flagi przed pierwszym wywołaniem
    // symuluje sytuację, w której RPC nie potwierdza, czy transakcja dotarła do sieci.
    fixture
        .hot_wallet
        .submission_uncertain
        .store(true, Ordering::SeqCst);
    fixture
}

#[tokio::test]
async fn insufficient_hot_wallet_balance_releases_client_lock_and_does_not_charge_fee() {
    let fixture = fixture_with_hot_wallet_failure(true);

    let result = fixture
        .service
        .execute("alice", "withdraw-no-liquidity", DESTINATION, 1_000_000)
        .await;

    assert!(matches!(
        result,
        Err(ExternalWithdrawalError::InsufficientHotWalletBalance)
    ));
    assert_eq!(fixture.hot_wallet.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.retail.account("alice").unwrap().available_raw,
        "2000000"
    );
    assert_eq!(fixture.retail.account("alice").unwrap().locked_raw, "0");
    assert_eq!(fixture.retail.fee_position().unwrap().pending_raw, "0");
}

#[tokio::test]
async fn uncertain_chain_submission_keeps_client_funds_locked_and_never_resubmits_same_operation() {
    let fixture = fixture_with_uncertain_submission();

    let first = fixture
        .service
        .execute("alice", "withdraw-uncertain", DESTINATION, 1_000_000)
        .await;
    let replay = fixture
        .service
        .execute("alice", "withdraw-uncertain", DESTINATION, 1_000_000)
        .await;

    assert!(matches!(
        first,
        Err(ExternalWithdrawalError::SubmissionUncertain(_))
    ));
    assert!(matches!(
        replay,
        Err(ExternalWithdrawalError::SubmissionUncertain(_))
    ));
    assert_eq!(fixture.hot_wallet.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.retail.account("alice").unwrap().available_raw,
        "990000"
    );
    assert_eq!(
        fixture.retail.account("alice").unwrap().locked_raw,
        "1010000"
    );
    assert_eq!(fixture.retail.fee_position().unwrap().pending_raw, "0");
}

#[tokio::test]
async fn withdrawal_debits_amount_and_fee_once_and_replay_does_not_resubmit_chain_transfer() {
    let fixture = fixture();

    let first = fixture
        .service
        .execute("alice", "withdraw-alice", DESTINATION, 1_000_000)
        .await
        .unwrap();
    let replay = fixture
        .service
        .execute("alice", "withdraw-alice", DESTINATION, 1_000_000)
        .await
        .unwrap();

    assert_eq!(first.status, "completed");
    assert_eq!(first.operation_id, replay.operation_id);
    assert_eq!(first.fee_raw, "10000");
    assert_eq!(fixture.hot_wallet.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.hot_wallet.transfers.lock().unwrap().as_slice(),
        &[(DESTINATION.parse().unwrap(), 1_000_000)]
    );
    assert_eq!(
        fixture.retail.account("alice").unwrap().available_raw,
        "990000"
    );
    assert_eq!(fixture.retail.account("alice").unwrap().locked_raw, "0");
    assert_eq!(fixture.retail.fee_position().unwrap().pending_raw, "10000");
}

#[tokio::test]
async fn blacklisted_destination_is_rejected_before_ledger_or_hot_wallet_change() {
    let fixture = fixture();
    fixture
        .blacklist
        .add(DESTINATION, "decyzja testowa")
        .unwrap();

    let result = fixture
        .service
        .execute("alice", "withdraw-blocked", DESTINATION, 1_000_000)
        .await;

    assert!(matches!(
        result,
        Err(ExternalWithdrawalError::BlacklistedAddress(_))
    ));
    assert_eq!(fixture.hot_wallet.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        fixture.retail.account("alice").unwrap().available_raw,
        "2000000"
    );
    assert_eq!(fixture.retail.account("alice").unwrap().locked_raw, "0");
}
