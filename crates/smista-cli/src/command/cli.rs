//! Main command to run (which runs without a subcommand).
//!
//! It start the TUI and the smista-router client; it is basically the main user interface for smista.ai

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use tokio_util::sync::CancellationToken;

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
    exit: CancellationToken,
    providers_credentials: Arc<ProvidersCredentials>,
    skills_store: Arc<SkillStore>,
}

/// Runs smista.ai TUI and smista-router client; base subcommand.
///
/// The `initial_prompt` is an optional string that will be used as the initial prompt for the TUI.
/// If it is `None`, the TUI will start with an empty prompt.
pub async fn run(_initial_prompt: Option<String>, enforce_keyring: bool) -> anyhow::Result<()> {
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
        exit: CancellationToken::new(),
        providers_credentials: Arc::new(ProvidersCredentials::new(credentials, &cwd)),
        skills_store: Arc::new(SkillStore::discover(&cwd)),
        cwd,
    };

    // TODO: link initial_prompt to the TUI; the tui will log it into the chat, and push to the router client.
    // TODO: check config auto-start for router and start it if needed
    // TODO: in next task we will run the main loop here

    tracing::debug!("smista-cli main command finished");
    Ok(())
}
