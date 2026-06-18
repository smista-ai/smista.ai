//! Hash module contains the logic for hashing and verifying secrets, such as API keys and
//! passwords.
//!
//! Each supported algorithm is a variant of [`SecretHasher`]. Generics are deliberately avoided:
//! the backing crates live on incompatible `password-hash` major versions (`argon2` on `0.5`,
//! `sha-crypt` on `0.6`) with incompatible trait shapes and hash types, so no single trait bound
//! can cover both. Enum dispatch keeps each algorithm native to its own crate.

use argon2::Argon2;
use argon2::password_hash::rand_core::{OsRng, RngCore as _};
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use password_hash::{PasswordHasher as _, PasswordVerifier as _};
use secrecy::{ExposeSecret as _, SecretString};
use sha_crypt::ShaCrypt;

use crate::auth::{AuthenticationResult, AuthenticatorError};

/// Length, in bytes, of the random salt generated for SHA-crypt hashes.
///
/// SHA-crypt accepts up to 16 salt characters; 16 random bytes are Base64-encoded by the hasher
/// into the salt field, giving the full salt budget. Changing this only affects newly issued
/// hashes; previously stored hashes embed their own salt and remain verifiable.
const SHA_CRYPT_SALT_LEN: usize = 16;

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
                let salt = SaltString::generate(&mut OsRng);
                Argon2::default()
                    .hash_password(password, &salt)
                    .map_err(|e| AuthenticatorError::InternalError(e.into()))?
                    .to_string()
            }
            Self::Sha512Crypt => {
                let mut salt = [0u8; SHA_CRYPT_SALT_LEN];
                OsRng.fill_bytes(&mut salt);
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
