//! The [`Model`] trait: the interface routing and execution use to invoke a
//! provider-backed model.
//!
//! Routing interacts with models *only* through this abstraction, so the
//! orchestration layer never couples to a provider's SDK or wire format. The
//! trait is also the single source of truth for a model's **facts** — its
//! capabilities, limits, auth requirements and costs — which are read from the
//! provider's knowledge at runtime rather than hand-declared in configuration.

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod openai_compat;

use smista_core::model::{ModelDescriptor, ModelReference};

use crate::ProviderResult;
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};

/// A provider-backed model: the interface routing and execution use, and the
/// source of truth for a model's facts.
///
/// Implementors adapt a concrete provider (OpenAI, Anthropic, Ollama, …) to the
/// provider-agnostic request/response vocabulary in [`crate::api`]. The trait is
/// object-safe — the router holds models as `Arc<dyn Model>` — which is why it
/// uses [`async_trait`] rather than native `async fn` in traits, and requires
/// [`Send`] + [`Sync`] so a model can be shared across tasks.
#[async_trait::async_trait]
pub trait Model: Send + Sync {
    /// Returns this model's stable `provider/model` identity.
    ///
    /// A cheap accessor for logging and map keys that avoids assembling a full
    /// [`ModelDescriptor`].
    fn reference(&self) -> &ModelReference;

    /// Returns the model's full descriptor.
    ///
    /// Covers identity, capabilities, context/output limits, auth requirement,
    /// locality, costs and default parameters — assembled from the provider's
    /// knowledge, not from configuration. The descriptor is the single source
    /// of truth for facts: routing reads `descriptor().capabilities` to decide
    /// whether a model can serve a task, so there is no separate
    /// `capabilities()` method.
    fn descriptor(&self) -> &ModelDescriptor;

    /// Sends a request and awaits the full completion.
    ///
    /// Returns the model's final content, any tool calls it issued for the
    /// router to mediate, usage totals and the reason generation stopped.
    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse>;

    /// Sends a request and returns a stream of response events.
    ///
    /// The streaming counterpart to [`Self::complete`]: events carry partial or
    /// final content, tool-call activity, usage updates and a terminal marker as
    /// the model produces them.
    async fn stream(&self, request: CompletionRequest) -> ProviderResult<ResponseStream>;
}
