use std::path::PathBuf;

/// Arguments for `smista start`.
///
/// `smista start` launches a local router. By default it daemonizes — it spawns
/// the router as a detached background process and returns — so the shell is
/// free again. Pass `--foreground` to run the router in the current process
/// instead, which is what a service manager wants.
#[derive(Debug, clap::Args)]
pub struct RouterArgs {
    /// Path to the router configuration file. When omitted, the router loads the
    /// per-user `router.toml`. Can also be set via the `SMISTA_ROUTER_CONFIG`
    /// environment variable.
    #[clap(short = 'c', long = "config", env = "SMISTA_ROUTER_CONFIG")]
    pub config: Option<PathBuf>,
    /// Path to the pidfile the router records its process id in. When omitted, a
    /// per-user default under the runtime directory is used. Can also be set via
    /// the `SMISTA_ROUTER_PIDFILE` environment variable.
    #[clap(short = 'p', long = "pidfile", env = "SMISTA_ROUTER_PIDFILE")]
    pub pidfile: Option<PathBuf>,
    /// Run the router in the foreground instead of daemonizing it.
    #[clap(short = 'f', long = "foreground")]
    pub foreground: bool,
}

/// Arguments for `smista stop`.
///
/// `smista stop` reads the router's process id from the pidfile and asks that
/// process to shut down.
#[derive(Debug, clap::Args)]
pub struct StopArgs {
    /// Path to the pidfile written by `smista start`. When omitted, the same
    /// per-user default `smista start` uses is read. Can also be set via the
    /// `SMISTA_ROUTER_PIDFILE` environment variable.
    #[clap(short = 'p', long = "pidfile", env = "SMISTA_ROUTER_PIDFILE")]
    pub pidfile: Option<PathBuf>,
}
