//! Main command to run (which runs without a subcommand).
//!
//! It start the TUI and the smista-router client; it is basically the main user interface for smista.ai

use crate::credentials::CredentialStorage;

/// Runs smista.ai TUI and smista-router client; base subcommand.
pub fn run(enforce_keyring: bool) -> anyhow::Result<()> {
    tracing::debug!("running smista-cli main command");
    let _credentials = CredentialStorage::new(enforce_keyring)?;

    // TODO: in the future Context will have to hold the E2EE keys storage, the provider credentials storage, and the apikey storage instead
    // TODO: context must also hold the CancellationToken
    // TODO: in next task we will run the main loop here

    tracing::debug!("smista-cli main command finished");
    Ok(())
}
