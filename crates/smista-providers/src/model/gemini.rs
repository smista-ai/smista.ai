//! Google Gemini models.

use std::sync::Arc;

use rig_core::providers::gemini::client::Client as GeminiClient;
use rust_decimal::Decimal;
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::{ModelDescriptor, ModelReference, Provider};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::auth::Authentication;
use crate::memory::{MemoryScope, MemoryStorage};
use crate::model::Model;

/// Returns the input and output price per million tokens for `model_id`.
///
/// The `v1beta/models` listing does not report price, so the per-token costs are
/// kept here in a hand-maintained census. Unlike Anthropic, Gemini has no public
/// per-family rate, so each model is priced individually and an id matching no
/// entry is left unpriced (`None`).
///
/// The match is longest-prefix first so that more specific ids (`gemini-2.5-pro`,
/// `gemini-2.5-flash-lite`) win over the broader families they share a stem with.
/// The `default text tier` rate is stored; modality-specific input pricing, the
/// higher tier above the 200k-token prompt breakpoint, and the batch/flex/priority
/// tiers are intentionally not modelled.
///
/// Sources (per million tokens, standard text tier):
/// - `gemini-2.5-pro`: $1.25 in / $10.00 out
/// - `gemini-2.5-flash`: $0.30 in / $2.50 out
/// - `gemini-2.5-flash-lite`: $0.10 in / $0.40 out
///   (Google AI for Developers pricing, <https://ai.google.dev/gemini-api/docs/pricing>)
/// - `gemini-3.1-pro-preview`: $2.00 in / $12.00 out
/// - `gemini-3.5-flash`: $1.50 in / $9.00 out
///   (current smista.ai census for the 3.x generation)
pub fn pricing(model_id: &str) -> (Option<Decimal>, Option<Decimal>) {
    // Decimal::new(mantissa, scale): scale 2 reads as cents, scale 0 as dollars.
    let (input, output) = if model_id.starts_with("gemini-3.5-flash") {
        (Decimal::new(150, 2), Decimal::new(9, 0))
    } else if model_id.starts_with("gemini-3.1-pro") {
        (Decimal::new(2, 0), Decimal::new(12, 0))
    } else if model_id.starts_with("gemini-2.5-pro") {
        (Decimal::new(125, 2), Decimal::new(10, 0))
    } else if model_id.starts_with("gemini-2.5-flash-lite") {
        (Decimal::new(10, 2), Decimal::new(40, 2))
    } else if model_id.starts_with("gemini-2.5-flash") {
        (Decimal::new(30, 2), Decimal::new(250, 2))
    } else {
        return (None, None);
    };
    (Some(input), Some(output))
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
/// [`ModelDescriptor`] it is constructed with (sourced from the provider's live
/// API listing), and its [`reference`](Model::reference) is derived from that
/// descriptor so the two can never disagree. The credential is supplied per
/// request as an [`Authentication`] when the model is resolved.
pub struct GeminiModel {
    agent: Agent<GeminiClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl GeminiModel {
    /// Creates a new Gemini model from the given arguments and descriptor,
    /// authenticating with the supplied [`Authentication`].
    ///
    /// `preamble_segments` are appended to the model's preamble after the base
    /// text and the memory, preserving order; pass an empty slice to add
    /// nothing.
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
        scope: MemoryScope,
        preamble_segments: &[String],
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
            preamble_segments: preamble_segments.to_vec(),
            storage,
            scope,
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
    use super::*;
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; only its type identity matters here.
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
}

#[cfg(test)]
mod pricing_tests {
    use super::*;

    #[test]
    fn should_price_each_known_model() {
        let cases = [
            ("gemini-2.5-pro", Decimal::new(125, 2), Decimal::new(10, 0)),
            (
                "gemini-2.5-flash",
                Decimal::new(30, 2),
                Decimal::new(250, 2),
            ),
            (
                "gemini-2.5-flash-lite",
                Decimal::new(10, 2),
                Decimal::new(40, 2),
            ),
            (
                "gemini-3.1-pro-preview",
                Decimal::new(2, 0),
                Decimal::new(12, 0),
            ),
            ("gemini-3.5-flash", Decimal::new(150, 2), Decimal::new(9, 0)),
        ];

        for (id, input, output) in cases {
            let (got_in, got_out) = pricing(id);
            assert_eq!(got_in, Some(input), "input price for {id}");
            assert_eq!(got_out, Some(output), "output price for {id}");
        }
    }

    #[test]
    fn should_prefer_the_more_specific_flash_lite_over_flash() {
        // `gemini-2.5-flash-lite` shares the `gemini-2.5-flash` stem, so the
        // longest-prefix order must price it at the lite rate, not the flash one.
        let (input, output) = pricing("gemini-2.5-flash-lite");
        assert_eq!(input, Some(Decimal::new(10, 2)));
        assert_eq!(output, Some(Decimal::new(40, 2)));
    }

    #[test]
    fn should_leave_an_unknown_model_unpriced() {
        let (input, output) = pricing("gemini-1.0-ultra");
        assert_eq!(input, None);
        assert_eq!(output, None);
    }
}
