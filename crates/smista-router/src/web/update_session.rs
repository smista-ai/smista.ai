//! `PUT /api/v1/sessions/{session_id}` — update a session's title or archive it.
//!
//! Takes a partial
//! [`UpdateSessionRequest`](smista_core::api::UpdateSessionRequest) and returns
//! the updated session summary.
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `PUT /api/v1/sessions/{session_id}`.
pub(crate) async fn update_session() -> WebError {
    WebError::not_implemented()
}
