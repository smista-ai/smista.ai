//! Container-backed integration tests for the smista.ai workspace.
//!
//! This crate carries no library code; the tests live under `tests/`. They are
//! kept out of the default `just test` run because they require a running
//! Docker daemon, and execute only under `just test_all`.
