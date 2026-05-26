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
//! Implementation is tracked in milestone M6.
