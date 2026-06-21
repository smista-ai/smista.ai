//! `GET /status` — public, unauthenticated health check.
//!
//! Reports that the service is up together with its version, taken from the
//! crate's `CARGO_PKG_VERSION` at compile time. This endpoint sits at the root,
//! outside `/api/v1`, and requires no authentication. The response shape is
//! [`StatusResponse`].

use axum::Json;
use axum::http::StatusCode;
use smista_core::api::StatusResponse;

use crate::web::routes::ApiResult;

/// The running service version, from the crate's `CARGO_PKG_VERSION`.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Handles `GET /status`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/status",
        operation_id = "getStatus",
        tag = "health",
        responses(
            (status = 200, description = "Router is up", body = smista_core::api::StatusResponse)
        )
    )
)]
pub(crate) async fn status() -> ApiResult<StatusResponse> {
    Ok((
        StatusCode::OK,
        Json(StatusResponse {
            status: "ok".to_string(),
            version: VERSION.to_string(),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::web::test_support::{get, send, test_router};

    #[tokio::test]
    async fn should_report_status_and_version() {
        let router = test_router().await;
        let (status, body) = send(router, get("/status")).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }
}
