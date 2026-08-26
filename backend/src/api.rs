use crate::{
    application::{BootstrapError, BootstrapService},
    domain::{BootstrapOperation, WalletBalances},
    reconciliation::{ReconciliationService, ReconciliationSnapshot},
    retail::{ClientAccount, FeePosition, InternalTransfer, RetailOrder, ServiceRecord},
    retail_application::{RetailError, RetailService},
};
use axum::{
    Json, Router,
    extract::{Path, State},
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
}
pub fn router(
    service: Arc<BootstrapService>,
    retail: Arc<RetailService>,
    reconciliation: Arc<ReconciliationService>,
) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(
            "/api/v1/admin/bootstrap-inventory",
            post(execute).get(operation),
        )
        .route("/api/v1/admin/wallets", get(wallets))
        .route("/api/v1/admin/reconciliation", get(get_reconciliation))
        .route("/api/v1/admin/fees", get(fee_position))
        .route("/api/v1/clients", get(accounts))
        .route("/api/v1/clients/{client_id}/account", get(account))
        .route("/api/v1/clients/{client_id}/records", get(records))
        .route("/api/v1/clients/{client_id}/purchases", post(purchase))
        .route("/api/v1/clients/{client_id}/sales", post(sale))
        .route("/api/v1/clients/{client_id}/transfers", post(transfer))
        .route("/api/v1/clients/{client_id}/redemptions", post(redemption))
        .with_state(AppState {
            service,
            retail,
            reconciliation,
        })
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
