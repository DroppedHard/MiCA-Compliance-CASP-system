use casp_backend::{
    account_restrictions::SqliteAccountRestrictions,
    api,
    application::{BootstrapService, PURCHASE_TOKEN_RAW},
    blacklist::SqliteAddressBlacklist,
    config::Config,
    external_deposits::ExternalDepositObserver,
    external_withdrawals::ExternalWithdrawalService,
    fee_sweep::FeeSweepService,
    infrastructure::{
        AlloyExternalDepositGateway, AlloyWalletGateway, HttpBankGateway, HttpIssuerGateway,
        HttpIssuerPublicGateway, SqliteBootstrapStore, SqliteExternalDepositStore,
        SqliteExternalWithdrawalStore, SqliteFeeSweepStore, SqliteInventoryStore,
        SqliteReconciliationStore, SqliteReportingStore, SqliteRetailStore, SqliteStatementStore,
    },
    inventory::{InventoryService, RebalancingService},
    public_info::PublicInfoService,
    reconciliation::ReconciliationService,
    reporting::ReportingService,
    retail_application::RetailService,
    statements::StatementService,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let c = Config::from_env()?;
    let hot_wallet = Arc::new(
        AlloyWalletGateway::connect_for_role(
            &c.rpc_url,
            c.token_address,
            &c.hot_private_key,
            c.hot_address,
            "hot",
        )
        .await?,
    );
    let cold_wallet = Arc::new(
        AlloyWalletGateway::connect_for_role(
            &c.rpc_url,
            c.token_address,
            &c.cold_private_key,
            c.cold_address,
            "cold",
        )
        .await?,
    );
    let issuer = Arc::new(HttpIssuerGateway::new(&c.issuer_url));
    let bank = Arc::new(HttpBankGateway::new(&c.mock_bank_url));
    let service = Arc::new(BootstrapService::new(
        Arc::new(SqliteBootstrapStore::open(&c.database_path)?),
        issuer.clone(),
        bank.clone(),
        hot_wallet.clone(),
        c.corporate_address,
        c.hot_address,
        c.cold_address,
    ));
    let retail_store = Arc::new(SqliteRetailStore::open(&c.database_path)?);
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&c.database_path)?),
        retail_store.clone(),
        hot_wallet.clone(),
        c.corporate_address,
        c.hot_address,
        c.cold_address,
    ));
    let blacklist = Arc::new(SqliteAddressBlacklist::open(&c.database_path)?);
    let account_restrictions = Arc::new(SqliteAccountRestrictions::open(&c.database_path)?);
    let retail = Arc::new(
        RetailService::new(
            retail_store.clone(),
            issuer.clone(),
            c.hot_address,
            c.token_address.to_checksum(None),
            c.chain_id,
            reconciliation.clone(),
        )
        .with_blacklist(blacklist.clone())
        .with_account_restrictions(account_restrictions.clone())
        .with_token_state(issuer.clone()),
    );
    let reporting_store = Arc::new(SqliteReportingStore::open(&c.database_path)?);
    if c.seed_reporting_demo_on_startup {
        reporting_store.seed_demo_history()?;
        info!("seeded idempotent CASP daily-report demo history");
    }
    let reporting = Arc::new(ReportingService::new(reporting_store));
    let inventory = Arc::new(InventoryService::new(
        Arc::new(SqliteInventoryStore::open(&c.database_path)?),
        issuer,
        bank,
        hot_wallet.clone(),
        retail_store,
        reconciliation.clone(),
        c.corporate_address,
        c.hot_address,
        c.cold_address,
    ));
    let rebalancing = Arc::new(RebalancingService::new(
        hot_wallet.clone(),
        hot_wallet.clone(),
        cold_wallet,
        reconciliation.clone(),
        c.corporate_address,
        c.hot_address,
        c.cold_address,
    ));
    let public_info = Arc::new(PublicInfoService::new(Arc::new(
        HttpIssuerPublicGateway::new(&c.issuer_url, &c.issuer_public_url),
    )));
    let statements = Arc::new(StatementService::new(Arc::new(SqliteStatementStore::open(
        &c.database_path,
    )?)));
    let fee_sweeps = Arc::new(FeeSweepService::new(
        Arc::new(SqliteFeeSweepStore::open(&c.database_path)?),
        hot_wallet.clone(),
        reconciliation.clone(),
        c.corporate_address,
    ));
    let withdrawals = Arc::new(ExternalWithdrawalService::new(
        Arc::new(SqliteExternalWithdrawalStore::open(&c.database_path)?),
        hot_wallet.clone(),
        blacklist.clone(),
        account_restrictions.clone(),
        reconciliation.clone(),
        c.token_address.to_checksum(None),
        c.chain_id,
    ));
    let deposit_observer = ExternalDepositObserver::new(
        Arc::new(
            AlloyExternalDepositGateway::connect(
                &c.rpc_url,
                c.deposit_router_address,
                c.deposit_confirmations,
            )
            .await?,
        ),
        Arc::new(SqliteExternalDepositStore::open(&c.database_path)?),
        reconciliation.clone(),
        c.chain_id,
    );
    // CASP cannot allocate customer entitlements before matching custody tokens
    // exist. They secure clients and are not CASP property. Resume the idempotent
    // 10,000 rUSD bootstrap on every startup;
    // completed boundaries are read from SQLite and are never executed twice.
    let bootstrap = service.execute().await?;
    retail.activate_bootstrap_inventory(PURCHASE_TOKEN_RAW)?;
    reconciliation.check().await?;
    tokio::spawn(
        reconciliation
            .as_ref()
            .clone()
            .run(std::time::Duration::from_secs(300)),
    );
    tokio::spawn(deposit_observer.run(std::time::Duration::from_secs(5)));
    info!(operation_id=%bootstrap.operation_id,status=?bootstrap.status,"CASP initial inventory is ready");
    let listener = TcpListener::bind(c.http_address).await?;
    info!(address=%c.http_address,"CASP HTTP server started");
    axum::serve(
        listener,
        api::router(api::RouterDependencies {
            service,
            retail,
            reconciliation,
            reporting,
            inventory,
            rebalancing,
            public_info,
            statements,
            fee_sweeps,
            blacklist,
            account_restrictions,
            withdrawals,
        }),
    )
    .await?;
    Ok(())
}
