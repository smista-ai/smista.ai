//! # smista-core
//!
//! Shared internal runtime for smista.ai, used by both the CLI and the router.
//!
//! It holds the reusable domain types: task intents, provider and model
//! descriptors, routing policy structures, tool permission and privacy models,
//! configuration schemas, error types and common validation logic. It must not
//! depend on terminal-specific or server-specific concerns.
//!
//! Implementation is tracked in milestone M1.
