//! Ollama API Client implementation.

use std::sync::Arc;

use reqwest::{Method, Request};
use secrecy::{ExposeSecret, SecretString};
use smista_core::error::ProviderError;
use smista_core::model::Provider;

use crate::ProviderResult;
use crate::model::ollama::OllamaEndpoint;

pub mod api;

/// Trait for the Ollama API client.
pub trait OllamaClient: Send + Sync {
    /// Get the available models and their parameters.
    fn tags(&self) -> impl Future<Output = ProviderResult<api::TagsResponse>> + Send;
}

/// HTTP client implementation of the Ollama API client.
#[derive(Debug, Clone)]
pub struct HttpOllamaClient {
    /// Api key for authentication, required for cloud. Passed as `Authorization: Bearer <api_key>` header.
    api_key: Option<SecretString>,
    /// Base URL for the Ollama API, e.g. `http://localhost:11434` for local or `https://ollama.com` for cloud.
    base_url: String,
    /// HTTP client for making requests to the Ollama API.
    client: Arc<reqwest::Client>,
    /// Extra headers for authentication for custom proxies. Each tuple is (header name, header value).
    extra_headers: Vec<(String, SecretString)>,
}

impl HttpOllamaClient {
    /// Instantiates a new [`HttpOllamaClient`] with the given base URL.
    pub fn new(base_url: String) -> Self {
        Self {
            api_key: None,
            base_url,
            client: Arc::new(reqwest::Client::new()),
            extra_headers: Vec::new(),
        }
    }

    /// Instantiates a new [`HttpOllamaClient`] configured for Ollama Cloud with the given API key.
    pub fn cloud(api_key: SecretString) -> Self {
        Self::new("https://ollama.com".to_string()).with_api_key(api_key)
    }

    /// Builds a client for an [`OllamaEndpoint`], the single source of truth.
    ///
    /// Base URL, API key and proxy headers are taken from `endpoint`, so the
    /// management client and the completion path always target the same
    /// instance. This is the production constructor; prefer it over assembling
    /// the connection facts by hand.
    pub fn from_endpoint(endpoint: &OllamaEndpoint) -> Self {
        let mut client = Self::new(endpoint.base_url().to_string());
        if let Some(api_key) = endpoint.api_key() {
            client = client.with_api_key(api_key.clone());
        }
        endpoint
            .extra_headers()
            .iter()
            .fold(client, |client, (key, value)| {
                client.with_extra_header(key.clone(), value.clone())
            })
    }

    /// Sets the API key for authentication, required for cloud. Passed as `Authorization: Bearer <api_key>` header.
    pub fn with_api_key(mut self, api_key: SecretString) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Adds an extra header for authentication, useful for custom proxies.
    pub fn with_extra_header(mut self, key: String, value: SecretString) -> Self {
        self.extra_headers.push((key, value));
        self
    }

    fn request<S>(&self, method: Method, url: S) -> ProviderResult<Request>
    where
        S: Into<String>,
    {
        let mut req = self.client.request(
            method,
            format!("{base}{url}", base = self.base_url, url = url.into()),
        );

        // Add API key for authentication if present.
        if let Some(api_key) = &self.api_key {
            req = req.bearer_auth(api_key.expose_secret());
        }

        // Add extra headers for authentication, useful for custom proxies.
        for (key, value) in &self.extra_headers {
            req = req.header(key, value.expose_secret());
        }

        req.build().map_err(normalize_reqwest_error)
    }
}

impl OllamaClient for HttpOllamaClient {
    async fn tags(&self) -> ProviderResult<api::TagsResponse> {
        tracing::debug!("Building request for Ollama API /api/tags endpoint");
        let request = self.request(Method::GET, "/api/tags")?;

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
        message: error.to_string(),
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
    async fn tags(&self) -> ProviderResult<api::TagsResponse> {
        self.tags_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.tags_response.clone()
    }
}
