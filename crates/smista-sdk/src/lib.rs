//! # smista-sdk
//!
//! Rust SDK facade for smista.ai. It is the single dependency a Rust consumer
//! (a frontend, an automation, another service) reaches for when building on
//! top of smista.ai: it bundles the shared domain vocabulary and, once it
//! lands, the router client behind one crate.
//!
//! The domain types live under [`core`], re-exported verbatim from
//! `smista-core`. The router client will be added later as `smista_sdk::client`
//! (re-exported from `smista-router-client`).
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

pub use smista_core as core;
