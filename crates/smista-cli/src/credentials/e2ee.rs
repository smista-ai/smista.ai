//! Client-side end-to-end encryption key storage.
//!
//! [`E2eeKeysCredentials`] owns the CLI-side key lifecycle for encrypted
//! sessions. It generates one local XChaCha20-Poly1305 key per session, stores
//! only the encoded key material in the configured credential backend, and
//! returns sealed [`EncryptedPayload`] envelopes for router storage.
//!
//! Raw keys and decrypted payloads must stay local to the CLI. This module only
//! logs public key identifiers and validation state; it never logs key material,
//! nonce bytes, ciphertext bytes, or plaintext.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use chacha20poly1305::aead::{Aead as _, Generate as _, KeyInit as _};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use secrecy::{ExposeSecret, SecretBox, SecretString};
use sha2::{Digest as _, Sha256};
use smista_sdk::core::api::EncryptedPayload;

use crate::credentials::CredentialsStorage;

const SECRET_KEY_ENCODING: base64::engine::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

const ENCRYPTED_PAYLOAD_V1: u8 = 1;
const KEY_ID_PREFIX: &str = "kf_";
const X_CHA_CHA_20_POLY_1305: &str = "xchacha20poly1305";

/// CLI-side storage and crypto helper for per-session E2EE keys.
///
/// Keys are scoped to the `cwd` supplied at construction time and are stored in
/// project-local credential storage. The public `key_id` is the local lookup
/// handle used in [`EncryptedPayload`] envelopes; the raw key never leaves this
/// storage wrapper.
#[derive(Clone, Debug)]
pub struct E2eeKeysCredentials {
    cwd: PathBuf,
    storage: Arc<CredentialsStorage>,
}

impl E2eeKeysCredentials {
    /// Creates E2EE key credentials bound to `cwd`.
    #[must_use]
    pub fn new(storage: Arc<CredentialsStorage>, cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            storage,
        }
    }

    /// Generates and stores a fresh local encryption key.
    ///
    /// The returned value is the public key identifier, derived as a stable
    /// fingerprint of the generated key, that can be sent to the router during
    /// encrypted session creation.
    ///
    /// # Errors
    ///
    /// Returns an error if secure random key generation fails or if the
    /// credential backend cannot persist the key.
    pub fn create_key(&self) -> anyhow::Result<String> {
        let secret_key = self.generate_secret_key()?;
        let key_name = Self::key_id_for_secret_key(&secret_key)?;
        tracing::debug!("Creating new E2EE key: {key_name}");
        self.storage.put_local(&self.cwd, &key_name, &secret_key)?;

        tracing::debug!("Created new E2EE key: {key_name}");
        Ok(key_name)
    }

    /// Deletes a locally stored E2EE key.
    ///
    /// Missing keys are treated as a successful no-op by the credential
    /// backend. The key identifier is public, but the removed key material is
    /// never read or logged.
    ///
    /// # Errors
    ///
    /// Returns an error if the credential backend cannot update local storage.
    pub fn delete_key(&self, key_id: &str) -> anyhow::Result<()> {
        tracing::debug!("Deleting E2EE key: {key_id}");
        self.storage.delete_local(&self.cwd, key_id)
    }

    /// Encrypts plaintext with the key identified by `key_id`.
    ///
    /// The returned envelope uses version 1, `xchacha20poly1305`, a fresh
    /// 24-byte nonce, and base64-url-no-padding encoding for nonce and
    /// ciphertext.
    ///
    /// # Errors
    ///
    /// Returns an error if `key_id` is missing, if the stored key cannot be
    /// decoded into a 32-byte XChaCha20-Poly1305 key, or if encryption fails.
    pub fn encrypt_payload(
        &self,
        key_id: &str,
        plaintext: &str,
    ) -> anyhow::Result<EncryptedPayload> {
        let secret_key = self.get_secret_key(key_id)?;
        let cipher = Self::cipher_from_secret_key(&secret_key)?;
        let nonce = XNonce::generate();
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("Could not encrypt E2EE payload."))?;

        Ok(EncryptedPayload {
            version: ENCRYPTED_PAYLOAD_V1,
            algorithm: X_CHA_CHA_20_POLY_1305.to_string(),
            key_id: key_id.to_string(),
            nonce: SECRET_KEY_ENCODING.encode(nonce),
            ciphertext: SECRET_KEY_ENCODING.encode(ciphertext),
        })
    }

    /// Decrypts an encrypted payload using the key named by its envelope.
    ///
    /// # Errors
    ///
    /// Returns an error when the envelope version or algorithm is unsupported,
    /// the referenced key is missing or invalid, nonce/ciphertext encoding is
    /// malformed, authentication fails, or the decrypted bytes are not UTF-8.
    pub fn decrypt_payload(&self, payload: &EncryptedPayload) -> anyhow::Result<String> {
        match payload.version {
            ENCRYPTED_PAYLOAD_V1 => self.decrypt_payload_v1(payload),
            _ => anyhow::bail!(
                "Unsupported encrypted payload version: {version}",
                version = payload.version
            ),
        }
    }

    fn decrypt_payload_v1(&self, payload: &EncryptedPayload) -> anyhow::Result<String> {
        match payload.algorithm.as_str() {
            X_CHA_CHA_20_POLY_1305 => self.decrypt_payload_v1_xchacha20poly1305(
                &payload.key_id,
                &payload.nonce,
                &payload.ciphertext,
            ),
            _ => anyhow::bail!(
                "Unsupported encrypted payload algorithm: {algorithm}",
                algorithm = payload.algorithm
            ),
        }
    }

    fn decrypt_payload_v1_xchacha20poly1305(
        &self,
        key_id: &str,
        nonce: &str,
        ciphertext: &str,
    ) -> anyhow::Result<String> {
        let secret_key = self.get_secret_key(key_id)?;
        let cipher = Self::cipher_from_secret_key(&secret_key)?;
        let nonce = Self::decode_nonce(nonce)?;
        let ciphertext = SECRET_KEY_ENCODING
            .decode(ciphertext)
            .map_err(|_| anyhow::anyhow!("Encrypted payload ciphertext is not valid base64."))?;
        let plaintext = cipher
            .decrypt(&nonce, ciphertext.as_ref())
            .map_err(|_| anyhow::anyhow!("Could not decrypt E2EE payload."))?;

        String::from_utf8(plaintext)
            .map_err(|_| anyhow::anyhow!("Decrypted E2EE payload is not valid UTF-8."))
    }

    fn get_secret_key(&self, key_id: &str) -> anyhow::Result<SecretBox<[u8]>> {
        let Some(base64_key) = self.storage.get(&self.cwd, key_id)? else {
            anyhow::bail!("E2EE key not found: {key_id}");
        };
        let key_bytes = SECRET_KEY_ENCODING
            .decode(base64_key.expose_secret())
            .map(SecretBox::from)
            .map_err(|_| anyhow::anyhow!("Stored E2EE key is not valid base64."))?;

        tracing::debug!("Retrieved valid E2EE key: {key_id}");

        Ok(key_bytes)
    }

    /// Generate a new secret key for E2EE encryption.
    ///
    /// The key is base64 encoded.
    fn generate_secret_key(&self) -> anyhow::Result<SecretString> {
        let key = Key::try_generate()?;
        let encoded_key = SECRET_KEY_ENCODING.encode(key);

        Ok(SecretString::from(encoded_key))
    }

    fn key_id_for_secret_key(secret_key: &SecretString) -> anyhow::Result<String> {
        let key = SECRET_KEY_ENCODING
            .decode(secret_key.expose_secret())
            .map_err(|_| anyhow::anyhow!("Generated E2EE key is not valid base64."))?;
        let fingerprint = Sha256::digest(&key);

        Ok(format!(
            "{KEY_ID_PREFIX}{fingerprint}",
            fingerprint = SECRET_KEY_ENCODING.encode(fingerprint)
        ))
    }

    fn cipher_from_secret_key(secret_key: &SecretBox<[u8]>) -> anyhow::Result<XChaCha20Poly1305> {
        XChaCha20Poly1305::new_from_slice(secret_key.expose_secret())
            .map_err(|_| anyhow::anyhow!("Stored E2EE key has an invalid length."))
    }

    fn decode_nonce(nonce: &str) -> anyhow::Result<XNonce> {
        let nonce = SECRET_KEY_ENCODING
            .decode(nonce)
            .map_err(|_| anyhow::anyhow!("Encrypted payload nonce is not valid base64."))?;

        XNonce::try_from(nonce.as_slice())
            .map_err(|_| anyhow::anyhow!("Encrypted payload nonce has an invalid length."))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::RwLock;

    use chacha20poly1305::aead::{Aead as _, Generate as _};
    use secrecy::ExposeSecret as _;

    use super::*;
    use crate::credentials::secrets::SecretStorage;
    use crate::credentials::{CredentialBackend, CredentialsStorage};

    #[derive(Debug, Default)]
    struct MockSecretStorage {
        local: RwLock<BTreeMap<(PathBuf, String), String>>,
    }

    impl MockSecretStorage {
        fn local_value(&self, path: &Path, key_name: &str) -> Option<String> {
            self.local
                .read()
                .unwrap()
                .get(&(path.to_path_buf(), key_name.to_string()))
                .cloned()
        }

        fn set_local(&self, path: &Path, key_name: &str, value: &str) {
            self.local.write().unwrap().insert(
                (path.to_path_buf(), key_name.to_string()),
                value.to_string(),
            );
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
            let _ = (key_name, value);
            Ok(())
        }

        fn get_local(&self, key_name: &str, path: &Path) -> anyhow::Result<Option<SecretString>> {
            Ok(self.local_value(path, key_name).map(SecretString::from))
        }

        fn get_global(&self, key_name: &str) -> anyhow::Result<Option<SecretString>> {
            let _ = key_name;
            Ok(None)
        }

        fn delete_local(&self, key_name: &str, path: &Path) -> anyhow::Result<()> {
            self.local
                .write()
                .unwrap()
                .remove(&(path.to_path_buf(), key_name.to_string()));
            Ok(())
        }

        fn delete_global(&self, key_name: &str) -> anyhow::Result<()> {
            let _ = key_name;
            Ok(())
        }
    }

    fn e2ee_credentials(cwd: &Path) -> (E2eeKeysCredentials, Arc<MockSecretStorage>) {
        let mock = Arc::new(MockSecretStorage::default());
        let storage = CredentialsStorage::from_secret_storage(
            CredentialBackend::File,
            Box::new(Arc::clone(&mock)),
        );

        (E2eeKeysCredentials::new(Arc::new(storage), cwd), mock)
    }

    fn assert_error_contains<T>(result: anyhow::Result<T>, expected: &str) {
        match result {
            Ok(_) => panic!("expected error containing {expected:?}"),
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains(expected),
                    "expected error to contain {expected:?}, got {message:?}"
                );
            }
        }
    }

    fn envelope_with_invalid_utf8_plaintext(
        credentials: &E2eeKeysCredentials,
        key_id: &str,
    ) -> EncryptedPayload {
        let secret_key = credentials.get_secret_key(key_id).unwrap();
        let cipher = E2eeKeysCredentials::cipher_from_secret_key(&secret_key).unwrap();
        let nonce = XNonce::generate();
        let invalid_utf8 = [0xff];
        let ciphertext = cipher.encrypt(&nonce, invalid_utf8.as_slice()).unwrap();

        EncryptedPayload {
            version: ENCRYPTED_PAYLOAD_V1,
            algorithm: X_CHA_CHA_20_POLY_1305.to_string(),
            key_id: key_id.to_string(),
            nonce: SECRET_KEY_ENCODING.encode(nonce),
            ciphertext: SECRET_KEY_ENCODING.encode(ciphertext),
        }
    }

    #[test]
    fn should_create_key_with_base64_encoded_32_byte_secret() {
        let cwd = Path::new("/repo");
        let (credentials, mock) = e2ee_credentials(cwd);

        let key_id = credentials.create_key().unwrap();
        let encoded_key = mock.local_value(cwd, &key_id).unwrap();
        let key = SECRET_KEY_ENCODING.decode(encoded_key).unwrap();

        assert!(key_id.starts_with(KEY_ID_PREFIX));
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn should_delete_existing_key() {
        let cwd = Path::new("/repo");
        let (credentials, mock) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();

        credentials.delete_key(&key_id).unwrap();

        assert!(mock.local_value(cwd, &key_id).is_none());
    }

    #[test]
    fn should_ignore_missing_key_delete() {
        let cwd = Path::new("/repo");
        let (credentials, mock) = e2ee_credentials(cwd);

        credentials.delete_key("missing").unwrap();

        assert!(mock.local_value(cwd, "missing").is_none());
    }

    #[test]
    fn should_encrypt_and_decrypt_payload() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();

        let encrypted = credentials
            .encrypt_payload(&key_id, "session plaintext")
            .unwrap();
        let decrypted = credentials.decrypt_payload(&encrypted).unwrap();

        assert_eq!(encrypted.version, ENCRYPTED_PAYLOAD_V1);
        assert_eq!(encrypted.algorithm, X_CHA_CHA_20_POLY_1305);
        assert_eq!(encrypted.key_id, key_id);
        assert_eq!(decrypted, "session plaintext");

        let nonce = SECRET_KEY_ENCODING.decode(&encrypted.nonce).unwrap();
        let ciphertext = SECRET_KEY_ENCODING.decode(&encrypted.ciphertext).unwrap();
        assert_eq!(nonce.len(), 24);
        assert_ne!(ciphertext, b"session plaintext");
        assert!(!encrypted.nonce.contains('='));
        assert!(!encrypted.ciphertext.contains('='));
    }

    #[test]
    fn should_generate_different_ciphertext_for_same_plaintext() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();

        let first = credentials.encrypt_payload(&key_id, "payload").unwrap();
        let second = credentials.encrypt_payload(&key_id, "payload").unwrap();

        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn should_fail_to_encrypt_when_key_is_missing() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);

        assert_error_contains(
            credentials.encrypt_payload("missing", "payload"),
            "E2EE key not found: missing",
        );
    }

    #[test]
    fn should_fail_to_encrypt_when_stored_key_is_not_base64() {
        let cwd = Path::new("/repo");
        let (credentials, mock) = e2ee_credentials(cwd);
        mock.set_local(cwd, "bad-key", "not base64");

        assert_error_contains(
            credentials.encrypt_payload("bad-key", "payload"),
            "Stored E2EE key is not valid base64.",
        );
    }

    #[test]
    fn should_fail_to_encrypt_when_stored_key_has_wrong_length() {
        let cwd = Path::new("/repo");
        let (credentials, mock) = e2ee_credentials(cwd);
        mock.set_local(cwd, "short-key", &SECRET_KEY_ENCODING.encode([1_u8; 31]));

        assert_error_contains(
            credentials.encrypt_payload("short-key", "payload"),
            "Stored E2EE key has an invalid length.",
        );
    }

    #[test]
    fn should_fail_to_decrypt_unsupported_version() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.version = ENCRYPTED_PAYLOAD_V1 + 1;

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Unsupported encrypted payload version",
        );
    }

    #[test]
    fn should_fail_to_decrypt_unsupported_algorithm() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.algorithm = "aes-256-gcm".to_string();

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Unsupported encrypted payload algorithm",
        );
    }

    #[test]
    fn should_fail_to_decrypt_when_key_is_missing() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.key_id = "missing".to_string();

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "E2EE key not found: missing",
        );
    }

    #[test]
    fn should_fail_to_decrypt_when_stored_key_is_not_base64() {
        let cwd = Path::new("/repo");
        let (credentials, mock) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.key_id = "bad-key".to_string();
        mock.set_local(cwd, "bad-key", "not base64");

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Stored E2EE key is not valid base64.",
        );
    }

    #[test]
    fn should_fail_to_decrypt_when_stored_key_has_wrong_length() {
        let cwd = Path::new("/repo");
        let (credentials, mock) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.key_id = "short-key".to_string();
        mock.set_local(cwd, "short-key", &SECRET_KEY_ENCODING.encode([1_u8; 31]));

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Stored E2EE key has an invalid length.",
        );
    }

    #[test]
    fn should_fail_to_decrypt_invalid_nonce_base64() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.nonce = "not base64".to_string();

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Encrypted payload nonce is not valid base64.",
        );
    }

    #[test]
    fn should_fail_to_decrypt_invalid_nonce_length() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.nonce = SECRET_KEY_ENCODING.encode([1_u8; 23]);

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Encrypted payload nonce has an invalid length.",
        );
    }

    #[test]
    fn should_fail_to_decrypt_invalid_ciphertext_base64() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        encrypted.ciphertext = "not base64".to_string();

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Encrypted payload ciphertext is not valid base64.",
        );
    }

    #[test]
    fn should_fail_to_decrypt_tampered_ciphertext() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials.encrypt_payload(&key_id, "payload").unwrap();
        let mut ciphertext = SECRET_KEY_ENCODING.decode(&encrypted.ciphertext).unwrap();
        ciphertext[0] ^= 1;
        encrypted.ciphertext = SECRET_KEY_ENCODING.encode(ciphertext);

        assert!(credentials.decrypt_payload(&encrypted).is_err());
    }

    #[test]
    fn should_fail_to_decrypt_with_wrong_key() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let original_key_id = credentials.create_key().unwrap();
        let wrong_key_id = credentials.create_key().unwrap();
        let mut encrypted = credentials
            .encrypt_payload(&original_key_id, "payload")
            .unwrap();
        encrypted.key_id = wrong_key_id;

        assert!(credentials.decrypt_payload(&encrypted).is_err());
    }

    #[test]
    fn should_fail_to_decrypt_non_utf8_plaintext() {
        let cwd = Path::new("/repo");
        let (credentials, _) = e2ee_credentials(cwd);
        let key_id = credentials.create_key().unwrap();
        let encrypted = envelope_with_invalid_utf8_plaintext(&credentials, &key_id);

        assert_error_contains(
            credentials.decrypt_payload(&encrypted),
            "Decrypted E2EE payload is not valid UTF-8.",
        );
    }
}
