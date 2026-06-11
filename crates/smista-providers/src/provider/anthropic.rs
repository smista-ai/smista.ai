//! Anthropic [`Provider`] implementation.

use std::sync::Arc;

use smista_core::error::ProviderErrorCategory;
use smista_core::model::{ModelReference, Provider as ProviderId};

use super::Provider;
use crate::ProviderResult;
use crate::memory::MemoryStorage;
use crate::model::Model;
use crate::model::anthropic::{
    self, AnthropicModelArgs, Haiku_4_5, Opus_4_6, Opus_4_7, Opus_4_8, Sonnet_4_6,
};

/// A [`Provider`] backed by a single Anthropic account.
///
/// Offers the Claude models smista.ai supports and resolves a [`ModelReference`]
/// into an executable [`Model`] by constructing it from the shared
/// [`AnthropicModelArgs`]. The credential and memory backend are captured once
/// at construction and reused for every resolved model.
pub struct AnthropicProvider<S>
where
    S: MemoryStorage + 'static,
{
    args: AnthropicModelArgs<S>,
}

impl<S> std::fmt::Debug for AnthropicProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicProvider")
            .field("args", &self.args)
            .finish()
    }
}

impl<S> From<AnthropicModelArgs<S>> for AnthropicProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn from(args: AnthropicModelArgs<S>) -> Self {
        Self::new(args)
    }
}

impl<S> AnthropicProvider<S>
where
    S: MemoryStorage + 'static,
{
    /// Creates a new [`AnthropicProvider`] from the given arguments.
    pub fn new(args: AnthropicModelArgs<S>) -> Self {
        Self { args }
    }
}

#[async_trait::async_trait]
impl<S> Provider for AnthropicProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    async fn resolve(&self, reference: &ModelReference) -> ProviderResult<Arc<dyn Model>> {
        Ok(match reference {
            reference if reference == &anthropic::haiku_4_5() => {
                Arc::new(Haiku_4_5::new(self.args.clone()).await?)
            }
            reference if reference == &anthropic::opus_4_6() => {
                Arc::new(Opus_4_6::new(self.args.clone()).await?)
            }
            reference if reference == &anthropic::opus_4_7() => {
                Arc::new(Opus_4_7::new(self.args.clone()).await?)
            }
            reference if reference == &anthropic::opus_4_8() => {
                Arc::new(Opus_4_8::new(self.args.clone()).await?)
            }
            reference if reference == &anthropic::sonnet_4_6() => {
                Arc::new(Sonnet_4_6::new(self.args.clone()).await?)
            }
            _ => {
                return Err(crate::error::provider_error(
                    ProviderErrorCategory::ModelNotFound,
                    self.id(),
                    Some(reference.model.clone()),
                    format!(
                        "model `{}` not found in Anthropic provider",
                        reference.model
                    ),
                ));
            }
        })
    }

    async fn list_models(&self) -> ProviderResult<Vec<ModelReference>> {
        Ok(vec![
            anthropic::haiku_4_5(),
            anthropic::opus_4_6(),
            anthropic::opus_4_7(),
            anthropic::opus_4_8(),
            anthropic::sonnet_4_6(),
        ])
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

    fn provider() -> AnthropicProvider<NoStorage> {
        AnthropicProvider::new(AnthropicModelArgs {
            api_key: SecretString::from("sk-ant-test"),
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        })
    }

    #[test]
    fn should_identify_as_anthropic() {
        assert_eq!(provider().id(), ProviderId::Anthropic);
    }

    #[tokio::test]
    async fn should_list_every_offered_model() {
        let listed = provider().list_models().await.expect("listing cannot fail");

        assert_eq!(
            listed,
            vec![
                anthropic::haiku_4_5(),
                anthropic::opus_4_6(),
                anthropic::opus_4_7(),
                anthropic::opus_4_8(),
                anthropic::sonnet_4_6(),
            ]
        );
    }

    #[tokio::test]
    async fn should_reject_unknown_model_with_model_not_found() {
        let unknown = ModelReference {
            provider: ProviderId::Anthropic,
            model: "claude-does-not-exist".to_string(),
        };

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider().resolve(&unknown).await else {
            panic!("an unoffered model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, ProviderId::Anthropic);
        assert_eq!(error.model.as_deref(), Some("claude-does-not-exist"));
    }
}
