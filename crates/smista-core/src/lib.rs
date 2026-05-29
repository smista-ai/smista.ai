#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-core
//!
//! Shared internal runtime for smista.ai, used by both the CLI and the router.
//!
//! It holds the reusable domain types: task intents, provider and model
//! descriptors, routing policy structures, tool permission and privacy models,
//! configuration schemas, error types and common validation logic. It must not
//! depend on terminal-specific or server-specific concerns.
//!

pub mod api;
pub mod effort;
pub mod error;
pub mod intent;
pub mod message;
pub mod model;
pub mod paths;
pub mod policy;
pub mod secret;
pub mod skill;
pub mod stream;
pub mod trace;
pub mod usage;
