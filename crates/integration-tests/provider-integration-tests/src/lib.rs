//! API-key-gated provider integration tests for the smista.ai workspace.
//!
//! This crate carries no library code; the tests live under `tests/`. They
//! exercise real provider backends and therefore require live API credentials
//! (e.g. `ANTHROPIC_API_KEY`). For this reason they are excluded from the
//! default `just test` run and from CI on every push: they execute only under
//! `just provider_integration_test`, which CI dispatches manually.
