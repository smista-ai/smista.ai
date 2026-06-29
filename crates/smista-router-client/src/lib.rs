#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-router-client
//!
//! Async Rust client for the `smista-router` HTTP JSON API (`/api/v1`).
//!
//! Exposes a `SmistaRouterClient` trait covering every router endpoint —
//! authentication, sessions, execution, streaming, route preview, approvals,
//! traces, providers/models and usage — plus a `reqwest`-backed implementation.
//! The CLI and other Rust frontends depend on this instead of calling the API
//! by hand; routing logic stays in the router.
//!
//! Request and response types come from `smista-core`. Credentials travel in
//! headers and are never logged, traced or sent as model context.
//!
//! # Scope
//!
//! This crate is the backend-agnostic contract: the [`Client`] trait plus the
//! credential ([`ApiKey`], [`SessionToken`], [`ProviderCredentials`]), error
//! ([`RouterClientError`], [`Result`]) and connection ([`RouterClientConfig`])
//! types it relies on. It depends on no HTTP library. The concrete HTTP clients
//! that implement [`Client`] live in separate crates, one per backend.
//!
//! # Using the client
//!
//! [`Client`] returns native [`impl Future`](std::future::Future), so it is not
//! object safe — consumers use it through a generic `C: Client`, not
//! `dyn Client`, and pick one concrete client per application:
//!
//! ```
//! use smista_router_client::{Client, Result};
//! use smista_core::api::StatusResponse;
//!
//! async fn check<C: Client>(client: &C) -> Result<StatusResponse> {
//!     client.status().await
//! }
//! ```

mod client;
mod config;
mod credentials;
mod error;
#[cfg(test)]
mod mock;

pub use self::client::Client;
pub use self::config::RouterClientConfig;
pub use self::credentials::{ApiKey, ProviderCredentials, SessionToken};
pub use self::error::{Result, RouterClientError};
