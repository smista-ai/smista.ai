#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-trace
//!
//! Execution trace types and logic for smista.ai. Records, for every task, the
//! selected model, matched routing rule, task type, provider, fallbacks,
//! overrides, selected and excluded context, tool calls, approvals and costs.
//!
//! Trace events are append-only and back the `/trace` and `/why` commands.
//!
