//! Logging and tracing setup for the `smista` CLI.
//!
//! The CLI owns the global `tracing` subscriber. It always installs a formatting
//! layer (to a file or to stderr) and, for the locally started router, may also
//! install an OpenTelemetry export layer on top — see [`crate::telemetry`]. Both
//! layers share the same [`EnvFilter`] directive, so traces and logs are
//! filtered identically.

use std::fs::OpenOptions;
use std::path::Path;

use anyhow::Context as _;
use smista_router::config::OpenTelemetryConfig;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer, fmt};

use crate::telemetry::{self, TelemetryGuard};

/// Initializes the global tracing subscriber.
///
/// `filter` is an `EnvFilter` directive (e.g. `"info,smista=debug"`). `file`
/// writes logs to the given path (truncated on open) instead of stderr. When
/// `otel` is `Some` and enabled, an OpenTelemetry export layer is installed
/// alongside the formatting layer; the returned [`TelemetryGuard`] must be kept
/// alive for the process lifetime and dropped on shutdown to flush traces.
///
/// # Errors
///
/// Returns an error if `filter` is not a valid `EnvFilter` directive, the log
/// file cannot be opened, the OpenTelemetry exporter cannot be built, or a
/// global subscriber is already set.
pub fn init(
    filter: &str,
    file: Option<&Path>,
    otel: Option<&OpenTelemetryConfig>,
) -> anyhow::Result<TelemetryGuard> {
    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![fmt_layer(filter, file)?];

    let guard = match otel {
        Some(config) if config.enabled => {
            let (layer, guard) = telemetry::layer::<Registry>(config)?;
            layers.push(layer.with_filter(env_filter(filter)?).boxed());
            guard
        }
        _ => TelemetryGuard::disabled(),
    };

    tracing_subscriber::registry()
        .with(layers)
        .try_init()
        .context("failed to set the global tracing subscriber")?;

    Ok(guard)
}

/// Initializes tracing without writing formatted logs to the terminal.
///
/// This is used by the interactive CLI when no log file was requested. The
/// inline terminal UI owns stdout and stderr, so formatted logs must not share
/// the same terminal stream.
///
/// # Errors
///
/// Returns an error if `filter` is not a valid `EnvFilter` directive or a
/// global subscriber is already set.
pub fn init_quiet(filter: &str) -> anyhow::Result<TelemetryGuard> {
    tracing_subscriber::registry()
        .with(fmt_sink_layer(filter)?)
        .try_init()
        .context("failed to set the global tracing subscriber")?;

    Ok(TelemetryGuard::disabled())
}

/// Parses an `EnvFilter` directive, mapping failures to a clear error.
fn env_filter(filter: &str) -> anyhow::Result<EnvFilter> {
    EnvFilter::try_new(filter).with_context(|| format!("invalid log filter `{filter}`"))
}

/// Builds the formatting layer, writing to `file` when given or stderr otherwise.
fn fmt_layer(
    filter: &str,
    file: Option<&Path>,
) -> anyhow::Result<Box<dyn Layer<Registry> + Send + Sync>> {
    let Some(path) = file else {
        // Compact CLI format on stderr, with ANSI colours for interactive use.
        return Ok(fmt::layer()
            .with_span_events(FmtSpan::CLOSE)
            .with_target(true)
            .with_line_number(true)
            .with_ansi(true)
            .with_writer(std::io::stderr)
            .with_filter(env_filter(filter)?)
            .boxed());
    };

    let log_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to open log file {}", path.display()))?;
    Ok(fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_line_number(true)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(log_file))
        .with_filter(env_filter(filter)?)
        .boxed())
}

/// Builds a formatting layer that preserves filtering but discards output.
fn fmt_sink_layer(filter: &str) -> anyhow::Result<Box<dyn Layer<Registry> + Send + Sync>> {
    Ok(fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_line_number(true)
        .with_ansi(false)
        .with_writer(std::io::sink)
        .with_filter(env_filter(filter)?)
        .boxed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_invalid_filter() {
        // An unparseable level rejects before any global subscriber is touched,
        // so this stays isolated from other tests in the binary.
        let err = init("warn,smista=notalevel", None, None)
            .expect_err("an invalid log filter was accepted");

        assert!(
            err.to_string().contains("invalid log filter"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn should_accept_a_valid_filter() {
        assert!(env_filter("info,smista=debug").is_ok());
    }

    #[test]
    fn should_reject_an_invalid_filter_directive() {
        let err = env_filter("smista=notalevel").expect_err("invalid directive was accepted");
        assert!(err.to_string().contains("invalid log filter"));
    }

    #[test]
    fn should_build_the_stderr_formatting_layer() {
        assert!(fmt_layer("info", None).is_ok());
    }

    #[test]
    fn should_build_the_quiet_formatting_layer() {
        assert!(fmt_sink_layer("info").is_ok());
    }

    #[test]
    fn should_build_the_file_formatting_layer_and_create_the_file() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("smista.log");
        assert!(fmt_layer("info", Some(&path)).is_ok());
        assert!(path.exists(), "the log file should have been created");
    }

    #[test]
    fn should_fail_to_open_an_unwritable_log_file() {
        // A directory cannot be opened for writing as a file.
        let dir = tempfile::tempdir().expect("a temp dir");
        let err = fmt_layer("info", Some(dir.path()))
            .err()
            .expect("a directory was opened as a file");
        assert!(err.to_string().contains("failed to open log file"));
    }
}
