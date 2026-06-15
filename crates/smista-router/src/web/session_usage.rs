//! `GET /api/v1/sessions/{session_id}/usage` — report session usage.
//!
//! Returns the session total plus per-model and per-task-type breakdowns as a
//! [`SessionUsageResponse`](smista_core::api::SessionUsageResponse).
//!
//! Scaffolded; the handler is implemented in the usage issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}/usage`.
pub(crate) async fn session_usage() -> WebError {
    WebError::not_implemented()
}
