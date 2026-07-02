//! Credentials storage for the CLI.
//!
//! [`CredentialsStorage`] is the CLI-facing wrapper around the secret backends.
//! It prefers the operating-system keyring, but can fall back to the file
//! backend for environments where a keyring is unavailable, such as headless
//! Linux sessions or CI jobs.

mod api_key;
mod providers;
mod secrets;

use std::fmt;
use std::path::Path;

use secrecy::SecretString;

pub use self::api_key::ApiKeyStorage;
pub use self::providers::ProvidersCredentials;
use crate::credentials::secrets::{FileSecretStorage, KeyringSecretStorage, SecretStorage};

/// Secret backend selected for a [`CredentialsStorage`] instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialBackend {
    /// Operating-system credential store.
    Keyring,
    /// Local TOML secrets file.
    File,
}

impl fmt::Display for CredentialBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialBackend::Keyring => write!(f, "keyring"),
            CredentialBackend::File => write!(f, "file"),
        }
    }
}

/// Stores CLI credentials through the configured secret backend.
pub struct CredentialsStorage {
    /// Backend used to persist raw provider secrets.
    secret_storage: Box<dyn SecretStorage>,
    /// Selected backend kind, kept separately because trait objects do not
    /// expose stable type identity for user-facing reporting.
    backend: CredentialBackend,
}

impl fmt::Debug for CredentialsStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialsStorage")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl CredentialsStorage {
    /// Creates credential storage for the CLI.
    ///
    /// When `enforce_keyring` is `false`, this tries the platform keyring first
    /// and falls back to the file backend if keyring construction fails. When
    /// `enforce_keyring` is `true`, keyring failure is returned to the caller
    /// and file storage is not attempted.
    ///
    /// # Errors
    ///
    /// Returns an error if the enforced keyring backend cannot be initialized,
    /// or if both the preferred keyring backend and fallback file backend fail.
    pub fn new(enforce_keyring: bool) -> anyhow::Result<Self> {
        Self::new_from_factories(
            enforce_keyring,
            || Ok(Box::new(KeyringSecretStorage::new()?)),
            || Ok(Box::new(FileSecretStorage::new()?)),
        )
    }

    /// Returns the selected backend.
    #[must_use]
    pub fn backend(&self) -> CredentialBackend {
        self.backend
    }

    /// Gets a credential value for the given `key_name`.
    ///
    /// The key is first searched in project-local storage. If it is missing
    /// there, global storage is checked. Backend errors are returned
    /// immediately; only a successful local miss falls through to global
    /// storage.
    ///
    /// If it is not found in either, `None` is returned.
    pub fn get(&self, cwd: &Path, key_name: &str) -> anyhow::Result<Option<SecretString>> {
        match self.secret_storage.get_local(key_name, cwd)? {
            Some(value) => Ok(Some(value)),
            None => self.secret_storage.get_global(key_name),
        }
    }

    /// Stores or replaces a global credential.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected backend cannot persist the credential.
    pub fn put_global(&self, key_name: &str, value: &SecretString) -> anyhow::Result<()> {
        self.secret_storage.put_global(key_name, value)
    }

    /// Stores or replaces a project-local credential.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected backend cannot persist the credential.
    pub fn put_local(
        &self,
        cwd: &Path,
        key_name: &str,
        value: &SecretString,
    ) -> anyhow::Result<()> {
        self.secret_storage.put_local(key_name, cwd, value)
    }

    /// Deletes a global credential.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected backend cannot update its storage.
    pub fn delete_global(&self, key_name: &str) -> anyhow::Result<()> {
        self.secret_storage.delete_global(key_name)
    }

    /// Deletes a project-local credential.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected backend cannot update its storage.
    pub fn delete_local(&self, cwd: &Path, key_name: &str) -> anyhow::Result<()> {
        self.secret_storage.delete_local(key_name, cwd)
    }

    /// Creates storage from injectable backend factories.
    fn new_from_factories<K, F>(
        enforce_keyring: bool,
        keyring_factory: K,
        file_factory: F,
    ) -> anyhow::Result<Self>
    where
        K: FnOnce() -> anyhow::Result<Box<dyn SecretStorage>>,
        F: FnOnce() -> anyhow::Result<Box<dyn SecretStorage>>,
    {
        match keyring_factory() {
            Ok(secret_storage) => Ok(Self {
                secret_storage,
                backend: CredentialBackend::Keyring,
            }),
            Err(keyring_error) if enforce_keyring => Err(keyring_error)
                .map_err(|err| err.context("keyring credential storage is required")),
            Err(keyring_error) => {
                tracing::debug!(
                    error = %keyring_error,
                    "Keyring credential storage is unavailable; falling back to file storage."
                );
                file_factory()
                    .map(|secret_storage| Self {
                        secret_storage,
                        backend: CredentialBackend::File,
                    })
                    .map_err(|file_error| {
                        file_error.context(format!(
                            "keyring credential storage is unavailable ({keyring_error}); file credential storage also failed"
                        ))
                    })
            }
        }
    }

    /// Creates storage from an already-built backend.
    #[cfg(test)]
    fn from_secret_storage(
        backend: CredentialBackend,
        secret_storage: Box<dyn SecretStorage>,
    ) -> Self {
        Self {
            secret_storage,
            backend,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};

    use anyhow::anyhow;
    use secrecy::ExposeSecret as _;

    use super::*;

    #[derive(Debug, Default)]
    struct MockState {
        calls: Vec<String>,
        local: BTreeMap<(PathBuf, String), String>,
        global: BTreeMap<String, String>,
        fail_local_get: Option<&'static str>,
        fail_global_get: Option<&'static str>,
        fail_local_put: Option<&'static str>,
        fail_global_put: Option<&'static str>,
        fail_local_delete: Option<&'static str>,
        fail_global_delete: Option<&'static str>,
    }

    #[derive(Clone, Debug, Default)]
    struct MockSecretStorage {
        state: Arc<RwLock<MockState>>,
    }

    impl MockSecretStorage {
        fn calls(&self) -> Vec<String> {
            self.state.read().unwrap().calls.clone()
        }

        fn set_local(&self, path: &Path, key_name: &str, value: &str) {
            self.state.write().unwrap().local.insert(
                (path.to_path_buf(), key_name.to_string()),
                value.to_string(),
            );
        }

        fn set_global(&self, key_name: &str, value: &str) {
            self.state
                .write()
                .unwrap()
                .global
                .insert(key_name.to_string(), value.to_string());
        }

        fn global_value(&self, key_name: &str) -> Option<String> {
            self.state.read().unwrap().global.get(key_name).cloned()
        }

        fn local_value(&self, path: &Path, key_name: &str) -> Option<String> {
            self.state
                .read()
                .unwrap()
                .local
                .get(&(path.to_path_buf(), key_name.to_string()))
                .cloned()
        }

        fn fail_local_get(&self, message: &'static str) {
            self.state.write().unwrap().fail_local_get = Some(message);
        }

        fn fail_global_get(&self, message: &'static str) {
            self.state.write().unwrap().fail_global_get = Some(message);
        }

        fn fail_local_put(&self, message: &'static str) {
            self.state.write().unwrap().fail_local_put = Some(message);
        }

        fn fail_global_put(&self, message: &'static str) {
            self.state.write().unwrap().fail_global_put = Some(message);
        }

        fn fail_local_delete(&self, message: &'static str) {
            self.state.write().unwrap().fail_local_delete = Some(message);
        }

        fn fail_global_delete(&self, message: &'static str) {
            self.state.write().unwrap().fail_global_delete = Some(message);
        }
    }

    impl SecretStorage for MockSecretStorage {
        fn put_local(
            &self,
            key_name: &str,
            path: &Path,
            value: &SecretString,
        ) -> anyhow::Result<()> {
            let mut state = self.state.write().unwrap();
            state
                .calls
                .push(format!("put_local:{}:{}", path.display(), key_name));
            if let Some(message) = state.fail_local_put {
                return Err(anyhow!(message));
            }
            state.local.insert(
                (path.to_path_buf(), key_name.to_string()),
                value.expose_secret().to_string(),
            );
            Ok(())
        }

        fn put_global(&self, key_name: &str, value: &SecretString) -> anyhow::Result<()> {
            let mut state = self.state.write().unwrap();
            state.calls.push(format!("put_global:{key_name}"));
            if let Some(message) = state.fail_global_put {
                return Err(anyhow!(message));
            }
            state
                .global
                .insert(key_name.to_string(), value.expose_secret().to_string());
            Ok(())
        }

        fn get_local(&self, key_name: &str, path: &Path) -> anyhow::Result<Option<SecretString>> {
            let mut state = self.state.write().unwrap();
            state
                .calls
                .push(format!("get_local:{}:{}", path.display(), key_name));
            if let Some(message) = state.fail_local_get {
                return Err(anyhow!(message));
            }
            Ok(state
                .local
                .get(&(path.to_path_buf(), key_name.to_string()))
                .cloned()
                .map(SecretString::from))
        }

        fn get_global(&self, key_name: &str) -> anyhow::Result<Option<SecretString>> {
            let mut state = self.state.write().unwrap();
            state.calls.push(format!("get_global:{key_name}"));
            if let Some(message) = state.fail_global_get {
                return Err(anyhow!(message));
            }
            Ok(state.global.get(key_name).cloned().map(SecretString::from))
        }

        fn delete_local(&self, key_name: &str, path: &Path) -> anyhow::Result<()> {
            let mut state = self.state.write().unwrap();
            state
                .calls
                .push(format!("delete_local:{}:{}", path.display(), key_name));
            if let Some(message) = state.fail_local_delete {
                return Err(anyhow!(message));
            }
            state
                .local
                .remove(&(path.to_path_buf(), key_name.to_string()));
            Ok(())
        }

        fn delete_global(&self, key_name: &str) -> anyhow::Result<()> {
            let mut state = self.state.write().unwrap();
            state.calls.push(format!("delete_global:{key_name}"));
            if let Some(message) = state.fail_global_delete {
                return Err(anyhow!(message));
            }
            state.global.remove(key_name);
            Ok(())
        }
    }

    fn storage(mock: MockSecretStorage) -> CredentialsStorage {
        CredentialsStorage::from_secret_storage(CredentialBackend::File, Box::new(mock))
    }

    fn secret(value: &str) -> SecretString {
        SecretString::from(value.to_string())
    }

    #[test]
    fn should_select_keyring_when_available() {
        let keyring = MockSecretStorage::default();
        let file_called = Arc::new(RwLock::new(false));
        let file_called_for_factory = Arc::clone(&file_called);

        let storage = CredentialsStorage::new_from_factories(
            false,
            || Ok(Box::new(keyring.clone())),
            || {
                *file_called_for_factory.write().unwrap() = true;
                Ok(Box::new(MockSecretStorage::default()))
            },
        )
        .unwrap();

        assert_eq!(storage.backend(), CredentialBackend::Keyring);
        assert!(!*file_called.read().unwrap());
    }

    #[test]
    fn should_fallback_to_file_when_keyring_is_not_enforced() {
        let file = MockSecretStorage::default();

        let storage = CredentialsStorage::new_from_factories(
            false,
            || Err(anyhow!("keyring unavailable")),
            || Ok(Box::new(file.clone())),
        )
        .unwrap();

        assert_eq!(storage.backend(), CredentialBackend::File);
    }

    #[test]
    fn should_not_fallback_to_file_when_keyring_is_enforced() {
        let file_called = Arc::new(RwLock::new(false));
        let file_called_for_factory = Arc::clone(&file_called);

        let err = CredentialsStorage::new_from_factories(
            true,
            || Err(anyhow!("keyring unavailable")),
            || {
                *file_called_for_factory.write().unwrap() = true;
                Ok(Box::new(MockSecretStorage::default()))
            },
        )
        .unwrap_err();

        assert!(!*file_called.read().unwrap());
        assert!(
            err.to_string().contains("keyring credential storage"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn should_report_file_failure_after_keyring_fallback_failure() {
        let err = CredentialsStorage::new_from_factories(
            false,
            || Err(anyhow!("keyring unavailable")),
            || Err(anyhow!("file unavailable")),
        )
        .unwrap_err();

        assert!(
            format!("{err:#}").contains("file unavailable"),
            "unexpected error chain: {err:#}"
        );
        assert!(
            format!("{err:#}").contains("keyring unavailable"),
            "unexpected error chain: {err:#}"
        );
    }

    #[test]
    fn should_get_local_credential_without_reading_global_storage() {
        let cwd = Path::new("/repo");
        let mock = MockSecretStorage::default();
        mock.set_local(cwd, "openai", "local-secret");

        let stored = storage(mock.clone()).get(cwd, "openai").unwrap().unwrap();

        assert_eq!(stored.expose_secret(), "local-secret");
        assert_eq!(mock.calls(), vec!["get_local:/repo:openai"]);
    }

    #[test]
    fn should_get_global_credential_when_local_credential_is_missing() {
        let cwd = Path::new("/repo");
        let mock = MockSecretStorage::default();
        mock.set_global("openai", "global-secret");

        let stored = storage(mock.clone()).get(cwd, "openai").unwrap().unwrap();

        assert_eq!(stored.expose_secret(), "global-secret");
        assert_eq!(
            mock.calls(),
            vec!["get_local:/repo:openai", "get_global:openai"]
        );
    }

    #[test]
    fn should_return_none_when_credential_is_missing_in_both_scopes() {
        let cwd = Path::new("/repo");
        let mock = MockSecretStorage::default();

        let stored = storage(mock.clone()).get(cwd, "openai").unwrap();

        assert!(stored.is_none());
        assert_eq!(
            mock.calls(),
            vec!["get_local:/repo:openai", "get_global:openai"]
        );
    }

    #[test]
    fn should_not_read_global_storage_when_local_read_fails() {
        let cwd = Path::new("/repo");
        let mock = MockSecretStorage::default();
        mock.set_global("openai", "global-secret");
        mock.fail_local_get("local backend failed");

        let err = storage(mock.clone()).get(cwd, "openai").unwrap_err();

        assert!(
            err.to_string().contains("local backend failed"),
            "unexpected error: {err}"
        );
        assert_eq!(mock.calls(), vec!["get_local:/repo:openai"]);
    }

    #[test]
    fn should_return_global_read_errors() {
        let cwd = Path::new("/repo");
        let mock = MockSecretStorage::default();
        mock.fail_global_get("global backend failed");

        let err = storage(mock).get(cwd, "openai").unwrap_err();

        assert!(
            err.to_string().contains("global backend failed"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn should_put_and_delete_global_credential() {
        let mock = MockSecretStorage::default();
        let storage = storage(mock.clone());

        storage.put_global("openai", &secret("stored")).unwrap();
        assert_eq!(mock.global_value("openai").as_deref(), Some("stored"));

        storage.delete_global("openai").unwrap();
        assert!(mock.global_value("openai").is_none());
        assert_eq!(
            mock.calls(),
            vec!["put_global:openai", "delete_global:openai"]
        );
    }

    #[test]
    fn should_put_and_delete_local_credential() {
        let cwd = Path::new("/repo");
        let mock = MockSecretStorage::default();
        let storage = storage(mock.clone());

        storage.put_local(cwd, "openai", &secret("stored")).unwrap();
        assert_eq!(mock.local_value(cwd, "openai").as_deref(), Some("stored"));

        storage.delete_local(cwd, "openai").unwrap();
        assert!(mock.local_value(cwd, "openai").is_none());
        assert_eq!(
            mock.calls(),
            vec!["put_local:/repo:openai", "delete_local:/repo:openai"]
        );
    }

    #[test]
    fn should_propagate_put_and_delete_errors() {
        let cwd = Path::new("/repo");
        let mock = MockSecretStorage::default();
        mock.fail_global_put("global put failed");
        mock.fail_local_put("local put failed");
        mock.fail_global_delete("global delete failed");
        mock.fail_local_delete("local delete failed");
        let storage = storage(mock);

        assert!(
            storage
                .put_global("openai", &secret("stored"))
                .unwrap_err()
                .to_string()
                .contains("global put failed")
        );
        assert!(
            storage
                .put_local(cwd, "openai", &secret("stored"))
                .unwrap_err()
                .to_string()
                .contains("local put failed")
        );
        assert!(
            storage
                .delete_global("openai")
                .unwrap_err()
                .to_string()
                .contains("global delete failed")
        );
        assert!(
            storage
                .delete_local(cwd, "openai")
                .unwrap_err()
                .to_string()
                .contains("local delete failed")
        );
    }

    #[test]
    fn should_debug_without_exposing_storage_details() {
        let mock = MockSecretStorage::default();
        mock.set_global("openai", "secret-value");

        let rendered = format!("{:?}", storage(mock));

        assert!(rendered.contains("CredentialsStorage"));
        assert!(rendered.contains("File"));
        assert!(!rendered.contains("secret-value"));
    }

    #[test]
    fn should_display_backend() {
        assert_eq!(format!("{}", CredentialBackend::Keyring), "keyring");
        assert_eq!(format!("{}", CredentialBackend::File), "file");
    }
}
