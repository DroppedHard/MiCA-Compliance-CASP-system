//! Deklaracje tras HTTP; bez logiki biznesowej.

use super::{handlers, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub(super) fn public() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route(
            "/api/v1/reports/daily-transactions",
            get(handlers::daily_report),
        )
        .route(
            "/api/v1/public/token-information",
            get(handlers::token_information),
        )
        .route("/api/v1/public/exchange-rate", get(handlers::exchange_rate))
}

pub(super) fn administration() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/bootstrap-inventory",
            post(handlers::execute_bootstrap).get(handlers::bootstrap_operation),
        )
        .route("/api/v1/admin/wallets", get(handlers::wallets))
        .route(
            "/api/v1/admin/reconciliation",
            get(handlers::reconciliation),
        )
        .route("/api/v1/admin/fees", get(handlers::fee_position))
        .route(
            "/api/v1/admin/exchange-rate",
            get(handlers::exchange_rate).post(handlers::set_exchange_rate),
        )
        .route(
            "/api/v1/admin/fee-sweeps",
            post(handlers::execute_fee_sweep),
        )
        .route(
            "/api/v1/admin/address-blacklist",
            get(handlers::list_blacklist).post(handlers::add_blacklist),
        )
        .route(
            "/api/v1/admin/address-blacklist/{address}",
            axum::routing::delete(handlers::remove_blacklist),
        )
        .route(
            "/api/v1/admin/client-account-restrictions",
            get(handlers::list_account_restrictions).post(handlers::block_client_account),
        )
        .route(
            "/api/v1/admin/client-account-restrictions/{client_id}",
            axum::routing::delete(handlers::unblock_client_account),
        )
        .route(
            "/api/v1/admin/inventory-replenishments",
            post(handlers::replenish).get(handlers::replenishments),
        )
        .route(
            "/api/v1/admin/rebalancing-plan",
            get(handlers::rebalance_plan),
        )
        .route(
            "/api/v1/admin/rebalancing",
            post(handlers::execute_rebalancing),
        )
        .route("/api/v1/admin/service-records", get(handlers::all_records))
        .route(
            "/api/v1/admin/service-record-amendments",
            post(handlers::amend_record).get(handlers::amendments),
        )
}

pub(super) fn customer() -> Router<AppState> {
    Router::new()
        .route("/api/v1/clients", get(handlers::accounts))
        .route(
            "/api/v1/clients/{client_id}/account",
            get(handlers::account),
        )
        .route(
            "/api/v1/clients/{client_id}/records",
            get(handlers::records),
        )
        .route(
            "/api/v1/clients/{client_id}/purchases",
            post(handlers::purchase),
        )
        .route("/api/v1/clients/{client_id}/sales", post(handlers::sale))
        .route(
            "/api/v1/clients/{client_id}/transfers",
            post(handlers::transfer),
        )
        .route(
            "/api/v1/clients/{client_id}/external-withdrawals",
            post(handlers::external_withdrawal),
        )
        .route(
            "/api/v1/clients/{client_id}/redemptions",
            post(handlers::redemption),
        )
        .route(
            "/api/v1/clients/{client_id}/statement",
            get(handlers::client_statement),
        )
}
