//! `GET /api/v1/sessions/{session_id}/traces` — fetch a session's trace.
//!
//! Returns the session's ordered trace events, windowed by the `limit` and
//! `offset` query parameters, as a
//! [`TraceResponse`](smista_core::api::TraceResponse).
//!
//! Scaffolded; the handler is implemented in the traces issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}/traces`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}/traces",
        operation_id = "getSessionTraces",
        tag = "traces",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id"),
            ("limit" = Option<u32>, Query, description = "Maximum number of events to return"),
            ("offset" = Option<u32>, Query, description = "Number of leading events to skip")
        ),
        responses(
            (status = 200, description = "Routing trace for the session", body = smista_core::api::TraceResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn get_traces() -> WebError {
    WebError::not_implemented()
}
