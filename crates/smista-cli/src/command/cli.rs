//! Main command to run (which runs without a subcommand).
//!
//! It start the TUI and the smista-router client; it is basically the main user interface for smista.ai

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;

use crate::config::Config;
use crate::credentials::{
    ApiKeyStorage, CredentialsStorage, E2eeKeysCredentials, ProvidersCredentials,
};
use crate::skills::SkillStore;

#[expect(dead_code, reason = "context will be used in the next tasks")]
pub struct Context {
    api_key: Arc<ApiKeyStorage>,
    config: Arc<Config>,
    cwd: PathBuf,
    e2ee_keys: Arc<E2eeKeysCredentials>,
    providers_credentials: Arc<ProvidersCredentials>,
    skills_store: Arc<SkillStore>,
}

/// Runs smista.ai TUI and smista-router client; base subcommand.
pub async fn run(enforce_keyring: bool) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    tracing::debug!(
        "running smista-cli main command; cwd is {cwd}",
        cwd = cwd.display()
    );
    let credentials = Arc::new(CredentialsStorage::new(enforce_keyring)?);
    tracing::debug!(
        "credentials storage initialized with backend {backend}",
        backend = credentials.backend()
    );

    // load config
    let config =
        crate::config::load_and_validate(&cwd).context("Failed to load CLI configuration")?;

    // build context
    let _context = Context {
        api_key: Arc::new(ApiKeyStorage::new(credentials.clone(), &cwd)),
        config: Arc::new(config),
        e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
        providers_credentials: Arc::new(ProvidersCredentials::new(credentials, &cwd)),
        skills_store: Arc::new(SkillStore::discover(&cwd)),
        cwd,
    };

    // TODO: check config auto-start for router and start it if needed
    // TODO: in the future Context will have to hold the E2EE keys storage, the provider credentials storage, and the apikey storage instead
    // TODO: context must also hold the CancellationToken
    // TODO: in next task we will run the main loop here

    tracing::debug!("smista-cli main command finished");
    Ok(())
}
