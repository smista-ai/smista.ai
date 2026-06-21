//! `POST /api/v1/sessions/{session_id}/preview` — preview a route.
//!
//! Takes the same body as `/execute` but never calls the model. Returns a
//! [`PreviewResponse`](smista_core::api::PreviewResponse) with the chosen
//! provider/model, matched rule, context and estimated cost.
//!
//! Scaffolded; the handler is implemented in the execution issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/preview`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions/{session_id}/preview",
        operation_id = "previewTurn",
        tag = "execution",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        request_body = smista_core::api::ExecuteRequest,
        responses(
            (status = 200, description = "Routing preview without model invocation", body = smista_core::api::PreviewResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 422, description = "Routing rejected the request", body = smista_core::api::ApiError),
            (status = 503, description = "Provider credentials missing or fallbacks exhausted", body = smista_core::api::ApiError),
            (status = 502, description = "Provider error", body = smista_core::api::ApiError),
            (status = 504, description = "Provider timed out", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn preview() -> WebError {
    WebError::not_implemented()
}
