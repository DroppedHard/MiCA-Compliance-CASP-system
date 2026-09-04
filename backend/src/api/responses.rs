//! Wspólne odpowiedzi i mapowanie błędów warstwy HTTP.

use crate::{
    account_restrictions::AccountRestrictionError, application::BootstrapError,
    blacklist::BlacklistError, external_withdrawals::ExternalWithdrawalError,
    fee_sweep::FeeSweepError, inventory::InventoryError, public_info::PublicInfoError,
    reporting::ReportingError, retail_application::RetailError, statements::StatementError,
};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct ErrorBody {
    pub(super) error: String,
}

macro_rules! api_error {
    ($name:ident, $inner:ty, $status:expr) => {
        pub(super) struct $name(pub(super) $inner);

        impl IntoResponse for $name {
            fn into_response(self) -> Response {
                let status = ($status)(&self.0);
                (
                    status,
                    Json(ErrorBody {
                        error: self.0.to_string(),
                    }),
                )
                    .into_response()
            }
        }
    };
}

api_error!(
    ApiError,
    BootstrapError,
    |error: &BootstrapError| match error {
        BootstrapError::NotStarted => StatusCode::NOT_FOUND,
        BootstrapError::Reconciliation(_) | BootstrapError::IssuanceBlocked => StatusCode::CONFLICT,
        BootstrapError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_GATEWAY,
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, response::IntoResponse};

    #[tokio::test]
    async fn uncertain_external_withdrawal_maps_to_conflict_with_actionable_polish_message() {
        let response = ExternalWithdrawalApiError(ExternalWithdrawalError::SubmissionUncertain(
            "brak potwierdzenia z dostawcy RPC".into(),
        ))
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(value["error"].as_str().unwrap().contains("niejednoznaczny"));
    }
}

api_error!(
    RetailApiError,
    RetailError,
    |error: &RetailError| match error {
        RetailError::Invalid(_) => StatusCode::BAD_REQUEST,
        RetailError::IdempotencyConflict
        | RetailError::InsufficientInventory
        | RetailError::InsufficientBalance
        | RetailError::TokenWindDown => StatusCode::CONFLICT,
        RetailError::BlacklistedAddress(_) | RetailError::AccountRestricted(_) =>
            StatusCode::FORBIDDEN,
        RetailError::Issuer(_) => StatusCode::BAD_GATEWAY,
        RetailError::Reconciliation(_) => StatusCode::SERVICE_UNAVAILABLE,
        RetailError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
);

api_error!(
    AccountRestrictionApiError,
    AccountRestrictionError,
    |error: &AccountRestrictionError| match error {
        AccountRestrictionError::Invalid(_) => StatusCode::BAD_REQUEST,
        AccountRestrictionError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
);

api_error!(
    BlacklistApiError,
    BlacklistError,
    |error: &BlacklistError| match error {
        BlacklistError::Invalid(_) => StatusCode::BAD_REQUEST,
        BlacklistError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
);

api_error!(
    ExternalWithdrawalApiError,
    ExternalWithdrawalError,
    |error: &ExternalWithdrawalError| match error {
        ExternalWithdrawalError::Invalid(_) => StatusCode::BAD_REQUEST,
        ExternalWithdrawalError::BlacklistedAddress(_)
        | ExternalWithdrawalError::AccountRestricted(_) => StatusCode::FORBIDDEN,
        ExternalWithdrawalError::IdempotencyConflict
        | ExternalWithdrawalError::InsufficientBalance
        | ExternalWithdrawalError::InsufficientHotWalletBalance
        | ExternalWithdrawalError::SubmissionUncertain(_) => StatusCode::CONFLICT,
        ExternalWithdrawalError::Wallet(_) => StatusCode::BAD_GATEWAY,
        ExternalWithdrawalError::Reconciliation(_) => StatusCode::SERVICE_UNAVAILABLE,
        ExternalWithdrawalError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
);

api_error!(
    FeeSweepApiError,
    FeeSweepError,
    |error: &FeeSweepError| match error {
        FeeSweepError::InvalidOperationId => StatusCode::BAD_REQUEST,
        FeeSweepError::NoPendingFees
        | FeeSweepError::IdempotencyConflict
        | FeeSweepError::InsufficientHotBalance => StatusCode::CONFLICT,
        FeeSweepError::Wallet(_) | FeeSweepError::Reconciliation(_) => StatusCode::BAD_GATEWAY,
        FeeSweepError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
);

api_error!(
    ReportingApiError,
    ReportingError,
    |error: &ReportingError| match error {
        ReportingError::InvalidDateRange => StatusCode::BAD_REQUEST,
        ReportingError::Overflow | ReportingError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
);

api_error!(
    InventoryApiError,
    InventoryError,
    |error: &InventoryError| match error {
        InventoryError::Invalid(_) => StatusCode::BAD_REQUEST,
        InventoryError::IdempotencyConflict
        | InventoryError::Reconciliation(_)
        | InventoryError::Bootstrap(BootstrapError::IssuanceBlocked) => StatusCode::CONFLICT,
        InventoryError::Bootstrap(_) => StatusCode::BAD_GATEWAY,
        InventoryError::Overflow | InventoryError::Storage(_) | InventoryError::Ledger(_) =>
            StatusCode::INTERNAL_SERVER_ERROR,
    }
);

api_error!(
    PublicInfoApiError,
    PublicInfoError,
    |_error: &PublicInfoError| StatusCode::BAD_GATEWAY
);

api_error!(
    StatementApiError,
    StatementError,
    |error: &StatementError| match error {
        StatementError::InvalidClient | StatementError::InvalidRange => StatusCode::BAD_REQUEST,
        StatementError::Overflow | StatementError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
);
