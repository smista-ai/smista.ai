//! API key command handlers.
//!
//! The handlers bridge parsed CLI arguments to API key storage.
//! They select the configured secret backend once per invocation, bind API keys
//! to the current working directory for local scope, and never print
//! secret values.

use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context as _;
use smista_sdk::client::{ApiKey, Client};
use smista_sdk::core::api::BootstrapResponse;

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
pub async fn run(
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
        ApikeyCommand::Check => check_api_key(apikey_storage),
        ApikeyCommand::New => new_api_key().await,
        ApikeyCommand::Remove => remove_api_key(apikey_storage, global),
        ApikeyCommand::Set { api_key } => set_api_key(apikey_storage, api_key, global),
    }
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

async fn new_api_key() -> anyhow::Result<()> {
    let config = crate::config::load_and_validate(&std::env::current_dir()?)
        .context("Failed to load CLI configuration")?;
    let client =
        crate::client::config_client(&config).context("Failed to configure router client")?;

    tracing::debug!(
        "requesting new API key from router at {url}",
        url = client.base_url()
    );
    let BootstrapResponse { api_key, user_id } = client
        .bootstrap()
        .await
        .context("Failed to request new API key from router")?;

    tracing::debug!(
        "received new API key from router for user {user_id}",
        user_id = user_id
    );

    let api_key =
        ApiKey::from_str(&api_key).context("Failed to parse API key from router response")?;

    println!("{api_key}", api_key = api_key.expose());

    Ok(())
}

fn remove_api_key(apikey_storage: Arc<ApiKeyStorage>, global: bool) -> anyhow::Result<()> {
    apikey_storage.delete(global)?;

    println!("API key removed successfully.");

    Ok(())
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use uuid::Uuid;

    use super::*;

    fn storage(cwd: &Path) -> Arc<ApiKeyStorage> {
        let credentials = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets.toml"))
            .expect("create test credentials storage");

        Arc::new(ApiKeyStorage::new(Arc::new(credentials), cwd))
    }

    fn api_key(secret: &str) -> ApiKey {
        ApiKey::from_parts(&Uuid::nil(), secret)
    }

    #[test]
    fn should_set_check_and_remove_local_api_key() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let storage = storage(tempdir.path());
        let api_key = api_key("local-secret");

        set_api_key(Arc::clone(&storage), api_key.expose().to_owned(), false)
            .expect("set local API key");
        check_api_key(Arc::clone(&storage)).expect("check local API key");

        assert_eq!(
            storage
                .get()
                .expect("read local API key")
                .expect("local API key exists")
                .expose(),
            api_key.expose()
        );

        remove_api_key(Arc::clone(&storage), false).expect("remove local API key");

        assert!(storage.get().expect("read removed API key").is_none());
    }

    #[test]
    fn should_set_check_and_remove_global_api_key() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let storage = storage(tempdir.path());
        let api_key = api_key("global-secret");

        set_api_key(Arc::clone(&storage), api_key.expose().to_owned(), true)
            .expect("set global API key");
        check_api_key(Arc::clone(&storage)).expect("check global API key");

        assert_eq!(
            storage
                .get()
                .expect("read global API key")
                .expect("global API key exists")
                .expose(),
            api_key.expose()
        );

        remove_api_key(Arc::clone(&storage), true).expect("remove global API key");

        assert!(storage.get().expect("read removed API key").is_none());
    }

    #[test]
    fn should_reject_invalid_api_key() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let storage = storage(tempdir.path());

        set_api_key(Arc::clone(&storage), "not-an-api-key".to_string(), false)
            .expect_err("reject invalid API key");

        assert!(storage.get().expect("read missing API key").is_none());
    }
}
