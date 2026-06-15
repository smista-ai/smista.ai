//! OpenAI-compatible [`Provider`] implementation.

use std::collections::HashMap;
use std::sync::Arc;

use smista_core::error::ProviderErrorCategory;
use smista_core::model::{
    ModelDescriptor, ModelReference, Provider as ProviderId, ProviderDescriptor,
};

use super::Provider;
use crate::ProviderResult;
use crate::auth::Authentication;
use crate::memory::{MemoryScope, MemoryStorage};
use crate::model::Model;
use crate::model::openai_compat::{OpenAICompatEndpoint, OpenAICompatModel, OpenAICompatRuntime};

/// A [`Provider`] backed by a single OpenAI-compatible endpoint.
///
/// Each instance is a named endpoint (`openai-compat:<name>`) declared in
/// configuration: a base URL and a fixed set of models with hand-supplied
/// facts. The provider offers exactly those models and resolves a
/// [`ModelReference`] into an executable [`Model`] built from the shared
/// [`OpenAICompatEndpoint`] and [`OpenAICompatRuntime`]; the credential is
/// supplied per request as an [`Authentication`].
///
/// The provider's [`id`](Provider::id) is its own named identity — a
/// [`ProviderId::OpenAICompatible`] — never the bare [`ProviderId::OpenAI`]: a
/// single deployment can configure several compatible endpoints, and each must
/// route and catalog independently of the others and of OpenAI itself.
pub struct OpenAICompatProvider<S>
where
    S: MemoryStorage + 'static,
{
    /// This instance's named identity, e.g. `openai-compat:my-vllm`.
    id: ProviderId,
    /// Human-friendly name shown to users, supplied by configuration.
    display_name: String,
    /// Whether this instance serves its models locally, supplied by
    /// configuration. Every model the instance offers must agree with it; a
    /// disagreement is rejected when the provider is built.
    local: bool,
    /// The single source of truth for the instance's base URL.
    endpoint: OpenAICompatEndpoint,
    /// Runtime (preamble + memory backend) used to construct each resolved model.
    runtime: OpenAICompatRuntime<S>,
    /// The models this instance offers, keyed by reference.
    models: HashMap<ModelReference, ModelDescriptor>,
}

// `#[derive(Debug)]` would add a spurious `S: Debug` bound; the fields already
// carry hand-written `Debug` impls (the endpoint redacts its credential), so
// forward to them explicitly.
impl<S> std::fmt::Debug for OpenAICompatProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAICompatProvider")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("local", &self.local)
            .field("endpoint", &self.endpoint)
            .field("runtime", &self.runtime)
            .field("models", &format_args!("[{} model(s)]", self.models.len()))
            .finish()
    }
}

impl<S> OpenAICompatProvider<S>
where
    S: MemoryStorage + 'static,
{
    /// Creates a new [`OpenAICompatProvider`] for a named instance.
    ///
    /// `id` is the instance's identity (a [`ProviderId::OpenAICompatible`]) and
    /// is returned verbatim from [`Provider::id`]. `display_name` is the
    /// human-friendly name and `is_local` whether the instance runs locally;
    /// both come from configuration. `endpoint` carries the base URL, `runtime`
    /// the preamble and memory backend, and `models` the facts for every model
    /// the instance offers, keyed by the reference each resolves under. The
    /// credential is supplied per request when resolving.
    ///
    /// Every supplied model's [`local`](ModelDescriptor::local) flag must equal
    /// `is_local`: the instance's locality is the source of truth, and a model
    /// that disagrees would let the provider advertise as one thing while a
    /// model it offers behaves as another.
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`](smista_core::error::ProviderError) with
    /// category
    /// [`InvalidConfiguration`](smista_core::error::ProviderErrorCategory::InvalidConfiguration)
    /// if any model's locality disagrees with `is_local`.
    pub fn new(
        id: ProviderId,
        display_name: impl Into<String>,
        is_local: bool,
        endpoint: OpenAICompatEndpoint,
        runtime: OpenAICompatRuntime<S>,
        models: HashMap<ModelReference, ModelDescriptor>,
    ) -> ProviderResult<Self> {
        if let Some((reference, _)) = models.iter().find(|(_, d)| d.local != is_local) {
            return Err(crate::error::provider_error(
                ProviderErrorCategory::InvalidConfiguration,
                id.clone(),
                Some(reference.model.clone()),
                format!(
                    "model `{}` has local={} but OpenAI-compatible instance `{id}` is configured local={is_local}",
                    reference.model, !is_local
                ),
            ));
        }

        Ok(Self {
            id,
            display_name: display_name.into(),
            local: is_local,
            endpoint,
            runtime,
            models,
        })
    }
}

#[async_trait::async_trait]
impl<S> Provider for OpenAICompatProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            local: self.local,
        }
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
        scope: MemoryScope,
    ) -> ProviderResult<Arc<dyn Model>> {
        let Some(descriptor) = self.models.get(reference).cloned() else {
            return Err(crate::error::provider_error(
                ProviderErrorCategory::ModelNotFound,
                self.id(),
                Some(reference.model.clone()),
                format!(
                    "model `{}` not found in OpenAI-compatible provider `{}`",
                    reference.model, self.id
                ),
            ));
        };

        Ok(Arc::new(
            OpenAICompatModel::new(
                &self.endpoint,
                &self.runtime,
                authentication,
                descriptor,
                scope,
            )
            .await?,
        ))
    }

    async fn list_models(
        &self,
        _authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelReference>> {
        Ok(self.models.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use smista_core::model::{ModelAuthRequirement, ModelCapabilities, ModelParameters};
    use uuid::Uuid;

    use super::*;

    /// A throwaway memory scope; resolution does not bake it into the provider.
    fn scope() -> MemoryScope {
        MemoryScope {
            user_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
        }
    }
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; resolution paths under test never
    /// touch it.
    struct NoStorage;

    impl MemoryStorage for NoStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_user_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    fn instance_id() -> ProviderId {
        ProviderId::OpenAICompatible("my-vllm".to_string())
    }

    fn reference(model: &str) -> ModelReference {
        ModelReference {
            provider: instance_id(),
            model: model.to_string(),
        }
    }

    fn descriptor(model: &str) -> ModelDescriptor {
        ModelDescriptor {
            provider: instance_id(),
            model: model.to_string(),
            display_name: None,
            local: true,
            auth: ModelAuthRequirement::None,
            capabilities: ModelCapabilities::default(),
            max_context_tokens: 32_768,
            max_output_tokens: None,
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    /// The fixture authors every descriptor as `local: true`, so a provider is
    /// only consistent when built as a local instance.
    fn provider_with(models: &[&str]) -> OpenAICompatProvider<NoStorage> {
        try_provider(models, true).expect("a local instance with local models is consistent")
    }

    fn try_provider(
        models: &[&str],
        is_local: bool,
    ) -> ProviderResult<OpenAICompatProvider<NoStorage>> {
        let models = models
            .iter()
            .map(|m| (reference(m), descriptor(m)))
            .collect();
        OpenAICompatProvider::new(
            instance_id(),
            "My vLLM",
            is_local,
            OpenAICompatEndpoint::new("http://localhost:8000/v1"),
            OpenAICompatRuntime {
                preamble: "be helpful".to_string(),
                storage: Arc::new(NoStorage),
            },
            models,
        )
    }

    fn authentication() -> Authentication {
        Authentication::None
    }

    #[test]
    fn should_identify_as_its_named_instance_not_bare_openai() {
        let provider = provider_with(&[]);

        assert_eq!(provider.id(), instance_id());
        assert_ne!(provider.id(), ProviderId::OpenAI);
    }

    #[test]
    fn should_expose_its_descriptor_with_configured_locality() {
        // An empty model set agrees with any locality, so a remote instance is
        // consistent here.
        let descriptor = try_provider(&[], false)
            .expect("an instance with no models is consistent")
            .descriptor();

        assert_eq!(descriptor.id, instance_id());
        assert_eq!(descriptor.display_name, "My vLLM");
        assert!(!descriptor.local);
    }

    #[test]
    fn should_reject_a_model_whose_locality_disagrees_with_the_instance() {
        // The fixture authors the model as `local: true`; building a remote
        // (`is_local = false`) instance with it is contradictory configuration.
        let Err(error) = try_provider(&["llama-3.1-70b"], false) else {
            panic!("a model that disagrees on locality must be rejected");
        };

        assert_eq!(error.category, ProviderErrorCategory::InvalidConfiguration);
        assert_eq!(error.provider, instance_id());
        assert_eq!(error.model.as_deref(), Some("llama-3.1-70b"));
    }

    #[tokio::test]
    async fn should_list_configured_models() {
        let provider = provider_with(&["llama-3.1-70b", "qwen2.5-coder"]);

        let mut listed: Vec<String> = provider
            .list_models(&authentication())
            .await
            .expect("listing cannot fail")
            .into_iter()
            .map(|r| r.model)
            .collect();
        listed.sort();

        assert_eq!(listed, vec!["llama-3.1-70b", "qwen2.5-coder"]);
    }

    #[tokio::test]
    async fn should_reject_unknown_model_with_model_not_found() {
        let provider = provider_with(&["llama-3.1-70b"]);

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider
            .resolve(&reference("does-not-exist"), &authentication(), scope())
            .await
        else {
            panic!("an unconfigured model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, instance_id());
        assert_eq!(error.model.as_deref(), Some("does-not-exist"));
    }

    #[tokio::test]
    async fn should_resolve_a_configured_model() {
        let provider = provider_with(&["llama-3.1-70b"]);

        let resolved = provider
            .resolve(&reference("llama-3.1-70b"), &authentication(), scope())
            .await;

        assert!(resolved.is_ok(), "a configured model must resolve");
    }

    #[tokio::test]
    async fn resolved_model_exposes_the_configured_reference() {
        let provider = provider_with(&["llama-3.1-70b"]);

        let model = provider
            .resolve(&reference("llama-3.1-70b"), &authentication(), scope())
            .await
            .expect("a configured model must resolve");

        assert_eq!(model.reference(), &reference("llama-3.1-70b"));
    }
}
