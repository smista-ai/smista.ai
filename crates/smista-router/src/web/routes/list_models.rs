//! `GET /api/v1/llm/models` — list available models.
//!
//! Protected endpoint. The router asks every configured provider to list its
//! models, passing through the per-request
//! `X-Smista-Provider-<Provider>-Api-Key` credentials, which remote providers
//! such as Anthropic and Gemini require. Each listed model is returned as a full
//! [`ModelDescriptor`](smista_core::model::ModelDescriptor); a provider that
//! could not be listed (most often because its credentials were missing or
//! rejected) is reported under `unavailable`, all wrapped in a
//! [`ListModelsResponse`].
//!
//! The per-provider deadline defaults to [`DEFAULT_FETCH_MODELS_TIMEOUT`] and
//! can be tuned per request with the [`TIMEOUT_HEADER`] header, clamped to
//! [`MAX_FETCH_MODELS_TIMEOUT`].

use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Extension, Json};
use smista_core::api::{ListModelsResponse, UnavailableProvider};

use crate::router::FetchModelsResult;
use crate::web::AppState;
use crate::web::middleware::RequestCredentials;
use crate::web::routes::ApiResult;

/// Header carrying the per-provider model-listing deadline, in milliseconds.
const TIMEOUT_HEADER: &str = "x-smista-timeout-ms";

/// Deadline applied to every provider's model listing when the caller sends no
/// [`TIMEOUT_HEADER`], so a slow provider cannot hold the response open
/// indefinitely.
const DEFAULT_FETCH_MODELS_TIMEOUT: Duration = Duration::from_secs(10);

/// Upper bound on the caller-supplied deadline; a larger [`TIMEOUT_HEADER`]
/// value is clamped down to this, bounding how long one request can hold a
/// connection open.
const MAX_FETCH_MODELS_TIMEOUT: Duration = Duration::from_secs(60);

/// Handles `GET /api/v1/llm/models`.
///
/// The credential-extraction middleware installs the [`RequestCredentials`]
/// extension only when the caller sent at least one provider key, so the
/// extractor is optional: its absence simply means no credentials, which still
/// lists keyless local providers.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/llm/models",
        operation_id = "listModels",
        tag = "llm",
        security(("bearer" = [])),
        params(
            ("X-Smista-Timeout-Ms" = Option<u64>, Header, description = "Per-request timeout, default 10000, max 60000")
        ),
        responses(
            (status = 200, description = "Available models", body = smista_core::api::ListModelsResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
    credentials: Option<Extension<RequestCredentials>>,
) -> ApiResult<ListModelsResponse> {
    let credentials = credentials
        .map(|Extension(credentials)| credentials.credentials())
        .unwrap_or_default();
    let fetched_models: FetchModelsResult = state
        .router
        .fetch_models(credentials, resolve_timeout(&headers))
        .await;

    Ok((
        StatusCode::OK,
        Json(ListModelsResponse {
            models: fetched_models.models.into_values().collect(),
            unavailable: fetched_models
                .failed_providers
                .into_iter()
                .map(|(provider, err)| UnavailableProvider {
                    provider,
                    reason: err.category,
                    message: Some(err.message),
                })
                .collect(),
        }),
    ))
}

/// Resolves the per-provider listing deadline from the request headers.
///
/// A well-formed [`TIMEOUT_HEADER`] is honored, clamped to
/// [`MAX_FETCH_MODELS_TIMEOUT`]. A missing, non-numeric, zero or out-of-range
/// header falls back to [`DEFAULT_FETCH_MODELS_TIMEOUT`] rather than failing the
/// request, since the deadline is a tuning hint, not part of the result.
fn resolve_timeout(headers: &HeaderMap) -> Duration {
    headers
        .get(TIMEOUT_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map(Duration::from_millis)
        .map(|requested| requested.min(MAX_FETCH_MODELS_TIMEOUT))
        .unwrap_or(DEFAULT_FETCH_MODELS_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use uuid::Uuid;

    use super::{
        DEFAULT_FETCH_MODELS_TIMEOUT, Duration, MAX_FETCH_MODELS_TIMEOUT, TIMEOUT_HEADER,
        resolve_timeout,
    };
    use crate::web::test_support::{authenticated_router, get, get_with_token, send};

    /// Builds a header map carrying `value` under [`TIMEOUT_HEADER`].
    fn timeout_headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(TIMEOUT_HEADER, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn should_default_the_timeout_when_the_header_is_absent() {
        assert_eq!(
            resolve_timeout(&HeaderMap::new()),
            DEFAULT_FETCH_MODELS_TIMEOUT
        );
    }

    #[test]
    fn should_honor_a_valid_timeout_header() {
        assert_eq!(
            resolve_timeout(&timeout_headers("2500")),
            Duration::from_millis(2500)
        );
    }

    #[test]
    fn should_clamp_a_timeout_above_the_cap() {
        assert_eq!(
            resolve_timeout(&timeout_headers("120000")),
            MAX_FETCH_MODELS_TIMEOUT
        );
    }

    #[test]
    fn should_fall_back_on_a_zero_or_unparseable_timeout() {
        for value in ["0", "-5", "fast", ""] {
            assert_eq!(
                resolve_timeout(&timeout_headers(value)),
                DEFAULT_FETCH_MODELS_TIMEOUT,
                "{value} should fall back to the default timeout"
            );
        }
    }

    #[tokio::test]
    async fn should_list_the_available_models() {
        let (router, token) = authenticated_router().await;

        let (status, body) = send(router, get_with_token("/api/v1/llm/models", &token)).await;

        assert_eq!(status, StatusCode::OK);
        let models = body["models"].as_array().expect("models was not an array");
        assert_eq!(models.len(), 2);

        // The mock router exposes a local Ollama model and a remote OpenAI one;
        // each entry is a full descriptor carrying provider, model and auth.
        let local = models
            .iter()
            .find(|model| model["provider"] == "ollama")
            .expect("the ollama model was not listed");
        assert_eq!(local["model"], "mock-local");
        assert_eq!(local["local"], true);
        assert_eq!(local["auth"], "none");

        let remote = models
            .iter()
            .find(|model| model["provider"] == "openai")
            .expect("the openai model was not listed");
        assert_eq!(remote["model"], "mock-remote");
        assert_eq!(remote["local"], false);
        assert_eq!(remote["auth"], "api_key");

        // Every provider was listed, so nothing is reported as unavailable.
        assert!(
            body["unavailable"]
                .as_array()
                .is_none_or(|entries| entries.is_empty())
        );
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) = send(router, get("/api/v1/llm/models")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }

    #[tokio::test]
    async fn should_reject_an_invalid_token() {
        let (router, _token) = authenticated_router().await;

        // A well-formed but unknown token never matches a stored session.
        let bogus = Uuid::new_v4().to_string();
        let (status, body) = send(router, get_with_token("/api/v1/llm/models", &bogus)).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_token");
    }
}
