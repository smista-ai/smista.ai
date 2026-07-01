use std::sync::Once;

use keyring_core::Entry;
use secrecy::{ExposeSecret, SecretString};

use crate::credentials::secrets::SecretStorage;
use crate::credentials::secrets::key::{Key, KeyScope};

const SERVICE_NAME: &str = "smista.ai";

static INIT_STORE: Once = Once::new();

pub struct KeyringSecretStorage {
    // prevents the keyring from being constructed without calling `new()`, which ensures that the default store is set up.
    _init: bool,
}

impl KeyringSecretStorage {
    pub fn new() -> anyhow::Result<Self> {
        Self::ensure_default_storage();

        tracing::debug!("Creating keyring secret storage");

        let storage = Self { _init: true };

        if !storage.is_available() {
            anyhow::bail!(
                "Keyring secret storage is not available on this platform or the keyring is locked."
            );
        }
        tracing::debug!("Keyring secret storage created successfully");

        Ok(storage)
    }

    fn ensure_default_storage() {
        INIT_STORE.call_once(|| {
            #[cfg(target_os = "macos")]
            if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
                keyring_core::set_default_store(store);
            }
            #[cfg(target_os = "windows")]
            if let Ok(store) = windows_native_keyring_store::Store::new() {
                keyring_core::set_default_store(store);
            }
            #[cfg(target_os = "linux")]
            if let Ok(store) = zbus_secret_service_keyring_store::Store::new() {
                keyring_core::set_default_store(store);
            }
        });
    }

    fn is_available(&self) -> bool {
        let Ok(entry) = Entry::new(SERVICE_NAME, "keyring-available") else {
            return false;
        };

        Self::is_available_result(entry.get_password())
    }

    fn put(&self, key: Key, value: &SecretString) -> anyhow::Result<()> {
        let entry = Entry::new(SERVICE_NAME, &key.to_string())?;
        entry.set_password(value.expose_secret())?;

        Ok(())
    }

    fn get(&self, key: Key) -> anyhow::Result<Option<SecretString>> {
        let entry = Entry::new(SERVICE_NAME, &key.to_string())?;

        Self::secret_from_get_result(entry.get_password())
    }

    fn delete(&self, key: Key) -> anyhow::Result<()> {
        let entry = Entry::new(SERVICE_NAME, &key.to_string())?;

        Self::delete_result(entry.delete_credential())
    }

    fn is_available_result(result: keyring_core::Result<String>) -> bool {
        match result {
            Ok(_) => true,
            Err(keyring_core::Error::NoStorageAccess(_))
            | Err(keyring_core::Error::PlatformFailure(_)) => false,
            Err(_) => true,
        }
    }

    fn secret_from_get_result(
        result: keyring_core::Result<String>,
    ) -> anyhow::Result<Option<SecretString>> {
        match result {
            Ok(password) => Ok(Some(SecretString::from(password))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }

    fn delete_result(result: keyring_core::Result<()>) -> anyhow::Result<()> {
        match result {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }
}

impl SecretStorage for KeyringSecretStorage {
    fn put_local(
        &self,
        key_name: &str,
        path: &std::path::Path,
        value: &SecretString,
    ) -> anyhow::Result<()> {
        self.put(
            Key::new(KeyScope::Local(path.to_path_buf()), key_name.to_string()),
            value,
        )
    }

    fn put_global(&self, key_name: &str, value: &SecretString) -> anyhow::Result<()> {
        self.put(Key::new(KeyScope::Global, key_name.to_string()), value)
    }

    fn get_local(
        &self,
        key_name: &str,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<SecretString>> {
        self.get(Key::new(
            KeyScope::Local(path.to_path_buf()),
            key_name.to_string(),
        ))
    }

    fn get_global(&self, key_name: &str) -> anyhow::Result<Option<SecretString>> {
        self.get(Key::new(KeyScope::Global, key_name.to_string()))
    }

    fn delete_local(&self, key_name: &str, path: &std::path::Path) -> anyhow::Result<()> {
        self.delete(Key::new(
            KeyScope::Local(path.to_path_buf()),
            key_name.to_string(),
        ))
    }

    fn delete_global(&self, key_name: &str) -> anyhow::Result<()> {
        self.delete(Key::new(KeyScope::Global, key_name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret as _;

    use super::*;

    fn platform_error() -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(std::io::Error::other("platform failure"))
    }

    #[test]
    fn should_report_keyring_available_when_probe_succeeds() {
        assert!(KeyringSecretStorage::is_available_result(Ok(
            "probe".to_string()
        )));
    }

    #[test]
    fn should_report_keyring_unavailable_when_storage_access_fails() {
        assert!(!KeyringSecretStorage::is_available_result(Err(
            keyring_core::Error::NoStorageAccess(platform_error())
        )));
    }

    #[test]
    fn should_report_keyring_unavailable_when_platform_fails() {
        assert!(!KeyringSecretStorage::is_available_result(Err(
            keyring_core::Error::PlatformFailure(platform_error())
        )));
    }

    #[test]
    fn should_treat_non_access_probe_errors_as_available() {
        assert!(KeyringSecretStorage::is_available_result(Err(
            keyring_core::Error::NoEntry
        )));
        assert!(KeyringSecretStorage::is_available_result(Err(
            keyring_core::Error::NoDefaultStore
        )));
    }

    #[test]
    fn should_convert_get_success_to_secret() {
        let secret = KeyringSecretStorage::secret_from_get_result(Ok("stored".to_string()))
            .unwrap()
            .unwrap();

        assert_eq!(secret.expose_secret(), "stored");
    }

    #[test]
    fn should_convert_missing_get_entry_to_none() {
        let secret =
            KeyringSecretStorage::secret_from_get_result(Err(keyring_core::Error::NoEntry))
                .unwrap();

        assert!(secret.is_none());
    }

    #[test]
    fn should_return_get_errors() {
        let err = KeyringSecretStorage::secret_from_get_result(Err(keyring_core::Error::Invalid(
            "user".to_string(),
            "empty".to_string(),
        )))
        .unwrap_err();

        assert!(err.to_string().contains("user"), "unexpected error: {err}");
    }

    #[test]
    fn should_ignore_missing_entry_on_delete() {
        KeyringSecretStorage::delete_result(Err(keyring_core::Error::NoEntry)).unwrap();
    }

    #[test]
    fn should_return_delete_errors() {
        let err = KeyringSecretStorage::delete_result(Err(keyring_core::Error::Invalid(
            "service".to_string(),
            "empty".to_string(),
        )))
        .unwrap_err();

        assert!(
            err.to_string().contains("service"),
            "unexpected error: {err}"
        );
    }
}
