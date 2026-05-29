#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-storage
//!
//! Storage layer for smista.ai. Defines the persistence entities (users,
//! sessions, tokens, messages, routing decisions, tool calls, approvals,
//! plans, diffs and trace events) and the storage traits used to access them.
//!
//! Application code depends on the storage traits rather than on SurrealDB
//! directly; SurrealDB-specific code stays behind this boundary and supports
//! both embedded (local-first) and remote (SaaS) deployments.
//!
