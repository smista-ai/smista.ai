use std::path::PathBuf;

mod router;

pub use self::router::{RouterArgs, StopArgs};

/// Top-level `smista` command-line arguments.
///
/// A subcommand selects the action to run; the logging flags are global, so
/// they may appear before or after the subcommand.
#[derive(Debug, clap::Parser)]
#[command(version)]
pub struct Args {
    /// The subcommand to run.
    #[clap(subcommand)]
    pub command: Command,
    /// Set log file path. If not set, logs will be printed to stdout. Can also be set via the `SMISTA_ROUTER_LOG_FILE` environment variable.
    #[clap(
        short = 'L',
        global = true,
        long = "log-file",
        env = "SMISTA_ROUTER_LOG_FILE"
    )]
    pub log_file: Option<PathBuf>,
    /// Log level filter. Can also be set via the `SMISTA_ROUTER_LOG_FILTER` environment variable.
    #[clap(
        short = 'l',
        global = true,
        long = "log-filter",
        env = "SMISTA_ROUTER_LOG_FILTER",
        default_value = "off"
    )]
    pub log_filter: String,
}

impl Args {
    /// Whether this invocation runs the router in the current process.
    ///
    /// The foreground router owns its telemetry setup, so logging is left for it
    /// to initialize rather than being set up up front.
    #[must_use]
    pub fn is_foreground_start(&self) -> bool {
        matches!(&self.command, Command::Start(args) if args.foreground)
    }
}

/// The action a `smista` invocation performs.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Start a local router. Daemonizes by default; pass `--foreground` to run
    /// it in the current process.
    Start(RouterArgs),
    /// Stop the local router recorded in the pidfile.
    Stop(StopArgs),
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::Parser as _;
    use smista_router::config::{OpenTelemetryConfig, OtlpProtocol};

    use super::*;

    fn start_args(argv: &[&str]) -> RouterArgs {
        let Command::Start(start) = Args::parse_from(argv).command else {
            panic!("expected the start command");
        };
        start
    }

    #[test]
    fn should_default_opentelemetry_flags_off() {
        let start = start_args(&["smista", "start"]);
        assert!(!start.otel);
        assert!(!start.no_otel);
        let resolved = start.resolve_opentelemetry(OpenTelemetryConfig::default());
        assert!(!resolved.enabled);
    }

    #[test]
    fn should_let_command_line_override_the_file_for_opentelemetry() {
        let start = start_args(&[
            "smista",
            "start",
            "--otel",
            "--otel-endpoint",
            "http://collector:4317",
            "--otel-protocol",
            "http-binary",
            "--otel-service-name",
            "router-a",
            "--otel-sample-ratio",
            "0.25",
        ]);
        // The file disables export; the command line must win.
        let resolved = start.resolve_opentelemetry(OpenTelemetryConfig::default());
        assert!(resolved.enabled);
        assert_eq!(resolved.endpoint, "http://collector:4317");
        assert_eq!(resolved.protocol, OtlpProtocol::HttpBinary);
        assert_eq!(resolved.service_name, "router-a");
        assert!((resolved.sample_ratio - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn should_disable_opentelemetry_from_the_command_line() {
        let start = start_args(&["smista", "start", "--no-otel"]);
        let from_file = OpenTelemetryConfig {
            enabled: true,
            ..OpenTelemetryConfig::default()
        };
        let resolved = start.resolve_opentelemetry(from_file);
        assert!(!resolved.enabled);
    }

    #[test]
    fn should_keep_the_file_value_when_no_toggle_is_given() {
        let start = start_args(&["smista", "start", "--otel-sample-ratio", "0.5"]);
        let from_file = OpenTelemetryConfig {
            enabled: true,
            ..OpenTelemetryConfig::default()
        };
        let resolved = start.resolve_opentelemetry(from_file);
        assert!(resolved.enabled);
        assert!((resolved.sample_ratio - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn should_reject_enabling_and_disabling_opentelemetry_together() {
        assert!(Args::try_parse_from(["smista", "start", "--otel", "--no-otel"]).is_err());
    }

    #[test]
    fn should_apply_defaults_for_start() {
        let args = Args::parse_from(["smista", "start"]);

        assert!(args.log_file.is_none());
        assert_eq!(args.log_filter, "off");
        let Command::Start(start) = args.command else {
            panic!("expected the start command");
        };
        assert!(start.config.is_none());
        assert!(start.pidfile.is_none());
        assert!(!start.foreground);
    }

    #[test]
    fn should_parse_global_flags_before_the_subcommand() {
        let args = Args::parse_from([
            "smista",
            "--log-file",
            "/var/log/smista.log",
            "--log-filter",
            "debug",
            "start",
        ]);

        assert_eq!(
            args.log_file.as_deref(),
            Some(Path::new("/var/log/smista.log"))
        );
        assert_eq!(args.log_filter, "debug");
        assert!(matches!(args.command, Command::Start(_)));
    }

    #[test]
    fn should_parse_start_flags() {
        let args = Args::parse_from([
            "smista",
            "start",
            "--config",
            "/etc/smista/router.toml",
            "--pidfile",
            "/run/user/1000/smista/router.pid",
            "--foreground",
        ]);

        let Command::Start(start) = args.command else {
            panic!("expected the start command");
        };
        assert_eq!(
            start.config.as_deref(),
            Some(Path::new("/etc/smista/router.toml"))
        );
        assert_eq!(
            start.pidfile.as_deref(),
            Some(Path::new("/run/user/1000/smista/router.pid"))
        );
        assert!(start.foreground);
    }

    #[test]
    fn should_parse_stop_flags() {
        let args = Args::parse_from([
            "smista",
            "stop",
            "--pidfile",
            "/run/user/1000/smista/router.pid",
        ]);

        let Command::Stop(stop) = args.command else {
            panic!("expected the stop command");
        };
        assert_eq!(
            stop.pidfile.as_deref(),
            Some(Path::new("/run/user/1000/smista/router.pid"))
        );
    }

    #[test]
    fn should_require_a_subcommand() {
        assert!(Args::try_parse_from(["smista"]).is_err());
    }
}
