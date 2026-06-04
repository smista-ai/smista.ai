use std::path::PathBuf;

#[derive(Debug, clap::Parser)]
pub struct Args {
    /// Path to the configuration file. Can also be set via the `SMISTA_ROUTER_CONFIG` environment variable.
    #[clap(
        short = 'c',
        global = true,
        long = "config",
        env = "SMISTA_ROUTER_CONFIG"
    )]
    pub config: Option<PathBuf>,
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::Parser as _;

    use super::*;

    #[test]
    fn should_apply_defaults_without_flags() {
        let args = Args::parse_from(["smista-router"]);

        assert!(args.config.is_none());
        assert!(args.log_file.is_none());
        assert_eq!(args.log_filter, "info");
    }

    #[test]
    fn should_parse_provided_flags() {
        let args = Args::parse_from([
            "smista-router",
            "--config",
            "/etc/smista/router.toml",
            "--log-file",
            "/var/log/smista.log",
            "--log-filter",
            "debug",
        ]);

        assert_eq!(
            args.config.as_deref(),
            Some(Path::new("/etc/smista/router.toml"))
        );
        assert_eq!(
            args.log_file.as_deref(),
            Some(Path::new("/var/log/smista.log"))
        );
        assert_eq!(args.log_filter, "debug");
    }
}
