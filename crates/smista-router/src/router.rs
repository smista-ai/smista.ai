//! The router: the set of providers smista.ai can route requests to.
//!
//! [`Router`] holds the local and remote [`Provider`]s assembled from the
//! validated [`RouterConfig`]. Providers are long-lived and shared; a request
//! resolves a model from one of them at execution time.

mod build;
mod fetch_models;
mod memory_storage;
#[cfg(test)]
mod mock_provider;
pub(crate) mod resolver;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use smista_core::model::{Provider as ProviderId, ProviderDescriptor};
use smista_providers::provider::Provider;
use smista_storage::database::surreal::SurrealDatabase;

pub use self::fetch_models::FetchModelsResult;
use crate::config::RouterConfig;

/// The providers a router can route requests to, split by locality.
#[derive(Default)]
pub struct Router {
    /// Local providers to route requests to.
    local: HashMap<ProviderId, Arc<dyn Provider>>,
    /// Remote providers to route requests to.
    remote: HashMap<ProviderId, Arc<dyn Provider>>,
}

impl fmt::Debug for Router {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Router")
            .field("local", &self.local.keys().collect::<Vec<_>>())
            .field("remote", &self.remote.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Router {
    /// Builds the router from `config`, opening providers against `database`.
    ///
    /// Ollama's local daemon is enabled through `[router.ollama]`; built-in
    /// cloud providers are seeded by the default router config, and extra
    /// provider instances are declared under `[router.providers.<id>]`. All
    /// providers share one memory backend and the same preamble; credentials are
    /// not held here but supplied per request when a model is resolved.
    ///
    /// # Errors
    ///
    /// Returns an error if a configured OpenAI-compatible provider cannot be
    /// built — for example, from a malformed endpoint URL.
    pub fn init(config: &RouterConfig, database: SurrealDatabase) -> anyhow::Result<Self> {
        Self::build(config, database)
    }

    /// Get the list of providers available on the router, keyed by provider ID.
    pub fn list_providers(&self) -> HashMap<ProviderId, ProviderDescriptor> {
        self.local
            .iter()
            .chain(self.remote.iter())
            .map(|(id, provider)| (id.clone(), provider.descriptor()))
            .collect()
    }

    /// Returns the provider registered under `id`, local or remote.
    ///
    /// The orchestrator uses this to obtain the handle it invokes; routing has
    /// already chosen the provider deterministically, so this is a pure lookup
    /// that prefers a local provider over a remote one of the same id.
    #[must_use]
    pub fn provider(&self, id: &ProviderId) -> Option<Arc<dyn Provider>> {
        self.local.get(id).or_else(|| self.remote.get(id)).cloned()
    }

    /// Builds a router wired with mock providers for tests.
    ///
    /// Installs two [`MockProvider`](mock_provider::MockProvider)s — one local
    /// and one remote — each resolving a canned model, so a test can exercise
    /// routing and the HTTP surface without a network call or an API key.
    #[cfg(test)]
    pub(crate) fn mock() -> Self {
        use crate::router::mock_provider::MockProvider;

        Self::default()
            .with_local(
                ProviderId::Ollama,
                Box::new(MockProvider::new(
                    ProviderId::Ollama,
                    "Mock Local",
                    true,
                    "mock-local",
                )),
            )
            .with_remote(
                ProviderId::OpenAI,
                Box::new(MockProvider::new(
                    ProviderId::OpenAI,
                    "Mock Remote",
                    false,
                    "mock-remote",
                )),
            )
    }

    /// Builds a mock router whose local model returns `local` in order.
    ///
    /// The local Ollama mock replays `local` completion-by-completion across a
    /// turn's invocations, so a test can drive the orchestrator through tool
    /// requests and the follow-up turns that answer them; once the script
    /// drains, the model returns the fixed response. The remote OpenAI mock is
    /// installed unscripted, as in [`Router::mock`].
    #[cfg(test)]
    pub(crate) fn mock_scripted(local: Vec<smista_providers::api::CompletionResponse>) -> Self {
        use crate::router::mock_provider::MockProvider;

        Self::default()
            .with_local(
                ProviderId::Ollama,
                Box::new(
                    MockProvider::new(ProviderId::Ollama, "Mock Local", true, "mock-local")
                        .with_script(local),
                ),
            )
            .with_remote(
                ProviderId::OpenAI,
                Box::new(MockProvider::new(
                    ProviderId::OpenAI,
                    "Mock Remote",
                    false,
                    "mock-remote",
                )),
            )
    }

    /// Builds a mock router whose local model cannot stream, for the buffered
    /// fallback path under a streaming request.
    #[cfg(test)]
    pub(crate) fn mock_non_streaming() -> Self {
        use crate::router::mock_provider::MockProvider;

        Self::default()
            .with_local(
                ProviderId::Ollama,
                Box::new(
                    MockProvider::new(ProviderId::Ollama, "Mock Local", true, "mock-local")
                        .with_streaming(false),
                ),
            )
            .with_remote(
                ProviderId::OpenAI,
                Box::new(MockProvider::new(
                    ProviderId::OpenAI,
                    "Mock Remote",
                    false,
                    "mock-remote",
                )),
            )
    }

    /// Builds a mock router whose local model fails part-way through its stream,
    /// for the mid-stream-error path.
    #[cfg(test)]
    pub(crate) fn mock_stream_error() -> Self {
        use crate::router::mock_provider::MockProvider;

        Self::default()
            .with_local(
                ProviderId::Ollama,
                Box::new(
                    MockProvider::new(ProviderId::Ollama, "Mock Local", true, "mock-local")
                        .with_stream_error(true),
                ),
            )
            .with_remote(
                ProviderId::OpenAI,
                Box::new(MockProvider::new(
                    ProviderId::OpenAI,
                    "Mock Remote",
                    false,
                    "mock-remote",
                )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_provider_by_id_for_mock() {
        let router = Router::mock();
        assert!(router.provider(&ProviderId::Ollama).is_some());
        assert!(router.provider(&ProviderId::OpenAI).is_some());
        assert!(router.provider(&ProviderId::Anthropic).is_none());
    }
}
