use crate::{
    external_withdrawals::ExternalWithdrawal,
    retail::{ClientAccount, InternalTransfer, RetailOrder, ServiceRecord},
    statements::ClientStatement,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;

use crate::api::{
    requests::{ExternalWithdrawalRequest, PurchaseRequest, TokenAmountRequest, TransferRequest},
    responses::{ExternalWithdrawalApiError, RetailApiError, StatementApiError},
    state::AppState,
    validators::{Validate, ValidatedJson, ValidatedQuery, iso_date},
};

#[derive(Deserialize)]
pub(crate) struct StatementQuery {
    pub(super) from: String,
    pub(super) to: String,
}
impl Validate for StatementQuery {
    fn validate(&self) -> Result<(), String> {
        iso_date(&self.from, "from")?;
        iso_date(&self.to, "to")
    }
}

pub(crate) async fn accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClientAccount>>, RetailApiError> {
    state.retail.accounts().map(Json).map_err(RetailApiError)
}

pub(crate) async fn account(
    State(state): State<AppState>,
    Path(client): Path<String>,
) -> Result<Json<ClientAccount>, RetailApiError> {
    state
        .retail
        .account(&client)
        .map(Json)
        .map_err(RetailApiError)
}

pub(crate) async fn records(
    State(state): State<AppState>,
    Path(client): Path<String>,
) -> Result<Json<Vec<ServiceRecord>>, RetailApiError> {
    state
        .retail
        .records(&client)
        .map(Json)
        .map_err(RetailApiError)
}

pub(crate) async fn purchase(
    State(state): State<AppState>,
    Path(client): Path<String>,
    ValidatedJson(body): ValidatedJson<PurchaseRequest>,
) -> Result<Json<RetailOrder>, RetailApiError> {
    state
        .retail
        .purchase(&client, &body.operation_id, body.amount_usd_minor)
        .await
        .map(Json)
        .map_err(RetailApiError)
}

pub(crate) async fn sale(
    State(state): State<AppState>,
    Path(client): Path<String>,
    ValidatedJson(body): ValidatedJson<TokenAmountRequest>,
) -> Result<Json<RetailOrder>, RetailApiError> {
    state
        .retail
        .sale(&client, &body.operation_id, body.token_amount_raw)
        .await
        .map(Json)
        .map_err(RetailApiError)
}

pub(crate) async fn redemption(
    State(state): State<AppState>,
    Path(client): Path<String>,
    ValidatedJson(body): ValidatedJson<TokenAmountRequest>,
) -> Result<Json<RetailOrder>, RetailApiError> {
    state
        .retail
        .redeem(&client, &body.operation_id, body.token_amount_raw)
        .await
        .map(Json)
        .map_err(RetailApiError)
}

pub(crate) async fn transfer(
    State(state): State<AppState>,
    Path(sender): Path<String>,
    ValidatedJson(body): ValidatedJson<TransferRequest>,
) -> Result<Json<InternalTransfer>, RetailApiError> {
    state
        .retail
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

pub(crate) async fn external_withdrawal(
    State(state): State<AppState>,
    Path(client): Path<String>,
    ValidatedJson(body): ValidatedJson<ExternalWithdrawalRequest>,
) -> Result<Json<ExternalWithdrawal>, ExternalWithdrawalApiError> {
    state
        .withdrawals
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

pub(crate) async fn client_statement(
    State(state): State<AppState>,
    Path(client): Path<String>,
    ValidatedQuery(query): ValidatedQuery<StatementQuery>,
) -> Result<Json<ClientStatement>, StatementApiError> {
    state
        .statements
        .generate(&client, &query.from, &query.to)
        .map(Json)
        .map_err(StatementApiError)
}
