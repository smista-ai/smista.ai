//! Cross-cutting HTTP middleware.
//!
//! Two layers wrap every route:
//!
//! - [`log_requests`] emits one structured event per request. It logs the
//!   method, the path, the response status and the latency, and deliberately
//!   never logs the query string or any header, so tokens, API keys and
//!   provider credentials cannot leak into the logs.
//! - [`reject_query_credentials`] refuses any request that smuggles a
//!   credential through a query parameter. Credentials travel only in headers;
//!   accepting them in the URL would expose them in logs, proxies and browser
//!   history.

use std::time::Instant;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::web::error::WebError;

/// Lowercased substrings that mark a query-parameter key as a credential.
///
/// Matched as substrings so variants such as `apiKey`, `api-key` and
/// `x-smista-provider-anthropic-api-key` are all caught.
const CREDENTIAL_QUERY_MARKERS: &[&str] = &[
    "api-key",
    "api_key",
    "apikey",
    "authorization",
    "password",
    "secret",
    "token",
    "x-smista",
];

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

/// Rejects any request that carries a credential as a query parameter.
pub(crate) async fn reject_query_credentials(request: Request, next: Next) -> Response {
    if let Some(query) = request.uri().query()
        && query_has_credential(query)
    {
        return WebError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Credentials must not be passed as query parameters.",
        )
        .into_response();
    }

    next.run(request).await
}

/// Returns `true` when any key in the `&`-separated `query` looks like a
/// credential.
fn query_has_credential(query: &str) -> bool {
    query.split('&').any(|pair| {
        let key = pair.split('=').next().unwrap_or(pair).to_ascii_lowercase();
        CREDENTIAL_QUERY_MARKERS
            .iter()
            .any(|marker| key.contains(marker))
    })
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::query_has_credential;
    use crate::web::test_support::{get, send, test_router};

    #[test]
    fn should_detect_credential_query_keys() {
        assert!(query_has_credential("api_key=x"));
        assert!(query_has_credential("foo=1&apiKey=secret"));
        assert!(query_has_credential(
            "X-Smista-Provider-Anthropic-Api-Key=x"
        ));
        assert!(query_has_credential("token=x"));
    }

    #[test]
    fn should_allow_innocuous_query_keys() {
        assert!(!query_has_credential("page=2&limit=20"));
        assert!(!query_has_credential("archived=true"));
        assert!(!query_has_credential(""));
    }

    #[tokio::test]
    async fn should_reject_credentials_in_query_parameters() {
        let router = test_router().await;
        let (status, body) = send(router, get("/status?api_key=leaked")).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_request");
    }
}
