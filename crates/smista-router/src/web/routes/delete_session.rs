//! `DELETE /api/v1/sessions/{session_id}` — delete a session.
//!
//! Deletes the session and its context memory, returning a
//! [`DeleteSessionResponse`](smista_core::api::DeleteSessionResponse).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `DELETE /api/v1/sessions/{session_id}`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/v1/sessions/{session_id}",
        operation_id = "deleteSession",
        tag = "sessions",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id")
        ),
        responses(
            (status = 204, description = "Session deleted"),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn delete_session() -> WebError {
    WebError::not_implemented()
}
