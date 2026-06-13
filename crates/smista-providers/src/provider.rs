//! The [`Provider`] trait: the interface the router uses to discover models and
//! resolve a reference into an executable [`Model`].
//!
//! A provider represents a single upstream account or endpoint (an OpenAI
//! account, an Anthropic account, a local Ollama daemon, …). It is the entry
//! point into the adapter layer: the router asks a provider which models it
//! offers and turns a [`ModelReference`] into an `Arc<dyn Model>` it can
//! execute. Provider implementations own connection and configuration details
//! only; credentials are not held by the provider but supplied per request as an
//! [`Authentication`], so a single long-lived provider can serve many callers.
//! Everything beyond resolution happens through the [`Model`] abstraction.
//!
//! Available providers include:
//!
//! - [`anthropic::AnthropicProvider`] for Anthropic's API.
//! - [`gemini::GeminiProvider`] for Google's Gemini API.
//! - [`openai::OpenAIProvider`] for OpenAI's API.
//! - [`openai_compat::OpenAICompatProvider`] for a named OpenAI-compatible
//!   endpoint (vLLM, LM Studio, a gateway, …).
//! - [`ollama::OllamaProvider`] for a local or self-hosted Ollama instance.

pub mod anthropic;
mod cache;
pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod openai_compat;

use std::sync::Arc;

use smista_core::model::{ModelReference, Provider as ProviderId, ProviderDescriptor};

use crate::ProviderResult;
use crate::auth::Authentication;
use crate::model::Model;

/// A provider account or endpoint: discovers models and resolves them for
/// execution.
///
/// Implementors adapt a concrete upstream (OpenAI, Anthropic, Ollama, …) to a
/// uniform discovery and resolution interface, so the router can enumerate
/// available models and obtain executable handles without coupling to a
/// provider's SDK or wire format. The trait is object-safe — the router holds
/// providers behind a trait object — which is why it uses [`async_trait`]
/// rather than native `async fn` in traits, and requires [`Send`] + [`Sync`] so
/// a provider can be shared across tasks.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Returns this provider's stable identity.
    ///
    /// A cheap accessor used as a map key and for logging, identifying which
    /// upstream (`openai`, `anthropic`, …) the provider speaks to.
    fn id(&self) -> ProviderId;

    /// Returns this provider's descriptor: its identity, a human-friendly name,
    /// and whether it serves models locally.
    ///
    /// This is the shape surfaced by `GET /llm/providers`, and
    /// [`ProviderDescriptor::local`] is the provider-level source of truth for
    /// locality — the router's routing discriminant for keeping sensitive work
    /// on local models. Every model this provider resolves inherits that flag,
    /// so a provider can never report itself local while a model it offers
    /// routes to the cloud.
    ///
    /// Unlike [`Self::id`], this allocates the display name, so the router uses
    /// [`Self::id`] for hot map-key lookups and this when it needs the full
    /// metadata.
    fn descriptor(&self) -> ProviderDescriptor;

    /// Resolves a reference into an executable model offered by this provider,
    /// authenticating with the supplied [`Authentication`].
    ///
    /// On success the returned `Arc<dyn Model>` is ready to serve completions
    /// and exposes the model's facts via [`Model::descriptor`]. The handle is
    /// reference-counted so the router can share one model across concurrent
    /// tasks. The credentials are read from `authentication` rather than held by
    /// the provider, so the same provider can resolve for different callers.
    ///
    /// # Errors
    ///
    /// Returns a [`smista_core::error::ProviderError`] whose
    /// [`category`](smista_core::error::ProviderError::category) classifies the failure.
    ///
    /// In particular, a reference this provider does not offer yields
    /// [`smista_core::error::ProviderErrorCategory::ModelNotFound`], and an
    /// [`Authentication`] variant the provider cannot use (for example
    /// [`None`](Authentication::None) where a key is required) yields
    /// [`smista_core::error::ProviderErrorCategory::MissingCredentials`].
    ///
    /// Authentication, credential and connectivity problems surfaced while resolving map to their respective categories.
    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
    ) -> ProviderResult<Arc<dyn Model>>;

    /// Lists every model this provider currently offers, authenticating with the
    /// supplied [`Authentication`].
    ///
    /// Each returned [`ModelReference`] is resolvable through
    /// [`Self::resolve`]. The router uses this to populate its catalog and to
    /// validate configured routes against what the provider actually exposes.
    /// Discovery can reach the upstream (Ollama and OpenAI-compatible endpoints
    /// query the live instance), so the credentials are taken per call rather
    /// than held by the provider.
    ///
    /// # Errors
    ///
    /// Returns a [`smista_core::error::ProviderError`] when the provider cannot be reached or
    /// rejects the request — for example, missing or invalid credentials, or an
    /// unavailable upstream.
    async fn list_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelReference>>;
}
