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

pub mod config;

use tracing_subscriber::EnvFilter;

const STACK_SIZE: usize = 10 * 1024 * 1024; // 10MiB

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(STACK_SIZE)
        .build()?
        .block_on(async { tokio_main().await })
}

async fn tokio_main() -> anyhow::Result<()> {
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
