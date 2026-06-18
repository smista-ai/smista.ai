//! `GET /api/v1/sessions` — list the caller's sessions, archived ones included.
//!
//! Returns each session as a
//! [`SessionSummary`](smista_core::api::SessionSummary).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions`.
pub(crate) async fn list_sessions() -> WebError {
    WebError::not_implemented()
}
