//! Model discovery: [`Router::fetch_models`] lists the models every configured
//! provider offers, in parallel and under a per-call timeout.

use std::collections::HashMap;
use std::time::Duration;

use secrecy::SecretString;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::{ModelDescriptor, ModelReference, Provider as ProviderId};
use smista_providers::auth::Authentication;

use super::Router;

/// Result for [`Router::fetch_models`]: the models successfully listed, and the
/// providers that could not be listed with the reasons for their failure.
#[derive(Debug, Default)]
pub struct FetchModelsResult {
    /// The models successfully listed, keyed by their reference.
    pub models: HashMap<ModelReference, ModelDescriptor>,
    /// The providers that could not be listed, keyed by provider ID, with the
    /// reason for their failure.
    pub failed_providers: HashMap<ProviderId, ProviderError>,
}

impl Router {
    /// Fetches the models offered by every configured provider.
    ///
    /// The `credentials` are supplied per call so the router can pass them to
    /// the providers as they list their models, which some providers require
    /// (remote providers such as Anthropic and Gemini reject the call without a
    /// key), and so the router can report credential problems to the caller.
    ///
    /// Models are fetched in parallel across providers, and the `timeout`
    /// applies to every provider call so a slow provider cannot block the
    /// response indefinitely; the call returns once every provider has
    /// responded or hit the timeout. A provider that fails or times out is left
    /// out of [`FetchModelsResult::models`] and recorded in
    /// [`FetchModelsResult::failed_providers`] with the reason: a timeout
    /// yields a [`ProviderError`] of category
    /// [`Timeout`](ProviderErrorCategory::Timeout).
    pub async fn fetch_models(
        &self,
        credentials: HashMap<ProviderId, SecretString>,
        timeout: Duration,
    ) -> FetchModelsResult {
        let mut promises = HashMap::new();
        for (id, provider) in self.local.iter().chain(self.remote.iter()) {
            let auth = self.authentication(id, &credentials);
            let id_t = id.clone();
            let provider = provider.clone();
            let promise = async move {
                tokio::time::timeout(timeout, provider.list_models(&auth))
                    .await
                    .map_err(|_| ProviderError {
                        category: ProviderErrorCategory::Timeout,
                        message: format!("provider timed out after {timeout:?}"),
                        provider: id_t,
                        model: None,
                    })
                    .and_then(|res| res)
            };

            promises.insert(id.clone(), tokio::spawn(promise));
        }

        // Await every provider's promise and separate the successful fetches
        // from the failures, which can include timeouts and credential problems.
        let mut result = FetchModelsResult::default();
        for (id, promise) in promises {
            match promise.await {
                Ok(Ok(models)) => {
                    for (reference, descriptor) in models {
                        tracing::debug!("fetched {reference} model from provider {id}");
                        result.models.insert(reference, descriptor);
                    }
                }
                Ok(Err(err)) => {
                    tracing::error!("failed to fetch models from provider {id}: {err}");
                    result.failed_providers.insert(id, err);
                }
                Err(err) => {
                    tracing::error!("failed to fetch models from provider {id}: {err}");
                    result.failed_providers.insert(
                        id.clone(),
                        ProviderError {
                            category: ProviderErrorCategory::Unknown,
                            message: format!("provider task panicked: {err}"),
                            provider: id,
                            model: None,
                        },
                    );
                }
            }
        }

        result
    }

    /// Returns the [`Authentication`] to use for `provider`, based on `credentials`.
    fn authentication(
        &self,
        provider: &ProviderId,
        credentials: &HashMap<ProviderId, SecretString>,
    ) -> Authentication {
        credentials
            .get(provider)
            .map(|secret| Authentication::ApiKey(secret.clone()))
            .unwrap_or(Authentication::None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use smista_core::model::ProviderDescriptor;
    use smista_providers::ProviderResult;
    use smista_providers::memory::MemoryScope;
    use smista_providers::model::Model;
    use smista_providers::provider::Provider;

    use super::*;
    use crate::router::mock_provider::MockProvider;

    /// How a [`StubProvider`] answers a `list_models` call.
    enum StubBehavior {
        /// Always fail with an [`Unknown`](ProviderErrorCategory::Unknown) error.
        Fail,
        /// Sleep past any realistic timeout, then succeed with no models.
        Slow,
        /// Succeed with no models only when an API key was supplied, otherwise
        /// fail with [`MissingCredentials`](ProviderErrorCategory::MissingCredentials).
        RequireKey,
    }

    /// A provider whose `list_models` answer is fixed by a [`StubBehavior`], so a
    /// test can drive the failure, timeout and credential paths of
    /// [`Router::fetch_models`] without a real upstream.
    struct StubProvider {
        id: ProviderId,
        behavior: StubBehavior,
    }

    #[async_trait]
    impl Provider for StubProvider {
        fn id(&self) -> ProviderId {
            self.id.clone()
        }

        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor {
                id: self.id.clone(),
                display_name: self.id.to_string(),
                local: false,
            }
        }

        async fn resolve(
            &self,
            _reference: &ModelReference,
            _authentication: &Authentication,
            _scope: MemoryScope,
        ) -> ProviderResult<Arc<dyn Model>> {
            Err(self.error(ProviderErrorCategory::Unknown, "resolve is unused"))
        }

        async fn list_models(
            &self,
            authentication: &Authentication,
        ) -> ProviderResult<HashMap<ModelReference, ModelDescriptor>> {
            match self.behavior {
                StubBehavior::Fail => {
                    Err(self.error(ProviderErrorCategory::Unknown, "listing failed"))
                }
                StubBehavior::Slow => {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    Ok(HashMap::new())
                }
                StubBehavior::RequireKey => match authentication {
                    Authentication::ApiKey(_) => Ok(HashMap::new()),
                    _ => Err(self.error(
                        ProviderErrorCategory::MissingCredentials,
                        "no credentials configured for the provider",
                    )),
                },
            }
        }
    }

    impl StubProvider {
        /// Builds a [`ProviderError`] attributed to this provider.
        fn error(&self, category: ProviderErrorCategory, message: &str) -> ProviderError {
            ProviderError {
                category,
                message: message.to_string(),
                provider: self.id.clone(),
                model: None,
            }
        }
    }

    /// A generous timeout for the paths that are expected to resolve well before it.
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn should_fetch_models_from_every_provider() {
        let router = Router::mock();

        let result = router.fetch_models(HashMap::new(), TEST_TIMEOUT).await;

        assert!(result.failed_providers.is_empty());
        let mut models: Vec<&str> = result.models.keys().map(|r| r.model.as_str()).collect();
        models.sort_unstable();
        assert_eq!(models, vec!["mock-local", "mock-remote"]);
    }

    #[tokio::test]
    async fn should_record_a_provider_that_fails_to_list() {
        let router = Router::default()
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
                Box::new(StubProvider {
                    id: ProviderId::OpenAI,
                    behavior: StubBehavior::Fail,
                }),
            );

        let result = router.fetch_models(HashMap::new(), TEST_TIMEOUT).await;

        // The working provider's model is listed; only the failing one drops out.
        let models: Vec<&str> = result.models.keys().map(|r| r.model.as_str()).collect();
        assert_eq!(models, vec!["mock-local"]);
        let failure = result
            .failed_providers
            .get(&ProviderId::OpenAI)
            .expect("the failing provider was not recorded");
        assert_eq!(failure.category, ProviderErrorCategory::Unknown);
    }

    #[tokio::test]
    async fn should_record_a_provider_that_times_out() {
        let router = Router::default().with_remote(
            ProviderId::OpenAI,
            Box::new(StubProvider {
                id: ProviderId::OpenAI,
                behavior: StubBehavior::Slow,
            }),
        );

        let result = router
            .fetch_models(HashMap::new(), Duration::from_millis(10))
            .await;

        assert!(result.models.is_empty());
        let failure = result
            .failed_providers
            .get(&ProviderId::OpenAI)
            .expect("the slow provider was not recorded");
        assert_eq!(failure.category, ProviderErrorCategory::Timeout);
    }

    #[tokio::test]
    async fn should_pass_credentials_to_providers() {
        let provider = || StubProvider {
            id: ProviderId::OpenAI,
            behavior: StubBehavior::RequireKey,
        };

        // Without a key the provider drops out as unavailable.
        let without = Router::default()
            .with_remote(ProviderId::OpenAI, Box::new(provider()))
            .fetch_models(HashMap::new(), TEST_TIMEOUT)
            .await;
        assert!(without.failed_providers.contains_key(&ProviderId::OpenAI));

        // Supplying the key lets the same provider list its models.
        let credentials = HashMap::from([(
            ProviderId::OpenAI,
            SecretString::from("sk-test".to_string()),
        )]);
        let with = Router::default()
            .with_remote(ProviderId::OpenAI, Box::new(provider()))
            .fetch_models(credentials, TEST_TIMEOUT)
            .await;
        assert!(with.failed_providers.is_empty());
    }
}
