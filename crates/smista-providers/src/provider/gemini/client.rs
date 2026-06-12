//! Gemini API client implementation.

use std::sync::Arc;

use reqwest::{Method, Request};
use secrecy::ExposeSecret;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::Provider;

use crate::ProviderResult;
use crate::auth::Authentication;

pub mod api;

/// Default base URL for the Gemini API.
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// Number of models requested per `v1beta/models` page.
///
/// The listing is far smaller than this; Gemini caps the page size at 1000, so
/// asking for the maximum fetches the whole catalog in a single request while
/// pagination stays in place for correctness.
const PAGE_SIZE: u32 = 1000;

/// Trait for the Gemini management API client.
pub trait GeminiClient: Send + Sync {
    /// Lists every available model, following pagination, authenticating with
    /// the supplied [`Authentication`].
    fn models(
        &self,
        authentication: &Authentication,
    ) -> impl Future<Output = ProviderResult<Vec<api::Model>>> + Send;
}

/// HTTP client implementation of the Gemini management API client.
///
/// Holds only the connection: credentials are supplied per request as an
/// [`Authentication`] and applied when the request is built.
#[derive(Debug, Clone)]
pub struct HttpGeminiClient {
    /// Base URL for the Gemini API.
    base_url: String,
    /// HTTP client for making requests to the Gemini API.
    client: Arc<reqwest::Client>,
}

impl Default for HttpGeminiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpGeminiClient {
    /// Instantiates a client targeting the public Gemini API.
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL.to_string())
    }

    /// Instantiates a client targeting `base_url`.
    ///
    /// This is a testing and advanced seam: production code uses
    /// [`HttpGeminiClient::new`], which targets the public API.
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            client: Arc::new(reqwest::Client::new()),
        }
    }

    fn request(&self, url: String, authentication: &Authentication) -> ProviderResult<Request> {
        let mut req = self
            .client
            .request(Method::GET, format!("{base}{url}", base = self.base_url));

        // Gemini authenticates the management API with the `x-goog-api-key` header.
        if let Some(api_key) = authentication.api_key() {
            req = req.header("x-goog-api-key", api_key.expose_secret());
        } else {
            tracing::error!(
                "Gemini API key missing from authentication; required for Gemini API requests"
            );
            return Err(ProviderError {
                message: "Gemini API key missing from authentication".to_string(),
                category: ProviderErrorCategory::Authentication,
                provider: Provider::Gemini,
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

impl GeminiClient for HttpGeminiClient {
    async fn models(&self, authentication: &Authentication) -> ProviderResult<Vec<api::Model>> {
        let mut all = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let url = match &page_token {
                Some(token) => {
                    format!("/v1beta/models?pageSize={PAGE_SIZE}&pageToken={token}")
                }
                None => format!("/v1beta/models?pageSize={PAGE_SIZE}"),
            };

            tracing::debug!("Fetching Gemini models page: {url}");
            let request = self.request(url, authentication)?;
            let response = self
                .client
                .execute(request)
                .await
                .map_err(normalize_reqwest_error)?;

            if !response.status().is_success() {
                tracing::error!(
                    "Gemini API request failed with status {}: {:?}",
                    response.status(),
                    response
                );
                return Err(ProviderError {
                    message: format!(
                        "Gemini API request failed with status {}",
                        response.status()
                    ),
                    category: ProviderErrorCategory::InvalidRequest,
                    provider: Provider::Gemini,
                    model: None,
                });
            }

            let page: api::ModelsResponse =
                response.json().await.map_err(normalize_reqwest_error)?;

            let next = page.next_page_token.clone();
            all.extend(page.models);

            // Stop when the API hands back no further cursor.
            match next {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }

        tracing::debug!("Fetched {} Gemini model(s)", all.len());
        Ok(all)
    }
}

fn normalize_reqwest_error(error: reqwest::Error) -> ProviderError {
    ProviderError {
        message: format!("Gemini API request failed: {error:?}"),
        category: crate::error::category_from_reqwest(&error),
        provider: Provider::Gemini,
        model: None,
    }
}

/// A test double that returns a canned model list and counts calls.
///
/// The call counter lets cache tests assert that a hot cache serves repeat
/// lookups without re-querying the API.
#[cfg(test)]
pub struct MockGeminiClient {
    /// The canned response every [`GeminiClient::models`] call clones.
    pub models_response: ProviderResult<Vec<api::Model>>,
    /// Number of times [`GeminiClient::models`] has been invoked.
    pub models_calls: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MockGeminiClient {
    /// Creates a mock that always returns `models_response`.
    pub fn new(models_response: ProviderResult<Vec<api::Model>>) -> Self {
        Self {
            models_response,
            models_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Returns how many times [`GeminiClient::models`] has been called.
    pub fn models_calls(&self) -> usize {
        self.models_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl GeminiClient for MockGeminiClient {
    async fn models(&self, _authentication: &Authentication) -> ProviderResult<Vec<api::Model>> {
        self.models_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.models_response.clone()
    }
}
