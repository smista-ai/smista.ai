//! OpenAI [`Provider`] implementation.

use std::sync::Arc;

use smista_core::error::ProviderErrorCategory;
use smista_core::model::{
    ModelDescriptor, ModelReference, Provider as ProviderId, ProviderDescriptor,
};

use super::Provider;
use crate::ProviderResult;
use crate::auth::Authentication;
use crate::memory::MemoryStorage;
use crate::model::Model;
use crate::model::openai::{self, OpenAIModel, OpenAIModelArgs};

/// A [`Provider`] backed by a single OpenAI account.
///
/// Offers the GPT models smista.ai supports and resolves a [`ModelReference`]
/// into an executable [`Model`] by constructing it from the shared
/// [`OpenAIModelArgs`]. The credential is not held by the provider: it is
/// supplied per request as an [`Authentication`] when a model is resolved, so a
/// single long-lived provider can serve many callers.
pub struct OpenAIProvider<S>
where
    S: MemoryStorage + 'static,
{
    args: OpenAIModelArgs<S>,
}

impl<S> std::fmt::Debug for OpenAIProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIProvider")
            .field("args", &self.args)
            .finish()
    }
}

impl<S> From<OpenAIModelArgs<S>> for OpenAIProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn from(args: OpenAIModelArgs<S>) -> Self {
        Self::new(args)
    }
}

impl<S> OpenAIProvider<S>
where
    S: MemoryStorage + 'static,
{
    /// Creates a new [`OpenAIProvider`] from the given arguments.
    pub fn new(args: OpenAIModelArgs<S>) -> Self {
        Self { args }
    }
}

#[async_trait::async_trait]
impl<S> Provider for OpenAIProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        ProviderId::OpenAI
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: ProviderId::OpenAI,
            display_name: "OpenAI".to_string(),
            local: false,
        }
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
    ) -> ProviderResult<Arc<dyn Model>> {
        let descriptor = openai::catalog()
            .into_iter()
            .find(|descriptor| &descriptor.reference() == reference)
            .ok_or_else(|| {
                crate::error::provider_error(
                    ProviderErrorCategory::ModelNotFound,
                    self.id(),
                    Some(reference.model.clone()),
                    format!("model `{}` not found in OpenAI provider", reference.model),
                )
            })?;

        Ok(Arc::new(
            OpenAIModel::new(self.args.clone(), authentication, descriptor).await?,
        ))
    }

    async fn list_models(
        &self,
        _authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelReference>> {
        Ok(openai::catalog()
            .iter()
            .map(ModelDescriptor::reference)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::*;
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; resolution paths under test never
    /// touch it.
    struct NoStorage;

    impl MemoryStorage for NoStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_user_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    fn provider() -> OpenAIProvider<NoStorage> {
        OpenAIProvider::new(OpenAIModelArgs {
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        })
    }

    fn authentication() -> Authentication {
        Authentication::ApiKey(SecretString::from("sk-openai-test"))
    }

    #[test]
    fn should_identify_as_openai() {
        assert_eq!(provider().id(), ProviderId::OpenAI);
    }

    #[tokio::test]
    async fn should_list_every_offered_model() {
        let listed = provider()
            .list_models(&authentication())
            .await
            .expect("listing cannot fail");

        assert_eq!(
            listed,
            vec![
                openai::gpt_5_4().reference(),
                openai::gpt_5_4_mini().reference(),
                openai::gpt_5_5().reference(),
            ]
        );
    }

    #[tokio::test]
    async fn should_reject_unknown_model_with_model_not_found() {
        let unknown = ModelReference {
            provider: ProviderId::OpenAI,
            model: "claude-does-not-exist".to_string(),
        };

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider().resolve(&unknown, &authentication()).await else {
            panic!("an unoffered model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, ProviderId::OpenAI);
        assert_eq!(error.model.as_deref(), Some("claude-does-not-exist"));
    }

    #[tokio::test]
    async fn should_reject_resolution_without_an_api_key() {
        // An offered model still cannot be built without a credential: the
        // provider holds none, so a keyless authentication is rejected before
        // any client is constructed.
        let Err(error) = provider()
            .resolve(&openai::gpt_5_4().reference(), &Authentication::None)
            .await
        else {
            panic!("a model must not resolve without an API key");
        };

        assert_eq!(error.category, ProviderErrorCategory::MissingCredentials);
        assert_eq!(error.provider, ProviderId::OpenAI);
    }
}
