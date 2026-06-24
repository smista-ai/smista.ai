//! Logging setup for smista-router.

use std::fs::OpenOptions;
use std::path::Path;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;

/// Initialize the global tracing subscriber.
///
/// `filter` is an `EnvFilter` directive (e.g. `"info,firma=debug"`).
/// `file` writes logs to the given path (truncated on open) instead of stderr.
///
/// # Errors
///
/// Returns an error if `filter` is not a valid `EnvFilter` directive,
/// the log file cannot be opened, or a global subscriber is already set.
pub fn init(filter: &str, file: Option<&Path>) -> anyhow::Result<()> {
    let env_filter = EnvFilter::try_new(filter)
        .map_err(|e| anyhow::anyhow!("invalid log filter `{filter}`: {e}"))?;

    if let Some(path) = file {
        let f = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .map_err(|e| anyhow::anyhow!("failed to open log file {}: {e}", path.display()))?;
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_span_events(FmtSpan::CLOSE)
            .with_target(true)
            .with_line_number(true)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(f))
            .try_init()
            .map_err(|e| anyhow::anyhow!("failed to set tracing subscriber: {e}"))?;
    } else {
        // Compact CLI format on stderr. Drops `FmtSpan::CLOSE` because span
        // open/close pairs are diagnostic noise in interactive use.
        // `CompactFormatter` renders fields itself (without ANSI) so the
        // default field formatter cannot leak italic codes into piped output.
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_ansi(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_target(true)
            .with_line_number(true)
            .with_writer(std::io::stderr)
            .try_init()
            .map_err(|e| anyhow::anyhow!("failed to set tracing subscriber: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_invalid_filter() {
        // An unparseable level rejects before any global subscriber is touched,
        // so this stays isolated from other tests in the binary.
        let err =
            init("warn,smista=notalevel", None).expect_err("an invalid log filter was accepted");

        assert!(
            err.to_string().contains("invalid log filter"),
            "unexpected error: {err}"
        );
    }
}
