//! File-backed secret storage for environments without a platform keyring.
//!
//! The storage keeps global secrets in the global configuration directory and
//! project-local secrets in `<cwd>/.smista/secrets`. Each file is a TOML
//! document containing only provider secret values by key name. On Unix
//! platforms, every secrets file must have mode `0600`; a file with broader or
//! narrower permissions is rejected instead of being silently corrected.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret as _, SecretString};

use crate::credentials::secrets::SecretStorage;

/// Permission mode required for secrets files on Unix platforms.
#[cfg(unix)]
const SECRET_FILE_MODE: u32 = 0o600;

/// File-backed implementation of [`SecretStorage`].
///
/// This backend is intentionally deterministic and local: it never calls a
/// platform credential manager, and it resolves paths through
/// `smista-core`'s path primitives. It is suitable for systems where the
/// platform keyring is unavailable, while still enforcing Unix file permissions
/// before any secret is read or written.
#[derive(Debug)]
pub struct FileSecretStorage {
    /// File used by global secret operations.
    global_path: PathBuf,
}

impl FileSecretStorage {
    /// Creates a file-backed secret storage and ensures the global secrets file
    /// exists with secure permissions.
    ///
    /// Local secrets files are created lazily when a local operation is
    /// performed because their path depends on the caller-provided project
    /// directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the global secrets path cannot be resolved, the file
    /// cannot be created, or an existing Unix secrets file does not have mode
    /// `0600`.
    pub fn new() -> anyhow::Result<Self> {
        let global_path = crate::config::paths::global_secrets_file()
            .ok_or_else(|| anyhow::anyhow!("Could not determine the global secrets file path."))?;
        let storage = Self { global_path };
        storage.ensure_file(&storage.global_path)?;

        Ok(storage)
    }

    /// Creates a storage value with a caller-provided global path.
    ///
    /// This constructor is test-only so tests can exercise global behavior
    /// without creating or modifying the invoking user's real secrets file.
    #[cfg(test)]
    fn with_global_path(global_path: PathBuf) -> anyhow::Result<Self> {
        let storage = Self { global_path };
        storage.ensure_file(&storage.global_path)?;

        Ok(storage)
    }

    /// Builds the project-local secrets file path from the shared core path
    /// helpers.
    fn local_path(&self, path: &Path) -> PathBuf {
        crate::config::paths::secrets_file(path)
    }

    /// Ensures a secrets file exists and has secure permissions.
    ///
    /// On Unix, a newly-created file is assigned `0600`, and an existing file is
    /// rejected unless it already has exactly that mode. On Windows, permission
    /// validation is skipped because POSIX mode bits do not represent the
    /// effective access-control model.
    fn ensure_file(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !path.exists() {
            self.create_file(path)?;
        }

        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            anyhow::bail!("Secrets path is not a regular file: {}", path.display());
        }

        self.ensure_permissions(path)
    }

    /// Creates an empty secrets file using secure permissions when the platform
    /// exposes POSIX file modes.
    fn create_file(&self, path: &Path) -> anyhow::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(SECRET_FILE_MODE)
                .open(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(SECRET_FILE_MODE))?;
        }

        #[cfg(not(unix))]
        {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
        }

        Ok(())
    }

    /// Verifies that the secrets file permissions are acceptable for the
    /// current platform.
    fn ensure_permissions(&self, path: &Path) -> anyhow::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            let mode = fs::metadata(path)?.permissions().mode() & 0o777;
            if mode != SECRET_FILE_MODE {
                anyhow::bail!(
                    "Secrets file {} must have permissions 600, found {:03o}.",
                    path.display(),
                    mode
                );
            }
        }

        #[cfg(not(unix))]
        {
            let _ = path;
        }

        Ok(())
    }

    /// Reads all entries from the secrets file, creating the file first when it
    /// does not exist.
    fn read_entries(&self, path: &Path) -> anyhow::Result<BTreeMap<String, SecretString>> {
        self.ensure_file(path)?;

        let contents = fs::read_to_string(path)?;
        if contents.trim().is_empty() {
            return Ok(BTreeMap::new());
        }

        let entries: BTreeMap<String, String> = toml::from_str(&contents).map_err(|err| {
            anyhow::anyhow!("Could not parse secrets file {}: {err}", path.display())
        })?;

        Ok(entries
            .into_iter()
            .map(|(key, value)| (key, SecretString::from(value)))
            .collect())
    }

    /// Writes all entries to a secrets file that has already passed permission
    /// validation.
    fn write_entries(
        &self,
        path: &Path,
        entries: &BTreeMap<String, SecretString>,
    ) -> anyhow::Result<()> {
        self.ensure_file(path)?;

        let exposed_entries: BTreeMap<&String, &str> = entries
            .iter()
            .map(|(key, value)| (key, value.expose_secret()))
            .collect();
        let encoded = toml::to_string(&exposed_entries)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)?;
        file.write_all(encoded.as_bytes())?;
        file.sync_all()?;

        self.ensure_permissions(path)
    }

    /// Inserts or replaces one secret in the selected secrets file.
    fn put(&self, path: &Path, key_name: &str, value: &SecretString) -> anyhow::Result<()> {
        let mut entries = self.read_entries(path)?;
        entries.insert(
            key_name.to_string(),
            SecretString::from(value.expose_secret().to_string()),
        );
        self.write_entries(path, &entries)
    }

    /// Reads one secret from the selected secrets file.
    fn get(&self, path: &Path, key_name: &str) -> anyhow::Result<Option<SecretString>> {
        let entries = self.read_entries(path)?;

        Ok(entries.get(key_name).cloned())
    }

    /// Removes one secret from the selected secrets file.
    fn delete(&self, path: &Path, key_name: &str) -> anyhow::Result<()> {
        let mut entries = self.read_entries(path)?;
        entries.remove(key_name);
        self.write_entries(path, &entries)
    }
}

impl SecretStorage for FileSecretStorage {
    fn put_local(&self, key_name: &str, path: &Path, value: &SecretString) -> anyhow::Result<()> {
        tracing::debug!(
            key_name,
            path = %path.display(),
            "Storing local secret in file secret storage."
        );
        self.put(&self.local_path(path), key_name, value)
    }

    fn put_global(&self, key_name: &str, value: &SecretString) -> anyhow::Result<()> {
        tracing::debug!(key_name, "Storing global secret in file secret storage.");
        self.put(&self.global_path, key_name, value)
    }

    fn get_local(&self, key_name: &str, path: &Path) -> anyhow::Result<Option<SecretString>> {
        tracing::debug!(
            key_name,
            path = %path.display(),
            "Reading local secret from file secret storage."
        );
        self.get(&self.local_path(path), key_name)
    }

    fn get_global(&self, key_name: &str) -> anyhow::Result<Option<SecretString>> {
        tracing::debug!(key_name, "Reading global secret from file secret storage.");
        self.get(&self.global_path, key_name)
    }

    fn delete_local(&self, key_name: &str, path: &Path) -> anyhow::Result<()> {
        tracing::debug!(
            key_name,
            path = %path.display(),
            "Deleting local secret from file secret storage."
        );
        self.delete(&self.local_path(path), key_name)
    }

    fn delete_global(&self, key_name: &str) -> anyhow::Result<()> {
        tracing::debug!(key_name, "Deleting global secret from file secret storage.");
        self.delete(&self.global_path, key_name)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    use secrecy::ExposeSecret as _;
    use tempfile::TempDir;

    use super::*;

    /// Creates a storage value without touching the real global secrets file.
    fn storage() -> FileSecretStorage {
        FileSecretStorage {
            global_path: PathBuf::from("unused-test-global-secrets"),
        }
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn should_create_missing_file_before_reading() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("secrets");

        let entries = storage().read_entries(&file).unwrap();

        assert!(entries.is_empty());
        assert!(file.is_file());
        #[cfg(unix)]
        assert_eq!(mode(&file), SECRET_FILE_MODE);
    }

    #[test]
    fn should_put_and_get_secret_from_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("secrets");
        let value = SecretString::from("test-secret".to_string());

        storage().put(&file, "openai", &value).unwrap();
        let stored = storage().get(&file, "openai").unwrap().unwrap();

        assert_eq!(stored.expose_secret(), "test-secret");
    }

    #[test]
    fn should_preserve_other_entries_when_updating_secret() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("secrets");

        storage()
            .put(&file, "openai", &SecretString::from("old".to_string()))
            .unwrap();
        storage()
            .put(&file, "anthropic", &SecretString::from("kept".to_string()))
            .unwrap();
        storage()
            .put(&file, "openai", &SecretString::from("new".to_string()))
            .unwrap();

        assert_eq!(
            storage()
                .get(&file, "openai")
                .unwrap()
                .unwrap()
                .expose_secret(),
            "new"
        );
        assert_eq!(
            storage()
                .get(&file, "anthropic")
                .unwrap()
                .unwrap()
                .expose_secret(),
            "kept"
        );
    }

    #[test]
    fn should_delete_existing_secret_and_keep_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("secrets");

        storage()
            .put(&file, "openai", &SecretString::from("deleted".to_string()))
            .unwrap();
        storage().delete(&file, "openai").unwrap();

        assert!(storage().get(&file, "openai").unwrap().is_none());
        assert!(file.is_file());
    }

    #[test]
    fn should_ignore_missing_secret_delete() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("secrets");

        storage().delete(&file, "missing").unwrap();

        assert!(storage().get(&file, "missing").unwrap().is_none());
    }

    #[test]
    fn should_use_project_secrets_file_for_local_operations() {
        let dir = TempDir::new().unwrap();
        let value = SecretString::from("local-secret".to_string());
        let expected_file = crate::config::paths::secrets_file(dir.path());

        storage().put_local("openai", dir.path(), &value).unwrap();
        let stored = storage().get_local("openai", dir.path()).unwrap().unwrap();

        assert_eq!(stored.expose_secret(), "local-secret");
        assert!(expected_file.is_file());
    }

    #[test]
    fn should_use_configured_secrets_file_for_global_operations() {
        let dir = TempDir::new().unwrap();
        let global_file = dir.path().join(".smista").join("secrets");
        let storage = FileSecretStorage::with_global_path(global_file.clone()).unwrap();

        storage
            .put_global("openai", &SecretString::from("global-secret".to_string()))
            .unwrap();
        let stored = storage.get_global("openai").unwrap().unwrap();
        storage.delete_global("openai").unwrap();

        assert_eq!(stored.expose_secret(), "global-secret");
        assert!(storage.get_global("openai").unwrap().is_none());
        assert!(global_file.is_file());
        #[cfg(unix)]
        assert_eq!(mode(&global_file), SECRET_FILE_MODE);
    }

    #[test]
    fn should_reject_directory_secrets_path() {
        let dir = TempDir::new().unwrap();

        let err = storage().read_entries(dir.path()).unwrap_err();

        assert!(
            err.to_string().contains("not a regular file"),
            "unexpected error: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn should_reject_existing_file_with_non_secret_permissions() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("secrets");
        fs::write(&file, "").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();

        let err = storage().read_entries(&file).unwrap_err();

        assert!(
            err.to_string().contains("must have permissions 600"),
            "unexpected error: {err}"
        );
    }
}
