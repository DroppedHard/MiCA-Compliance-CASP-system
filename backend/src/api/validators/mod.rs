use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Query, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

pub(super) trait Validate {
    fn validate(&self) -> Result<(), String>;
}

pub(super) struct ValidatedJson<T>(pub(super) T);
pub(super) struct ValidatedQuery<T>(pub(super) T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state)
            .await
            .map_err(IntoResponse::into_response)?;
        value.validate().map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error })),
            )
                .into_response()
        })?;
        Ok(Self(value))
    }
}

impl<S, T> FromRequestParts<S> for ValidatedQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        value.validate().map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error })),
            )
                .into_response()
        })?;
        Ok(Self(value))
    }
}

pub(super) fn required(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} nie może być puste"))
    } else {
        Ok(())
    }
}

pub(super) fn positive(value: u64, field: &str) -> Result<(), String> {
    if value == 0 {
        Err(format!("{field} musi być większe od zera"))
    } else {
        Ok(())
    }
}

pub(super) fn iso_date(value: &str, field: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let is_valid = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if is_valid {
        Ok(())
    } else {
        Err(format!("{field} musi mieć format RRRR-MM-DD"))
    }
}

#[cfg(test)]
mod tests {
    use super::iso_date;

    #[test]
    fn accepts_canonical_iso_date() {
        assert!(iso_date("2026-09-04", "from").is_ok());
    }

    #[test]
    fn rejects_non_canonical_date() {
        assert!(iso_date("04.09.2026", "from").is_err());
    }
}
