//! SIGINT/SIGTERM handling for the router service.

use tokio_util::sync::CancellationToken;

/// A shutdown listener whose operating-system signal handlers are already
/// installed.
///
/// [`install`](Self::install) registers the handlers up front — before the
/// router writes its pidfile or starts serving — so a stop signal that races
/// start-up is buffered and handled gracefully rather than falling through to
/// the operating system's default disposition. On Unix that default terminates
/// the process abruptly, skipping the router's own clean-up (notably removing
/// its pidfile and leaving a stale one behind). [`wait`](Self::wait) then
/// resolves on the first signal and cancels the supplied token.
#[derive(Debug)]
pub struct ShutdownSignals {
    /// The SIGTERM stream, registered eagerly so a signal arriving before
    /// [`wait`](Self::wait) runs is buffered rather than lost. `None` when the
    /// handler could not be registered, in which case only SIGINT is watched.
    #[cfg(unix)]
    sigterm: Option<tokio::signal::unix::Signal>,
}

impl ShutdownSignals {
    /// Installs the shutdown signal handlers.
    ///
    /// Must be called from within a Tokio runtime. Registering here, ahead of
    /// any work the router does, ensures a SIGTERM (the signal `smista stop`
    /// sends) is caught even if it arrives in the narrow window during
    /// start-up.
    #[cfg(unix)]
    #[must_use]
    pub fn install() -> Self {
        use tokio::signal::unix::{SignalKind, signal};

        let sigterm = match signal(SignalKind::terminate()) {
            Ok(sigterm) => Some(sigterm),
            Err(e) => {
                tracing::warn!(%e, "failed to register SIGTERM handler; listening for SIGINT only");
                None
            }
        };
        Self { sigterm }
    }

    /// Installs the shutdown signal handlers.
    ///
    /// On non-Unix platforms only Ctrl-C is watched, registered lazily when
    /// [`wait`](Self::wait) first polls it.
    #[cfg(not(unix))]
    #[must_use]
    pub fn install() -> Self {
        Self {}
    }

    /// Waits for the first shutdown signal, then cancels `token` so the services
    /// watching it can wind down.
    pub async fn wait(self, token: CancellationToken) {
        self.wait_for_signal().await;
        token.cancel();
    }

    /// Resolves on the first SIGINT or SIGTERM.
    #[cfg(unix)]
    async fn wait_for_signal(self) {
        match self.sigterm {
            Some(mut sigterm) => tokio::select! {
                res = tokio::signal::ctrl_c() => log_ctrl_c(res),
                _ = sigterm.recv() => tracing::info!("received SIGTERM, shutting down"),
            },
            None => log_ctrl_c(tokio::signal::ctrl_c().await),
        }
    }

    /// Resolves on the first Ctrl-C event.
    #[cfg(not(unix))]
    async fn wait_for_signal(self) {
        log_ctrl_c(tokio::signal::ctrl_c().await);
    }
}

/// Log the outcome of waiting for a Ctrl-C / SIGINT signal.
fn log_ctrl_c(res: std::io::Result<()>) {
    match res {
        Ok(()) => tracing::info!("received SIGINT, shutting down"),
        Err(e) => tracing::error!(%e, "failed to listen for SIGINT"),
    }
}
