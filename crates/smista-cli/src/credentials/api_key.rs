//! API key storage.
//!
//! [`ApiKeyStorage`] stores the smista.ai router API key in either the global or
//! workspace credential scope. It keeps the CLI's local/global scope decision
//! separate from key parsing and never exposes secret values in logs.

const API_KEY_NAME: &str = "api_key";

use std::path::{Path, PathBuf};
use std::sync::Arc;

use smista_sdk::client::ApiKey;

use crate::credentials::CredentialsStorage;

/// Stores and reads the CLI's router API key.
///
/// Local API keys are scoped to the `cwd` supplied at construction time.
/// Global API keys use the backend's global scope. Reads always ask
/// [`CredentialsStorage`] to resolve local credentials first and then global
/// credentials, so a project-local key overrides a global one.
pub struct ApiKeyStorage {
    cwd: PathBuf,
    storage: Arc<CredentialsStorage>,
}

impl ApiKeyStorage {
    /// Creates API key storage bound to `cwd`.
    #[must_use]
    pub fn new(storage: Arc<CredentialsStorage>, cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            storage,
        }
    }

    /// Reads the configured router API key, if one is available.
    ///
    /// Local storage is preferred over global storage through
    /// [`CredentialsStorage::get`]. A stored value that is not a valid
    /// [`ApiKey`] is ignored and reported as missing, so a corrupted credential
    /// does not prevent the CLI from starting.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected credential backend cannot read the
    /// relevant local or global scope.
    pub fn get(&self) -> anyhow::Result<Option<ApiKey>> {
        tracing::debug!(
            "Reading API key for smista.ai router with local directory {cwd}",
            cwd = self.cwd.display()
        );
        match self.storage.get(&self.cwd, API_KEY_NAME)? {
            None => Ok(None),
            Some(secret_string) => {
                tracing::debug!(
                    "Found API key for smista.ai router; parsing API key with local directory {cwd}",
                    cwd = self.cwd.display()
                );
                match ApiKey::try_from(secret_string) {
                    Ok(api_key) => Ok(Some(api_key)),
                    Err(err) => {
                        tracing::error!(
                            "Failed to parse API key for smista.ai router in local directory {cwd}: {err}. Ignoring the value.",
                            cwd = self.cwd.display()
                        );
                        Ok(None)
                    }
                }
            }
        }
    }

    /// Stores or replaces the router API key.
    ///
    /// When `global` is `true`, the key is stored in global credential storage.
    /// Otherwise, it is stored in the project-local scope for this value's
    /// configured `cwd`.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected credential backend cannot write the
    /// API key.
    pub fn set(&self, api_key: &ApiKey, global: bool) -> anyhow::Result<()> {
        if global {
            self.set_global(api_key)
        } else {
            self.set_local(api_key)
        }
    }

    /// Removes the router API key from the selected scope.
    ///
    /// When `global` is `true`, the key is removed from global credential
    /// storage. Otherwise, it is removed only from this value's project-local
    /// scope.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected credential backend cannot update the
    /// relevant scope.
    pub fn delete(&self, global: bool) -> anyhow::Result<()> {
        if global {
            self.delete_global()
        } else {
            self.delete_local()
        }
    }

    fn set_local(&self, api_key: &ApiKey) -> anyhow::Result<()> {
        tracing::debug!(
            "Storing API key for smista.ai router in local directory {cwd}",
            cwd = self.cwd.display()
        );
        self.storage
            .put_local(&self.cwd, API_KEY_NAME, api_key.as_secret_string())
    }

    fn set_global(&self, api_key: &ApiKey) -> anyhow::Result<()> {
        tracing::debug!("Storing API key for smista.ai router globally");
        self.storage
            .put_global(API_KEY_NAME, api_key.as_secret_string())
    }

    fn delete_local(&self) -> anyhow::Result<()> {
        tracing::debug!(
            "Removing API key for smista.ai router in local directory {cwd}",
            cwd = self.cwd.display()
        );
        self.storage.delete_local(&self.cwd, API_KEY_NAME)
    }

    fn delete_global(&self) -> anyhow::Result<()> {
        tracing::debug!("Removing API key for smista.ai router globally");
        self.storage.delete_global(API_KEY_NAME)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::RwLock;

    use anyhow::anyhow;
    use secrecy::{ExposeSecret as _, SecretString};
    use uuid::Uuid;

    use super::*;
    use crate::credentials::secrets::SecretStorage;
    use crate::credentials::{CredentialBackend, CredentialsStorage};

    #[derive(Debug, Default)]
    struct MockSecretStorage {
        local: RwLock<BTreeMap<(PathBuf, String), String>>,
        global: RwLock<BTreeMap<String, String>>,
        fail_get: RwLock<Option<&'static str>>,
        fail_put: RwLock<Option<&'static str>>,
        fail_delete: RwLock<Option<&'static str>>,
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

        fn set_local_raw(&self, path: &Path, key_name: &str, value: &str) {
            self.local.write().unwrap().insert(
                (path.to_path_buf(), key_name.to_string()),
                value.to_string(),
            );
        }

        fn set_global_raw(&self, key_name: &str, value: &str) {
            self.global
                .write()
                .unwrap()
                .insert(key_name.to_string(), value.to_string());
        }

        fn fail_get(&self, message: &'static str) {
            *self.fail_get.write().unwrap() = Some(message);
        }

        fn fail_put(&self, message: &'static str) {
            *self.fail_put.write().unwrap() = Some(message);
        }

        fn fail_delete(&self, message: &'static str) {
            *self.fail_delete.write().unwrap() = Some(message);
        }
    }

    impl SecretStorage for Arc<MockSecretStorage> {
        fn put_local(
            &self,
            key_name: &str,
            path: &Path,
            value: &SecretString,
        ) -> anyhow::Result<()> {
            if let Some(message) = *self.fail_put.read().unwrap() {
                return Err(anyhow!(message));
            }
            self.local.write().unwrap().insert(
                (path.to_path_buf(), key_name.to_string()),
                value.expose_secret().to_string(),
            );
            Ok(())
        }

        fn put_global(&self, key_name: &str, value: &SecretString) -> anyhow::Result<()> {
            if let Some(message) = *self.fail_put.read().unwrap() {
                return Err(anyhow!(message));
            }
            self.global
                .write()
                .unwrap()
                .insert(key_name.to_string(), value.expose_secret().to_string());
            Ok(())
        }

        fn get_local(&self, key_name: &str, path: &Path) -> anyhow::Result<Option<SecretString>> {
            if let Some(message) = *self.fail_get.read().unwrap() {
                return Err(anyhow!(message));
            }
            Ok(self.local_value(path, key_name).map(SecretString::from))
        }

        fn get_global(&self, key_name: &str) -> anyhow::Result<Option<SecretString>> {
            if let Some(message) = *self.fail_get.read().unwrap() {
                return Err(anyhow!(message));
            }
            Ok(self.global_value(key_name).map(SecretString::from))
        }

        fn delete_local(&self, key_name: &str, path: &Path) -> anyhow::Result<()> {
            if let Some(message) = *self.fail_delete.read().unwrap() {
                return Err(anyhow!(message));
            }
            self.local
                .write()
                .unwrap()
                .remove(&(path.to_path_buf(), key_name.to_string()));
            Ok(())
        }

        fn delete_global(&self, key_name: &str) -> anyhow::Result<()> {
            if let Some(message) = *self.fail_delete.read().unwrap() {
                return Err(anyhow!(message));
            }
            self.global.write().unwrap().remove(key_name);
            Ok(())
        }
    }

    fn api_key(secret: &str) -> ApiKey {
        ApiKey::from_parts(&Uuid::nil(), secret)
    }

    fn storage(cwd: &Path, mock: Arc<MockSecretStorage>) -> ApiKeyStorage {
        let storage =
            CredentialsStorage::from_secret_storage(CredentialBackend::File, Box::new(mock));

        ApiKeyStorage::new(Arc::new(storage), cwd)
    }

    #[test]
    fn should_store_api_key_locally_by_default() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, Arc::clone(&mock));
        let key = api_key("local-secret");

        storage.set(&key, false).unwrap();

        assert_eq!(
            mock.local_value(cwd, API_KEY_NAME).as_deref(),
            Some(key.expose())
        );
        assert!(mock.global_value(API_KEY_NAME).is_none());
    }

    #[test]
    fn should_store_api_key_globally_when_requested() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, Arc::clone(&mock));
        let key = api_key("global-secret");

        storage.set(&key, true).unwrap();

        assert_eq!(
            mock.global_value(API_KEY_NAME).as_deref(),
            Some(key.expose())
        );
        assert!(mock.local_value(cwd, API_KEY_NAME).is_none());
    }

    #[test]
    fn should_read_global_api_key_when_local_key_is_missing() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, Arc::clone(&mock));
        let key = api_key("global-secret");
        mock.set_global_raw(API_KEY_NAME, key.expose());

        let stored = storage.get().unwrap().unwrap();

        assert_eq!(stored.expose(), key.expose());
    }

    #[test]
    fn should_prefer_local_api_key_over_global() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, Arc::clone(&mock));
        let local = api_key("local-secret");
        let global = api_key("global-secret");
        mock.set_global_raw(API_KEY_NAME, global.expose());
        mock.set_local_raw(cwd, API_KEY_NAME, local.expose());

        let stored = storage.get().unwrap().unwrap();

        assert_eq!(stored.expose(), local.expose());
    }

    #[test]
    fn should_return_none_when_api_key_is_missing() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, mock);

        assert!(storage.get().unwrap().is_none());
    }

    #[test]
    fn should_ignore_stored_value_that_is_not_an_api_key() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, Arc::clone(&mock));
        mock.set_local_raw(cwd, API_KEY_NAME, "not-an-api-key");

        assert!(storage.get().unwrap().is_none());
    }

    #[test]
    fn should_delete_api_key_from_selected_scope_only() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, Arc::clone(&mock));
        let local = api_key("local-secret");
        let global = api_key("global-secret");
        storage.set(&local, false).unwrap();
        storage.set(&global, true).unwrap();

        storage.delete(false).unwrap();

        assert!(mock.local_value(cwd, API_KEY_NAME).is_none());
        assert_eq!(
            mock.global_value(API_KEY_NAME).as_deref(),
            Some(global.expose())
        );

        storage.delete(true).unwrap();

        assert!(mock.global_value(API_KEY_NAME).is_none());
    }

    #[test]
    fn should_propagate_storage_errors() {
        let cwd = Path::new("/repo");
        let mock = Arc::new(MockSecretStorage::default());
        let storage = storage(cwd, Arc::clone(&mock));
        let key = api_key("secret");

        mock.fail_get("read failed");
        assert!(
            storage
                .get()
                .unwrap_err()
                .to_string()
                .contains("read failed")
        );

        mock.fail_put("write failed");
        assert!(
            storage
                .set(&key, false)
                .unwrap_err()
                .to_string()
                .contains("write failed")
        );

        mock.fail_delete("delete failed");
        assert!(
            storage
                .delete(false)
                .unwrap_err()
                .to_string()
                .contains("delete failed")
        );
    }
}
