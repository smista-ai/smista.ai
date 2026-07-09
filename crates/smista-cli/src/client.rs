//! Module to configure smista-router client.

use anyhow::Context as _;
use smista_sdk::client::{Client, ProviderCredentials, ReqwestClient, RouterClientConfig};
use smista_sdk::core::secret::SecretRef;
use url::Url;

use crate::config::Config;
use crate::credentials::ProvidersCredentials;

/// Get a configured smista-router client from the given configuration.
///
/// If the `router.url` field is not set in the configuration, the default
/// router URL will be used.
///
/// # Errors
///
/// Returns an error if the configured router URL is invalid or the HTTP client
/// cannot be constructed.
pub fn config_client(config: &Config) -> anyhow::Result<ReqwestClient> {
    let router_url = config
        .router
        .url
        .as_deref()
        .unwrap_or(smista_sdk::client::ROUTER_DEFAULT_URL);
    tracing::info!("using router URL: {router_url}");

    let router_url = Url::parse(router_url).context("Failed to parse router URL")?;
    tracing::debug!("parsed router URL: {router_url}");
    ReqwestClient::new(RouterClientConfig::new(router_url)).map_err(anyhow::Error::from)
}

/// Returns a router client seeded with provider credentials known to the CLI.
///
/// The router's provider registry decides which provider identities are
/// considered. For each advertised provider, project/global provider credential
/// storage takes precedence; if it is empty, the provider's `api_key` secret
/// reference in CLI config is resolved.
///
/// # Errors
///
/// Returns an error if the provider registry cannot be listed, if credential
/// storage cannot be read, or if a config secret reference cannot be resolved.
pub async fn inject_credentials(
    client: ReqwestClient,
    providers_credentials: &ProvidersCredentials,
    config: &Config,
) -> anyhow::Result<ReqwestClient> {
    tracing::debug!("injecting credentials into router client");
    let mut credentials = ProviderCredentials::new();

    let available_providers = client
        .list_providers()
        .await
        .context("Failed to list providers from router")?
        .providers;

    for provider in available_providers {
        if let Some(secret) = providers_credentials.get_provider_api_key(&provider.id)? {
            tracing::debug!(
                "found credential for provider {provider} inside of the credentials storage",
                provider = provider.id
            );
            credentials.insert(provider.id, secret);

            continue;
        }
        if let Some(secret) = config
            .providers
            .get(&provider.id)
            .and_then(|p| p.api_key.as_deref())
        {
            tracing::debug!(
                "found credential for provider {provider} inside of the config",
                provider = provider.id
            );
            let Some(secret_ref) = SecretRef::parse(secret) else {
                tracing::warn!(
                    "provider {provider} config credential is not a secret reference; skipping",
                    provider = provider.id
                );
                continue;
            };
            let Some(secret) = providers_credentials
                .get_config_api_key(&secret_ref)
                .with_context(|| {
                    format!(
                        "Failed to resolve config credential for provider {}",
                        provider.id
                    )
                })?
            else {
                tracing::info!(
                    "config credential for provider {provider} points to a missing secret",
                    provider = provider.id
                );
                continue;
            };
            credentials.insert(provider.id, secret);

            continue;
        }

        tracing::info!(
            "no credential found for provider {provider}, skipping",
            provider = provider.id
        );
    }

    Ok(client.with_provider_credentials(credentials))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use secrecy::SecretString;
    use smista_mock_web_server::{Endpoint, MockRouter, Request, ResponseTemplate};
    use smista_sdk::client::Client;
    use smista_sdk::core::api::ListProvidersResponse;
    use smista_sdk::core::model::{Provider, ProviderDescriptor};

    use super::*;
    use crate::credentials::{CredentialBackend, CredentialsStorage};

    #[test]
    fn should_get_client_from_config() {
        let config = Config::default();
        let client = config_client(&config).expect("Failed to get client from config");
        assert_eq!(
            client.base_url(),
            &Url::parse(smista_sdk::client::ROUTER_DEFAULT_URL)
                .expect("the default router URL parses")
        );

        let mut config = Config::default();
        config.router.url = Some("http://localhost:8080".to_string());
        let client = config_client(&config).expect("Failed to get client from config");
        assert_eq!(
            client.base_url(),
            &Url::parse("http://localhost:8080").expect("the configured router URL parses")
        );
    }

    #[test]
    fn should_reject_an_invalid_router_url() {
        let mut config = Config::default();
        config.router.url = Some("not a url".to_string());

        let error = config_client(&config).expect_err("invalid URLs are rejected");

        assert!(
            format!("{error:#}").contains("Failed to parse router URL"),
            "unexpected error chain: {error:#}"
        );
    }

    #[tokio::test]
    async fn should_inject_credentials_from_storage_for_advertised_providers() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::ListProviders,
                list_providers_response([Provider::OpenAI]),
            )
            .start()
            .await;
        let cwd = tempfile::tempdir().expect("test cwd is created");
        let credentials = providers_credentials(cwd.path());
        credentials
            .set_provider_api_key(&Provider::OpenAI, &SecretString::from("sk-stored"), false)
            .expect("provider credential is stored");
        let client = signed_in_client(&router).await;

        let client = inject_credentials(client, &credentials, &Config::default())
            .await
            .expect("credentials are injected");

        client.list_models().await.expect("list models succeeds");
        let requests = router.received_requests().await;
        assert_eq!(
            header_of(&requests, "/llm/models", "x-smista-provider-openai-api-key").as_deref(),
            Some("sk-stored")
        );
    }

    #[tokio::test]
    async fn should_resolve_config_credentials_for_advertised_providers() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::ListProviders,
                list_providers_response([Provider::Anthropic]),
            )
            .start()
            .await;
        let cwd = tempfile::tempdir().expect("test cwd is created");
        let storage = credentials_storage(cwd.path());
        storage
            .put_global("anthropic_config", &SecretString::from("sk-config"))
            .expect("config-referenced credential is stored");
        let credentials = ProvidersCredentials::new(Arc::clone(&storage), cwd.path());
        let config: Config = toml::from_str(
            r#"
            [providers.anthropic]
            type = "anthropic"
            api_key = "${secret:anthropic_config}"
            "#,
        )
        .expect("config parses");
        let client = signed_in_client(&router).await;

        let client = inject_credentials(client, &credentials, &config)
            .await
            .expect("credentials are injected");

        client.list_models().await.expect("list models succeeds");
        let requests = router.received_requests().await;
        assert_eq!(
            header_of(
                &requests,
                "/llm/models",
                "x-smista-provider-anthropic-api-key"
            )
            .as_deref(),
            Some("sk-config")
        );
    }

    #[tokio::test]
    async fn should_prefer_storage_credentials_over_config_credentials() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::ListProviders,
                list_providers_response([Provider::OpenAI]),
            )
            .start()
            .await;
        let cwd = tempfile::tempdir().expect("test cwd is created");
        let storage = credentials_storage(cwd.path());
        let credentials = ProvidersCredentials::new(Arc::clone(&storage), cwd.path());
        credentials
            .set_provider_api_key(&Provider::OpenAI, &SecretString::from("sk-stored"), false)
            .expect("provider credential is stored");
        storage
            .put_global("openai_config", &SecretString::from("sk-config"))
            .expect("config-referenced credential is stored");
        let config: Config = toml::from_str(
            r#"
            [providers.openai]
            type = "openai"
            api_key = "${secret:openai_config}"
            "#,
        )
        .expect("config parses");
        let client = signed_in_client(&router).await;

        let client = inject_credentials(client, &credentials, &config)
            .await
            .expect("credentials are injected");

        client.list_models().await.expect("list models succeeds");
        let requests = router.received_requests().await;
        assert_eq!(
            header_of(&requests, "/llm/models", "x-smista-provider-openai-api-key").as_deref(),
            Some("sk-stored")
        );
    }

    #[tokio::test]
    async fn should_ignore_config_credentials_for_unadvertised_providers() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::ListProviders,
                list_providers_response([Provider::OpenAI]),
            )
            .start()
            .await;
        let cwd = tempfile::tempdir().expect("test cwd is created");
        let storage = credentials_storage(cwd.path());
        storage
            .put_global("anthropic_config", &SecretString::from("sk-config"))
            .expect("config-referenced credential is stored");
        let credentials = ProvidersCredentials::new(Arc::clone(&storage), cwd.path());
        let config: Config = toml::from_str(
            r#"
            [providers.anthropic]
            type = "anthropic"
            api_key = "${secret:anthropic_config}"
            "#,
        )
        .expect("config parses");
        let client = signed_in_client(&router).await;

        let client = inject_credentials(client, &credentials, &config)
            .await
            .expect("credentials are injected");

        client.list_models().await.expect("list models succeeds");
        let requests = router.received_requests().await;
        assert!(
            header_of(
                &requests,
                "/llm/models",
                "x-smista-provider-anthropic-api-key"
            )
            .is_none()
        );
    }

    fn list_providers_response(providers: impl IntoIterator<Item = Provider>) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(ListProvidersResponse {
            providers: providers
                .into_iter()
                .map(|provider| ProviderDescriptor {
                    display_name: provider.to_string(),
                    id: provider,
                    local: false,
                })
                .collect(),
        })
    }

    async fn signed_in_client(router: &MockRouter) -> ReqwestClient {
        let client =
            ReqwestClient::new(RouterClientConfig::new(router.base_url())).expect("client builds");
        client.sign_in().await.expect("sign-in succeeds");
        client
    }

    fn providers_credentials(cwd: &std::path::Path) -> ProvidersCredentials {
        ProvidersCredentials::new(credentials_storage(cwd), cwd)
    }

    fn credentials_storage(cwd: &std::path::Path) -> Arc<CredentialsStorage> {
        let storage = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets"))
            .expect("test credentials storage builds");
        assert_eq!(storage.backend(), CredentialBackend::File);
        Arc::new(storage)
    }

    fn header_of(requests: &[Request], suffix: &str, name: &str) -> Option<String> {
        requests
            .iter()
            .find(|request| request.url.path().ends_with(suffix))
            .and_then(|request| request.headers.get(name))
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }
}
