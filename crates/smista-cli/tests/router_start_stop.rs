//! End-to-end tests for `smista start` and `smista stop`.
//!
//! These drive the real `smista` binary against a fixture `router.toml`: they
//! start a local router in the foreground, wait for it to come up, then stop it
//! through the CLI and confirm it wound down. They run on every platform; the
//! one platform-specific difference is the clean-up after `stop`: on Unix `stop`
//! delivers `SIGTERM` and the router removes its own pidfile on the way out,
//! while on Windows `stop` falls back to a forceful termination that skips that
//! cleanup, so the pidfile is left for the next `start` to treat as stale.

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Path to the built `smista` binary, provided by Cargo to integration tests.
const SMISTA_BIN: &str = env!("CARGO_BIN_EXE_smista");

/// Reserves a free localhost port by binding and immediately releasing it.
///
/// The router config rejects port 0, so the test picks a concrete free port to
/// bind. There is a small race between releasing the probe socket and the router
/// binding it, but it is good enough for a local test.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind a probe socket");
    let port = listener
        .local_addr()
        .expect("probe socket has no local address")
        .port();
    drop(listener);
    port
}

/// Writes a minimal, valid `router.toml` into `dir` and returns its path.
///
/// Storage is embedded under a temporary directory and Ollama is disabled, so
/// starting the router needs no network and leaves nothing behind once `dir` is
/// removed.
fn write_router_config(dir: &Path, port: u16) -> PathBuf {
    let db_dir = dir.join("db");
    let config = format!(
        "[router]\n\
         host = \"127.0.0.1\"\n\
         port = {port}\n\
         \n\
         [router.storage]\n\
         engine = \"surrealdb\"\n\
         mode = \"embedded\"\n\
         path = {db:?}\n\
         namespace = \"smista\"\n\
         database = \"local\"\n\
         \n\
         [router.ollama]\n\
         enabled = false\n",
        db = db_dir.to_string_lossy(),
    );
    let path = dir.join("router.toml");
    fs::write(&path, config).expect("failed to write the router config");
    path
}

/// Polls `check` until it returns `true` or `timeout` elapses.
fn wait_until(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if check() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Reads the process id recorded in `pidfile`, if any.
fn pid_in(pidfile: &Path) -> Option<u32> {
    fs::read_to_string(pidfile).ok()?.trim().parse().ok()
}

/// Returns `true` once the router is accepting connections on `port`.
///
/// Used as the readiness probe: the pidfile is written at process start, well
/// before the router is serving, so connecting to the port is what actually
/// proves the router (and its shutdown signal handlers) are up.
fn port_is_serving(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

#[test]
fn should_start_a_router_in_the_foreground_and_stop_it() {
    let tmp = tempfile::tempdir().expect("failed to create a temp dir");
    let port = free_port();
    let config = write_router_config(tmp.path(), port);
    let pidfile = tmp.path().join("router.pid");

    // Start the router in the foreground as a child, so the test keeps running
    // while the router serves.
    let mut child = Command::new(SMISTA_BIN)
        .arg("--log-filter")
        .arg("warn")
        .arg("start")
        .arg("--foreground")
        .arg("--config")
        .arg(&config)
        .arg("--pidfile")
        .arg(&pidfile)
        .spawn()
        .expect("failed to spawn `smista start`");

    // Wait until the router is actually serving — not merely until the pidfile
    // exists, which it does almost immediately at process start, before the
    // router is up. Serving implies the shutdown signal handlers are installed,
    // so the SIGTERM from `stop` below cannot race start-up. Also confirm the
    // child has not exited early; if it never comes up, kill it so it does not
    // leak.
    let started = wait_until(Duration::from_secs(20), || {
        port_is_serving(port) && matches!(child.try_wait(), Ok(None))
    });
    if !started {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the router did not start within the timeout");
    }

    let pid = pid_in(&pidfile).expect("the pidfile should hold a pid");
    assert_eq!(
        pid,
        child.id(),
        "the pidfile should record the foreground router process"
    );

    // Stop it through the CLI; `stop` signals the recorded pid.
    let stopped = Command::new(SMISTA_BIN)
        .arg("stop")
        .arg("--pidfile")
        .arg(&pidfile)
        .status()
        .expect("failed to run `smista stop`");
    assert!(stopped.success(), "`smista stop` should succeed");

    // The router should wind down: the foreground process exits and removes its
    // pidfile on the way out.
    let exited = wait_until(Duration::from_secs(20), || {
        matches!(child.try_wait(), Ok(Some(_)))
    });
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the router did not exit after `smista stop`");
    }

    // On Unix, `stop` delivers SIGTERM and the router removes its own pidfile as
    // it shuts down. On Windows the forceful termination skips that cleanup, so
    // the pidfile is left behind on purpose; only assert removal where the stop
    // is graceful.
    #[cfg(unix)]
    assert!(
        wait_until(Duration::from_secs(5), || !pidfile.exists()),
        "the pidfile should be removed after a clean stop"
    );
}

#[test]
fn should_refuse_to_start_when_a_router_is_already_running() {
    let tmp = tempfile::tempdir().expect("failed to create a temp dir");
    let config = write_router_config(tmp.path(), free_port());
    let pidfile = tmp.path().join("router.pid");
    // Seed the pidfile with this test process's id, which is certainly alive, so
    // `start` sees a live router without us having to spin a real one up.
    fs::write(&pidfile, format!("{}\n", std::process::id())).expect("failed to seed the pidfile");

    let output = Command::new(SMISTA_BIN)
        .arg("start")
        .arg("--foreground")
        .arg("--config")
        .arg(&config)
        .arg("--pidfile")
        .arg(&pidfile)
        .output()
        .expect("failed to run `smista start`");

    assert!(
        !output.status.success(),
        "`smista start` must refuse when a router is already running"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already running"),
        "the error should explain why start was refused, got: {stderr}"
    );
    // The live pidfile must be left untouched.
    assert_eq!(
        pid_in(&pidfile),
        Some(std::process::id()),
        "the existing pidfile must not be overwritten"
    );
}
