use crate::{
    account_restrictions::{
        AccountRestrictionError, ClientAccountRestriction, SqliteAccountRestrictions,
    },
    application::{BootstrapError, BootstrapService},
    blacklist::{BlacklistEntry, BlacklistError, SqliteAddressBlacklist},
    domain::{BootstrapOperation, WalletBalances},
    external_withdrawals::{
        ExternalWithdrawal, ExternalWithdrawalError, ExternalWithdrawalService,
    },
    fee_sweep::{FeeSweep, FeeSweepError, FeeSweepService},
    inventory::{
        InventoryError, InventoryOperation, InventoryService, RebalancingPlan, RebalancingResult,
        RebalancingService, rebalancing_plan,
    },
    public_info::{PublicInfoError, PublicInfoService, TokenInformation},
    reconciliation::{ReconciliationService, ReconciliationSnapshot},
    reporting::{DailyTransactionReport, ReportingError, ReportingService},
    retail::{
        ClientAccount, ExchangeRate, FeePosition, InternalTransfer, RetailOrder, ServiceRecord,
    },
    retail_application::{RetailError, RetailService},
    statements::{ClientStatement, StatementError, StatementService},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone)]
struct AppState {
    service: Arc<BootstrapService>,
    retail: Arc<RetailService>,
    reconciliation: Arc<ReconciliationService>,
    reporting: Arc<ReportingService>,
    inventory: Arc<InventoryService>,
    rebalancing: Arc<RebalancingService>,
    public_info: Arc<PublicInfoService>,
    statements: Arc<StatementService>,
    fee_sweeps: Arc<FeeSweepService>,
    blacklist: Arc<SqliteAddressBlacklist>,
    account_restrictions: Arc<SqliteAccountRestrictions>,
    withdrawals: Arc<ExternalWithdrawalService>,
}
pub struct RouterDependencies {
    pub service: Arc<BootstrapService>,
    pub retail: Arc<RetailService>,
    pub reconciliation: Arc<ReconciliationService>,
    pub reporting: Arc<ReportingService>,
    pub inventory: Arc<InventoryService>,
    pub rebalancing: Arc<RebalancingService>,
    pub public_info: Arc<PublicInfoService>,
    pub statements: Arc<StatementService>,
    pub fee_sweeps: Arc<FeeSweepService>,
    pub blacklist: Arc<SqliteAddressBlacklist>,
    pub account_restrictions: Arc<SqliteAccountRestrictions>,
    pub withdrawals: Arc<ExternalWithdrawalService>,
}

pub fn router(dependencies: RouterDependencies) -> Router {
    let RouterDependencies {
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
    } = dependencies;
    Router::new()
        .route("/health", get(health))
        .route(
            "/api/v1/admin/bootstrap-inventory",
            post(execute).get(operation),
        )
        .route("/api/v1/admin/wallets", get(wallets))
        .route("/api/v1/admin/reconciliation", get(get_reconciliation))
        .route("/api/v1/admin/fees", get(fee_position))
        .route(
            "/api/v1/admin/exchange-rate",
            get(exchange_rate).post(set_exchange_rate),
        )
        .route("/api/v1/admin/fee-sweeps", post(execute_fee_sweep))
        .route(
            "/api/v1/admin/address-blacklist",
            get(list_blacklist).post(add_blacklist),
        )
        .route(
            "/api/v1/admin/address-blacklist/{address}",
            axum::routing::delete(remove_blacklist),
        )
        .route(
            "/api/v1/admin/client-account-restrictions",
            get(list_account_restrictions).post(block_client_account),
        )
        .route(
            "/api/v1/admin/client-account-restrictions/{client_id}",
            axum::routing::delete(unblock_client_account),
        )
        .route(
            "/api/v1/admin/inventory-replenishments",
            post(replenish).get(replenishments),
        )
        .route("/api/v1/admin/rebalancing-plan", get(rebalance_plan))
        .route("/api/v1/admin/rebalancing", post(execute_rebalancing))
        .route("/api/v1/admin/service-records", get(all_records))
        .route(
            "/api/v1/admin/service-record-amendments",
            post(amend_record).get(amendments),
        )
        .route("/api/v1/reports/daily-transactions", get(daily_report))
        .route("/api/v1/public/token-information", get(token_information))
        .route("/api/v1/public/exchange-rate", get(exchange_rate))
        .route("/api/v1/clients", get(accounts))
        .route("/api/v1/clients/{client_id}/account", get(account))
        .route("/api/v1/clients/{client_id}/records", get(records))
        .route("/api/v1/clients/{client_id}/purchases", post(purchase))
        .route("/api/v1/clients/{client_id}/sales", post(sale))
        .route("/api/v1/clients/{client_id}/transfers", post(transfer))
        .route(
            "/api/v1/clients/{client_id}/external-withdrawals",
            post(external_withdrawal),
        )
        .route("/api/v1/clients/{client_id}/redemptions", post(redemption))
        .route(
            "/api/v1/clients/{client_id}/statement",
            get(client_statement),
        )
        .with_state(AppState {
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
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalWithdrawalRequest {
    operation_id: String,
    destination_address: String,
    token_amount_raw: u64,
}
async fn external_withdrawal(
    State(s): State<AppState>,
    Path(client): Path<String>,
    Json(body): Json<ExternalWithdrawalRequest>,
) -> Result<Json<ExternalWithdrawal>, ExternalWithdrawalApiError> {
    s.withdrawals
        .execute(
            &client,
            &body.operation_id,
            &body.destination_address,
            body.token_amount_raw,
        )
        .await
        .map(Json)
        .map_err(ExternalWithdrawalApiError)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlacklistRequest {
    address: String,
    reason: String,
}
async fn list_blacklist(
    State(s): State<AppState>,
) -> Result<Json<Vec<BlacklistEntry>>, BlacklistApiError> {
    s.blacklist.list().map(Json).map_err(BlacklistApiError)
}
async fn add_blacklist(
    State(s): State<AppState>,
    Json(body): Json<BlacklistRequest>,
) -> Result<Json<BlacklistEntry>, BlacklistApiError> {
    s.blacklist
        .add(&body.address, &body.reason)
        .map(Json)
        .map_err(BlacklistApiError)
}
async fn remove_blacklist(
    State(s): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<RemovedBlacklistEntry>, BlacklistApiError> {
    s.blacklist
        .remove(&address)
        .map(|removed| Json(RemovedBlacklistEntry { removed }))
        .map_err(BlacklistApiError)
}
#[derive(Serialize)]
struct RemovedBlacklistEntry {
    removed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRestrictionRequest {
    client_id: String,
    reason: String,
}
async fn list_account_restrictions(
    State(s): State<AppState>,
) -> Result<Json<Vec<ClientAccountRestriction>>, AccountRestrictionApiError> {
    s.account_restrictions
        .list()
        .map(Json)
        .map_err(AccountRestrictionApiError)
}
async fn block_client_account(
    State(s): State<AppState>,
    Json(body): Json<AccountRestrictionRequest>,
) -> Result<Json<ClientAccountRestriction>, AccountRestrictionApiError> {
    s.account_restrictions
        .block(&body.client_id, &body.reason)
        .map(Json)
        .map_err(AccountRestrictionApiError)
}
async fn unblock_client_account(
    State(s): State<AppState>,
    Path(client_id): Path<String>,
) -> Result<Json<RemovedBlacklistEntry>, AccountRestrictionApiError> {
    s.account_restrictions
        .unblock(&client_id)
        .map(|removed| Json(RemovedBlacklistEntry { removed }))
        .map_err(AccountRestrictionApiError)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeeSweepRequest {
    operation_id: String,
}

async fn execute_fee_sweep(
    State(state): State<AppState>,
    Json(body): Json<FeeSweepRequest>,
) -> Result<Json<FeeSweep>, FeeSweepApiError> {
    state
        .fee_sweeps
        .execute(&body.operation_id)
        .await
        .map(Json)
        .map_err(FeeSweepApiError)
}
#[derive(Deserialize)]
struct StatementQuery {
    from: String,
    to: String,
}
async fn client_statement(
    State(s): State<AppState>,
    Path(client): Path<String>,
    Query(query): Query<StatementQuery>,
) -> Result<Json<ClientStatement>, StatementApiError> {
    s.statements
        .generate(&client, &query.from, &query.to)
        .map(Json)
        .map_err(StatementApiError)
}
async fn token_information(
    State(s): State<AppState>,
) -> Result<Json<TokenInformation>, PublicInfoApiError> {
    s.public_info
        .information()
        .await
        .map(Json)
        .map_err(PublicInfoApiError)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplenishmentRequest {
    operation_id: String,
    amount_usd_minor: u64,
}
async fn replenish(
    State(s): State<AppState>,
    Json(body): Json<ReplenishmentRequest>,
) -> Result<Json<InventoryOperation>, InventoryApiError> {
    s.inventory
        .execute(&body.operation_id, body.amount_usd_minor)
        .await
        .map(Json)
        .map_err(InventoryApiError)
}
async fn replenishments(
    State(s): State<AppState>,
) -> Result<Json<Vec<InventoryOperation>>, InventoryApiError> {
    s.inventory.list().map(Json).map_err(InventoryApiError)
}
async fn rebalance_plan(
    State(s): State<AppState>,
) -> Result<Json<RebalancingPlan>, InventoryApiError> {
    let balances = s
        .service
        .balances()
        .await
        .map_err(|error| InventoryApiError(InventoryError::from(error)))?;
    rebalancing_plan(&balances)
        .map(Json)
        .map_err(InventoryApiError)
}
async fn execute_rebalancing(
    State(s): State<AppState>,
) -> Result<Json<RebalancingResult>, InventoryApiError> {
    s.rebalancing
        .execute()
        .await
        .map(Json)
        .map_err(InventoryApiError)
}
#[derive(Deserialize)]
struct DailyReportQuery {
    from: String,
    to: String,
}
async fn daily_report(
    State(state): State<AppState>,
    Query(query): Query<DailyReportQuery>,
) -> Result<Json<DailyTransactionReport>, ReportingApiError> {
    state
        .reporting
        .daily(&query.from, &query.to)
        .map(Json)
        .map_err(ReportingApiError)
}
async fn get_reconciliation(
    State(state): State<AppState>,
) -> Result<Json<ReconciliationSnapshot>, ApiError> {
    state
        .reconciliation
        .current()
        .map(Json)
        .map_err(|error| ApiError(BootstrapError::Reconciliation(error.to_string())))
}
async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}
async fn execute(State(s): State<AppState>) -> Result<Json<BootstrapOperation>, ApiError> {
    let operation = s.service.execute().await.map_err(ApiError)?;
    s.retail
        .activate_bootstrap_inventory(crate::application::PURCHASE_TOKEN_RAW)
        .map_err(|error| ApiError(BootstrapError::Storage(error.to_string())))?;
    Ok(Json(operation))
}
async fn operation(State(s): State<AppState>) -> Result<Json<BootstrapOperation>, ApiError> {
    s.service
        .operation()
        .map_err(ApiError)?
        .map(Json)
        .ok_or(ApiError(BootstrapError::NotStarted))
}
async fn wallets(State(s): State<AppState>) -> Result<Json<WalletBalances>, ApiError> {
    s.service.balances().await.map(Json).map_err(ApiError)
}
async fn accounts(State(s): State<AppState>) -> Result<Json<Vec<ClientAccount>>, RetailApiError> {
    s.retail.accounts().map(Json).map_err(RetailApiError)
}
async fn account(
    State(s): State<AppState>,
    Path(client): Path<String>,
) -> Result<Json<ClientAccount>, RetailApiError> {
    s.retail.account(&client).map(Json).map_err(RetailApiError)
}
async fn records(
    State(s): State<AppState>,
    Path(client): Path<String>,
) -> Result<Json<Vec<ServiceRecord>>, RetailApiError> {
    s.retail.records(&client).map(Json).map_err(RetailApiError)
}
async fn fee_position(State(s): State<AppState>) -> Result<Json<FeePosition>, RetailApiError> {
    s.retail.fee_position().map(Json).map_err(RetailApiError)
}
async fn exchange_rate(State(s): State<AppState>) -> Result<Json<ExchangeRate>, RetailApiError> {
    s.retail.exchange_rate().map(Json).map_err(RetailApiError)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeRateRequest {
    usd_minor_per_rusd: u64,
}
async fn set_exchange_rate(
    State(s): State<AppState>,
    Json(body): Json<ExchangeRateRequest>,
) -> Result<Json<ExchangeRate>, RetailApiError> {
    s.retail
        .set_exchange_rate(body.usd_minor_per_rusd)
        .map(Json)
        .map_err(RetailApiError)
}
async fn all_records(
    State(s): State<AppState>,
) -> Result<Json<Vec<ServiceRecord>>, RetailApiError> {
    s.retail.all_records().map(Json).map_err(RetailApiError)
}
async fn amendments(
    State(s): State<AppState>,
) -> Result<Json<Vec<crate::retail::ServiceRecordAmendment>>, RetailApiError> {
    s.retail.amendments().map(Json).map_err(RetailApiError)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AmendmentRequest {
    original_record_id: String,
    amendment_type: String,
    reason: String,
}
async fn amend_record(
    State(s): State<AppState>,
    Json(body): Json<AmendmentRequest>,
) -> Result<Json<crate::retail::ServiceRecordAmendment>, RetailApiError> {
    s.retail
        .amend_record(&body.original_record_id, &body.amendment_type, &body.reason)
        .map(Json)
        .map_err(RetailApiError)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseRequest {
    operation_id: String,
    amount_usd_minor: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RedemptionRequest {
    operation_id: String,
    token_amount_raw: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferRequest {
    operation_id: String,
    recipient_client_id: String,
    token_amount_raw: u64,
    purpose_classification: String,
}
async fn purchase(
    State(s): State<AppState>,
    Path(client): Path<String>,
    Json(body): Json<PurchaseRequest>,
) -> Result<Json<RetailOrder>, RetailApiError> {
    s.retail
        .purchase(&client, &body.operation_id, body.amount_usd_minor)
        .await
        .map(Json)
        .map_err(RetailApiError)
}
async fn sale(
    State(s): State<AppState>,
    Path(client): Path<String>,
    Json(body): Json<RedemptionRequest>,
) -> Result<Json<RetailOrder>, RetailApiError> {
    s.retail
        .sale(&client, &body.operation_id, body.token_amount_raw)
        .await
        .map(Json)
        .map_err(RetailApiError)
}
async fn redemption(
    State(s): State<AppState>,
    Path(client): Path<String>,
    Json(body): Json<RedemptionRequest>,
) -> Result<Json<RetailOrder>, RetailApiError> {
    s.retail
        .redeem(&client, &body.operation_id, body.token_amount_raw)
        .await
        .map(Json)
        .map_err(RetailApiError)
}
async fn transfer(
    State(s): State<AppState>,
    Path(sender): Path<String>,
    Json(body): Json<TransferRequest>,
) -> Result<Json<InternalTransfer>, RetailApiError> {
    s.retail
        .transfer(
            &sender,
            &body.recipient_client_id,
            &body.operation_id,
            body.token_amount_raw,
            &body.purpose_classification,
        )
        .await
        .map(Json)
        .map_err(RetailApiError)
}
#[derive(Serialize)]
struct Health {
    status: &'static str,
}
#[derive(Serialize)]
struct ErrorBody {
    error: String,
}
struct ApiError(BootstrapError);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            BootstrapError::NotStarted => StatusCode::NOT_FOUND,
            BootstrapError::Reconciliation(_) => StatusCode::CONFLICT,
            BootstrapError::IssuanceBlocked => StatusCode::CONFLICT,
            BootstrapError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_GATEWAY,
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
struct RetailApiError(RetailError);
impl IntoResponse for RetailApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            RetailError::Invalid(_) => StatusCode::BAD_REQUEST,
            RetailError::IdempotencyConflict
            | RetailError::InsufficientInventory
            | RetailError::InsufficientBalance => StatusCode::CONFLICT,
            RetailError::BlacklistedAddress(_) => StatusCode::FORBIDDEN,
            RetailError::AccountRestricted(_) => StatusCode::FORBIDDEN,
            RetailError::TokenWindDown => StatusCode::CONFLICT,
            RetailError::Issuer(_) => StatusCode::BAD_GATEWAY,
            RetailError::Reconciliation(_) => StatusCode::SERVICE_UNAVAILABLE,
            RetailError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
struct AccountRestrictionApiError(AccountRestrictionError);
impl IntoResponse for AccountRestrictionApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            AccountRestrictionError::Invalid(_) => StatusCode::BAD_REQUEST,
            AccountRestrictionError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
struct BlacklistApiError(BlacklistError);
impl IntoResponse for BlacklistApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            BlacklistError::Invalid(_) => StatusCode::BAD_REQUEST,
            BlacklistError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
struct ExternalWithdrawalApiError(ExternalWithdrawalError);
impl IntoResponse for ExternalWithdrawalApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ExternalWithdrawalError::Invalid(_) => StatusCode::BAD_REQUEST,
            ExternalWithdrawalError::BlacklistedAddress(_) => StatusCode::FORBIDDEN,
            ExternalWithdrawalError::AccountRestricted(_) => StatusCode::FORBIDDEN,
            ExternalWithdrawalError::IdempotencyConflict
            | ExternalWithdrawalError::InsufficientBalance
            | ExternalWithdrawalError::InsufficientHotWalletBalance => StatusCode::CONFLICT,
            ExternalWithdrawalError::Wallet(_) => StatusCode::BAD_GATEWAY,
            ExternalWithdrawalError::Reconciliation(_) => StatusCode::SERVICE_UNAVAILABLE,
            ExternalWithdrawalError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

struct FeeSweepApiError(FeeSweepError);
impl IntoResponse for FeeSweepApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            FeeSweepError::InvalidOperationId => StatusCode::BAD_REQUEST,
            FeeSweepError::NoPendingFees
            | FeeSweepError::IdempotencyConflict
            | FeeSweepError::InsufficientHotBalance => StatusCode::CONFLICT,
            FeeSweepError::Wallet(_) | FeeSweepError::Reconciliation(_) => StatusCode::BAD_GATEWAY,
            FeeSweepError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
struct ReportingApiError(ReportingError);
impl IntoResponse for ReportingApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ReportingError::InvalidDateRange => StatusCode::BAD_REQUEST,
            ReportingError::Overflow | ReportingError::Storage(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

struct InventoryApiError(InventoryError);
impl IntoResponse for InventoryApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            InventoryError::Invalid(_) => StatusCode::BAD_REQUEST,
            InventoryError::IdempotencyConflict
            | InventoryError::Reconciliation(_)
            | InventoryError::Bootstrap(BootstrapError::IssuanceBlocked) => StatusCode::CONFLICT,
            InventoryError::Bootstrap(_) => StatusCode::BAD_GATEWAY,
            InventoryError::Overflow | InventoryError::Storage(_) | InventoryError::Ledger(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
struct PublicInfoApiError(PublicInfoError);
impl IntoResponse for PublicInfoApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_GATEWAY,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
struct StatementApiError(StatementError);
impl IntoResponse for StatementApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            StatementError::InvalidClient | StatementError::InvalidRange => StatusCode::BAD_REQUEST,
            StatementError::Overflow | StatementError::Storage(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
