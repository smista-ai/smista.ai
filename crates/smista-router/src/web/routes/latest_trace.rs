//! `GET /api/v1/sessions/{session_id}/traces/latest` — fetch the latest trace.
//!
//! Returns the most recent trace for the session as a
//! [`TraceResponse`](smista_core::api::TraceResponse).
//!
//! Scaffolded; the handler is implemented in the traces issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}/traces/latest`.
pub(crate) async fn latest_trace() -> WebError {
    WebError::not_implemented()
}
