//! Main command to run (which runs without a subcommand).
//!
//! It start the TUI and the smista-router client; it is basically the main user interface for smista.ai

use std::path::PathBuf;
use std::sync::Arc;

use crate::credentials::{ApiKeyStorage, CredentialsStorage, ProvidersCredentials};

#[expect(dead_code, reason = "context will be used in the next tasks")]
pub struct Context {
    api_key: Arc<ApiKeyStorage>,
    cwd: PathBuf,
    providers_credentials: Arc<ProvidersCredentials>,
}

/// Runs smista.ai TUI and smista-router client; base subcommand.
pub fn run(enforce_keyring: bool) -> anyhow::Result<()> {
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

    // build context
    let _context = Context {
        api_key: Arc::new(ApiKeyStorage::new(credentials.clone(), &cwd)),
        providers_credentials: Arc::new(ProvidersCredentials::new(credentials, &cwd)),
        cwd,
    };

    // TODO: in the future Context will have to hold the E2EE keys storage, the provider credentials storage, and the apikey storage instead
    // TODO: context must also hold the CancellationToken
    // TODO: in next task we will run the main loop here

    tracing::debug!("smista-cli main command finished");
    Ok(())
}
