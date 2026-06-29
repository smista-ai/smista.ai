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
//! The trait alone pulls in no HTTP library. The default
//! [`reqwest`](https://docs.rs/reqwest)-backed [`ReqwestClient`] is opt-in behind
//! the crate's `reqwest-client` feature, which forwards to
//! `smista-router-client/reqwest`.
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

#[cfg(feature = "reqwest-client")]
#[cfg_attr(docsrs, doc(cfg(feature = "reqwest-client")))]
#[doc(inline)]
pub use smista_router_client::ReqwestClient;
#[doc(inline)]
pub use smista_router_client::{
    ApiKey, Client, ProviderCredentials, Result, RouterClientConfig, RouterClientError,
    SessionToken,
};
