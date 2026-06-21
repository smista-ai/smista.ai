//! Construction of a [`Router`] from a [`RouterConfig`].
//!
//! [`Router::init`](super::Router::init) delegates here. This module turns the
//! validated configuration into the live, long-lived providers a router routes
//! to: Ollama's local daemon from `[router.ollama]`, and every opt-in provider
//! declared under `[router.providers.<id>]`. All providers share one memory
//! backend and the base [`PREAMBLE`]; credentials are supplied per request, not
//! held here.

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

use super::Router;
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

impl Router {
    /// Builds the router from `config`, opening providers against `database`.
    ///
    /// Backs [`Router::init`](super::Router::init); see it for the registration
    /// rules and the error conditions.
    pub(super) fn build(config: &RouterConfig, database: SurrealDatabase) -> anyhow::Result<Self> {
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

    /// Registers a local provider under `provider_id`, replacing any existing one.
    pub(super) fn with_local(
        mut self,
        provider_id: ProviderId,
        provider: Box<dyn Provider>,
    ) -> Self {
        self.local.insert(provider_id, Arc::from(provider));
        self
    }

    /// Registers a remote provider under `provider_id`, replacing any existing one.
    pub(super) fn with_remote(
        mut self,
        provider_id: ProviderId,
        provider: Box<dyn Provider>,
    ) -> Self {
        self.remote.insert(provider_id, Arc::from(provider));
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::OllamaConfig;
    use crate::web::test_support::test_database;

    /// Builds a router from `config` over a fresh in-memory database.
    async fn router_from(config: RouterConfig) -> Router {
        let database = test_database().await;
        Router::init(&config, database).expect("failed to build the router")
    }

    /// A provider config that is available: it carries a base URL.
    fn available(local: bool) -> RouterProviderConfig {
        RouterProviderConfig {
            base_url: Some("http://localhost:1234/v1".to_string()),
            local,
            ..RouterProviderConfig::default()
        }
    }

    #[tokio::test]
    async fn should_list_only_configured_providers() {
        let mut providers = BTreeMap::new();
        providers.insert(ProviderId::Anthropic, RouterProviderConfig::default());
        providers.insert(ProviderId::OpenAI, RouterProviderConfig::default());
        let config = RouterConfig {
            providers,
            ..RouterConfig::default()
        };

        let listed = router_from(config).await.list_providers();

        // The two configured providers are listed; Gemini and Ollama, left out
        // of the configuration, are absent.
        assert_eq!(listed.len(), 2);
        assert!(listed.contains_key(&ProviderId::Anthropic));
        assert!(listed.contains_key(&ProviderId::OpenAI));
        assert!(!listed.contains_key(&ProviderId::Gemini));
        assert!(!listed.contains_key(&ProviderId::Ollama));
    }

    #[tokio::test]
    async fn should_omit_an_openai_compatible_provider_without_a_base_url() {
        let ready = ProviderId::OpenAICompatible("ready".to_string());
        let unconfigured = ProviderId::OpenAICompatible("unconfigured".to_string());
        let mut providers = BTreeMap::new();
        // Available: a base URL gives the endpoint somewhere to reach.
        providers.insert(ready.clone(), available(false));
        // Unavailable: no base URL, so the endpoint is skipped.
        providers.insert(unconfigured.clone(), RouterProviderConfig::default());
        let config = RouterConfig {
            providers,
            ..RouterConfig::default()
        };

        let listed = router_from(config).await.list_providers();

        // Only the provider with a usable base URL is listed.
        assert_eq!(listed.len(), 1);
        assert!(listed.contains_key(&ready));
        assert!(!listed.contains_key(&unconfigured));
    }

    #[tokio::test]
    async fn should_report_the_local_flag_each_provider_advertises() {
        let on_host = ProviderId::OpenAICompatible("on-host".to_string());
        let mut providers = BTreeMap::new();
        providers.insert(ProviderId::Anthropic, RouterProviderConfig::default());
        providers.insert(on_host.clone(), available(true));
        let config = RouterConfig {
            providers,
            ollama: OllamaConfig {
                enabled: true,
                ..OllamaConfig::default()
            },
            ..RouterConfig::default()
        };

        let listed = router_from(config).await.list_providers();

        // Ollama's local daemon and the on-host OpenAI-compatible endpoint are
        // local; the hosted Anthropic provider is not.
        assert!(listed[&ProviderId::Ollama].local);
        assert!(listed[&on_host].local);
        assert!(!listed[&ProviderId::Anthropic].local);
    }
}
