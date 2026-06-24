#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-router
//!
//! Routing and orchestration service for smista.ai. It authenticates users,
//! loads sessions, classifies tasks, evaluates routing policies, selects models
//! and providers, mediates tool calls and records traces. It is the source of
//! truth for routing decisions and hosts the HTTP JSON API.
//!
//! ## Running the router
//!
//! [`run`] takes a [`RouterArgs`] carrying the configuration file to load and a
//! [`CancellationToken`] the caller cancels to request a graceful shutdown. It
//! validates the configuration, initializes storage, and spawns the retention
//! task and the HTTP server, returning a [`RouterHandle`] that resolves once
//! every service has wound down. If the HTTP server fails to start, the router
//! cancels the token so the remaining services also stop, and the handle
//! resolves with the error.
//!
//! ## Feature flags
//!
//! | name      | description                                            | default |
//! |-----------|--------------------------------------------------------|---------|
//! | `openapi` | Derive the OpenAPI schema for the HTTP API via utoipa. |         |

mod auth;
mod config;
mod retention;
mod router;
mod session;
mod storage;
mod trace;
mod web;

use std::path::PathBuf;
use std::time::Duration;

use smista_storage::database::surreal::SurrealDatabase;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::storage::StorageError;

/// Arguments for [`run`].
///
/// The router is configuration-driven: the only knobs the caller provides are
/// where to read that configuration from and how to ask the service to stop.
#[derive(Debug)]
pub struct RouterArgs {
    /// Path to the router configuration file. When `None`, the router falls
    /// back to the per-user `router.toml` in the global configuration directory.
    pub config: Option<PathBuf>,
    /// Token the caller cancels to request a graceful shutdown. The router also
    /// cancels it itself if a service fails to start.
    pub exit: CancellationToken,
}

/// Errors returned while starting or running the router.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// The configuration file could not be parsed or failed validation.
    #[error("configuration file is invalid: {0}")]
    ConfigInvalid(String),
    /// No configuration file was given and none could be resolved.
    #[error("no available configuration file found. You must specify a configuration file")]
    ConfigNotFound,
    /// A storage backend could not be initialized or accessed.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// The HTTP server could not be bound or stopped with an error.
    #[error("web server error: {0}")]
    Web(String),
}

/// A specialized [`Result`] for router operations.
pub type RouterResult<T> = Result<T, RouterError>;

/// Handle to a running router, resolving once every service has wound down.
///
/// Await it to block until the router stops; the inner [`RouterResult`] reports
/// whether it stopped cleanly or because a service failed.
pub type RouterHandle = JoinHandle<RouterResult<()>>;

/// Starts the router, returning a [`RouterHandle`] for the running service.
///
/// Loads and validates the configuration, initializes storage, and spawns the
/// retention task and the HTTP server. The returned handle resolves once both
/// services have wound down after the [`CancellationToken`] in `args` is
/// cancelled.
///
/// # Errors
///
/// Returns [`RouterError::ConfigNotFound`] when no configuration file can be
/// resolved, [`RouterError::ConfigInvalid`] when it cannot be parsed or fails
/// validation, and [`RouterError::Storage`] when storage cannot be initialized.
/// A failure to start the HTTP server is reported through the handle, not here.
pub async fn run(RouterArgs { config, exit }: RouterArgs) -> RouterResult<RouterHandle> {
    tracing::info!("smista-router starting");

    // resolve and parse the configuration file
    let config_path = config
        .or_else(config::paths::router_toml)
        .ok_or(RouterError::ConfigNotFound)?;
    tracing::debug!("using configuration file: {}", config_path.display());
    let config =
        config::load(&config_path).map_err(|e| RouterError::ConfigInvalid(e.to_string()))?;
    tracing::debug!("configuration loaded successfully");

    // validate configuration
    let validate_report = config::validate::validate(&config);
    if !validate_report.is_ok() {
        return Err(RouterError::ConfigInvalid(format!(
            "configuration is invalid:\n{}",
            validate_report.to_human()
        )));
    }
    for warning in validate_report.warnings() {
        tracing::warn!("configuration warning: {}", warning.to_human());
    }
    tracing::info!("configuration loaded and validated successfully");

    // initialize storage
    tracing::debug!("initializing storage");
    let database = storage::build_storage(&config.storage)
        .await
        .map_err(RouterError::Storage)?;
    tracing::debug!("storage initialized successfully");

    Ok(tokio::spawn(run_router(database, config, exit)))
}

/// Runs the router's services until the shared cancellation token is triggered.
///
/// Spawns the retention task and the HTTP server. If the HTTP server fails to
/// start, the cancellation token is triggered so the retention task winds down
/// too, and the error is returned.
async fn run_router(
    database: SurrealDatabase,
    config: config::RouterConfig,
    exit: CancellationToken,
) -> RouterResult<()> {
    // start the storage retention task
    let retention_service = retention::RetentionService::new(retention::RetentionServiceConfig {
        database: database.clone(),
        exit: exit.clone(),
        trace_retention_days: config.retention.trace_retention_days,
        session_retention_days: config.retention.session_retention_days,
        archived_session_retention_days: config.retention.archived_session_retention_days,
        cleanup_interval: Duration::from_secs(config.retention.cleanup_interval_seconds),
    })
    .run();

    // start the HTTP server; a failure here must not leave the retention task
    // running on its own, so cancel the shared token to wind it down too.
    tracing::debug!("starting web server");
    let web_service = match start_web_server(config, database, exit.clone()).await {
        Ok(web_service) => web_service,
        Err(e) => {
            tracing::error!(error = %e, "web server failed to start; shutting down router");
            exit.cancel();
            let _ = retention_service.await;
            return Err(e);
        }
    };

    // wait for all services to complete
    let _ = tokio::join!(retention_service, web_service);
    tracing::info!("smista-router stopped");

    Ok(())
}

/// Initializes and starts the HTTP server, mapping its failures to
/// [`RouterError::Web`].
async fn start_web_server(
    config: config::RouterConfig,
    database: SurrealDatabase,
    exit: CancellationToken,
) -> RouterResult<JoinHandle<()>> {
    web::WebServer::init(web::WebServerConfig {
        config,
        database,
        exit,
    })
    .map_err(|e| RouterError::Web(e.to_string()))?
    .run()
    .await
    .map_err(|e| RouterError::Web(e.to_string()))
}
