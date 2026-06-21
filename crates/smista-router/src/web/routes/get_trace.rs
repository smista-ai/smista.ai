//! `GET /api/v1/sessions/{session_id}/traces/{trace_id}` — fetch a trace by id.
//!
//! Returns the trace with the given id as a
//! [`TraceResponse`](smista_core::api::TraceResponse).
//!
//! Scaffolded; the handler is implemented in the traces issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}/traces/{trace_id}`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}/traces/{trace_id}",
        operation_id = "getTrace",
        tag = "traces",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id"),
            ("trace_id" = String, Path, description = "Trace id")
        ),
        responses(
            (status = 200, description = "Routing trace for the given id", body = smista_core::api::TraceResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session or trace not found", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn get_trace() -> WebError {
    WebError::not_implemented()
}
