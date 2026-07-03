//! Subcommand dispatch for the `smista` CLI.
//!
//! Each `smista` subcommand maps to a handler. The process-management commands
//! that start and stop a local router live under [`router`].

mod apikey;
mod cli;
mod config;
mod credentials;
mod router;
mod status;

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
        enforce_keyring,
        log_file,
        log_filter,
        prompt,
        ..
    } = args;
    match command {
        Some(Command::Apikey(args)) => apikey::run(args, enforce_keyring),
        Some(Command::Config(args)) => config::run(args),
        Some(Command::Credentials(args)) => credentials::run(args, enforce_keyring),
        Some(Command::Start(start)) => router::start(start, log_file.as_deref(), &log_filter).await,
        Some(Command::Status(args)) => status::run(args).await,
        Some(Command::Stop(stop)) => router::stop(stop),
        None => cli::run(prompt, enforce_keyring).await,
    }
}
