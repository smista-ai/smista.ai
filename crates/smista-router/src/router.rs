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
mod resolver;

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
    /// Ollama's local daemon is enabled through `[router.ollama]`; every other
    /// provider is opt-in under `[router.providers.<id>]`, so only the providers
    /// a deployment configures are registered. All providers share one memory
    /// backend and the same preamble; credentials are not held here but
    /// supplied per request when a model is resolved.
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
}
