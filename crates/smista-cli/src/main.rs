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

mod app;
mod args;
mod client;
mod command;
mod config;
mod credentials;
mod signal;
mod skills;
mod telemetry;
mod tools;

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
    let is_interactive_cli = args.command.is_none();

    // The foreground router configures its own telemetry — it may enable
    // OpenTelemetry export from the router configuration — so it initializes
    // logging itself once that configuration is loaded. Every other invocation
    // uses plain logging, set up here and kept alive for the process lifetime.
    let (_telemetry, log_sink) = if args.is_foreground_start() {
        (None, None)
    } else if is_interactive_cli {
        (
            None,
            Some(crate::app::log::init_tui(
                &args.log_filter,
                args.log_file.as_deref(),
            )?),
        )
    } else {
        (
            Some(crate::app::log::init(
                &args.log_filter,
                args.log_file.as_deref(),
                None,
            )?),
            None,
        )
    };
    tracing::info!("smista-cli starting");

    // dispatch the selected subcommand. A foreground router runs until it is
    // told to stop and owns its own shutdown handling; one-shot commands return
    // as soon as their work is done.
    command::run(args, log_sink).await
}
