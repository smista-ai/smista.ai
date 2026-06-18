//! `POST /api/v1/auth/bootstrap` — create a user and issue a long-lived API key.
//!
//! Public endpoint. Returns the new user ID together with an API key shown only
//! once, as a [`BootstrapResponse`](smista_core::api::BootstrapResponse).
//!
//! Scaffolded; the handler is implemented in the auth issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/auth/bootstrap`.
pub(crate) async fn bootstrap() -> WebError {
    WebError::not_implemented()
}
