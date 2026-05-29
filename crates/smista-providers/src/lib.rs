#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-providers
//!
//! Provider integration layer for smista.ai. Exposes a common internal model
//! interface so the router can execute requests against different providers
//! without coupling routing logic to provider-specific APIs.
//!
//! Initial providers are OpenAI, Anthropic, Ollama and OpenAI-compatible
//! endpoints, integrated through `rig` where practical. `rig` remains an
//! implementation detail of this adapter layer.
//!
