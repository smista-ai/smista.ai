//! `GET /status` — public, unauthenticated health check.
//!
//! Reports that the service is up together with its version, taken from the
//! crate's `CARGO_PKG_VERSION` at compile time. This endpoint sits at the root,
//! outside `/api/v1`, and requires no authentication.

use axum::Json;
use serde::Serialize;

/// The running service version, from the crate's `CARGO_PKG_VERSION`.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Body of a successful `GET /status` response.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusResponse {
    /// Service liveness indicator; always `"ok"` when the server answers.
    pub(crate) status: &'static str,
    /// The running service version.
    pub(crate) version: &'static str,
}

/// Handles `GET /status`.
pub(crate) async fn status() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok",
        version: VERSION,
    })
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
