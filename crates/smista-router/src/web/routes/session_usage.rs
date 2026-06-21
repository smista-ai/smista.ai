//! `GET /api/v1/sessions/{session_id}/usage` — report session usage.
//!
//! Returns the session total plus per-model and per-task-type breakdowns as a
//! [`SessionUsageResponse`](smista_core::api::SessionUsageResponse).
//!
//! Scaffolded; the handler is implemented in the usage issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/sessions/{session_id}/usage`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}/usage",
        operation_id = "getSessionUsage",
        tag = "usage",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        responses(
            (status = 200, description = "Session usage totals and breakdowns", body = smista_core::api::SessionUsageResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn session_usage() -> WebError {
    WebError::not_implemented()
}
