//! Credential command handlers.
//!
//! The handlers bridge parsed CLI arguments to provider credential storage.
//! They select the configured secret backend once per invocation, bind provider
//! credentials to the current working directory for local scope, and never print
//! secret values.

use std::sync::Arc;

use secrecy::SecretString;
use smista_sdk::core::model::Provider;

use crate::args::{CredentialsArgs, CredentialsCommand};
use crate::credentials::{CredentialsStorage, ProvidersCredentials};

/// Runs a `smista credentials` invocation.
///
/// The command stores credentials locally by default and globally when
/// `--global` is present. `enforce_keyring` controls whether the command may
/// fall back to file-backed storage when the platform keyring is unavailable.
///
/// # Errors
///
/// Returns an error if the current directory cannot be resolved, credential
/// storage cannot be initialized, or the selected storage operation fails.
pub fn run(
    CredentialsArgs { command, global }: CredentialsArgs,
    enforce_keyring: bool,
) -> anyhow::Result<()> {
    // setup credentials storage
    let cwd = std::env::current_dir()?;
    tracing::debug!(
        "running smista-cli credentials command; cwd is {cwd}",
        cwd = cwd.display()
    );
    let credentials = Arc::new(CredentialsStorage::new(enforce_keyring)?);
    tracing::debug!(
        "credentials storage initialized with backend {backend}",
        backend = credentials.backend()
    );
    let providers_credentials = Arc::new(ProvidersCredentials::new(credentials, &cwd));

    match command {
        CredentialsCommand::Set { provider, api_key } => {
            add_credentials(providers_credentials, provider, api_key, global)
        }
        CredentialsCommand::Check { provider } => {
            check_credentials(providers_credentials, provider)
        }
        CredentialsCommand::Remove { provider } => {
            remove_credentials(providers_credentials, provider, global)
        }
    }
}

fn add_credentials(
    providers_credentials: Arc<ProvidersCredentials>,
    provider: Provider,
    api_key: String,
    global: bool,
) -> anyhow::Result<()> {
    let api_key = SecretString::from(api_key);

    providers_credentials.set_provider_api_key(&provider, &api_key, global)?;

    println!("Credentials for provider {provider} added successfully.");

    Ok(())
}

fn check_credentials(
    providers_credentials: Arc<ProvidersCredentials>,
    provider: Provider,
) -> anyhow::Result<()> {
    let is_set = providers_credentials
        .get_provider_api_key(&provider)?
        .is_some();

    if is_set {
        println!("Credentials for provider {provider} are present.");
    } else {
        println!("Credentials for provider {provider} are not present.");
    }

    Ok(())
}

fn remove_credentials(
    providers_credentials: Arc<ProvidersCredentials>,
    provider: Provider,
    global: bool,
) -> anyhow::Result<()> {
    providers_credentials.delete_provider_api_key(&provider, global)?;

    println!("Credentials for provider {provider} removed successfully.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use secrecy::ExposeSecret as _;

    use super::*;

    fn providers(cwd: &Path) -> Arc<ProvidersCredentials> {
        let credentials = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets.toml"))
            .expect("create test credentials storage");

        Arc::new(ProvidersCredentials::new(Arc::new(credentials), cwd))
    }

    #[test]
    fn should_add_check_and_remove_local_credentials() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let providers = providers(tempdir.path());

        add_credentials(
            Arc::clone(&providers),
            Provider::OpenAI,
            "local-secret".to_string(),
            false,
        )
        .expect("add local credentials");
        check_credentials(Arc::clone(&providers), Provider::OpenAI)
            .expect("check local credentials");

        let stored = providers
            .get_provider_api_key(&Provider::OpenAI)
            .expect("read local credentials")
            .expect("local credentials exist");
        assert_eq!(stored.expose_secret(), "local-secret");

        remove_credentials(Arc::clone(&providers), Provider::OpenAI, false)
            .expect("remove local credentials");
        check_credentials(Arc::clone(&providers), Provider::OpenAI)
            .expect("check missing local credentials");

        assert!(
            providers
                .get_provider_api_key(&Provider::OpenAI)
                .expect("read removed local credentials")
                .is_none()
        );
    }

    #[test]
    fn should_add_check_and_remove_global_credentials() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let providers = providers(tempdir.path());

        add_credentials(
            Arc::clone(&providers),
            Provider::Anthropic,
            "global-secret".to_string(),
            true,
        )
        .expect("add global credentials");
        check_credentials(Arc::clone(&providers), Provider::Anthropic)
            .expect("check global credentials");

        let stored = providers
            .get_provider_api_key(&Provider::Anthropic)
            .expect("read global credentials")
            .expect("global credentials exist");
        assert_eq!(stored.expose_secret(), "global-secret");

        remove_credentials(Arc::clone(&providers), Provider::Anthropic, true)
            .expect("remove global credentials");
        check_credentials(Arc::clone(&providers), Provider::Anthropic)
            .expect("check missing global credentials");

        assert!(
            providers
                .get_provider_api_key(&Provider::Anthropic)
                .expect("read removed global credentials")
                .is_none()
        );
    }
}
