//! Ollama API Client implementation.

use std::sync::Arc;

use reqwest::{Method, Request};
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::Provider;

use crate::ProviderResult;
use crate::auth::Authentication;
use crate::model::ollama::OllamaEndpoint;

pub mod api;

/// Trait for the Ollama API client.
pub trait OllamaClient: Send + Sync {
    /// Get the available models and their parameters, authenticating with the
    /// supplied [`Authentication`].
    fn tags(
        &self,
        authentication: &Authentication,
    ) -> impl Future<Output = ProviderResult<api::TagsResponse>> + Send;
}

/// HTTP client implementation of the Ollama API client.
///
/// Holds only the connection: credentials are supplied per request as an
/// [`Authentication`] and applied when the request is built.
#[derive(Debug, Clone)]
pub struct HttpOllamaClient {
    /// Base URL for the Ollama API, e.g. `http://localhost:11434` for local or `https://ollama.com` for cloud.
    base_url: String,
    /// HTTP client for making requests to the Ollama API.
    client: Arc<reqwest::Client>,
}

impl HttpOllamaClient {
    /// Instantiates a new [`HttpOllamaClient`] with the given base URL.
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Arc::new(reqwest::Client::new()),
        }
    }

    /// Builds a client for an [`OllamaEndpoint`], the single source of truth for
    /// the connection.
    ///
    /// The base URL is taken from `endpoint`, so the management client and the
    /// completion path always target the same instance. Credentials are not
    /// baked in: they are supplied per request when calling [`OllamaClient::tags`].
    pub fn from_endpoint(endpoint: &OllamaEndpoint) -> Self {
        Self::new(endpoint.base_url().to_string())
    }

    fn request<S>(
        &self,
        method: Method,
        url: S,
        authentication: &Authentication,
    ) -> ProviderResult<Request>
    where
        S: Into<String>,
    {
        let mut req = self.client.request(
            method,
            format!("{base}{url}", base = self.base_url, url = url.into()),
        );

        // Add the bearer API key for authentication if one was supplied.
        if let Some(api_key) = authentication.api_key() {
            req = req.bearer_auth(api_key.expose_secret());
        }

        // Add extra headers for authentication, useful for custom proxies.
        for (key, value) in authentication.headers() {
            req = req.header(key, value.expose_secret());
        }

        req.build().map_err(normalize_reqwest_error)
    }
}

impl OllamaClient for HttpOllamaClient {
    async fn tags(&self, authentication: &Authentication) -> ProviderResult<api::TagsResponse> {
        tracing::debug!("Building request for Ollama API /api/tags endpoint");
        let request = self.request(Method::GET, "/api/tags", authentication)?;

        tracing::debug!("Fetching available models from Ollama API");
        self.client
            .execute(request)
            .await
            .map_err(normalize_reqwest_error)?
            .json::<api::TagsResponse>()
            .await
            .map_err(normalize_reqwest_error)
    }
}

fn normalize_reqwest_error(error: reqwest::Error) -> ProviderError {
    ProviderError {
        message: format!("Ollama API request failed: {error:?}"),
        category: crate::error::category_from_reqwest(&error),
        provider: Provider::Ollama,
        model: None,
    }
}

/// A test double that returns a canned `/api/tags` response and counts calls.
///
/// The call counter lets cache tests assert that a hot cache serves repeat
/// lookups without re-querying the instance.
#[cfg(test)]
pub struct MockOllamaClient {
    /// The canned response every [`OllamaClient::tags`] call clones.
    pub tags_response: ProviderResult<api::TagsResponse>,
    /// Number of times [`OllamaClient::tags`] has been invoked.
    pub tags_calls: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MockOllamaClient {
    /// Creates a mock that always returns `tags_response`.
    pub fn new(tags_response: ProviderResult<api::TagsResponse>) -> Self {
        Self {
            tags_response,
            tags_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Returns how many times [`OllamaClient::tags`] has been called.
    pub fn tags_calls(&self) -> usize {
        self.tags_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl OllamaClient for MockOllamaClient {
    async fn tags(&self, _authentication: &Authentication) -> ProviderResult<api::TagsResponse> {
        self.tags_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.tags_response.clone()
    }
}
