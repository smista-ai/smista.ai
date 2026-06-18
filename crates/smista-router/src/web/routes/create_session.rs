//! `POST /api/v1/sessions` — create a session.
//!
//! Takes a [`CreateSessionRequest`](smista_core::api::CreateSessionRequest) and
//! returns `201` with a
//! [`CreateSessionResponse`](smista_core::api::CreateSessionResponse).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions`.
pub(crate) async fn create_session() -> WebError {
    WebError::not_implemented()
}
