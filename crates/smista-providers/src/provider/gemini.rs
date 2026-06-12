//! Google Gemini [`Provider`] implementation.

use std::sync::Arc;

use smista_core::error::ProviderErrorCategory;
use smista_core::model::{ModelReference, Provider as ProviderId};

use super::Provider;
use crate::ProviderResult;
use crate::auth::Authentication;
use crate::memory::MemoryStorage;
use crate::model::Model;
use crate::model::gemini::{
    self, Gemini_2_5_Flash, Gemini_2_5_Pro, Gemini_3_1_Pro_Preview, Gemini_3_5_Flash,
    GeminiModelArgs,
};

/// A [`Provider`] backed by a single Google Gemini account.
///
/// Offers the Gemini models smista.ai supports and resolves a [`ModelReference`]
/// into an executable [`Model`] by constructing it from the shared
/// [`GeminiModelArgs`]. The credential is not held by the provider: it is
/// supplied per request as an [`Authentication`] when a model is resolved, so a
/// single long-lived provider can serve many callers.
pub struct GeminiProvider<S>
where
    S: MemoryStorage + 'static,
{
    args: GeminiModelArgs<S>,
}

impl<S> std::fmt::Debug for GeminiProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiProvider")
            .field("args", &self.args)
            .finish()
    }
}

impl<S> From<GeminiModelArgs<S>> for GeminiProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn from(args: GeminiModelArgs<S>) -> Self {
        Self::new(args)
    }
}

impl<S> GeminiProvider<S>
where
    S: MemoryStorage + 'static,
{
    /// Creates a new [`GeminiProvider`] from the given arguments.
    pub fn new(args: GeminiModelArgs<S>) -> Self {
        Self { args }
    }
}

#[async_trait::async_trait]
impl<S> Provider for GeminiProvider<S>
where
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
    ) -> ProviderResult<Arc<dyn Model>> {
        Ok(match reference {
            reference if reference == &gemini::gemini_2_5_pro() => {
                Arc::new(Gemini_2_5_Pro::new(self.args.clone(), authentication).await?)
            }
            reference if reference == &gemini::gemini_2_5_flash() => {
                Arc::new(Gemini_2_5_Flash::new(self.args.clone(), authentication).await?)
            }
            reference if reference == &gemini::gemini_3_1_pro_preview() => {
                Arc::new(Gemini_3_1_Pro_Preview::new(self.args.clone(), authentication).await?)
            }
            reference if reference == &gemini::gemini_3_5_flash() => {
                Arc::new(Gemini_3_5_Flash::new(self.args.clone(), authentication).await?)
            }
            _ => {
                return Err(crate::error::provider_error(
                    ProviderErrorCategory::ModelNotFound,
                    self.id(),
                    Some(reference.model.clone()),
                    format!("model `{}` not found in Gemini provider", reference.model),
                ));
            }
        })
    }

    async fn list_models(
        &self,
        _authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelReference>> {
        Ok(vec![
            gemini::gemini_2_5_pro(),
            gemini::gemini_2_5_flash(),
            gemini::gemini_3_1_pro_preview(),
            gemini::gemini_3_5_flash(),
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

    fn provider() -> GeminiProvider<NoStorage> {
        GeminiProvider::new(GeminiModelArgs {
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        })
    }

    fn authentication() -> Authentication {
        Authentication::ApiKey(SecretString::from("gemini-test"))
    }

    #[test]
    fn should_identify_as_gemini() {
        assert_eq!(provider().id(), ProviderId::Gemini);
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
                gemini::gemini_2_5_pro(),
                gemini::gemini_2_5_flash(),
                gemini::gemini_3_1_pro_preview(),
                gemini::gemini_3_5_flash(),
            ]
        );
    }

    #[tokio::test]
    async fn should_reject_unknown_model_with_model_not_found() {
        let unknown = ModelReference {
            provider: ProviderId::Gemini,
            model: "gemini-2.0-flash".to_string(),
        };

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider().resolve(&unknown, &authentication()).await else {
            panic!("an unoffered model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, ProviderId::Gemini);
        assert_eq!(error.model.as_deref(), Some("gemini-2.0-flash"));
    }

    #[tokio::test]
    async fn should_reject_resolution_without_an_api_key() {
        // An offered model still cannot be built without a credential: the
        // provider holds none, so a keyless authentication is rejected before
        // any client is constructed.
        let Err(error) = provider()
            .resolve(&gemini::gemini_2_5_flash(), &Authentication::None)
            .await
        else {
            panic!("a model must not resolve without an API key");
        };

        assert_eq!(error.category, ProviderErrorCategory::MissingCredentials);
        assert_eq!(error.provider, ProviderId::Gemini);
    }
}
