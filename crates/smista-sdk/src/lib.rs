#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-sdk
//!
//! Rust SDK facade for smista.ai. It is the single dependency a Rust consumer
//! (a frontend, an automation, another service) reaches for when building on
//! top of smista.ai: it bundles the shared domain vocabulary and the router
//! client behind one crate.
//!
//! The domain types live under [`core`], re-exported verbatim from
//! `smista-core`. The async router client lives under [`client`], re-exported
//! from `smista-router-client`.
//!
//! ```
//! use smista_sdk::core::policy::PermissionMode;
//!
//! let mode = PermissionMode::default();
//! let _ = mode;
//! ```
//!
//! This crate deliberately carries no logic of its own; it is a thin,
//! audience-facing re-export layer. `smista-core` stays a leaf dependency so
//! the router and other internal binaries can depend on it without pulling in a
//! client.
//!
//! # Feature flags
//!
//! No backend is enabled by default; [`client`] always exposes the
//! backend-agnostic [`Client`](client::Client) trait and its shared types, and a
//! consumer enables exactly the concrete client it wants:
//!
//! | name             | description                                                                                                                                          | default |
//! |------------------|------------------------------------------------------------------------------------------------------------------------------------------------------|---------|
//! | `isahc-client`   | Surface [`client::IsahcClient`], the runtime-agnostic async [`isahc`](https://docs.rs/isahc)-backed client, by enabling `smista-router-client/isahc`. |         |
//! | `reqwest-client` | Surface [`client::ReqwestClient`], the async [`reqwest`](https://docs.rs/reqwest)-backed client, by enabling `smista-router-client/reqwest`.          |         |
//! | `ureq-client`    | Surface [`client::UreqClient`], the blocking [`ureq`](https://docs.rs/ureq)-backed client, by enabling `smista-router-client/ureq`.                   |         |

pub mod client;

pub use smista_core as core;
