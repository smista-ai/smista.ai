//! Hash module contains the logic for hashing and verifying secrets, such as API keys and
//! passwords.
//!
//! Each supported algorithm is a variant of [`SecretHasher`]. Generics are deliberately avoided:
//! `argon2` and `sha-crypt` each bring their own hash type through `password-hash`'s
//! [`PasswordHasher`]/[`PasswordVerifier`] traits, so a single generic function would still need
//! to branch on the concrete algorithm to name its output type. Enum dispatch keeps that branch
//! in one place instead of pushing it onto every caller.

use argon2::Argon2;
use password_hash::phc::PasswordHash;
use password_hash::{PasswordHasher as _, PasswordVerifier as _};
use rand::RngExt as _;
use secrecy::{ExposeSecret as _, SecretString};
use sha_crypt::ShaCrypt;

use crate::auth::{AuthenticationResult, AuthenticatorError};

/// Length, in bytes, of the random salt generated for a new hash, for both algorithms.
///
/// 16 bytes is `password-hash`'s own recommended salt length, and is also the full salt budget
/// SHA-crypt's Base64 encoding accepts. Changing this only affects newly issued hashes;
/// previously stored hashes embed their own salt and remain verifiable.
const RANDOM_SALT_LEN: usize = 16;

/// Hashes and verifies secrets using a selectable password-hashing algorithm.
///
/// Pick an algorithm via the constructors ([`SecretHasher::argon2`],
/// [`SecretHasher::sha512_crypt`]), then call [`SecretHasher::hash`] or [`SecretHasher::verify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretHasher {
    /// Argon2id with default parameters.
    Argon2,
    /// SHA-512 crypt (`$6$`).
    Sha512Crypt,
}

impl SecretHasher {
    /// Creates a [`SecretHasher`] backed by the Argon2 algorithm.
    pub fn argon2() -> Self {
        Self::Argon2
    }

    /// Creates a [`SecretHasher`] backed by the SHA-512 crypt algorithm.
    pub fn sha512_crypt() -> Self {
        Self::Sha512Crypt
    }

    /// Hashes `secret` with a freshly generated random salt and returns the encoded hash.
    ///
    /// The returned [`SecretString`] is the self-describing PHC/MCF string for the selected
    /// algorithm and can be passed back to [`SecretHasher::verify`].
    pub fn hash(&self, secret: &SecretString) -> AuthenticationResult<SecretString> {
        let password = secret.expose_secret().as_bytes();

        let hash = match self {
            Self::Argon2 => {
                let mut salt = [0u8; RANDOM_SALT_LEN];
                rand::rng().fill(&mut salt);
                Argon2::default()
                    .hash_password_with_salt(password, &salt)
                    .map_err(|e| AuthenticatorError::InternalError(e.into()))?
                    .to_string()
            }
            Self::Sha512Crypt => {
                let mut salt = [0u8; RANDOM_SALT_LEN];
                rand::rng().fill(&mut salt);
                ShaCrypt::SHA512
                    .hash_password_with_salt(password, &salt)
                    .map_err(|e| AuthenticatorError::InternalError(e.into()))?
                    .to_string()
            }
        };

        Ok(SecretString::from(hash))
    }

    /// Verifies `secret` against the previously produced `hashed` value.
    ///
    /// Returns `Ok(true)` if the secret matches, `Ok(false)` if it does not, and an error if the
    /// stored hash is malformed for the selected algorithm.
    pub fn verify(&self, secret: &SecretString, hashed: &str) -> AuthenticationResult<bool> {
        let password = secret.expose_secret().as_bytes();

        let verified = match self {
            Self::Argon2 => {
                let parsed_hash =
                    PasswordHash::new(hashed).map_err(|_| AuthenticatorError::InvalidHash)?;
                Argon2::default()
                    .verify_password(password, &parsed_hash)
                    .is_ok()
            }
            Self::Sha512Crypt => ShaCrypt::SHA512.verify_password(password, hashed).is_ok(),
        };

        Ok(verified)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn should_hash_secret_to_a_different_value() {
        let secret = SecretString::from("super-secret");
        let hash = SecretHasher::argon2()
            .hash(&secret)
            .expect("failed to hash secret");

        assert_ne!(hash.expose_secret(), secret.expose_secret());
    }

    #[test]
    fn should_produce_distinct_hashes_for_the_same_secret() {
        let secret = SecretString::from("super-secret");
        let hasher = SecretHasher::argon2();
        let first = hasher.hash(&secret).expect("failed to hash secret");
        let second = hasher.hash(&secret).expect("failed to hash secret");

        // A random salt yields a different hash for the same input each time.
        assert_ne!(first.expose_secret(), second.expose_secret());
    }

    #[test]
    fn should_verify_a_matching_secret() {
        let secret = SecretString::from("super-secret");
        let hasher = SecretHasher::argon2();
        let hash = hasher.hash(&secret).expect("failed to hash secret");

        assert!(
            hasher
                .verify(&secret, hash.expose_secret())
                .expect("failed to verify secret")
        );
    }

    #[test]
    fn should_reject_a_non_matching_secret() {
        let hasher = SecretHasher::argon2();
        let hash = hasher
            .hash(&SecretString::from("super-secret"))
            .expect("failed to hash secret");
        let wrong = SecretString::from("wrong-secret");

        assert!(
            !hasher
                .verify(&wrong, hash.expose_secret())
                .expect("failed to verify secret")
        );
    }

    #[test]
    fn should_error_on_a_malformed_hash() {
        let secret = SecretString::from("super-secret");
        let malformed = "not-a-valid-argon2-hash";

        assert!(SecretHasher::argon2().verify(&secret, malformed).is_err());
    }

    #[test]
    fn should_hash_and_verify_with_sha512_crypt() {
        let secret = SecretString::from("super-secret");
        let hasher = SecretHasher::sha512_crypt();
        let hash = hasher.hash(&secret).expect("failed to hash secret");

        // SHA-512 crypt hashes are tagged with the `$6$` identifier.
        assert!(hash.expose_secret().starts_with("$6$"));
        assert!(
            hasher
                .verify(&secret, hash.expose_secret())
                .expect("failed to verify secret")
        );
        assert!(
            !hasher
                .verify(&SecretString::from("wrong-secret"), hash.expose_secret())
                .expect("failed to verify secret")
        );
    }
}
