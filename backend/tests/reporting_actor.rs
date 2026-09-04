//! Integracyjny test projekcji raportu dziennego i wyciągu z ledgeru CASP.

use alloy::primitives::Address;
use async_trait::async_trait;
use casp_backend::{
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
    infrastructure::{
        SqliteReconciliationStore, SqliteReportingStore, SqliteRetailStore, SqliteStatementStore,
    },
    reconciliation::ReconciliationService,
    reporting::ReportingService,
    retail_application::{
        IssuerRedemption, RetailError, RetailIssuerGateway, RetailService, RetailStore,
    },
    statements::StatementService,
};
use rusqlite::Connection;
use std::{fs, sync::Arc};

const CORPORATE: Address = Address::with_last_byte(1);
const HOT: Address = Address::with_last_byte(2);
const COLD: Address = Address::with_last_byte(3);

struct TestIssuer;

#[async_trait]
impl RetailIssuerGateway for TestIssuer {
    async fn create_redemption(&self, _: &str, _: Address, _: u64) -> Result<(), RetailError> {
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
            hot_raw: "4000000".into(),
            cold_raw: "16000000".into(),
            evidence_block: Some(1),
        })
    }
}

#[tokio::test]
async fn daily_report_and_client_statements_are_consistent_projections_of_retail_operations() {
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-reporting-{}-{}",
        std::process::id(),
        uuid::Uuid::now_v7()
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite").to_string_lossy().to_string();
    let retail = Arc::new(SqliteRetailStore::open(&database).unwrap());
    retail.activate_inventory(20_000_000).unwrap();
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&database).unwrap()),
        retail.clone(),
        Arc::new(TestWallet),
        CORPORATE,
        HOT,
        COLD,
    ));
    let service = RetailService::new(
        retail,
        Arc::new(TestIssuer),
        HOT,
        "0xtoken".into(),
        31337,
        reconciliation,
    );

    service
        .purchase("alice", "report-purchase", 1_000)
        .await
        .unwrap();
    service
        .transfer(
            "alice",
            "bob",
            "report-transfer",
            5_000_000,
            "goods_or_services",
        )
        .await
        .unwrap();

    let report = ReportingService::new(Arc::new(SqliteReportingStore::open(&database).unwrap()))
        .daily("1970-01-01", "9999-12-31")
        .unwrap();
    assert_eq!(report.days.len(), 1);
    let day = &report.days[0];
    assert_eq!(day.total_operation_count, 2);
    assert_eq!(day.total_value_raw, "15000000");
    assert_eq!(day.total_value_usd_minor, "1500");
    assert_eq!(day.means_of_exchange_count, 1);
    assert_eq!(day.means_of_exchange_value_raw, "5000000");
    assert_eq!(day.excluded_operation_count, 1);

    let statements =
        StatementService::new(Arc::new(SqliteStatementStore::open(&database).unwrap()));
    let alice = statements
        .generate("alice", "1970-01-01", "9999-12-31")
        .unwrap();
    let bob = statements
        .generate("bob", "1970-01-01", "9999-12-31")
        .unwrap();
    assert_eq!(alice.total_purchases_raw, "10000000");
    assert_eq!(alice.total_transfers_sent_raw, "5000000");
    assert_eq!(alice.total_fees_raw, "5000");
    assert_eq!(alice.closing_available_raw, "5000000");
    assert_eq!(bob.total_transfers_received_raw, "4995000");
    assert_eq!(bob.closing_available_raw, "4995000");
}

#[test]
fn daily_report_keeps_year_boundary_events_in_separate_utc_aggregates() {
    let directory = std::env::temp_dir().join(format!(
        "rusd-casp-reporting-boundary-{}-{}",
        std::process::id(),
        uuid::Uuid::now_v7()
    ));
    fs::create_dir_all(&directory).unwrap();
    let database = directory.join("actor.sqlite").to_string_lossy().to_string();
    let store = SqliteReportingStore::open(&database).unwrap();
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "INSERT INTO demo_reporting_events VALUES(?1,?2,?3,?4,?5,?6)",
            (
                "year-end-goods",
                "2025-12-31",
                "goods_or_services",
                5_000_000_i64,
                0_i64,
                0_i64,
            ),
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO demo_reporting_events VALUES(?1,?2,?3,?4,?5,?6)",
            (
                "new-year-private",
                "2026-01-01",
                "private_transfer",
                7_000_000_i64,
                0_i64,
                0_i64,
            ),
        )
        .unwrap();

    let report = ReportingService::new(Arc::new(store))
        .daily("2025-12-31", "2026-01-01")
        .unwrap();

    assert_eq!(report.days.len(), 2);
    assert_eq!(report.days[0].date_utc, "2025-12-31");
    assert_eq!(report.days[0].means_of_exchange_count, 1);
    assert_eq!(report.days[1].date_utc, "2026-01-01");
    assert_eq!(report.days[1].means_of_exchange_count, 0);
    assert_eq!(report.days[1].excluded_operation_count, 1);
}
