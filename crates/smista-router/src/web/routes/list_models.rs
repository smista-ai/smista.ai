//! `GET /api/v1/llm/models` — list available models.
//!
//! Returns each available model as a full descriptor, plus the providers that
//! could not be listed, as a
//! [`ListModelsResponse`](smista_core::api::ListModelsResponse).
//!
//! Scaffolded; the handler is implemented in the providers issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/llm/models`.
pub(crate) async fn list_models() -> WebError {
    WebError::not_implemented()
}
