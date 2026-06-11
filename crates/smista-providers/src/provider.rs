//! The [`Provider`] trait: the interface the router uses to discover models and
//! resolve a reference into an executable [`Model`].
//!
//! A provider represents a single upstream account or endpoint (an OpenAI key,
//! an Anthropic key, a local Ollama daemon, …). It is the entry point into the
//! adapter layer: the router asks a provider which models it offers and turns a
//! [`ModelReference`] into an `Arc<dyn Model>` it can execute. Provider
//! implementations own credentials and connection details; everything beyond
//! resolution happens through the [`Model`] abstraction.
//!
//! Available providers include:
//!
//! - [`anthropic::AnthropicProvider`] for Anthropic's API.
//! - [`openai::OpenAIProvider`] for OpenAI's API.
//! - [`openai_compat::OpenAICompatProvider`] for a named OpenAI-compatible
//!   endpoint (vLLM, LM Studio, a gateway, …).
//! - [`ollama::OllamaProvider`] for a local or self-hosted Ollama instance.

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod openai_compat;

use std::sync::Arc;

use smista_core::model::{ModelReference, Provider as ProviderId};

use crate::ProviderResult;
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

    /// Resolves a reference into an executable model offered by this provider.
    ///
    /// On success the returned `Arc<dyn Model>` is ready to serve completions
    /// and exposes the model's facts via [`Model::descriptor`]. The handle is
    /// reference-counted so the router can share one model across concurrent
    /// tasks.
    ///
    /// # Errors
    ///
    /// Returns a [`smista_core::error::ProviderError`] whose
    /// [`category`](smista_core::error::ProviderError::category) classifies the failure.
    ///
    /// In particular, a reference this provider does not offer yields
    /// [`smista_core::error::ProviderErrorCategory::ModelNotFound`].
    ///
    /// Authentication, credential and connectivity problems surfaced while resolving map to their respective categories.
    async fn resolve(&self, reference: &ModelReference) -> ProviderResult<Arc<dyn Model>>;

    /// Lists every model this provider currently offers.
    ///
    /// Each returned [`ModelReference`] is resolvable through
    /// [`Self::resolve`]. The router uses this to populate its catalog and to
    /// validate configured routes against what the provider actually exposes.
    ///
    /// # Errors
    ///
    /// Returns a [`smista_core::error::ProviderError`] when the provider cannot be reached or
    /// rejects the request — for example, missing or invalid credentials, or an
    /// unavailable upstream.
    async fn list_models(&self) -> ProviderResult<Vec<ModelReference>>;
}
