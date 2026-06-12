//! Anthropic API client implementation.

use std::sync::Arc;

use reqwest::{Method, Request};
use secrecy::ExposeSecret;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::Provider;

use crate::ProviderResult;
use crate::auth::Authentication;

pub mod api;

/// Default base URL for the Anthropic API.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Required API version for Anthropic API requests, sent in the `anthropic-version` header.
///
/// See <https://platform.claude.com/docs/en/api/versioning> for details on Anthropic's API versioning scheme.
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Number of models requested per `/v1/models` page.
///
/// The catalog is far smaller than one page; pagination exists for correctness,
/// not because the limit is expected to be hit.
const PAGE_LIMIT: u32 = 50;

/// Trait for the Anthropic management API client.
pub trait AnthropicClient: Send + Sync {
    /// Lists every available model, following pagination, authenticating with
    /// the supplied [`Authentication`].
    fn models(
        &self,
        authentication: &Authentication,
    ) -> impl Future<Output = ProviderResult<Vec<api::Model>>> + Send;
}

/// HTTP client implementation of the Anthropic management API client.
///
/// Holds only the connection: credentials are supplied per request as an
/// [`Authentication`] and applied when the request is built.
#[derive(Debug, Clone)]
pub struct HttpAnthropicClient {
    /// Base URL for the Anthropic API.
    base_url: String,
    /// HTTP client for making requests to the Anthropic API.
    client: Arc<reqwest::Client>,
}

impl Default for HttpAnthropicClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpAnthropicClient {
    /// Instantiates a client targeting the public Anthropic API.
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL.to_string())
    }

    /// Instantiates a client targeting `base_url`.
    ///
    /// This is a testing and advanced seam: production code uses
    /// [`HttpAnthropicClient::new`], which targets the public API.
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            client: Arc::new(reqwest::Client::new()),
        }
    }

    fn request(&self, url: String, authentication: &Authentication) -> ProviderResult<Request> {
        let mut req = self
            .client
            .request(Method::GET, format!("{base}{url}", base = self.base_url))
            .header("anthropic-version", ANTHROPIC_API_VERSION);

        // Anthropic authenticates the management API with the `X-Api-Key` header.
        if let Some(api_key) = authentication.api_key() {
            req = req.header("X-Api-Key", api_key.expose_secret());
        } else {
            tracing::error!(
                "Anthropic API key missing from authentication; required for Anthropic API requests"
            );
            return Err(ProviderError {
                message: "Anthropic API key missing from authentication".to_string(),
                category: ProviderErrorCategory::Authentication,
                provider: Provider::Anthropic,
                model: None,
            });
        }

        // Extra headers for authentication, useful for custom proxies.
        for (key, value) in authentication.headers() {
            req = req.header(key, value.expose_secret());
        }

        req.build().map_err(normalize_reqwest_error)
    }
}

impl AnthropicClient for HttpAnthropicClient {
    async fn models(&self, authentication: &Authentication) -> ProviderResult<Vec<api::Model>> {
        let mut all = Vec::new();
        let mut after: Option<String> = None;

        loop {
            let url = match &after {
                Some(id) => format!("/v1/models?limit={PAGE_LIMIT}&after_id={id}"),
                None => format!("/v1/models?limit={PAGE_LIMIT}"),
            };

            tracing::debug!("Fetching Anthropic models page: {url}");
            let request = self.request(url, authentication)?;
            let response = self
                .client
                .execute(request)
                .await
                .map_err(normalize_reqwest_error)?;

            if !response.status().is_success() {
                tracing::error!(
                    "Anthropic API request failed with status {}: {:?}",
                    response.status(),
                    response
                );
                return Err(ProviderError {
                    message: format!(
                        "Anthropic API request failed with status {}",
                        response.status()
                    ),
                    category: ProviderErrorCategory::InvalidRequest,
                    provider: Provider::Anthropic,
                    model: None,
                });
            }

            let page: api::ModelsResponse =
                response.json().await.map_err(normalize_reqwest_error)?;

            let has_more = page.has_more;
            let last_id = page.last_id.clone();
            all.extend(page.data);

            // Stop when the API reports no more pages, or when it claims more but
            // gives no cursor to advance with (defensive against a malformed page).
            match (has_more, last_id) {
                (true, Some(id)) => after = Some(id),
                _ => break,
            }
        }

        tracing::debug!("Fetched {} Anthropic model(s)", all.len());
        Ok(all)
    }
}

fn normalize_reqwest_error(error: reqwest::Error) -> ProviderError {
    ProviderError {
        message: format!("Anthropic API request failed: {error:?}"),
        category: crate::error::category_from_reqwest(&error),
        provider: Provider::Anthropic,
        model: None,
    }
}

/// A test double that returns a canned model list and counts calls.
///
/// The call counter lets cache tests assert that a hot cache serves repeat
/// lookups without re-querying the API.
#[cfg(test)]
pub struct MockAnthropicClient {
    /// The canned response every [`AnthropicClient::models`] call clones.
    pub models_response: ProviderResult<Vec<api::Model>>,
    /// Number of times [`AnthropicClient::models`] has been invoked.
    pub models_calls: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MockAnthropicClient {
    /// Creates a mock that always returns `models_response`.
    pub fn new(models_response: ProviderResult<Vec<api::Model>>) -> Self {
        Self {
            models_response,
            models_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Returns how many times [`AnthropicClient::models`] has been called.
    pub fn models_calls(&self) -> usize {
        self.models_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl AnthropicClient for MockAnthropicClient {
    async fn models(&self, _authentication: &Authentication) -> ProviderResult<Vec<api::Model>> {
        self.models_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.models_response.clone()
    }
}
