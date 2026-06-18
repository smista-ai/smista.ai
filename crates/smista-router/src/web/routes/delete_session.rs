//! `DELETE /api/v1/sessions/{session_id}` — delete a session.
//!
//! Deletes the session and its context memory, returning a
//! [`DeleteSessionResponse`](smista_core::api::DeleteSessionResponse).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `DELETE /api/v1/sessions/{session_id}`.
pub(crate) async fn delete_session() -> WebError {
    WebError::not_implemented()
}
