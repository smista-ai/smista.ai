//! `GET /api/v1/llm/providers` — list available providers.
//!
//! Returns the providers configured with usable credentials as a
//! [`ListProvidersResponse`](smista_core::api::ListProvidersResponse).
//!
//! Scaffolded; the handler is implemented in the providers issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/llm/providers`.
pub(crate) async fn list_providers() -> WebError {
    WebError::not_implemented()
}
