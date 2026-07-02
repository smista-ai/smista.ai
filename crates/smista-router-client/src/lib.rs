#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-router-client
//!
//! Async Rust client for the `smista-router` HTTP JSON API (`/api/v1`).
//!
//! Exposes a `SmistaRouterClient` trait covering every router endpoint —
//! authentication, sessions, execution, streaming, route preview, approvals,
//! traces, providers/models and usage — plus `reqwest`-backed (async, Tokio),
//! `isahc`-backed (async, runtime-agnostic) and `ureq`-backed (blocking)
//! implementations.
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
//!
//! # Feature flags
//!
//! The crate ships no backend by default; the [`Client`] trait and its shared
//! types compile without any HTTP library. Concrete clients are opt-in, and a
//! frontend enables exactly the one it wants:
//!
//! | name      | description                                                                                                                       | default |
//! |-----------|-----------------------------------------------------------------------------------------------------------------------------------|---------|
//! | `isahc`   | Enable [`IsahcClient`], the runtime-agnostic async [`isahc`](https://docs.rs/isahc)-backed [`Client`] over a `rustls` TLS stack.   |         |
//! | `reqwest` | Enable [`ReqwestClient`], the async [`reqwest`](https://docs.rs/reqwest)-backed [`Client`] over a `rustls` TLS stack.              |         |
//! | `ureq`    | Enable [`UreqClient`], the blocking [`ureq`](https://docs.rs/ureq)-backed [`Client`] over a `rustls` TLS stack.                    |         |
//!
//! `ReqwestClient` is `async` end to end and needs a `tokio` reactor; it is the
//! backend `smista-cli` uses. `IsahcClient` is also `async` but runtime-agnostic:
//! `isahc` drives I/O on its own agent thread, so its `Send` futures resolve on
//! any executor without a `tokio` reactor — see the [type docs](IsahcClient).
//! `UreqClient` is synchronous: its [`Client`] methods are `async` but block
//! internally, so they run on any executor without a `tokio` reactor — see the
//! [type docs](UreqClient) for how to drive it from a work-stealing runtime.

mod client;
mod config;
mod credentials;
mod error;
#[cfg(test)]
mod mock;

pub use self::client::Client;
#[cfg(feature = "isahc")]
#[cfg_attr(docsrs, doc(cfg(feature = "isahc")))]
pub use self::client::IsahcClient;
#[cfg(feature = "reqwest")]
#[cfg_attr(docsrs, doc(cfg(feature = "reqwest")))]
pub use self::client::ReqwestClient;
#[cfg(feature = "ureq")]
#[cfg_attr(docsrs, doc(cfg(feature = "ureq")))]
pub use self::client::UreqClient;
pub use self::config::{DEFAULT_URL, RouterClientConfig};
pub use self::credentials::{ApiKey, ProviderCredentials, SessionToken};
pub use self::error::{Result, RouterClientError};
