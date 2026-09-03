use crate::{
    public_info::TokenInformation, reporting::DailyTransactionReport, retail::ExchangeRate,
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::api::{
    responses::{PublicInfoApiError, ReportingApiError, RetailApiError},
    state::AppState,
    validators::{Validate, ValidatedQuery, iso_date},
};

#[derive(Deserialize)]
pub(crate) struct DailyReportQuery {
    pub(super) from: String,
    pub(super) to: String,
}
impl Validate for DailyReportQuery {
    fn validate(&self) -> Result<(), String> {
        iso_date(&self.from, "from")?;
        iso_date(&self.to, "to")
    }
}

#[derive(Serialize)]
pub(crate) struct Health {
    pub(super) status: &'static str,
}

pub(crate) async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

pub(crate) async fn daily_report(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<DailyReportQuery>,
) -> Result<Json<DailyTransactionReport>, ReportingApiError> {
    state
        .reporting
        .daily(&query.from, &query.to)
        .map(Json)
        .map_err(ReportingApiError)
}

pub(crate) async fn token_information(
    State(state): State<AppState>,
) -> Result<Json<TokenInformation>, PublicInfoApiError> {
    state
        .public_info
        .information()
        .await
        .map(Json)
        .map_err(PublicInfoApiError)
}

pub(crate) async fn exchange_rate(
    State(state): State<AppState>,
) -> Result<Json<ExchangeRate>, RetailApiError> {
    state
        .retail
        .exchange_rate()
        .map(Json)
        .map_err(RetailApiError)
}
