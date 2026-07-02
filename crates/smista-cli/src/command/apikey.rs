//! API key command handlers.
//!
//! The handlers bridge parsed CLI arguments to API key storage.
//! They select the configured secret backend once per invocation, bind API keys
//! to the current working directory for local scope, and never print
//! secret values.

use std::str::FromStr;
use std::sync::Arc;

use smista_sdk::client::ApiKey;

use crate::args::{ApikeyArgs, ApikeyCommand};
use crate::credentials::{ApiKeyStorage, CredentialsStorage};

/// Runs a `smista apikey` invocation.
///
/// The command stores API keys locally by default and globally when
/// `--global` is present. `enforce_keyring` controls whether the command may
/// fall back to file-backed storage when the platform keyring is unavailable.
///
/// # Errors
///
/// Returns an error if the current directory cannot be resolved, API key
/// storage cannot be initialized, or the selected storage operation fails.
pub fn run(
    ApikeyArgs { command, global }: ApikeyArgs,
    enforce_keyring: bool,
) -> anyhow::Result<()> {
    // setup credentials storage
    let cwd = std::env::current_dir()?;
    tracing::debug!(
        "running smista-cli apikey command; cwd is {cwd}",
        cwd = cwd.display()
    );
    let credentials = Arc::new(CredentialsStorage::new(enforce_keyring)?);
    tracing::debug!(
        "apikey storage initialized with backend {backend}",
        backend = credentials.backend()
    );
    let apikey_storage = Arc::new(ApiKeyStorage::new(credentials, &cwd));

    match command {
        ApikeyCommand::Set { api_key } => set_api_key(apikey_storage, api_key, global),
        ApikeyCommand::Check => check_api_key(apikey_storage),
        ApikeyCommand::Remove => remove_api_key(apikey_storage, global),
    }
}

fn set_api_key(
    apikey_storage: Arc<ApiKeyStorage>,
    api_key: String,
    global: bool,
) -> anyhow::Result<()> {
    let api_key = ApiKey::from_str(&api_key)?;

    apikey_storage.set(&api_key, global)?;

    println!("API key set successfully.");

    Ok(())
}

fn check_api_key(apikey_storage: Arc<ApiKeyStorage>) -> anyhow::Result<()> {
    let is_set = apikey_storage.get()?.is_some();

    if is_set {
        println!("API key is set.");
    } else {
        println!("API key is not set.");
    }

    Ok(())
}

fn remove_api_key(apikey_storage: Arc<ApiKeyStorage>, global: bool) -> anyhow::Result<()> {
    apikey_storage.delete(global)?;

    println!("API key removed successfully.");

    Ok(())
}
