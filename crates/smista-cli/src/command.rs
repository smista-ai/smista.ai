//! Subcommand dispatch for the `smista` CLI.
//!
//! Each `smista` subcommand maps to a handler. The process-management commands
//! that start and stop a local router live under [`router`].

pub mod router;

use crate::args::{Args, Command};

/// Runs the subcommand selected on the command line.
///
/// A foreground `start` runs until shutdown; every other command returns as soon
/// as its work is done.
///
/// # Errors
///
/// Returns an error when the selected subcommand fails.
pub async fn run(args: Args) -> anyhow::Result<()> {
    let Args {
        command,
        log_file,
        log_filter,
    } = args;
    match command {
        Command::Start(start) => router::start(start, log_file.as_deref(), &log_filter).await,
        Command::Stop(stop) => router::stop(stop),
    }
}
