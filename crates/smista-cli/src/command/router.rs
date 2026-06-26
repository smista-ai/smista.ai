//! `start` and `stop` for a locally managed router process.
//!
//! These are the CLI's router-lifecycle commands; the rest of the CLI talks to
//! an already-running router over its HTTP API. Because smista.ai ships as a
//! single binary, `start` runs the router in-process via [`smista_router::run`]
//! rather than shelling out to a second executable: `--foreground` runs it on
//! the CLI's runtime, while a plain `start` daemonizes by re-invoking the
//! `smista` binary as a detached background child that runs that same foreground
//! path.
//!
//! Starting and stopping the router is a process-lifecycle concern the router
//! itself knows nothing about, so it lives here: the CLI writes the [`pidfile`]
//! around the running router and refuses to start a second one on top of a live
//! pidfile, and `stop` reads that pidfile and signals the [`process`] to shut
//! down.

mod pidfile;
mod process;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, bail};
use tokio_util::sync::CancellationToken;

use self::pidfile::Pidfile;
use crate::args::{RouterArgs, StopArgs};
use crate::config::paths;
use crate::{log, signal};

/// Starts a local router, daemonizing unless `--foreground` was given.
///
/// `log_file` and `log_filter` are the global logging settings; when
/// daemonizing they are forwarded to the detached child so it logs the same way.
///
/// # Errors
///
/// Returns an error when a router is already running for the target pidfile, the
/// router fails to start, or the background process cannot be spawned.
pub async fn start(
    args: RouterArgs,
    log_file: Option<&Path>,
    log_filter: &str,
) -> anyhow::Result<()> {
    let pidfile_path = args.pidfile.clone().unwrap_or_else(paths::router_pidfile);

    // Refuse to start a second router on top of a live one. The foreground
    // process re-checks this atomically while acquiring the pidfile; checking
    // here as well lets a plain `smista start` fail fast before daemonizing.
    if let Some(pid) = pidfile::running_pid(&pidfile_path) {
        bail!(
            "a router is already running (pid {pid}); stop it first or use a different --pidfile"
        );
    }

    if args.foreground {
        run_foreground(&args, log_file, log_filter, pidfile_path).await
    } else {
        daemonize(&args, log_file, log_filter)
    }
}

/// Stops the local router recorded in the pidfile.
///
/// # Errors
///
/// Returns an error when no pidfile is present, it cannot be read, or the
/// recorded process could not be signalled.
pub fn stop(args: StopArgs) -> anyhow::Result<()> {
    let pidfile_path = args.pidfile.unwrap_or_else(paths::router_pidfile);
    if !pidfile_path.exists() {
        bail!(
            "no pidfile at {path}; is a local router running?",
            path = pidfile_path.display()
        );
    }

    let pid = pidfile::read(&pidfile_path)?;
    if process::terminate(pid) {
        println!("asked smista router (pid {pid}) to stop");
        Ok(())
    } else {
        bail!("could not stop the router (pid {pid}); it may have already exited");
    }
}

/// Runs the router in the current process until a shutdown signal arrives.
///
/// Acquires the pidfile first, so a clean or panicking exit drops the guard and
/// leaves no stale pidfile behind. The CLI owns the configuration lifecycle:
/// it resolves, loads and validates `router.toml`, folds the `--otel*` overrides
/// in (command line wins), installs logging — with OpenTelemetry export when
/// enabled — and only then hands the validated configuration to the router.
/// Runs on the ambient runtime built by `main`.
async fn run_foreground(
    args: &RouterArgs,
    log_file: Option<&Path>,
    log_filter: &str,
    pidfile_path: PathBuf,
) -> anyhow::Result<()> {
    let _pidfile = Pidfile::acquire(pidfile_path)?;

    let config_path = args
        .config
        .clone()
        .or_else(smista_router::config::paths::router_toml)
        .context("no router configuration file could be resolved; specify one with --config")?;
    let mut config = smista_router::config::load(&config_path)
        .with_context(|| format!("failed to load router config {}", config_path.display()))?;

    // Fold the command-line OpenTelemetry overrides over the file settings, then
    // validate the merged result so command-line values are checked too.
    config.opentelemetry = args.resolve_opentelemetry(config.opentelemetry.clone());
    let report = smista_router::config::validate::validate(&config);
    if !report.is_ok() {
        bail!(
            "router configuration is invalid:\n{report}",
            report = report.to_human()
        );
    }

    // Install logging (and OpenTelemetry export when enabled) before starting the
    // router, then surface any validation warnings now that a subscriber is set.
    let _telemetry = log::init(log_filter, log_file, Some(&config.opentelemetry))?;
    for warning in report.warnings() {
        tracing::warn!("configuration warning: {}", warning.to_human());
    }

    let exit = CancellationToken::new();
    // Cancel the token on SIGINT/SIGTERM — including the SIGTERM that
    // `smista stop` sends — so the router winds its services down.
    let shutdown = tokio::spawn(signal::wait_for_shutdown(exit.clone()));

    let handle = smista_router::run(smista_router::RouterArgs { config, exit })
        .await
        .context("failed to start the router")?;

    handle
        .await
        .context("the router task panicked")?
        .context("the router stopped with an error")?;

    // The router has stopped; if it was not a signal that brought it down, the
    // shutdown listener is still waiting, so abort it to avoid lingering.
    shutdown.abort();
    Ok(())
}

/// Re-invokes the `smista` binary as a detached background child running the
/// foreground path, then returns immediately.
fn daemonize(args: &RouterArgs, log_file: Option<&Path>, log_filter: &str) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("failed to resolve the smista executable path")?;
    let mut command = Command::new(exe);

    // Global logging flags come before the subcommand; the child re-parses them
    // and configures logging itself.
    if let Some(log_file) = log_file {
        command.arg("--log-file").arg(log_file);
    }
    command.arg("--log-filter").arg(log_filter);
    command.arg("start").arg("--foreground");
    if let Some(config) = &args.config {
        command.arg("--config").arg(config);
    }
    if let Some(pidfile) = &args.pidfile {
        command.arg("--pidfile").arg(pidfile);
    }

    // Forward the OpenTelemetry overrides so the detached child resolves the
    // same effective configuration the foreground path would.
    if args.otel {
        command.arg("--otel");
    }
    if args.no_otel {
        command.arg("--no-otel");
    }
    if let Some(endpoint) = &args.otel_endpoint {
        command.arg("--otel-endpoint").arg(endpoint);
    }
    if let Some(protocol) = args.otel_protocol {
        command.arg("--otel-protocol").arg(protocol.to_string());
    }
    if let Some(service_name) = &args.otel_service_name {
        command.arg("--otel-service-name").arg(service_name);
    }
    if let Some(sample_ratio) = args.otel_sample_ratio {
        command
            .arg("--otel-sample-ratio")
            .arg(sample_ratio.to_string());
    }

    // Detach from the controlling terminal: no inherited stdio and an
    // independent process group, so the router survives the launching shell.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);

    let child = command
        .spawn()
        .context("failed to spawn the router process")?;
    println!(
        "smista router started in the background (pid {pid})",
        pid = child.id()
    );
    Ok(())
}

/// Detaches `command` from the launching process group on Unix.
#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;

    // A new process group means the child is not killed when the launching
    // shell exits or the terminal forwards Ctrl-C to the foreground group.
    command.process_group(0);
}

/// Detaches `command` from the launching console on Windows.
#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    /// Run without inheriting the parent's console.
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    /// Start the child in its own process group.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}
