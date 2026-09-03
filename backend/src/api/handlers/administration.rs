use crate::{
    account_restrictions::ClientAccountRestriction,
    application::BootstrapError,
    blacklist::BlacklistEntry,
    domain::{BootstrapOperation, WalletBalances},
    fee_sweep::FeeSweep,
    inventory::{
        InventoryError, InventoryOperation, RebalancingPlan, RebalancingResult, rebalancing_plan,
    },
    reconciliation::ReconciliationSnapshot,
    retail::{ExchangeRate, FeePosition, ServiceRecord, ServiceRecordAmendment},
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::api::{
    requests::{
        AccountRestrictionRequest, AmendmentRequest, BlacklistRequest, ExchangeRateRequest,
        FeeSweepRequest, ReplenishmentRequest,
    },
    responses::{
        AccountRestrictionApiError, ApiError, BlacklistApiError, FeeSweepApiError,
        InventoryApiError, RetailApiError,
    },
    state::AppState,
    validators::ValidatedJson,
};

#[derive(Serialize)]
pub(crate) struct RemovedEntry {
    pub(super) removed: bool,
}

pub(crate) async fn execute_bootstrap(
    State(state): State<AppState>,
) -> Result<Json<BootstrapOperation>, ApiError> {
    let operation = state.service.execute().await.map_err(ApiError)?;
    state
        .retail
        .activate_bootstrap_inventory(crate::application::PURCHASE_TOKEN_RAW)
        .map_err(|error| ApiError(BootstrapError::Storage(error.to_string())))?;
    Ok(Json(operation))
}

pub(crate) async fn bootstrap_operation(
    State(state): State<AppState>,
) -> Result<Json<BootstrapOperation>, ApiError> {
    state
        .service
        .operation()
        .map_err(ApiError)?
        .map(Json)
        .ok_or(ApiError(BootstrapError::NotStarted))
}

pub(crate) async fn wallets(
    State(state): State<AppState>,
) -> Result<Json<WalletBalances>, ApiError> {
    state.service.balances().await.map(Json).map_err(ApiError)
}

pub(crate) async fn reconciliation(
    State(state): State<AppState>,
) -> Result<Json<ReconciliationSnapshot>, ApiError> {
    state
        .reconciliation
        .current()
        .map(Json)
        .map_err(|error| ApiError(BootstrapError::Reconciliation(error.to_string())))
}

pub(crate) async fn fee_position(
    State(state): State<AppState>,
) -> Result<Json<FeePosition>, RetailApiError> {
    state
        .retail
        .fee_position()
        .map(Json)
        .map_err(RetailApiError)
}

pub(crate) async fn set_exchange_rate(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<ExchangeRateRequest>,
) -> Result<Json<ExchangeRate>, RetailApiError> {
    state
        .retail
        .set_exchange_rate(body.usd_minor_per_rusd)
        .map(Json)
        .map_err(RetailApiError)
}

pub(crate) async fn execute_fee_sweep(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<FeeSweepRequest>,
) -> Result<Json<FeeSweep>, FeeSweepApiError> {
    state
        .fee_sweeps
        .execute(&body.operation_id)
        .await
        .map(Json)
        .map_err(FeeSweepApiError)
}

pub(crate) async fn list_blacklist(
    State(state): State<AppState>,
) -> Result<Json<Vec<BlacklistEntry>>, BlacklistApiError> {
    state.blacklist.list().map(Json).map_err(BlacklistApiError)
}

pub(crate) async fn add_blacklist(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<BlacklistRequest>,
) -> Result<Json<BlacklistEntry>, BlacklistApiError> {
    state
        .blacklist
        .add(&body.address, &body.reason)
        .map(Json)
        .map_err(BlacklistApiError)
}

pub(crate) async fn remove_blacklist(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<RemovedEntry>, BlacklistApiError> {
    state
        .blacklist
        .remove(&address)
        .map(|removed| Json(RemovedEntry { removed }))
        .map_err(BlacklistApiError)
}

pub(crate) async fn list_account_restrictions(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClientAccountRestriction>>, AccountRestrictionApiError> {
    state
        .account_restrictions
        .list()
        .map(Json)
        .map_err(AccountRestrictionApiError)
}

pub(crate) async fn block_client_account(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<AccountRestrictionRequest>,
) -> Result<Json<ClientAccountRestriction>, AccountRestrictionApiError> {
    state
        .account_restrictions
        .block(&body.client_id, &body.reason)
        .map(Json)
        .map_err(AccountRestrictionApiError)
}

pub(crate) async fn unblock_client_account(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
) -> Result<Json<RemovedEntry>, AccountRestrictionApiError> {
    state
        .account_restrictions
        .unblock(&client_id)
        .map(|removed| Json(RemovedEntry { removed }))
        .map_err(AccountRestrictionApiError)
}

pub(crate) async fn replenish(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<ReplenishmentRequest>,
) -> Result<Json<InventoryOperation>, InventoryApiError> {
    state
        .inventory
        .execute(&body.operation_id, body.amount_usd_minor)
        .await
        .map(Json)
        .map_err(InventoryApiError)
}

pub(crate) async fn replenishments(
    State(state): State<AppState>,
) -> Result<Json<Vec<InventoryOperation>>, InventoryApiError> {
    state.inventory.list().map(Json).map_err(InventoryApiError)
}

pub(crate) async fn rebalance_plan(
    State(state): State<AppState>,
) -> Result<Json<RebalancingPlan>, InventoryApiError> {
    let balances = state
        .service
        .balances()
        .await
        .map_err(|error| InventoryApiError(InventoryError::from(error)))?;
    rebalancing_plan(&balances)
        .map(Json)
        .map_err(InventoryApiError)
}

pub(crate) async fn execute_rebalancing(
    State(state): State<AppState>,
) -> Result<Json<RebalancingResult>, InventoryApiError> {
    state
        .rebalancing
        .execute()
        .await
        .map(Json)
        .map_err(InventoryApiError)
}

pub(crate) async fn all_records(
    State(state): State<AppState>,
) -> Result<Json<Vec<ServiceRecord>>, RetailApiError> {
    state.retail.all_records().map(Json).map_err(RetailApiError)
}

pub(crate) async fn amendments(
    State(state): State<AppState>,
) -> Result<Json<Vec<ServiceRecordAmendment>>, RetailApiError> {
    state.retail.amendments().map(Json).map_err(RetailApiError)
}

pub(crate) async fn amend_record(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<AmendmentRequest>,
) -> Result<Json<ServiceRecordAmendment>, RetailApiError> {
    state
        .retail
        .amend_record(&body.original_record_id, &body.amendment_type, &body.reason)
        .map(Json)
        .map_err(RetailApiError)
}
