//! `GET /api/v1/sessions/{session_id}/traces/{trace_id}` — fetch a trace by id.
//!
//! Returns the trace with the given id as a
//! [`TraceResponse`](smista_core::api::TraceResponse).
//!
//! Scaffolded; the handler is implemented in the traces issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}/traces/{trace_id}`.
pub(crate) async fn get_trace() -> WebError {
    WebError::not_implemented()
}
