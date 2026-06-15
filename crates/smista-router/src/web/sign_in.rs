//! `POST /api/v1/auth/sign-in` — exchange an API key for a session token.
//!
//! Public endpoint. Takes a [`SignInRequest`](smista_core::api::SignInRequest)
//! plus the `X-Smista-Api-Key` header and returns a
//! [`SignInResponse`](smista_core::api::SignInResponse) with a short-lived token
//! and its expiry.
//!
//! Scaffolded; the handler is implemented in the auth issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/auth/sign-in`.
pub(crate) async fn sign_in() -> WebError {
    WebError::not_implemented()
}
