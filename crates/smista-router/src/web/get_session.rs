//! `GET /api/v1/sessions/{session_id}` — fetch or resume a session.
//!
//! Returns the full session, including its messages and metadata, as a
//! [`GetSessionResponse`](smista_core::api::GetSessionResponse).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}`.
pub(crate) async fn get_session() -> WebError {
    WebError::not_implemented()
}
