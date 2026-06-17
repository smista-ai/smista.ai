//! Structured request logging.
//!
//! [`log_requests`] emits one event per request with the method, path, response
//! status and latency. It deliberately logs neither the query string nor any
//! header, so tokens, API keys and provider credentials cannot leak into the
//! logs.

use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Logs the outcome of every request without leaking secrets.
pub(crate) async fn log_requests(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    // Path only: the query string may carry secrets and must never be logged.
    let path = request.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status();
    let latency_ms = start.elapsed().as_millis();
    tracing::info!(
        http.request.method = %method,
        url.path = %path,
        http.response.status_code = status.as_u16(),
        http.server.duration_ms = latency_ms,
        "handled request"
    );

    response
}
