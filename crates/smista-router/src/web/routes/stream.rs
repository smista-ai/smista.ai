//! `POST /api/v1/sessions/{session_id}/stream` — run a task, streaming events.
//!
//! Takes the same body as `/execute` and responds with a Server-Sent Events
//! stream of [`StreamEvent`](smista_core::stream::StreamEvent) values.
//!
//! Scaffolded; the handler is implemented in the execution issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/stream`.
pub(crate) async fn stream() -> WebError {
    WebError::not_implemented()
}
