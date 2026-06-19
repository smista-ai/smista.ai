//! `POST /api/v1/sessions/{session_id}/continue` — advance an in-flight run.
//!
//! Takes a [`ContinueRequest`](smista_core::api::ContinueRequest) — tool
//! results, approval decisions, queued user input or an interrupt — and returns
//! the next [`TurnResponse`](smista_core::api::TurnResponse), buffered or
//! streamed by the `Accept` header. It replaces the standalone approval
//! endpoint.
//!
//! Scaffolded; the handler is implemented in the execution flow.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/continue`.
pub(crate) async fn continue_run() -> WebError {
    WebError::not_implemented()
}
