//! `POST /api/v1/sessions/{session_id}/execute` — run a task.
//!
//! Takes an [`ExecuteRequest`](smista_core::api::ExecuteRequest), routes it
//! deterministically, calls the selected model and returns an
//! [`ExecuteResponse`](smista_core::api::ExecuteResponse) with the routing
//! explanation.
//!
//! Scaffolded; the handler is implemented in the execution issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/execute`.
pub(crate) async fn execute() -> WebError {
    WebError::not_implemented()
}
