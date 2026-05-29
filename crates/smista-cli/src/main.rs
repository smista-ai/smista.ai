//! # smista
//!
//! Command-line interface for smista.ai. Handles user interaction, command
//! parsing, terminal rendering, local workspace discovery and approval
//! prompts, and communicates with smista-router over its HTTP API.
//!
//! The CLI never decides which model executes a task; model selection belongs
//! to the router. The developer may, however, express a preference.
//!
//! Implementation is tracked in milestone M6.

pub mod config;

use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    // TODO: change, use different and adeguate config
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    tracing::debug!("smista CLI starting");

    println!("smista: not yet implemented");

    Ok(())
}
