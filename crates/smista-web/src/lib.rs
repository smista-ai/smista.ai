#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-web
//!
//! HTTP JSON API server for smista-router, built on `axum`. Exposes the local
//! REST API under `/api/v1`: authentication, sessions, execution, streaming,
//! route preview, approvals, traces, providers/models and usage.
//!
//! It handles request authentication, session tokens, credential headers,
//! streaming responses and secret redaction.
//!
