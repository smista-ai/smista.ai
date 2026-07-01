//! Provider credential storage.
//!
//! [`ProvidersCredentials`] maps typed provider identities to the string keys
//! used by the lower-level [`CredentialsStorage`] backend. It keeps the CLI's
//! local/global scope decision separate from provider parsing and never exposes
//! secret values in logs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use secrecy::SecretString;
use smista_sdk::core::model::Provider;

use crate::credentials::CredentialsStorage;

/// Stores and reads API keys by provider identity.
///
/// Local credentials are scoped to the `cwd` supplied at construction time.
/// Global credentials use the backend's global scope. Reads always ask
/// [`CredentialsStorage`] to resolve local credentials first and then global
/// credentials.
pub struct ProvidersCredentials {
    cwd: PathBuf,
    storage: Arc<CredentialsStorage>,
}

impl ProvidersCredentials {
    /// Creates provider credential storage bound to `cwd`.
    #[must_use]
    pub fn new(storage: Arc<CredentialsStorage>, cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            storage,
        }
    }

    /// Stores or replaces the API key for `provider`.
    ///
    /// When `global` is `true`, the key is stored in global credential storage.
    /// Otherwise, it is stored in the project-local scope for this value's
    /// configured `cwd`.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected credential backend cannot write the
    /// secret.
    pub fn set_provider_api_key(
        &self,
        provider: &Provider,
        api_key: &SecretString,
        global: bool,
    ) -> anyhow::Result<()> {
        let key_name = provider.to_string();
        if global {
            tracing::debug!("Storing API key for provider {key_name} globally",);
            self.storage.put_global(&key_name, api_key)
        } else {
            tracing::debug!(
                "Storing API key for provider {key_name} in local directory {cwd}",
                cwd = self.cwd.display()
            );
            self.storage.put_local(&self.cwd, &key_name, api_key)
        }
    }

    /// Reads the API key for `provider`, if one is configured.
    ///
    /// Local credentials take precedence over global credentials through
    /// [`CredentialsStorage::get`].
    ///
    /// # Errors
    ///
    /// Returns an error if the selected credential backend cannot read the
    /// relevant local or global scope.
    pub fn get_provider_api_key(
        &self,
        provider: &Provider,
    ) -> anyhow::Result<Option<SecretString>> {
        let key_name = provider.to_string();
        tracing::debug!("Retrieving API key for provider {key_name}");
        self.storage.get(&self.cwd, &key_name)
    }

    /// Removes the API key for `provider` from the selected scope.
    ///
    /// When `global` is `true`, the key is removed from global credential
    /// storage. Otherwise, it is removed only from this value's project-local
    /// scope.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected credential backend cannot update the
    /// relevant scope.
    pub fn delete_provider_api_key(&self, provider: &Provider, global: bool) -> anyhow::Result<()> {
        let key_name = provider.to_string();
        if global {
            tracing::debug!("Deleting API key for provider {key_name} globally",);
            self.storage.delete_global(&key_name)
        } else {
            tracing::debug!(
                "Deleting API key for provider {key_name} in local directory {cwd}",
                cwd = self.cwd.display()
            );
            self.storage.delete_local(&self.cwd, &key_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::RwLock;

    use secrecy::ExposeSecret as _;

    use super::*;
    use crate::credentials::secrets::SecretStorage;
    use crate::credentials::{CredentialBackend, CredentialsStorage};

    #[derive(Debug, Default)]
    struct MockSecretStorage {
        local: RwLock<BTreeMap<(PathBuf, String), String>>,
        global: RwLock<BTreeMap<String, String>>,
    }

    impl MockSecretStorage {
        fn local_value(&self, path: &Path, key_name: &str) -> Option<String> {
            self.local
                .read()
                .unwrap()
                .get(&(path.to_path_buf(), key_name.to_string()))
                .cloned()
        }

        fn global_value(&self, key_name: &str) -> Option<String> {
            self.global.read().unwrap().get(key_name).cloned()
        }
    }

    impl SecretStorage for Arc<MockSecretStorage> {
        fn put_local(
            &self,
            key_name: &str,
            path: &Path,
            value: &SecretString,
        ) -> anyhow::Result<()> {
            self.local.write().unwrap().insert(
                (path.to_path_buf(), key_name.to_string()),
                value.expose_secret().to_string(),
            );
            Ok(())
        }

        fn put_global(&self, key_name: &str, value: &SecretString) -> anyhow::Result<()> {
            self.global
                .write()
                .unwrap()
                .insert(key_name.to_string(), value.expose_secret().to_string());
            Ok(())
        }

        fn get_local(&self, key_name: &str, path: &Path) -> anyhow::Result<Option<SecretString>> {
            Ok(self.local_value(path, key_name).map(SecretString::from))
        }

        fn get_global(&self, key_name: &str) -> anyhow::Result<Option<SecretString>> {
            Ok(self.global_value(key_name).map(SecretString::from))
        }

        fn delete_local(&self, key_name: &str, path: &Path) -> anyhow::Result<()> {
            self.local
                .write()
                .unwrap()
                .remove(&(path.to_path_buf(), key_name.to_string()));
            Ok(())
        }

        fn delete_global(&self, key_name: &str) -> anyhow::Result<()> {
            self.global.write().unwrap().remove(key_name);
            Ok(())
        }
    }

    fn providers(cwd: &Path, mock: Arc<MockSecretStorage>) -> ProvidersCredentials {
        let storage =
            CredentialsStorage::from_secret_storage(CredentialBackend::File, Box::new(mock));

        ProvidersCredentials::new(Arc::new(storage), cwd)
    }

    #[test]
    fn should_store_provider_api_key_locally_by_default() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let providers = providers(cwd, Arc::clone(&mock));

        providers
            .set_provider_api_key(
                &Provider::OpenAI,
                &SecretString::from("local-secret"),
                false,
            )
            .unwrap();

        assert_eq!(
            mock.local_value(cwd, "openai").as_deref(),
            Some("local-secret")
        );
        assert!(mock.global_value("openai").is_none());
    }

    #[test]
    fn should_store_provider_api_key_globally_when_requested() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let providers = providers(cwd, Arc::clone(&mock));

        providers
            .set_provider_api_key(
                &Provider::Anthropic,
                &SecretString::from("global-secret"),
                true,
            )
            .unwrap();

        assert_eq!(
            mock.global_value("anthropic").as_deref(),
            Some("global-secret")
        );
        assert!(mock.local_value(cwd, "anthropic").is_none());
    }

    #[test]
    fn should_use_openai_compatible_provider_identity_as_key_name() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let providers = providers(cwd, Arc::clone(&mock));
        let provider = Provider::OpenAICompatible("my-vllm".to_string());

        providers
            .set_provider_api_key(&provider, &SecretString::from("compat-secret"), false)
            .unwrap();
        let stored = providers.get_provider_api_key(&provider).unwrap().unwrap();

        assert_eq!(stored.expose_secret(), "compat-secret");
        assert_eq!(
            mock.local_value(cwd, "openai-compat:my-vllm").as_deref(),
            Some("compat-secret")
        );
    }

    #[test]
    fn should_prefer_local_provider_api_key_over_global() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let providers = providers(cwd, Arc::clone(&mock));

        providers
            .set_provider_api_key(&Provider::Gemini, &SecretString::from("global"), true)
            .unwrap();
        providers
            .set_provider_api_key(&Provider::Gemini, &SecretString::from("local"), false)
            .unwrap();
        let stored = providers
            .get_provider_api_key(&Provider::Gemini)
            .unwrap()
            .unwrap();

        assert_eq!(stored.expose_secret(), "local");
    }

    #[test]
    fn should_delete_provider_api_key_from_selected_scope_only() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let providers = providers(cwd, Arc::clone(&mock));

        providers
            .set_provider_api_key(&Provider::Ollama, &SecretString::from("local"), false)
            .unwrap();
        providers
            .set_provider_api_key(&Provider::Ollama, &SecretString::from("global"), true)
            .unwrap();
        providers
            .delete_provider_api_key(&Provider::Ollama, false)
            .unwrap();

        assert!(mock.local_value(cwd, "ollama").is_none());
        assert_eq!(mock.global_value("ollama").as_deref(), Some("global"));

        providers
            .delete_provider_api_key(&Provider::Ollama, true)
            .unwrap();

        assert!(mock.global_value("ollama").is_none());
    }
}
