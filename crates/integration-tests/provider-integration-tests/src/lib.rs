//! API-key-gated provider integration tests for the smista.ai workspace.
//!
//! The tests live under `tests/`; this library carries only the test fixtures
//! they share — chiefly [`InMemoryStorage`], a process-local [`smista_providers::memory::MemoryStorage`]
//! backend. The suites exercise real provider backends and therefore require
//! live API credentials (e.g. `ANTHROPIC_API_KEY`). For this reason they are
//! excluded from the default `just test` run and from CI on every push: they
//! execute only under `just provider_integration_test`, which CI dispatches
//! manually.

mod memory_storage;

pub use self::memory_storage::InMemoryStorage;
