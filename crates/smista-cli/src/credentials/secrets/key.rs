//! Key names used by secret storage backends.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const GLOBAL_KEY_SCOPE: &str = "*";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    /// The key scope, either global or local to the current directory.
    pub scope: KeyScope,
    /// The key name, used to identify the key in the keychain or secrets file.
    pub name: String,
}

/// The key scope, either global or local to the current directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyScope {
    /// The key is stored in the global keychain or secrets file.
    Global,
    /// The key is stored in the local keychain or secrets file, in the current directory.
    Local(PathBuf),
}

impl Key {
    /// Creates a new key with the given scope and name.
    pub fn new(scope: KeyScope, name: String) -> Self {
        Self { scope, name }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.scope {
            KeyScope::Global => write!(f, "{}:{}", GLOBAL_KEY_SCOPE, self.name),
            KeyScope::Local(path) => write!(f, "{}:{}", path.to_string_lossy(), self.name),
        }
    }
}

impl Serialize for KeyScope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            KeyScope::Global => serializer.serialize_str(GLOBAL_KEY_SCOPE),
            KeyScope::Local(path) => serializer.serialize_str(&path.to_string_lossy()),
        }
    }
}

impl<'de> Deserialize<'de> for KeyScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == GLOBAL_KEY_SCOPE {
            Ok(KeyScope::Global)
        } else {
            Ok(KeyScope::Local(PathBuf::from(s)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn should_display_global_key_with_global_scope_marker() {
        let key = Key::new(KeyScope::Global, "openai".to_string());

        assert_eq!(key.to_string(), "*:openai");
    }

    #[test]
    fn should_display_local_key_with_path_scope() {
        let path = PathBuf::from("workspace");
        let key = Key::new(KeyScope::Local(path.clone()), "anthropic".to_string());

        assert_eq!(
            key.to_string(),
            format!("{}:anthropic", path.to_string_lossy())
        );
    }

    #[test]
    fn should_serialize_global_scope_as_marker() {
        let encoded = toml::to_string(&Key::new(KeyScope::Global, "gemini".to_string())).unwrap();

        assert!(encoded.contains("scope = \"*\""));
        assert!(encoded.contains("name = \"gemini\""));
    }

    #[test]
    fn should_serialize_local_scope_as_path() {
        let encoded = toml::to_string(&Key::new(
            KeyScope::Local(PathBuf::from("repo")),
            "openai".to_string(),
        ))
        .unwrap();

        assert!(encoded.contains("scope = \"repo\""));
        assert!(encoded.contains("name = \"openai\""));
    }

    #[test]
    fn should_deserialize_global_scope_marker() {
        let decoded: Key = toml::from_str("scope = \"*\"\nname = \"openai\"\n").unwrap();

        assert_eq!(decoded, Key::new(KeyScope::Global, "openai".to_string()));
    }

    #[test]
    fn should_deserialize_non_marker_scope_as_local_path() {
        let decoded: Key = toml::from_str("scope = \"repo\"\nname = \"anthropic\"\n").unwrap();

        assert_eq!(
            decoded,
            Key::new(
                KeyScope::Local(Path::new("repo").to_path_buf()),
                "anthropic".to_string()
            )
        );
    }
}
