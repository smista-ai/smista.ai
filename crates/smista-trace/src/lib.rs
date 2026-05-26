//! # smista-trace
//!
//! Execution trace types and logic for smista.ai. Records, for every task, the
//! selected model, matched routing rule, task type, provider, fallbacks,
//! overrides, selected and excluded context, tool calls, approvals and costs.
//!
//! Trace events are append-only and back the `/trace` and `/why` commands.
//!
