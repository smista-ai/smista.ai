//! Resolving secret references into secret values.
//!
//! A configuration value referencing a secret uses the `${secret:NAME}`
//! interpolation form (see [`SecretRef`]). Resolution consults sources in the
//! order defined by the specification, from most to least precedence:
//!
//! 1. an environment variable named `NAME`;
//! 2. the `.smista/secrets` file, where the project file overrides the global
//!    `~/.smista/secrets` file.
//!
//! OS keychain and credential-helper sources are not yet implemented (deferred
//! to the authentication milestone). A value that is not a `${secret:NAME}`
//! reference is treated as an inline literal and used verbatim.
//!
//! Resolved values are wrapped in [`secrecy::SecretString`] so they are not
//! accidentally logged, and are never persisted or echoed.

use std::collections::HashMap;
use std::path::Path;

use secrecy::SecretString;
use smista_sdk::core::secret::SecretRef;

/// Errors raised while resolving secrets.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// A secrets file existed but could not be read.
    #[error("failed to read secrets file {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: String,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// A secrets file contained a line that is not `NAME=value`.
    #[error("invalid secrets file {path}: line {line} is not a NAME=value pair")]
    Parse {
        /// Path of the offending file.
        path: String,
        /// One-based line number of the offending line.
        line: usize,
    },
    /// The referenced key was found in no source.
    #[error("unresolved secret reference: {0}")]
    Unresolved(String),
}

/// Resolves secrets referenced by name from the environment and the
/// `.smista/secrets` files.
pub struct SecretResolver {
    /// Merged file values, with project entries overriding global ones.
    file_values: HashMap<String, String>,
}

impl SecretResolver {
    /// Builds a resolver from the global and project secrets files.
    ///
    /// Both files are optional: a missing file contributes nothing. When a key
    /// is present in both, the project file wins. Resolution still falls back to
    /// the environment for keys absent from every file.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Io`] if a file exists but cannot be read, or
    /// [`SecretError::Parse`] if a file contains a malformed line.
    pub fn from_paths(global: Option<&Path>, project: &Path) -> Result<Self, SecretError> {
        tracing::debug!(
            secrets.has_global = global.is_some(),
            secrets.project_path = %project.display(),
            "building secret resolver"
        );
        let mut file_values = HashMap::new();
        let mut file_count = 0usize;
        if let Some(global) = global {
            let values = read_dotenv(global)?;
            file_count += 1;
            tracing::trace!(
                secrets.path = %global.display(),
                secrets.key_count = values.len(),
                "loaded {{secrets.key_count}} keys from global secrets file"
            );
            file_values.extend(values);
        }
        let project_values = read_dotenv(project)?;
        file_count += 1;
        tracing::trace!(
            secrets.path = %project.display(),
            secrets.key_count = project_values.len(),
            "loaded {{secrets.key_count}} keys from project secrets file"
        );
        file_values.extend(project_values);
        tracing::debug!(
            secrets.file_count = file_count,
            secrets.key_count = file_values.len(),
            "secret resolver ready with {{secrets.key_count}} keys"
        );
        Ok(Self { file_values })
    }

    /// Builds a resolver directly from key/value pairs, for testing.
    #[cfg(test)]
    fn from_values(file_values: HashMap<String, String>) -> Self {
        Self { file_values }
    }

    /// Resolves `reference`, checking the environment variable of the same name
    /// first, then the merged secrets files.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Unresolved`] if no source provides the key.
    pub fn resolve(&self, reference: &SecretRef) -> Result<SecretString, SecretError> {
        tracing::trace!(secrets.key = %reference.key(), "resolving secret {{secrets.key}}");
        if let Ok(value) = std::env::var(reference.key()) {
            tracing::trace!(secrets.key = %reference.key(), "resolved {{secrets.key}} from environment");
            return Ok(SecretString::from(value));
        }
        if let Some(value) = self.file_values.get(reference.key()) {
            tracing::trace!(secrets.key = %reference.key(), "resolved {{secrets.key}} from secrets file");
            return Ok(SecretString::from(value.clone()));
        }
        tracing::error!(
            secrets.key = %reference.key(),
            "secret reference {{secrets.key}} could not be resolved from any source"
        );
        Err(SecretError::Unresolved(reference.key().to_string()))
    }

    /// Resolves a raw configuration value.
    ///
    /// When `raw` is a `${secret:NAME}` reference it is resolved through
    /// [`resolve`](Self::resolve); otherwise it is treated as an inline literal
    /// and returned verbatim.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Unresolved`] when a reference cannot be resolved.
    pub fn resolve_value(&self, raw: &str) -> Result<SecretString, SecretError> {
        match SecretRef::parse(raw) {
            Some(reference) => {
                tracing::trace!(
                    secrets.key = %reference.key(),
                    "config value is a secret reference to {{secrets.key}}"
                );
                self.resolve(&reference)
            }
            None => {
                // The raw value is an inline literal and is never logged.
                tracing::trace!("config value is an inline literal; using verbatim");
                Ok(SecretString::from(raw.to_string()))
            }
        }
    }
}

/// Reads and parses a dotenv-style secrets file, returning an empty map when the
/// file is absent.
fn read_dotenv(path: &Path) -> Result<HashMap<String, String>, SecretError> {
    tracing::trace!(secrets.path = %path.display(), "reading secrets file {{secrets.path}}");
    match std::fs::read_to_string(path) {
        Ok(contents) => parse_dotenv(&contents, path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(
                secrets.path = %path.display(),
                "secrets file {{secrets.path}} is absent; contributing no keys"
            );
            Ok(HashMap::new())
        }
        Err(source) => {
            tracing::error!(
                secrets.path = %path.display(),
                error.message = %source,
                "failed to read secrets file {{secrets.path}}"
            );
            Err(SecretError::Io {
                path: path.display().to_string(),
                source,
            })
        }
    }
}

/// Parses dotenv-style `NAME=value` lines.
///
/// Blank lines and lines whose first non-whitespace character is `#` are
/// ignored. Values are taken verbatim after the first `=` (no quoting). A
/// non-blank, non-comment line without `=` is a [`SecretError::Parse`].
fn parse_dotenv(contents: &str, path: &Path) -> Result<HashMap<String, String>, SecretError> {
    tracing::trace!(
        secrets.path = %path.display(),
        secrets.line_count = contents.lines().count(),
        "parsing secrets file {{secrets.path}} ({{secrets.line_count}} lines)"
    );
    let mut values = HashMap::new();
    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            // Only the path and line number are logged; the line content (which
            // may contain a secret) is never echoed.
            tracing::error!(
                secrets.path = %path.display(),
                secrets.line = index + 1,
                "malformed secrets line {{secrets.line}} in {{secrets.path}}; expected NAME=value"
            );
            return Err(SecretError::Parse {
                path: path.display().to_string(),
                line: index + 1,
            });
        };
        values.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "smista-secrets-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn should_parse_dotenv_skipping_comments_and_blanks() {
        let contents = "# comment\n\nopenai=sk-test\n  anthropic = sk-ant  \n";
        let values = parse_dotenv(contents, Path::new("secrets")).unwrap();
        assert_eq!(values.get("openai").map(String::as_str), Some("sk-test"));
        assert_eq!(values.get("anthropic").map(String::as_str), Some("sk-ant"));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn should_error_on_malformed_dotenv_line() {
        let err = parse_dotenv("openai sk-test\n", Path::new("secrets")).unwrap_err();
        assert!(matches!(err, SecretError::Parse { line: 1, .. }));
    }

    #[test]
    fn should_treat_missing_files_as_empty() {
        let resolver = SecretResolver::from_paths(None, Path::new("/no/such/secrets")).unwrap();
        assert!(resolver.file_values.is_empty());
    }

    #[test]
    fn should_resolve_a_reference_from_the_secrets_file() {
        let dir = temp_dir("file");
        let path = dir.join("secrets");
        std::fs::write(&path, "openai=sk-from-file\n").unwrap();

        let resolver = SecretResolver::from_paths(None, &path).unwrap();
        let secret = resolver.resolve(&SecretRef::new("openai")).unwrap();
        assert_eq!(secret.expose_secret(), "sk-from-file");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn should_let_project_secrets_override_global() {
        let dir = temp_dir("override");
        let global = dir.join("global-secrets");
        let project = dir.join("project-secrets");
        std::fs::write(&global, "openai=global-value\n").unwrap();
        std::fs::write(&project, "openai=project-value\n").unwrap();

        let resolver = SecretResolver::from_paths(Some(&global), &project).unwrap();
        let secret = resolver.resolve(&SecretRef::new("openai")).unwrap();
        assert_eq!(secret.expose_secret(), "project-value");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn should_prefer_environment_over_files() {
        // Use a unique key so the environment variable cannot collide with other
        // tests running in parallel.
        let key = "SMISTA_TEST_SECRET_ENV_FIRST";
        // SAFETY: the key is unique to this test; no other thread reads it.
        unsafe {
            std::env::set_var(key, "from-env");
        }

        let resolver = SecretResolver::from_values(HashMap::from([(
            key.to_string(),
            "from-file".to_string(),
        )]));
        let secret = resolver.resolve(&SecretRef::new(key)).unwrap();
        assert_eq!(secret.expose_secret(), "from-env");

        // SAFETY: paired with the set_var above.
        unsafe {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn should_pass_through_an_inline_literal_value() {
        let resolver = SecretResolver::from_values(HashMap::new());
        let secret = resolver.resolve_value("sk-inline-literal").unwrap();
        assert_eq!(secret.expose_secret(), "sk-inline-literal");
    }

    #[test]
    fn should_resolve_a_reference_through_resolve_value() {
        let resolver = SecretResolver::from_values(HashMap::from([(
            "openai".to_string(),
            "sk-resolved".to_string(),
        )]));
        let secret = resolver.resolve_value("${secret:openai}").unwrap();
        assert_eq!(secret.expose_secret(), "sk-resolved");
    }

    #[test]
    fn should_error_on_unresolved_reference() {
        let resolver = SecretResolver::from_values(HashMap::new());
        let err = resolver
            .resolve(&SecretRef::new("definitely_absent_key_xyz"))
            .unwrap_err();
        assert!(matches!(err, SecretError::Unresolved(_)));
    }
}
