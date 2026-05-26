//! Resolving [`SecretRef`]s into secret values.
//!
//! Resolution order for a key is: the `.smista/secrets` file (TOML key/value),
//! then an environment variable of the same key name. Resolved values are wrapped
//! in [`secrecy::SecretString`] so they are not accidentally logged, and are
//! never persisted or echoed.

use std::collections::HashMap;
use std::path::Path;

use secrecy::SecretString;
use smista_core::secret::SecretRef;

/// Errors raised while resolving secrets.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The secrets file existed but could not be read.
    #[error("failed to read secrets file: {0}")]
    Io(#[from] std::io::Error),
    /// The secrets file was not valid TOML.
    #[error("invalid secrets file: {0}")]
    Parse(#[from] toml::de::Error),
    /// The referenced key was found in no source.
    #[error("unresolved secret reference: {0}")]
    Unresolved(String),
}

/// Resolves secrets referenced by name from a secrets file and the environment.
pub struct SecretResolver {
    file_values: HashMap<String, String>,
}

impl SecretResolver {
    /// Loads the resolver from a secrets file, if present.
    ///
    /// A missing file is not an error — resolution then falls back to the
    /// environment only.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Io`] if the file exists but cannot be read, or
    /// [`SecretError::Parse`] if it is not valid TOML.
    pub fn from_file(path: &Path) -> Result<Self, SecretError> {
        let file_values = match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(err) => return Err(SecretError::Io(err)),
        };
        Ok(Self { file_values })
    }

    /// Resolves `reference`, checking the secrets file first, then the
    /// environment variable of the same name.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Unresolved`] if no source provides the key.
    pub fn resolve(&self, reference: &SecretRef) -> Result<SecretString, SecretError> {
        if let Some(value) = self.file_values.get(reference.key()) {
            return Ok(SecretString::from(value.clone()));
        }
        if let Ok(value) = std::env::var(reference.key()) {
            return Ok(SecretString::from(value));
        }
        Err(SecretError::Unresolved(reference.key().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    #[test]
    fn should_treat_missing_file_as_empty() {
        let resolver = SecretResolver::from_file(Path::new("/no/such/file")).unwrap();
        assert!(resolver.file_values.is_empty());
    }

    #[test]
    fn should_resolve_from_file() {
        let dir = std::env::temp_dir().join(format!("smista-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets");
        std::fs::write(&path, "openai_api_key = \"sk-test\"\n").unwrap();

        let resolver = SecretResolver::from_file(&path).unwrap();
        let secret = resolver.resolve(&SecretRef::new("openai_api_key")).unwrap();
        assert_eq!(secret.expose_secret(), "sk-test");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn should_error_on_unresolved_reference() {
        let resolver = SecretResolver::from_file(Path::new("/no/such/file")).unwrap();
        let err = resolver
            .resolve(&SecretRef::new("definitely_absent_key_xyz"))
            .unwrap_err();
        assert!(matches!(err, SecretError::Unresolved(_)));
    }
}
