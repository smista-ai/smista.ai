//! `GET /api/v1/sessions/{session_id}/traces/latest` — fetch the latest trace.
//!
//! Returns the most recent trace for the session as a
//! [`TraceResponse`](smista_core::api::TraceResponse).
//!
//! Scaffolded; the handler is implemented in the traces issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}/traces/latest`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}/traces/latest",
        operation_id = "getLatestTrace",
        tag = "traces",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        responses(
            (status = 200, description = "Latest routing trace for the session", body = smista_core::api::TraceResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session or trace not found", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn latest_trace() -> WebError {
    WebError::not_implemented()
}
