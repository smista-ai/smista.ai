//! Module to configure smista-router client.

use anyhow::Context as _;
use smista_sdk::client::{ReqwestClient, RouterClientConfig};
use url::Url;

use crate::config::Config;

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

#[cfg(test)]
mod tests {

    use smista_sdk::client::Client;

    use super::*;

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
}
