//! Cross-platform process inspection and signalling for `smista stop`.
//!
//! These helpers act on an arbitrary operating-system process id — the one the
//! router recorded in its pidfile — so `stop` can tell whether a router is still
//! running and ask it to shut down identically on Linux, macOS and Windows.

use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

/// Returns `true` when a process with `pid` is currently running.
///
/// The lookup refreshes only the single target process, so it stays cheap even
/// on a busy machine.
#[must_use]
pub fn is_running(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).is_some()
}

/// Asks the process with `pid` to terminate, returning `true` when a running
/// process was found and a stop request was delivered to it.
///
/// On Unix this delivers `SIGTERM`, the graceful signal the router's own handler
/// watches for, so it can wind its services down and remove its pidfile. On
/// Windows, where there is no `SIGTERM`, it falls back to the platform's forceful
/// termination.
///
/// Returns `false` when no process with `pid` is running, or when the operating
/// system refused to deliver the signal.
pub fn terminate(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let Some(process) = system.process(pid) else {
        tracing::warn!(
            process.pid = pid.as_u32(),
            "no running process to terminate"
        );
        return false;
    };

    // `kill_with` returns `None` when the platform cannot represent the signal
    // (Windows has no `SIGTERM`); fall back to a forceful kill there.
    match process.kill_with(Signal::Term) {
        Some(delivered) => {
            tracing::info!(
                process.pid = pid.as_u32(),
                delivered,
                "sent SIGTERM to process"
            );
            delivered
        }
        None => {
            tracing::warn!(
                process.pid = pid.as_u32(),
                "SIGTERM is unsupported on this platform; forcing termination"
            );
            process.kill()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_report_current_process_as_running() {
        assert!(is_running(std::process::id()));
    }

    #[test]
    fn should_report_improbable_pid_as_not_running() {
        // No real process is expected to own this id within a test run.
        assert!(!is_running(u32::MAX - 1));
    }

    #[test]
    fn should_not_terminate_a_missing_process() {
        assert!(!terminate(u32::MAX - 1));
    }

    /// End-to-end round trip: spawn a real, long-lived child, confirm it is
    /// running, signal it through the same path `smista stop` uses, and confirm
    /// the signal actually brought it down. Unix-only, as it relies on `sleep`
    /// and on `SIGTERM` being deliverable to another process.
    #[cfg(unix)]
    #[test]
    fn should_signal_and_stop_a_real_child_process() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("failed to spawn the test child");
        let pid = child.id();

        assert!(is_running(pid), "the spawned child should be running");

        // SIGTERM the child by its pid, exactly as `stop` does.
        assert!(terminate(pid), "terminate should signal a running child");

        // Reap it and confirm it exited because of the signal rather than
        // finishing its 30 second sleep on its own.
        let status = child.wait().expect("failed to wait on the test child");
        assert!(
            !status.success(),
            "the child should have been stopped by the signal"
        );

        // Once reaped, its pid is no longer a live process.
        let deadline = Instant::now() + Duration::from_secs(5);
        while is_running(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !is_running(pid),
            "the child should no longer be running after termination"
        );
    }
}
