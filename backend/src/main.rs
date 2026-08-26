use casp_backend::{
    api,
    application::{BootstrapService, PURCHASE_TOKEN_RAW},
    config::Config,
    infrastructure::{
        AlloyWalletGateway, HttpBankGateway, HttpIssuerGateway, SqliteBootstrapStore,
        SqliteReconciliationStore, SqliteReportingStore, SqliteRetailStore,
    },
    reconciliation::ReconciliationService,
    reporting::ReportingService,
    retail_application::RetailService,
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
    let wallet = Arc::new(
        AlloyWalletGateway::connect(
            &c.rpc_url,
            c.token_address,
            &c.corporate_private_key,
            c.corporate_address,
        )
        .await?,
    );
    let issuer = Arc::new(HttpIssuerGateway::new(&c.issuer_url));
    let service = Arc::new(BootstrapService::new(
        Arc::new(SqliteBootstrapStore::open(&c.database_path)?),
        issuer.clone(),
        Arc::new(HttpBankGateway::new(&c.mock_bank_url)),
        wallet.clone(),
        c.corporate_address,
        c.hot_address,
        c.cold_address,
    ));
    let retail_store = Arc::new(SqliteRetailStore::open(&c.database_path)?);
    let reconciliation = Arc::new(ReconciliationService::new(
        Arc::new(SqliteReconciliationStore::open(&c.database_path)?),
        retail_store.clone(),
        wallet,
        c.corporate_address,
        c.hot_address,
        c.cold_address,
    ));
    let retail = Arc::new(RetailService::new(
        retail_store,
        issuer,
        c.hot_address,
        c.token_address.to_checksum(None),
        c.chain_id,
        reconciliation.clone(),
    ));
    let reporting = Arc::new(ReportingService::new(Arc::new(SqliteReportingStore::open(
        &c.database_path,
    )?)));
    // A CASP cannot allocate customer entitlements before it owns the matching
    // rUSD pool. Resume the idempotent 10,000 rUSD bootstrap on every startup;
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
    info!(operation_id=%bootstrap.operation_id,status=?bootstrap.status,"CASP initial inventory is ready");
    let listener = TcpListener::bind(c.http_address).await?;
    info!(address=%c.http_address,"CASP HTTP server started");
    axum::serve(
        listener,
        api::router(service, retail, reconciliation, reporting),
    )
    .await?;
    Ok(())
}
