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
//! [`run`] takes a [`RouterArgs`] carrying an already validated
//! [`RouterConfig`] and a [`CancellationToken`] the caller
//! cancels to request a graceful shutdown. The caller owns loading and
//! validating the configuration (see [`config`]); the router initializes
//! storage and spawns the retention task and the HTTP server, returning a
//! [`RouterHandle`] that resolves once every service has wound down. If the HTTP
//! server fails to start, the router cancels the token so the remaining services
//! also stop, and the handle resolves with the error.
//!
//! ## Feature flags
//!
//! | name      | description                                            | default |
//! |-----------|--------------------------------------------------------|---------|
//! | `openapi` | Derive the OpenAPI schema for the HTTP API via utoipa. |         |

mod auth;
pub mod config;
mod orchestrator;
mod retention;
mod router;
mod session;
pub mod storage;
mod trace;
mod usage;
pub mod web;

use std::time::Duration;

use smista_storage::database::surreal::SurrealDatabase;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::RouterConfig;
use crate::storage::StorageError;

/// Arguments for [`run`].
///
/// The router is configuration-driven: the caller provides an already validated
/// configuration and a way to ask the service to stop. Loading and validating
/// the configuration is the caller's responsibility (see [`config`]), which lets
/// the host process layer its own observability — such as OpenTelemetry export —
/// on top of the same settings before the router starts.
#[derive(Debug)]
pub struct RouterArgs {
    /// Validated router runtime configuration.
    pub config: RouterConfig,
    /// Token the caller cancels to request a graceful shutdown. The router also
    /// cancels it itself if a service fails to start.
    pub exit: CancellationToken,
}

/// Errors returned while starting or running the router.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
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
/// Initializes storage from the supplied validated configuration and spawns the
/// retention task and the HTTP server. The returned handle resolves once both
/// services have wound down after the [`CancellationToken`] in `args` is
/// cancelled.
///
/// # Errors
///
/// Returns [`RouterError::Storage`] when storage cannot be initialized. A
/// failure to start the HTTP server is reported through the handle, not here.
pub async fn run(RouterArgs { config, exit }: RouterArgs) -> RouterResult<RouterHandle> {
    tracing::info!("smista-router starting");

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
