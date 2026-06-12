//! Google Gemini models.

mod flash;
mod pro;

use std::sync::Arc;

use rig_core::providers::gemini::client::Client as GeminiClient;
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::{ModelDescriptor, ModelReference, Provider};

#[doc(inline)]
pub use self::flash::{gemini_2_5_flash, gemini_3_5_flash};
#[doc(inline)]
pub use self::pro::{gemini_2_5_pro, gemini_3_1_pro_preview};
use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::auth::Authentication;
use crate::memory::MemoryStorage;
use crate::model::Model;

/// Returns the descriptors for every Gemini model smista.ai offers.
///
/// The single catalog the provider reads to resolve and list models, so the set
/// of offered models is declared in one place.
pub fn catalog() -> Vec<ModelDescriptor> {
    vec![
        gemini_2_5_pro(),
        gemini_2_5_flash(),
        gemini_3_1_pro_preview(),
        gemini_3_5_flash(),
    ]
}

/// Arguments for creating a new Gemini model.
///
/// Carries the base system prompt and memory backend every Gemini model needs to
/// construct its underlying agent. The credential is not held here: it is
/// supplied per request as an [`Authentication`]
/// when the model is resolved.
pub struct GeminiModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    /// The base system prompt; the model appends its memory preamble to this.
    pub preamble: String,
    /// The memory backend the model reads to build its preamble and writes
    /// through for the memory tool.
    pub storage: Arc<S>,
}

// `#[derive(Debug)]` would add a spurious `S: Debug` bound; render the storage
// type name by hand instead.
impl<S> std::fmt::Debug for GeminiModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiModelArgs")
            .field("preamble", &self.preamble)
            .field(
                "storage",
                &format_args!("Arc<{}>", std::any::type_name::<S>()),
            )
            .finish()
    }
}

// `#[derive(Clone)]` would add a spurious `S: Clone` bound; `Arc<S>` is `Clone`
// for any `S`, so implement it by hand without the bound.
impl<S> Clone for GeminiModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    fn clone(&self) -> Self {
        Self {
            preamble: self.preamble.clone(),
            storage: Arc::clone(&self.storage),
        }
    }
}

/// A Gemini model.
///
/// One shared adapter for every Gemini model: its facts come from the
/// [`ModelDescriptor`] it is constructed with (returned by a facts function such
/// as [`gemini_2_5_pro`]), and its [`reference`](Model::reference) is derived
/// from that descriptor so the two can never disagree. The credential is
/// supplied per request as an [`Authentication`] when the model is resolved.
pub struct GeminiModel {
    agent: Agent<GeminiClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl GeminiModel {
    /// Creates a new Gemini model from the given arguments and descriptor,
    /// authenticating with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        GeminiModelArgs { preamble, storage }: GeminiModelArgs<S>,
        authentication: &Authentication,
        descriptor: ModelDescriptor,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        let reference = descriptor.reference();
        tracing::debug!("Creating Gemini {} model", reference.model);
        let api_key = authentication.require_api_key(Provider::Gemini, &reference.model)?;
        let client = GeminiClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Gemini,
                Some(reference.model.clone()),
                "Failed to create Gemini client",
            )
        })?;

        tracing::debug!("Creating agent for Gemini {} model", reference.model);
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor.clone(),
            preamble,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Gemini {} model", reference.model);
        Ok(Self {
            agent,
            descriptor,
            reference,
        })
    }
}

#[async_trait::async_trait]
impl Model for GeminiModel {
    fn reference(&self) -> &ModelReference {
        &self.reference
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        self.agent.complete(request).await
    }

    async fn stream(&self, request: CompletionRequest) -> ProviderResult<ResponseStream> {
        self.agent.stream(request).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use smista_core::model::Provider;

    use super::*;
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; only its type identity matters here.
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

    #[test]
    fn should_render_storage_type_in_debug_without_an_s_debug_bound() {
        let args = GeminiModelArgs {
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        };

        let rendered = format!("{args:?}");
        assert!(rendered.contains("GeminiModelArgs"));
        assert!(rendered.contains("be helpful"));
    }

    #[test]
    fn should_clone_args_without_requiring_storage_clone() {
        let args = GeminiModelArgs {
            preamble: "preamble".to_string(),
            storage: Arc::new(NoStorage),
        };

        let cloned = args.clone();
        assert_eq!(cloned.preamble, "preamble");
        // The clone shares the same backend rather than duplicating it.
        assert!(Arc::ptr_eq(&args.storage, &cloned.storage));
    }

    #[test]
    fn should_expose_distinct_model_descriptors() {
        let descriptors = catalog();

        // Every descriptor targets the Gemini provider...
        assert!(
            descriptors
                .iter()
                .all(|descriptor| descriptor.provider == Provider::Gemini)
        );
        // ...and names a distinct model.
        let models: BTreeSet<&str> = descriptors.iter().map(|d| d.model.as_str()).collect();
        assert_eq!(models.len(), descriptors.len());
    }
}
