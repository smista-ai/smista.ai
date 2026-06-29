//! Per-request provider credentials and their request-header rendering.

use std::collections::HashMap;

use secrecy::{ExposeSecret as _, SecretString};
use smista_core::model::Provider;

/// Literal prefix of a provider credential header name. See
/// [`ProviderCredentials::headers_map`].
const HEADER_PREFIX: &str = "X-Smista-Provider-";
/// Literal suffix that closes a provider credential header name.
const HEADER_SUFFIX: &str = "-Api-Key";

/// API keys for upstream model providers, sent per request.
///
/// Provider credentials are distinct from router authentication (the API key and
/// session token): they are the keys the router needs to call a remote model on
/// the caller's behalf. They are held by the concrete client and used by the
/// methods that can reach a model (`execute`, `continue_run`, `stream_execute`,
/// `stream_continue` and `list_models`), travelling as
/// `X-Smista-Provider-<provider>-Api-Key` request headers; the router never
/// persists them.
///
/// Each key is wrapped in a [`SecretString`], so it is redacted in `Debug` output
/// and never appears in logs, traces or errors.
#[derive(Debug, Clone, Default)]
pub struct ProviderCredentials(HashMap<Provider, SecretString>);

impl ProviderCredentials {
    /// Creates an empty set of provider credentials.
    #[must_use]
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Creates an empty set with room for `capacity` providers without
    /// reallocating.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity(capacity))
    }

    /// Creates a set from an existing provider-to-key map.
    #[must_use]
    pub fn with_credentials(credentials: HashMap<Provider, SecretString>) -> Self {
        Self(credentials)
    }

    /// Adds a provider's key, returning the updated set for chaining.
    ///
    /// An existing key for the same provider is replaced.
    #[must_use]
    pub fn with_provider(mut self, provider: Provider, secret: SecretString) -> Self {
        self.0.insert(provider, secret);
        self
    }

    /// Inserts or replaces a provider's key in place.
    pub fn insert(&mut self, provider: Provider, secret: SecretString) {
        self.0.insert(provider, secret);
    }

    /// Returns the stored key for `provider`, if any.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "consumed by the concrete HTTP clients that attach the headers (#188)"
        )
    )]
    pub(crate) fn get(&self, provider: &Provider) -> Option<&SecretString> {
        self.0.get(provider)
    }

    /// Renders the credentials as a map of request header name to exposed key.
    ///
    /// The header name is `X-Smista-Provider-<segment>-Api-Key`, where `segment`
    /// is the provider's canonical name (`anthropic`, `openai`, …) or, for an
    /// [`OpenAICompatible`](Provider::OpenAICompatible) instance, its bare
    /// instance name. The `openai-compat:` scheme tag from the provider's
    /// [`Display`](std::fmt::Display) form is deliberately omitted, since a `:`
    /// cannot appear in a header name and the router matches the instance name
    /// directly.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "consumed by the concrete HTTP clients that attach the headers (#188)"
        )
    )]
    pub(crate) fn headers_map(&self) -> HashMap<String, String> {
        self.0
            .iter()
            .map(|(provider, secret)| {
                // Canonical providers use their lowercase `Display` form; an
                // OpenAI-compatible instance uses its bare name, dropping the
                // `openai-compat:` tag its `Display` would add — a `:` cannot
                // appear in a header name and the router matches the bare name.
                let segment = match provider {
                    Provider::OpenAICompatible(name) => name.clone(),
                    other => other.to_string(),
                };
                let header_name = format!("{HEADER_PREFIX}{segment}{HEADER_SUFFIX}");
                (header_name, secret.expose_secret().to_string())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(value: &str) -> SecretString {
        SecretString::from(value.to_string())
    }

    #[test]
    fn should_start_empty() {
        let credentials = ProviderCredentials::new();
        assert!(credentials.0.is_empty());
        assert!(ProviderCredentials::default().0.is_empty());
        assert!(ProviderCredentials::with_capacity(4).0.is_empty());
    }

    #[test]
    fn should_insert_and_replace_a_provider_key() {
        let mut credentials = ProviderCredentials::new();
        credentials.insert(Provider::Anthropic, secret("first"));
        credentials.insert(Provider::Anthropic, secret("second"));
        assert_eq!(credentials.0.len(), 1);
        assert_eq!(
            credentials.0[&Provider::Anthropic].expose_secret(),
            "second"
        );
    }

    #[test]
    fn should_build_from_a_map_and_via_chaining() {
        let mut map = HashMap::new();
        map.insert(Provider::OpenAI, secret("sk-openai"));
        let credentials = ProviderCredentials::with_credentials(map)
            .with_provider(Provider::Ollama, secret("local"));
        assert_eq!(credentials.0.len(), 2);
        assert_eq!(
            credentials.0[&Provider::OpenAI].expose_secret(),
            "sk-openai"
        );
        assert_eq!(credentials.0[&Provider::Ollama].expose_secret(), "local");
    }

    #[test]
    fn should_redact_keys_in_debug_output() {
        let credentials =
            ProviderCredentials::new().with_provider(Provider::Anthropic, secret("super-secret"));
        let rendered = format!("{credentials:?}");
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn should_return_a_stored_key_and_none_for_a_missing_one() {
        let credentials =
            ProviderCredentials::new().with_provider(Provider::Anthropic, secret("sk-ant"));
        assert_eq!(
            credentials
                .get(&Provider::Anthropic)
                .map(|s| s.expose_secret()),
            Some("sk-ant")
        );
        assert!(credentials.get(&Provider::OpenAI).is_none());
    }

    #[test]
    fn should_render_canonical_provider_headers() {
        let credentials = ProviderCredentials::new()
            .with_provider(Provider::Anthropic, secret("sk-ant"))
            .with_provider(Provider::OpenAI, secret("sk-openai"));
        let headers = credentials.headers_map();
        assert_eq!(
            headers
                .get("X-Smista-Provider-anthropic-Api-Key")
                .map(String::as_str),
            Some("sk-ant")
        );
        assert_eq!(
            headers
                .get("X-Smista-Provider-openai-Api-Key")
                .map(String::as_str),
            Some("sk-openai")
        );
    }

    #[test]
    fn should_render_an_openai_compatible_header_by_bare_instance_name() {
        // The `openai-compat:` tag from `Display` must not leak into the header,
        // since `:` is invalid there and the router matches the bare instance
        // name (`x-smista-provider-my-vllm-api-key`).
        let provider = Provider::OpenAICompatible("my-vllm".to_string());
        assert_eq!(provider.to_string(), "openai-compat:my-vllm");
        let credentials = ProviderCredentials::new().with_provider(provider, secret("sk-vllm"));
        let headers = credentials.headers_map();
        assert_eq!(
            headers
                .get("X-Smista-Provider-my-vllm-Api-Key")
                .map(String::as_str),
            Some("sk-vllm")
        );
    }
}
