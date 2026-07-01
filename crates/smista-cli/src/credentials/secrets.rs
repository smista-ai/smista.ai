mod file;
mod key;
mod keyring;

use std::path::Path;

use secrecy::SecretString;

pub use self::file::FileSecretStorage;
pub use self::keyring::KeyringSecretStorage;

/// Stores provider secrets for the CLI.
///
/// Implementations can choose a platform keyring or a local file, but they must
/// expose the same split between global secrets and project-local secrets.
/// `key_name` identifies the provider credential, while `path` identifies the
/// project root used by local operations.
pub trait SecretStorage {
    /// Stores or replaces a project-local secret for `key_name`.
    fn put_local(&self, key_name: &str, path: &Path, value: &SecretString) -> anyhow::Result<()>;

    /// Stores or replaces a global secret for `key_name`.
    fn put_global(&self, key_name: &str, value: &SecretString) -> anyhow::Result<()>;

    /// Reads a project-local secret for `key_name`.
    fn get_local(&self, key_name: &str, path: &Path) -> anyhow::Result<Option<SecretString>>;

    /// Reads a global secret for `key_name`.
    fn get_global(&self, key_name: &str) -> anyhow::Result<Option<SecretString>>;

    /// Removes a project-local secret for `key_name`.
    fn delete_local(&self, key_name: &str, path: &Path) -> anyhow::Result<()>;

    /// Removes a global secret for `key_name`.
    fn delete_global(&self, key_name: &str) -> anyhow::Result<()>;
}
