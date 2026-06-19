//! `POST /api/v1/sessions/{session_id}/execute` — run a task.
//!
//! Takes an [`ExecuteRequest`](smista_core::api::ExecuteRequest), routes it
//! deterministically, calls the selected model and returns an
//! [`TurnResponse`](smista_core::api::TurnResponse) with the routing
//! explanation.
//!
//! Scaffolded; the handler is implemented in the execution issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/execute`.
pub(crate) async fn execute() -> WebError {
    WebError::not_implemented()
}
