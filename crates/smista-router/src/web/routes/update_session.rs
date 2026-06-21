//! `PUT /api/v1/sessions/{session_id}` — update a session's title or archive it.
//!
//! Takes a partial
//! [`UpdateSessionRequest`](smista_core::api::UpdateSessionRequest) and returns
//! the updated session summary.
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `PUT /api/v1/sessions/{session_id}`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        put,
        path = "/api/v1/sessions/{session_id}",
        operation_id = "updateSession",
        tag = "sessions",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id")
        ),
        request_body = smista_core::api::UpdateSessionRequest,
        responses(
            (status = 200, description = "Updated session summary", body = smista_core::api::SessionSummary),
            (status = 400, description = "Invalid request", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn update_session() -> WebError {
    WebError::not_implemented()
}
