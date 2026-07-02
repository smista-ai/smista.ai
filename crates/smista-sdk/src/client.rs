//! The async router client, re-exported from `smista-router-client`.
//!
//! This module is the SDK's view of [`smista-router-client`]: the backend-agnostic
//! [`Client`] trait and the credential ([`ApiKey`], [`SessionToken`],
//! [`ProviderCredentials`]), connection ([`RouterClientConfig`]) and error
//! ([`RouterClientError`], [`Result`]) types it relies on. A consumer building on
//! `smista-sdk` reaches the client here and the domain types under
//! [`core`](crate::core) through the single `smista-sdk` dependency, instead of
//! depending on `smista-router-client` directly.
//!
//! The trait alone pulls in no HTTP library. The
//! [`reqwest`](https://docs.rs/reqwest)-backed (async, Tokio) [`ReqwestClient`] is
//! opt-in behind the crate's `reqwest-client` feature, the
//! [`isahc`](https://docs.rs/isahc)-backed (async, runtime-agnostic)
//! [`IsahcClient`] behind the `isahc-client` feature, and the
//! [`ureq`](https://docs.rs/ureq)-backed (blocking) [`UreqClient`] behind the
//! `ureq-client` feature; each forwards to the matching `smista-router-client`
//! backend.
//!
//! # Examples
//!
//! ```
//! use smista_sdk::client::{Client, Result};
//! use smista_sdk::core::api::StatusResponse;
//!
//! async fn check<C: Client>(client: &C) -> Result<StatusResponse> {
//!     client.status().await
//! }
//! ```
//!
//! [`smista-router-client`]: https://docs.rs/smista-router-client

#[cfg(feature = "isahc-client")]
#[cfg_attr(docsrs, doc(cfg(feature = "isahc-client")))]
#[doc(inline)]
pub use smista_router_client::IsahcClient;
#[cfg(feature = "reqwest-client")]
#[cfg_attr(docsrs, doc(cfg(feature = "reqwest-client")))]
#[doc(inline)]
pub use smista_router_client::ReqwestClient;
#[cfg(feature = "ureq-client")]
#[cfg_attr(docsrs, doc(cfg(feature = "ureq-client")))]
#[doc(inline)]
pub use smista_router_client::UreqClient;
#[doc(inline)]
pub use smista_router_client::{
    ApiKey, Client, DEFAULT_URL as ROUTER_DEFAULT_URL, ProviderCredentials, Result,
    RouterClientConfig, RouterClientError, SessionToken,
};
