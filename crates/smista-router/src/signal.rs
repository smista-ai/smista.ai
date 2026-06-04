//! SIGINT/SIGTERM handling for the router service.

use tokio_util::sync::CancellationToken;

/// Wait for a shutdown signal (Ctrl-C, or SIGTERM on Unix), then cancel
/// `token` so the services watching it can wind down.
pub async fn wait_for_shutdown(token: CancellationToken) {
    wait_for_signal().await;
    token.cancel();
}

/// Resolve on the first SIGINT or SIGTERM.
#[cfg(unix)]
async fn wait_for_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(sigterm) => sigterm,
        Err(e) => {
            tracing::warn!(%e, "failed to register SIGTERM handler; listening for SIGINT only");
            log_ctrl_c(tokio::signal::ctrl_c().await);
            return;
        }
    };

    tokio::select! {
        res = tokio::signal::ctrl_c() => log_ctrl_c(res),
        _ = sigterm.recv() => tracing::info!("received SIGTERM, shutting down"),
    }
}

/// Resolve on the first Ctrl-C event.
#[cfg(not(unix))]
async fn wait_for_signal() {
    log_ctrl_c(tokio::signal::ctrl_c().await);
}

/// Log the outcome of waiting for a Ctrl-C / SIGINT signal.
fn log_ctrl_c(res: std::io::Result<()>) {
    match res {
        Ok(()) => tracing::info!("received SIGINT, shutting down"),
        Err(e) => tracing::error!(%e, "failed to listen for SIGINT"),
    }
}
