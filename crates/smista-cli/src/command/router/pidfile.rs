//! The router pidfile, owned by the foreground `smista start` process.
//!
//! A pidfile records the operating-system process id of a running router in a
//! small text file. The CLI writes it (via [`Pidfile::acquire`]) around the
//! router it runs in-process and removes it when the guard is dropped, so `stop`
//! has a reliable place to find a live router and refuse to start a second one.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

use super::process;

/// Reads the process id recorded in the pidfile at `path`.
///
/// # Errors
///
/// Returns an error when the file cannot be read or its contents are not a valid
/// process id.
pub fn read(path: &Path) -> anyhow::Result<u32> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read pidfile {path}", path = path.display()))?;
    raw.trim().parse::<u32>().with_context(|| {
        format!(
            "pidfile {path} does not contain a valid process id",
            path = path.display()
        )
    })
}

/// Returns the process id in the pidfile at `path` when it points at a process
/// that is still running.
///
/// Returns `None` when the pidfile is absent, unreadable, or stale (its process
/// has since exited), so callers can treat all three the same way: no live
/// router owns this pidfile.
#[must_use]
pub fn running_pid(path: &Path) -> Option<u32> {
    let pid = read(path).ok()?;
    process::is_running(pid).then_some(pid)
}

/// An owned, live pidfile.
///
/// Holds the path of a pidfile this process wrote; dropping the guard removes
/// the file, so a router that exits — cleanly or by unwinding — leaves no stale
/// pidfile behind.
#[derive(Debug)]
pub struct Pidfile {
    /// Location of the pidfile this guard owns.
    path: PathBuf,
}

impl Pidfile {
    /// Writes the current process id to `path`, returning a guard that removes
    /// the file when dropped.
    ///
    /// A pidfile that already points at a live process is left untouched; a
    /// stale one (whose process has exited) is replaced.
    ///
    /// # Errors
    ///
    /// Returns an error when a live router already owns the pidfile, or when the
    /// parent directory or the file itself cannot be created.
    pub fn acquire(path: PathBuf) -> anyhow::Result<Self> {
        if let Some(pid) = running_pid(&path) {
            anyhow::bail!(
                "a router is already running (pid {pid}); refusing to overwrite {path}",
                path = path.display()
            );
        }
        if path.exists() {
            tracing::warn!(pidfile = %path.display(), "replacing stale pidfile");
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create pidfile directory {parent}",
                    parent = parent.display()
                )
            })?;
        }

        let pid = std::process::id();
        fs::write(&path, format!("{pid}\n"))
            .with_context(|| format!("failed to write pidfile {path}", path = path.display()))?;
        tracing::info!(pidfile = %path.display(), pid, "wrote pidfile");

        Ok(Self { path })
    }
}

impl Drop for Pidfile {
    fn drop(&mut self) {
        match fs::remove_file(&self.path) {
            Ok(()) => tracing::info!(pidfile = %self.path.display(), "removed pidfile"),
            // Already gone (for example, removed by an operator); nothing to do.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(pidfile = %self.path.display(), error = %e, "failed to remove pidfile");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_round_trip_the_recorded_pid() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.pid");

        let _guard = Pidfile::acquire(path.clone()).expect("failed to acquire pidfile");
        assert_eq!(
            read(&path).expect("failed to read pidfile"),
            std::process::id()
        );
    }

    #[test]
    fn should_remove_the_pidfile_on_drop() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.pid");

        let guard = Pidfile::acquire(path.clone()).expect("failed to acquire pidfile");
        assert!(path.exists());
        drop(guard);
        assert!(!path.exists());
    }

    #[test]
    fn should_refuse_to_acquire_over_a_live_pidfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.pid");
        // Our own pid is, by definition, a live process.
        fs::write(&path, format!("{}\n", std::process::id())).expect("failed to seed pidfile");

        let err = Pidfile::acquire(path.clone()).expect_err("expected an already-running error");
        assert!(err.to_string().contains("already running"));
        // The live pidfile must be left untouched.
        assert!(path.exists());
    }

    #[test]
    fn should_replace_a_stale_pidfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.pid");
        // An improbable pid that no live process is expected to own.
        fs::write(&path, format!("{}\n", u32::MAX - 1)).expect("failed to seed pidfile");

        let _guard = Pidfile::acquire(path.clone()).expect("failed to replace stale pidfile");
        assert_eq!(
            read(&path).expect("failed to read pidfile"),
            std::process::id()
        );
    }

    #[test]
    fn should_reject_a_pidfile_without_a_valid_pid() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.pid");
        fs::write(&path, "not-a-pid\n").expect("failed to seed pidfile");

        assert!(read(&path).is_err());
        assert!(running_pid(&path).is_none());
    }
}
