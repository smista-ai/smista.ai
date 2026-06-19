//! Query-parameter credential guard.
//!
//! [`reject_query_credentials`] refuses any request that smuggles a credential
//! through a query parameter. Credentials travel only in headers; accepting them
//! in the URL would expose them in logs, proxies and browser history.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use smista_core::api::ApiErrorCode;

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

/// Rejects any request that carries a credential as a query parameter.
pub(crate) async fn reject_query_credentials(request: Request, next: Next) -> Response {
    if let Some(query) = request.uri().query()
        && query_has_credential(query)
    {
        return WebError::from_code(
            ApiErrorCode::CredentialsInQuery,
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
        assert_eq!(body["error"]["code"], "credentials_in_query");
    }
}
