//! The web layer's error response.
//!
//! [`WebError`] is a thin newtype over [`ApiErrorResponse`] — the single source
//! of truth for the domain-error-to-wire mapping in `smista-core` — that adds
//! the `axum` [`IntoResponse`] glue. Handlers return `Result<_, WebError>` (or a
//! `WebError` directly) and the status code plus the structured JSON body flow
//! straight through.
//!
//! Error bodies never carry secrets: the underlying [`ApiErrorBody`] is built
//! from already-redacted domain errors.
//!
//! [`ApiErrorBody`]: smista_core::api::ApiErrorBody

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use smista_core::api::ApiErrorResponse;
use smista_core::error::CoreError;

/// An error rendered as a structured JSON response.
#[derive(Debug, Clone)]
pub(crate) struct WebError(ApiErrorResponse);

impl WebError {
    /// Builds a [`WebError`] with the given status, machine-readable code and
    /// human-readable message, and no details.
    pub(crate) fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self(ApiErrorResponse::new(status, code, message))
    }

    /// Builds the placeholder error returned by endpoints that are scaffolded
    /// but not yet implemented.
    pub(crate) fn not_implemented() -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            "not_implemented",
            "This endpoint is not implemented yet.",
        )
    }
}

impl From<ApiErrorResponse> for WebError {
    fn from(response: ApiErrorResponse) -> Self {
        Self(response)
    }
}

impl From<CoreError> for WebError {
    fn from(error: CoreError) -> Self {
        Self(ApiErrorResponse::from(error))
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        (self.0.status, Json(self.0.body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_render_status_and_body() {
        let response =
            WebError::new(StatusCode::BAD_REQUEST, "invalid_request", "bad").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn should_build_not_implemented() {
        let response = WebError::not_implemented().into_response();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn should_map_core_error() {
        let error = WebError::from(CoreError::Internal("boom".to_string()));
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
