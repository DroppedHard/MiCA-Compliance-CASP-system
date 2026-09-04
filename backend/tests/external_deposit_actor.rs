//! Integracyjne testy obserwatora depozytów zewnętrznych CASP.

use alloy::primitives::{Address, B256, keccak256};
use async_trait::async_trait;
use casp_backend::{
    account_restrictions::SqliteAccountRestrictions,
    application::{BootstrapError, WalletGateway},
    blacklist::SqliteAddressBlacklist,
    domain::WalletBalances,
    external_deposits::{
        ExternalDepositError, ExternalDepositEvent, ExternalDepositGateway, ExternalDepositObserver,
    },
    infrastructure::{SqliteExternalDepositStore, SqliteReconciliationStore, SqliteRetailStore},
    reconciliation::ReconciliationService,
    retail_application::RetailStore,
};
use rusqlite::Connection;
use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const CORPORATE: Address = Address::with_last_byte(1);
const HOT: Address = Address::with_last_byte(2);
const COLD: Address = Address::with_last_byte(3);
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestWallet {
    hot_raw: u64,
}

#[async_trait]
impl WalletGateway for TestWallet {
    async fn ensure_balance(&self, _: Address, _: u64) -> Result<Option<String>, BootstrapError> {
        unreachable!("obserwator depozytu nie wykonuje rebalansu")
    }

    async fn balances(
        &self,
        _: Address,
        _: Address,
        _: Address,
    ) -> Result<WalletBalances, BootstrapError> {
        Ok(WalletBalances {
            corporate_raw: "0".into(),
            hot_raw: self.hot_raw.to_string(),
            cold_raw: "0".into(),
            evidence_block: Some(10),
        })
    }
}

struct TestGateway {
    events: Vec<ExternalDepositEvent>,
    confirmed_block: u64,
    event_ranges: Mutex<Vec<(u64, u64)>>,
    fail_next_events_read: AtomicBool,
}

#[async_trait]
impl ExternalDepositGateway for TestGateway {
    async fn confirmed_block(&self) -> Result<u64, ExternalDepositError> {
        Ok(self.confirmed_block)
    }

    async fn events(
        &self,
        from: u64,
        to: u64,
    ) -> Result<Vec<ExternalDepositEvent>, ExternalDepositError> {
        self.event_ranges.lock().unwrap().push((from, to));
        if self.fail_next_events_read.swap(false, Ordering::SeqCst) {
            return Err(ExternalDepositError::Rpc(
                "symulowana chwilowa awaria odczytu łańcucha".into(),
            ));
        }
        Ok(self.events.clone())
    }
}

fn event(hash: &str, reference: B256) -> ExternalDepositEvent {
    ExternalDepositEvent {
        transaction_hash: hash.into(),
        log_index: 0,
        block_number: 10,
        sender: Address::with_last_byte(9),
        client_reference: reference,
        amount_raw: 25_000_000,
    }
}

struct Fixture {
    observer: ExternalDepositObserver,
    gateway: Arc<TestGateway>,
    retail: Arc<SqliteRetailStore>,
    database: String,
    restrictions: Arc<SqliteAccountRestrictions>,
    blacklist: Arc<SqliteAddressBlacklist>,
    _database_directory: std::path::PathBuf,
}

fn fixture(events: Vec<ExternalDepositEvent>, hot_raw: u64) -> Fixture {
    let sequence = NEXT_DATABASE.fetch_add(1, Ordering::SeqCst);
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-deposit-{}-{}-{}",
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
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&path).unwrap()),
        retail.clone(),
        Arc::new(TestWallet { hot_raw }),
        CORPORATE,
        HOT,
        COLD,
    ));
    let gateway = Arc::new(TestGateway {
        events,
        confirmed_block: 10,
        event_ranges: Mutex::new(Vec::new()),
        fail_next_events_read: AtomicBool::new(false),
    });
    let observer = ExternalDepositObserver::new(
        gateway.clone(),
        Arc::new(SqliteExternalDepositStore::open(&path).unwrap()),
        reconciliation,
        31337,
    );
    Fixture {
        observer,
        gateway,
        retail,
        database: path.clone(),
        restrictions: Arc::new(SqliteAccountRestrictions::open(&path).unwrap()),
        blacklist: Arc::new(SqliteAddressBlacklist::open(&path).unwrap()),
        _database_directory: directory,
    }
}

#[tokio::test]
async fn failed_chain_read_does_not_advance_checkpoint_and_retry_credits_deposit_once() {
    let fixture = fixture(
        vec![event("0xretry", keccak256(b"rusd:casp:alice"))],
        25_000_000,
    );
    fixture
        .gateway
        .fail_next_events_read
        .store(true, Ordering::SeqCst);

    let error = fixture.observer.poll_once().await.unwrap_err();
    assert!(matches!(error, ExternalDepositError::Rpc(_)));
    fixture.observer.poll_once().await.unwrap();

    assert_eq!(
        *fixture.gateway.event_ranges.lock().unwrap(),
        vec![(1, 10), (1, 10)],
        "po nieudanym odczycie obserwator ponawia identyczny niepotwierdzony zakres"
    );
    assert_eq!(
        fixture.retail.account("alice").unwrap().available_raw,
        "25000000"
    );
    let connection = Connection::open(&fixture.database).unwrap();
    let checkpoint: i64 = connection
        .query_row(
            "SELECT last_confirmed_block FROM external_deposit_checkpoint WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let records: i64 = connection
        .query_row("SELECT count(*) FROM service_records", [], |row| row.get(0))
        .unwrap();
    assert_eq!(checkpoint, 10);
    assert_eq!(records, 1);
}

#[tokio::test]
async fn known_logical_address_is_credited_once_and_checkpoint_prevents_replay() {
    let fixture = fixture(
        vec![event("0xknown", keccak256(b"rusd:casp:alice"))],
        25_000_000,
    );

    fixture.observer.poll_once().await.unwrap();
    fixture.observer.poll_once().await.unwrap();

    assert_eq!(
        fixture.retail.account("alice").unwrap().available_raw,
        "25000000"
    );
    let connection = Connection::open(&fixture.database).unwrap();
    let records: i64 = connection
        .query_row("SELECT count(*) FROM service_records", [], |row| row.get(0))
        .unwrap();
    assert_eq!(records, 1);
}

#[tokio::test]
async fn restricted_client_deposit_is_audited_without_crediting_client_ledger() {
    let fixture = fixture(
        vec![event("0xrestricted", keccak256(b"rusd:casp:alice"))],
        0,
    );
    fixture
        .restrictions
        .block("alice", "polecenie organu")
        .unwrap();

    fixture.observer.poll_once().await.unwrap();

    assert_eq!(fixture.retail.account("alice").unwrap().available_raw, "0");
    let connection = Connection::open(&fixture.database).unwrap();
    let attempts: i64 = connection
        .query_row(
            "SELECT count(*) FROM blocked_transfer_attempts WHERE transfer_kind='external_deposit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempts, 1);
}

#[tokio::test]
async fn blacklisted_external_sender_is_audited_without_crediting_client_ledger() {
    let fixture = fixture(
        vec![event("0xblocked-sender", keccak256(b"rusd:casp:alice"))],
        0,
    );
    fixture
        .blacklist
        .add(
            &Address::with_last_byte(9).to_checksum(None),
            "decyzja ograniczająca adres źródłowy",
        )
        .unwrap();

    fixture.observer.poll_once().await.unwrap();

    assert_eq!(fixture.retail.account("alice").unwrap().available_raw, "0");
    let connection = Connection::open(&fixture.database).unwrap();
    let attempts: i64 = connection
        .query_row(
            "SELECT count(*) FROM blocked_transfer_attempts WHERE transfer_kind='external_deposit' AND reason='adres źródłowy lub docelowy jest na czarnej liście'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let deposits: i64 = connection
        .query_row("SELECT count(*) FROM external_deposits", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(attempts, 1);
    assert_eq!(
        deposits, 0,
        "zablokowany depozyt nie staje się pozycją klienta"
    );
}

#[tokio::test]
async fn unknown_logical_reference_is_recorded_but_does_not_credit_any_client() {
    let fixture = fixture(
        vec![event("0xunknown", keccak256(b"unknown-client"))],
        25_000_000,
    );

    fixture.observer.poll_once().await.unwrap();

    for client in ["alice", "bob", "carol"] {
        assert_eq!(fixture.retail.account(client).unwrap().available_raw, "0");
    }
    let connection = Connection::open(&fixture.database).unwrap();
    let status: String = connection
        .query_row(
            "SELECT status FROM external_deposits WHERE transaction_hash='0xunknown'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let service_records: i64 = connection
        .query_row("SELECT count(*) FROM service_records", [], |row| row.get(0))
        .unwrap();
    assert_eq!(status, "unknown_reference");
    assert_eq!(service_records, 0);
}
