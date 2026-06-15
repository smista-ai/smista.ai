//! `POST /api/v1/sessions/{session_id}/preview` — preview a route.
//!
//! Takes the same body as `/execute` but never calls the model. Returns a
//! [`PreviewResponse`](smista_core::api::PreviewResponse) with the chosen
//! provider/model, matched rule, context and estimated cost.
//!
//! Scaffolded; the handler is implemented in the execution issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/preview`.
pub(crate) async fn preview() -> WebError {
    WebError::not_implemented()
}
