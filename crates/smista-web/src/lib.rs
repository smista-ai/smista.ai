//! # smista-web
//!
//! HTTP JSON API server for smista-router, built on `axum`. Exposes the local
//! REST API under `/api/v1`: authentication, sessions, execution, streaming,
//! route preview, approvals, traces, providers/models and usage.
//!
//! It handles request authentication, session tokens, credential headers,
//! streaming responses and secret redaction.
//!
