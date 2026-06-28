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
use axum::response::{IntoResponse, Response};
use smista_core::api::{ApiErrorCode, ApiErrorResponse};
use smista_core::error::CoreError;

use crate::auth::AuthenticatorError;

/// An error rendered as a structured JSON response.
#[derive(Debug, Clone)]
pub(crate) struct WebError(ApiErrorResponse);

impl WebError {
    /// Builds a [`WebError`] from a typed [`ApiErrorCode`], using the code's
    /// canonical HTTP status and the given message, and no details.
    ///
    /// Codes are only ever produced through [`ApiErrorCode`] so the wire code
    /// and its status can never drift from the documented contract.
    pub(crate) fn from_code(code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self(ApiErrorResponse::from_code(code, message))
    }
}

impl From<AuthenticatorError> for WebError {
    fn from(err: AuthenticatorError) -> Self {
        // The HTTP status is derived from the code, not from `err`: a malformed
        // stored hash is a server-side fault (500), and an unknown user is
        // reported as an invalid API key so sign-in never leaks which users
        // exist.
        let (code, message) = match err {
            AuthenticatorError::ExpiredToken => {
                (ApiErrorCode::TokenExpired, "The session token has expired.")
            }
            AuthenticatorError::InternalError(_) => {
                (ApiErrorCode::InternalError, "An internal error occurred.")
            }
            AuthenticatorError::InvalidHash => {
                (ApiErrorCode::InternalError, "An internal error occurred.")
            }
            AuthenticatorError::InvalidApiKey | AuthenticatorError::UserNotFound => {
                (ApiErrorCode::InvalidApiKey, "The API key is invalid.")
            }
            AuthenticatorError::InvalidToken => {
                (ApiErrorCode::InvalidToken, "The session token is invalid.")
            }
            AuthenticatorError::RevokedToken => (
                ApiErrorCode::TokenRevoked,
                "The session token has been revoked.",
            ),
        };

        Self::from_code(code, message)
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
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn should_render_status_and_body() {
        let response = WebError::from_code(ApiErrorCode::CredentialsInQuery, "bad").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn should_map_core_error() {
        let error = WebError::from(CoreError::Internal("boom".to_string()));
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn should_map_authenticator_errors_to_codes_and_statuses() {
        let cases = [
            (
                AuthenticatorError::InvalidToken,
                ApiErrorCode::InvalidToken,
                StatusCode::UNAUTHORIZED,
            ),
            // A token known to the holder but past its expiry, or signed out, is
            // reported with its own code so the holder knows to sign in again.
            (
                AuthenticatorError::ExpiredToken,
                ApiErrorCode::TokenExpired,
                StatusCode::UNAUTHORIZED,
            ),
            (
                AuthenticatorError::RevokedToken,
                ApiErrorCode::TokenRevoked,
                StatusCode::UNAUTHORIZED,
            ),
            (
                AuthenticatorError::InvalidApiKey,
                ApiErrorCode::InvalidApiKey,
                StatusCode::UNAUTHORIZED,
            ),
            // An unknown user is reported as an invalid key, never leaking existence.
            (
                AuthenticatorError::UserNotFound,
                ApiErrorCode::InvalidApiKey,
                StatusCode::UNAUTHORIZED,
            ),
            // A malformed stored hash is a server-side fault, not a client error.
            (
                AuthenticatorError::InvalidHash,
                ApiErrorCode::InternalError,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (error, expected_code, expected_status) in cases {
            let WebError(response) = WebError::from(error);
            assert_eq!(response.status, expected_status);
            assert_eq!(response.body.error.code, expected_code.as_str());
        }
    }
}
