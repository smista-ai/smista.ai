//! Logging and tracing setup for the `smista` CLI.
//!
//! The CLI owns the global `tracing` subscriber. It always installs a formatting
//! layer (to a file, stderr, or the interactive UI) and, for the locally started
//! router, may also install an OpenTelemetry export layer on top — see
//! [`crate::telemetry`]. All layers share the same [`EnvFilter`] directive, so
//! traces and logs are filtered identically.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Context as _;
use smista_router::config::OpenTelemetryConfig;
use tracing::Metadata;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer, fmt};

use crate::telemetry::{self, TelemetryGuard};

/// Maximum number of formatted events retained for the interactive logs view.
const LOG_CAPACITY: usize = 2048;

/// Severity attached to a retained application log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    /// Highly detailed diagnostic event.
    Trace,
    /// Development diagnostic event.
    Debug,
    /// Informational runtime event.
    Info,
    /// Recoverable or potentially problematic event.
    Warn,
    /// Failed operation or runtime error.
    Error,
}

impl LogLevel {
    fn from_tracing(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::TRACE => Self::Trace,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::INFO => Self::Info,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::ERROR => Self::Error,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        };
        formatter.write_str(level)
    }
}

/// A formatted application log event with its original severity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLogEntry {
    level: LogLevel,
    message: String,
}

impl AppLogEntry {
    /// Creates an entry with `level` and its formatted `message`.
    #[must_use]
    pub fn new(level: LogLevel, message: String) -> Self {
        Self { level, message }
    }

    /// Returns the event severity.
    #[must_use]
    pub fn level(&self) -> LogLevel {
        self.level
    }

    /// Returns the formatted event message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for AppLogEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// A bounded, thread-safe sink for formatted TUI log entries.
///
/// Clones share the same queue. Writers briefly take an exclusive lock to append
/// an entry, while readers clone a consistent snapshot and immediately release
/// the lock.
#[derive(Clone, Debug)]
pub struct AppLogSink {
    inner: Arc<LogSinkInner>,
}

#[derive(Debug)]
struct LogSinkInner {
    capacity: usize,
    entries: Mutex<VecDeque<AppLogEntry>>,
}

impl AppLogSink {
    /// Creates a sink retaining the latest 2048 formatted events.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(LOG_CAPACITY)
    }

    /// Returns a slice of retained entries from `offset` to `offset + limit`.
    ///
    /// Newest entries are at the beginning of the slice.
    #[must_use]
    pub fn entries_at(&self, offset: usize, limit: usize) -> Vec<AppLogEntry> {
        let entries = self.lock_entries();
        entries
            .iter()
            .rev()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "log sink capacity must be greater than zero");
        Self {
            inner: Arc::new(LogSinkInner {
                capacity,
                entries: Mutex::new(VecDeque::with_capacity(capacity)),
            }),
        }
    }

    pub(crate) fn push(&self, entry: AppLogEntry) {
        let mut entries = self.lock_entries();
        if entries.len() == self.inner.capacity {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    fn lock_entries(&self) -> MutexGuard<'_, VecDeque<AppLogEntry>> {
        self.inner
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for AppLogSink {
    fn default() -> Self {
        Self::new()
    }
}

struct LogWriter {
    buffer: Vec<u8>,
    level: LogLevel,
    sink: AppLogSink,
}

impl std::io::Write for LogWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        let entry = String::from_utf8_lossy(&self.buffer);
        let entry = entry.trim_end_matches(['\r', '\n']);
        if !entry.is_empty() {
            self.sink
                .push(AppLogEntry::new(self.level, entry.to_owned()));
        }
    }
}

#[derive(Debug, Clone)]
struct AppLogMakeWriter {
    sink: AppLogSink,
}

impl AppLogMakeWriter {
    fn writer(&self, level: LogLevel) -> LogWriter {
        LogWriter {
            buffer: Vec::new(),
            level,
            sink: self.sink.clone(),
        }
    }
}

impl<'writer> MakeWriter<'writer> for AppLogMakeWriter {
    type Writer = LogWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        // The formatting layer uses `make_writer_for`; INFO is a safe fallback
        // for callers that have no event metadata.
        self.writer(LogLevel::Info)
    }

    fn make_writer_for(&'writer self, metadata: &Metadata<'_>) -> Self::Writer {
        self.writer(LogLevel::from_tracing(metadata.level()))
    }
}

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

/// Initializes tracing for the interactive terminal UI.
///
/// Formatted events are retained in the returned [`AppLogSink`]. When `file` is
/// present, events are also written there. The inline terminal UI owns stdout
/// and stderr, so this function never writes formatted logs to either stream.
///
/// # Errors
///
/// Returns an error if `filter` is not a valid `EnvFilter` directive, the log
/// file cannot be opened, or a global subscriber is already set.
pub fn init_tui(filter: &str, file: Option<&Path>) -> anyhow::Result<AppLogSink> {
    let sink = AppLogSink::new();
    let mut layers = vec![fmt_tui_layer(filter, sink.clone())?];
    if file.is_some() {
        layers.push(fmt_layer(filter, file)?);
    }

    tracing_subscriber::registry()
        .with(layers)
        .try_init()
        .context("failed to set the global tracing subscriber")?;

    Ok(sink)
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

/// Builds a formatting layer that retains events for the interactive UI.
fn fmt_tui_layer(
    filter: &str,
    sink: AppLogSink,
) -> anyhow::Result<Box<dyn Layer<Registry> + Send + Sync>> {
    let make_writer = AppLogMakeWriter { sink };
    Ok(fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_line_number(true)
        .with_ansi(false)
        .with_writer(make_writer)
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
    fn should_build_the_tui_formatting_layer() {
        assert!(fmt_tui_layer("info", AppLogSink::new()).is_ok());
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

    #[test]
    fn should_keep_only_the_newest_entries() {
        let sink = AppLogSink::with_capacity(2);

        sink.push(AppLogEntry::new(LogLevel::Info, "first".to_owned()));
        sink.push(AppLogEntry::new(LogLevel::Warn, "second".to_owned()));
        sink.push(AppLogEntry::new(LogLevel::Error, "third".to_owned()));

        assert_eq!(
            sink.entries_at(0, 100),
            [
                AppLogEntry::new(LogLevel::Error, "third".to_owned()),
                AppLogEntry::new(LogLevel::Warn, "second".to_owned()),
            ]
        );
    }

    #[test]
    fn should_page_entries_from_newest_to_oldest() {
        let sink = AppLogSink::with_capacity(4);
        for message in ["first", "second", "third", "fourth"] {
            sink.push(AppLogEntry::new(LogLevel::Info, message.to_owned()));
        }

        assert_eq!(
            sink.entries_at(1, 2),
            [
                AppLogEntry::new(LogLevel::Info, "third".to_owned()),
                AppLogEntry::new(LogLevel::Info, "second".to_owned())
            ]
        );
    }

    #[test]
    fn should_capture_formatted_events_across_clones() {
        let sink = AppLogSink::with_capacity(2);
        let layer = fmt_tui_layer("info", sink.clone()).expect("the TUI layer should build");
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!("filtered out");
            tracing::info!(event.answer = 42, "captured event");
            tracing::warn!("warning event");
        });

        let entries = sink.entries_at(0, 100);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].level(), LogLevel::Info);
        assert!(entries[1].message().contains("captured event"));
        assert!(entries[1].message().contains("event.answer=42"));
        assert!(!entries[1].message().ends_with('\n'));
        assert_eq!(entries[0].level(), LogLevel::Warn);
        assert!(entries[0].message().contains("warning event"));
    }
}
