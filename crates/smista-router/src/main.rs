#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-router
//!
//! Routing and orchestration service for smista.ai. It authenticates users,
//! loads sessions, classifies tasks, evaluates routing policies, selects
//! models and providers, mediates tool calls and records traces. It is the
//! source of truth for routing decisions and hosts the HTTP JSON API.
//!

mod args;
mod auth;
mod config;
mod log;
mod retention;
mod router;
mod signal;
mod storage;
mod trace;
mod web;

use std::time::Duration;

use clap::Parser as _;
use tokio_util::sync::CancellationToken;

const STACK_SIZE: usize = 10 * 1024 * 1024; // 10MiB

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(STACK_SIZE)
        .build()?
        .block_on(tokio_main())
}

async fn tokio_main() -> anyhow::Result<()> {
    // parse CLI args and env vars
    let args = args::Args::parse();
    // init logging
    log::init(&args.log_filter, args.log_file.as_deref())?;
    tracing::info!("smista-router starting");

    // parse configuration file
    let config_path = args
        .config
        .or(config::paths::router_toml())
        .ok_or_else(|| anyhow::anyhow!("no available configuration file. Specify a configuration file with `-c` or `--config`"))?;
    tracing::debug!("using configuration file: {}", config_path.display());
    let config = config::load(&config_path)?;
    tracing::debug!("configuration loaded successfully");

    // validate configuration
    let validate_report = config::validate::validate(&config);
    if !validate_report.is_ok() {
        anyhow::bail!("configuration is invalid:\n{}", validate_report.to_human());
    }
    for warning in validate_report.warnings() {
        tracing::warn!("configuration warning: {}", warning.to_human());
    }
    tracing::info!("configuration loaded and validated successfully");

    // run services
    let exit = CancellationToken::new();

    // init storage
    tracing::debug!("initializing storage");
    let database = storage::build_storage(&config.storage).await?;
    tracing::debug!("storage initialized successfully");

    // start storage retention task
    let retention_service = retention::RetentionService::new(retention::RetentionServiceConfig {
        database: database.clone(),
        exit: exit.clone(),
        trace_retention_days: config.retention.trace_retention_days,
        session_retention_days: config.retention.session_retention_days,
        archived_session_retention_days: config.retention.archived_session_retention_days,
        cleanup_interval: Duration::from_secs(config.retention.cleanup_interval_seconds),
    })
    .run();

    // start the HTTP server
    tracing::debug!("starting web server");
    let web_service = web::WebServer::init(web::WebServerConfig {
        config,
        database,
        exit: exit.clone(),
    })?
    .run()
    .await?;

    // cancel the exit token on SIGINT/SIGTERM so services wind down
    let shutdown_listener = tokio::spawn(signal::wait_for_shutdown(exit));

    // wait for all services to complete
    let _ = tokio::join!(retention_service, web_service, shutdown_listener);
    tracing::info!("smista-router stopped");

    Ok(())
}
