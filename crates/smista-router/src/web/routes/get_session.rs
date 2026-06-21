//! `GET /api/v1/sessions/{session_id}` — fetch or resume a session.
//!
//! Returns the full session, including its messages and metadata, as a
//! [`GetSessionResponse`](smista_core::api::GetSessionResponse).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}",
        operation_id = "getSession",
        tag = "sessions",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id")
        ),
        responses(
            (status = 200, description = "Session detail", body = smista_core::api::GetSessionResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn get_session() -> WebError {
    WebError::not_implemented()
}
