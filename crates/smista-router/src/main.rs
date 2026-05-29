//! # smista-router
//!
//! Routing and orchestration service for smista.ai. It authenticates users,
//! loads sessions, classifies tasks, evaluates routing policies, selects
//! models and providers, mediates tool calls and records traces. It is the
//! source of truth for routing decisions and hosts the HTTP JSON API.
//!

pub mod config;

use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    // TODO: change, use different and adeguate config
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    tracing::debug!("smista-router starting");

    println!("smista-router: not yet implemented");

    Ok(())
}
