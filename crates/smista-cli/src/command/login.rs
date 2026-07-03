//! Implements `smista login`.
//!
//! The command bootstraps a router user and stores the returned router API key
//! in the global credential scope. A later prompt can then sign in without
//! asking the router to mint another key.

use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context as _;
use smista_sdk::client::{ApiKey, Client};

use crate::credentials::{ApiKeyStorage, CredentialsStorage};

/// Bootstraps a router user and stores the returned API key.
///
/// The command is idempotent from a user's perspective: if an API key is
/// already configured in the resolved credential scope, it prints a short
/// message and returns successfully.
///
/// # Errors
///
/// Returns an error when credentials cannot be read or written, configuration
/// cannot be loaded, the router cannot be reached, or the router returns an API
/// key that fails client-side validation.
pub async fn run(enforce_keyring: bool) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    tracing::info!(
        "running smista-cli login command; cwd is {cwd}",
        cwd = cwd.display()
    );
    let credentials = Arc::new(CredentialsStorage::new(enforce_keyring)?);
    tracing::debug!(
        "credentials storage initialized with backend {backend}",
        backend = credentials.backend()
    );

    let api_key_storage = ApiKeyStorage::new(credentials, &cwd);

    // load config
    let config =
        crate::config::load_and_validate(&cwd).context("Failed to load CLI configuration")?;

    // setup router client
    let router_client =
        crate::client::config_client(&config).context("Failed to configure router client")?;

    tracing::debug!(
        "logging in to router at {url}",
        url = router_client.base_url()
    );
    login(&api_key_storage, &router_client).await
}

/// Logs in unless the credential scope already has a router API key.
async fn login<C>(api_key_storage: &ApiKeyStorage, router_client: &C) -> anyhow::Result<()>
where
    C: Client,
{
    if api_key_storage.get()?.is_some() {
        tracing::info!("login skipped because an API key is already configured");
        println!("Already logged in.");
        return Ok(());
    }

    bootstrap_and_store_api_key(api_key_storage, router_client).await
}

/// Bootstraps through `router_client` and stores the returned API key globally.
async fn bootstrap_and_store_api_key<C>(
    api_key_storage: &ApiKeyStorage,
    router_client: &C,
) -> anyhow::Result<()>
where
    C: Client,
{
    let response = router_client
        .bootstrap()
        .await
        .context("Failed to bootstrap user")?;

    tracing::debug!(
        "parsed bootstrap response; user_id={user_id}",
        user_id = response.user_id
    );
    let api_key = ApiKey::from_str(&response.api_key).context("Failed to parse API key")?;

    tracing::debug!("storing API key in credentials storage");
    api_key_storage
        .set(&api_key, true)
        .context("Failed to store API key")?;

    tracing::info!(
        "successfully logged in with user ID: {user_id}",
        user_id = response.user_id
    );
    println!(
        "Successfully logged in with user ID: {user_id}",
        user_id = response.user_id
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smista_mock_web_server::{Endpoint, EndpointStatus, MockRouter, defaults};
    use smista_sdk::client::{ReqwestClient, RouterClientConfig};

    use super::*;

    fn api_key_storage(cwd: &std::path::Path) -> ApiKeyStorage {
        let global_path = cwd.join("global-secrets");
        let credentials = CredentialsStorage::new_file_for_tests(global_path)
            .expect("test credentials storage builds");
        ApiKeyStorage::new(Arc::new(credentials), cwd)
    }

    fn client_for(router: &MockRouter) -> ReqwestClient {
        ReqwestClient::new(RouterClientConfig::new(router.base_url()))
            .expect("the test client builds")
    }

    #[tokio::test]
    async fn should_bootstrap_and_store_the_returned_api_key_globally() {
        let tmp = tempfile::tempdir().expect("temp dir is created");
        let storage = api_key_storage(tmp.path());
        let router = MockRouter::start().await;

        login(&storage, &client_for(&router))
            .await
            .expect("login succeeds");

        let stored = storage.get().expect("stored API key is readable");
        assert_eq!(
            stored.as_ref().map(ApiKey::expose),
            Some(defaults::bootstrap().api_key.as_str())
        );
        let requests = router.received_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/api/v1/auth/bootstrap");
    }

    #[tokio::test]
    async fn should_skip_bootstrap_when_already_logged_in() {
        let tmp = tempfile::tempdir().expect("temp dir is created");
        let storage = api_key_storage(tmp.path());
        storage
            .set(
                &ApiKey::from_str(&defaults::bootstrap().api_key)
                    .expect("the canned API key parses"),
                true,
            )
            .expect("API key is stored");
        let router = MockRouter::start().await;

        login(&storage, &client_for(&router))
            .await
            .expect("login remains idempotent");

        assert!(router.received_requests().await.is_empty());
    }

    #[tokio::test]
    async fn should_return_bootstrap_errors_without_storing_an_api_key() {
        let tmp = tempfile::tempdir().expect("temp dir is created");
        let storage = api_key_storage(tmp.path());
        let router = MockRouter::builder()
            .endpoint_status(Endpoint::Bootstrap, EndpointStatus::ServerError)
            .start()
            .await;

        let error = bootstrap_and_store_api_key(&storage, &client_for(&router))
            .await
            .expect_err("login reports bootstrap failure");

        assert!(
            format!("{error:#}").contains("Failed to bootstrap user"),
            "unexpected error chain: {error:#}"
        );
        assert!(storage.get().expect("storage is readable").is_none());
    }
}
