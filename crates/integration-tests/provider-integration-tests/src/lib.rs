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

/// Installs a `DEBUG`-level tracing subscriber for the current test binary.
///
/// Each integration test calls this first so the provider request and response
/// tracing is captured and printed alongside the test output (run with
/// `--nocapture` to see it). The subscriber writes through the test writer, so
/// lines are attributed to the test that emitted them.
///
/// The call is idempotent: only the first installation in a test binary takes
/// effect, and later calls are silently ignored, so every test may call it
/// without coordinating with the others.
pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}
