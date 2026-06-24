#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista
//!
//! Command-line interface for smista.ai. Handles user interaction, command
//! parsing, terminal rendering, local workspace discovery and approval
//! prompts, and communicates with smista-router over its HTTP API.
//!
//! The CLI never decides which model executes a task; model selection belongs
//! to the router. The developer may, however, express a preference.
//!
//! The CLI is both the client for the router, and the router itself, based on the subcommand invoked.

mod args;
mod command;
mod config;
mod log;
mod signal;

use clap::Parser as _;

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
    tracing::info!("smista-cli starting");

    // dispatch the selected subcommand. A foreground router runs until it is
    // told to stop and owns its own shutdown handling; one-shot commands return
    // as soon as their work is done.
    command::run(args).await
}
