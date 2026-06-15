//! `POST /api/v1/auth/sign-out` — revoke the current session token.
//!
//! Returns a [`SignOutResponse`](smista_core::api::SignOutResponse) confirming
//! the token was revoked.
//!
//! Scaffolded; the handler is implemented in the auth issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/auth/sign-out`.
pub(crate) async fn sign_out() -> WebError {
    WebError::not_implemented()
}
