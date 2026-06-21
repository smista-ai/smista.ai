//! `GET /api/v1/sessions` — list the caller's sessions, archived ones included.
//!
//! Returns each session as a
//! [`SessionSummary`](smista_core::api::SessionSummary).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions",
        operation_id = "listSessions",
        tag = "sessions",
        security(("bearer" = [])),
        responses(
            (status = 200, description = "List of sessions", body = Vec<smista_core::api::SessionSummary>),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn list_sessions() -> WebError {
    WebError::not_implemented()
}
