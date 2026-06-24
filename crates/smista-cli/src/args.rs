use std::path::PathBuf;

mod router;

pub use self::router::{RouterArgs, StopArgs};

/// Top-level `smista` command-line arguments.
///
/// A subcommand selects the action to run; the logging flags are global, so
/// they may appear before or after the subcommand.
#[derive(Debug, clap::Parser)]
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
        default_value = "info"
    )]
    pub log_filter: String,
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

    use super::*;

    #[test]
    fn should_apply_defaults_for_start() {
        let args = Args::parse_from(["smista", "start"]);

        assert!(args.log_file.is_none());
        assert_eq!(args.log_filter, "info");
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
