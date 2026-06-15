//! The router: the set of providers smista.ai can route requests to.
//!
//! [`Router`] holds the local and remote [`Provider`]s assembled from the
//! validated [`RouterConfig`]. Providers are long-lived and shared; a request
//! resolves a model from one of them at execution time.

mod memory_storage;
#[cfg(test)]
mod mock_provider;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use smista_core::model::Provider as ProviderId;
use smista_providers::model::anthropic::AnthropicModelArgs;
use smista_providers::model::gemini::GeminiModelArgs;
use smista_providers::model::ollama::{OllamaEndpoint, OllamaModelRuntime};
use smista_providers::model::openai::OpenAIModelArgs;
use smista_providers::model::openai_compat::{OpenAICompatEndpoint, OpenAICompatRuntime};
use smista_providers::provider::Provider;
use smista_providers::provider::anthropic::AnthropicProvider;
use smista_providers::provider::gemini::GeminiProvider;
use smista_providers::provider::ollama::OllamaProvider;
use smista_providers::provider::openai::OpenAIProvider;
use smista_providers::provider::openai_compat::OpenAICompatProvider;
use smista_storage::database::surreal::SurrealDatabase;

use crate::config::{RouterConfig, RouterProviderConfig};
use crate::router::memory_storage::SurrealMemoryStorage;

/// The base system prompt every agent starts from.
///
/// Each model appends its memory preamble to this. It keeps the agent on the
/// task and explains the memory tool, without assuming any knowledge of the
/// surrounding system.
const PREAMBLE: &str = "\
You are a capable assistant. Complete the task you are given accurately and \
concisely, using the tools available to you. You have a memory tool: use it to \
record durable facts about the user and useful context for the current \
session, and rely on the memories provided to you as background. When that \
background conflicts with the current conversation, follow the latest \
instructions.";

/// The providers a router can route requests to, split by locality.
#[derive(Default)]
pub struct Router {
    /// Local providers to route requests to.
    local: HashMap<ProviderId, Box<dyn Provider>>,
    /// Remote providers to route requests to.
    remote: HashMap<ProviderId, Box<dyn Provider>>,
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
    /// backend and the base [`PREAMBLE`]; credentials are not held here but
    /// supplied per request when a model is resolved.
    ///
    /// # Errors
    ///
    /// Returns an error if a configured OpenAI-compatible provider cannot be
    /// built — for example, from a malformed endpoint URL.
    pub fn init(config: &RouterConfig, database: SurrealDatabase) -> anyhow::Result<Self> {
        tracing::debug!("building router from configuration");
        let storage = Arc::new(SurrealMemoryStorage::from(database));
        let mut router = Self::default();

        // Ollama's local daemon has its own dedicated configuration block.
        if config.ollama.enabled {
            tracing::debug!(base_url = %config.ollama.base_url, "adding local Ollama provider");
            let provider = OllamaProvider::new(
                OllamaEndpoint::local(config.ollama.base_url.clone()),
                OllamaModelRuntime {
                    preamble: PREAMBLE.to_string(),
                    storage: storage.clone(),
                },
            );
            router = router.with_local(ProviderId::Ollama, Box::new(provider));
        }

        // Every other provider is opt-in: registered only when it appears under
        // `[router.providers.<id>]`.
        for (provider_id, provider_config) in &config.providers {
            router = router.with_configured_provider(provider_id, provider_config, &storage)?;
        }

        tracing::info!(
            local_count = router.local.len(),
            remote_count = router.remote.len(),
            "router initialized with {{local_count}} local and {{remote_count}} remote providers"
        );
        Ok(router)
    }

    /// Registers the provider described by one `[router.providers.<id>]` entry.
    ///
    /// Built-in providers (Anthropic, Gemini, OpenAI and cloud Ollama) are added
    /// on the strength of their presence in the configuration; an
    /// `openai-compat:<name>` entry is built from its endpoint and declared
    /// models.
    fn with_configured_provider(
        self,
        provider_id: &ProviderId,
        provider_config: &RouterProviderConfig,
        storage: &Arc<SurrealMemoryStorage>,
    ) -> anyhow::Result<Self> {
        match provider_id {
            ProviderId::Anthropic => {
                let provider = AnthropicProvider::new(AnthropicModelArgs {
                    preamble: PREAMBLE.to_string(),
                    storage: storage.clone(),
                });
                Ok(self.with_remote(ProviderId::Anthropic, Box::new(provider)))
            }
            ProviderId::Gemini => {
                let provider = GeminiProvider::new(GeminiModelArgs {
                    preamble: PREAMBLE.to_string(),
                    storage: storage.clone(),
                });
                Ok(self.with_remote(ProviderId::Gemini, Box::new(provider)))
            }
            ProviderId::OpenAI => {
                let provider = OpenAIProvider::new(OpenAIModelArgs {
                    preamble: PREAMBLE.to_string(),
                    storage: storage.clone(),
                });
                Ok(self.with_remote(ProviderId::OpenAI, Box::new(provider)))
            }
            ProviderId::Ollama => {
                // The local daemon is configured under `[router.ollama]`; an
                // entry here is the cloud endpoint, optionally at a custom host.
                let endpoint = match &provider_config.base_url {
                    Some(base_url) => OllamaEndpoint::Cloud {
                        base_url: base_url.clone(),
                    },
                    None => OllamaEndpoint::cloud(),
                };
                let provider = OllamaProvider::new(
                    endpoint,
                    OllamaModelRuntime {
                        preamble: PREAMBLE.to_string(),
                        storage: storage.clone(),
                    },
                );
                Ok(self.with_remote(ProviderId::Ollama, Box::new(provider)))
            }
            ProviderId::OpenAICompatible(name) => {
                self.with_openai_compat_provider(name, provider_config, storage)
            }
        }
    }

    /// Registers one generic OpenAI-compatible provider from its configuration.
    ///
    /// Skips the entry when no `base_url` is set, since these endpoints have no
    /// default. Each declared model's descriptor is built from the configured
    /// facts, as the endpoint publishes none of its own.
    fn with_openai_compat_provider(
        self,
        name: &str,
        provider_config: &RouterProviderConfig,
        storage: &Arc<SurrealMemoryStorage>,
    ) -> anyhow::Result<Self> {
        let provider_id = ProviderId::OpenAICompatible(name.to_string());
        let Some(base_url) = provider_config.base_url.clone() else {
            tracing::warn!("skipping provider {provider_id} because base_url is not set");
            return Ok(self);
        };
        let is_local = provider_config.local;
        let display_name = provider_config
            .display_name
            .clone()
            .unwrap_or_else(|| name.to_string());

        let models = provider_config
            .models
            .iter()
            .map(|model| {
                let descriptor = model.to_descriptor(provider_id.clone(), is_local);
                (descriptor.reference(), descriptor)
            })
            .collect();

        tracing::debug!(
            provider.id = %provider_id,
            provider.local = is_local,
            provider.base_url = %base_url,
            "adding OpenAI-compatible provider {{provider.id}} at {{provider.base_url}}"
        );
        let provider = OpenAICompatProvider::new(
            provider_id.clone(),
            display_name,
            is_local,
            OpenAICompatEndpoint::new(base_url),
            OpenAICompatRuntime {
                preamble: PREAMBLE.to_string(),
                storage: storage.clone(),
            },
            models,
        )?;
        if is_local {
            Ok(self.with_local(provider_id, Box::new(provider)))
        } else {
            Ok(self.with_remote(provider_id, Box::new(provider)))
        }
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

    pub fn with_local(mut self, provider_id: ProviderId, provider: Box<dyn Provider>) -> Self {
        self.local.insert(provider_id, provider);
        self
    }

    pub fn with_remote(mut self, provider_id: ProviderId, provider: Box<dyn Provider>) -> Self {
        self.remote.insert(provider_id, provider);
        self
    }
}
