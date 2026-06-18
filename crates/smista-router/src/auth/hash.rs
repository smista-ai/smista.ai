//! Hash module contains the logic for hashing and verifying secrets, such as API keys and passwords, using the Argon2 algorithm.

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use secrecy::{ExposeSecret as _, SecretString};

/// The [`SecretHasher`] struct provides methods for hashing and verifying secrets using the Argon2 algorithm.
pub struct SecretHasher;

impl SecretHasher {
    /// Hashes the provided secret using the Argon2 algorithm and returns the hashed value as a [`SecretString`].
    pub fn hash(secret: &SecretString) -> anyhow::Result<SecretString> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(secret.expose_secret().as_bytes(), &salt)?
            .to_string();
        Ok(SecretString::from(password_hash))
    }

    /// Verifies the provided secret against the hashed value using the Argon2 algorithm.
    ///
    /// Returns `Ok(true)` if the secret matches the hashed value, `Ok(false)` if it does not match,
    /// and an error if there was an issue during verification.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by bearer authentication middleware (#136)")
    )]
    pub fn verify(secret: &SecretString, hashed_secret: &SecretString) -> anyhow::Result<bool> {
        let parsed_hash = PasswordHash::new(hashed_secret.expose_secret())?;
        let argon2 = Argon2::default();

        match argon2.verify_password(secret.expose_secret().as_bytes(), &parsed_hash) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn should_hash_secret_to_a_different_value() {
        let secret = SecretString::from("super-secret");
        let hash = SecretHasher::hash(&secret).expect("failed to hash secret");

        assert_ne!(hash.expose_secret(), secret.expose_secret());
    }

    #[test]
    fn should_produce_distinct_hashes_for_the_same_secret() {
        let secret = SecretString::from("super-secret");
        let first = SecretHasher::hash(&secret).expect("failed to hash secret");
        let second = SecretHasher::hash(&secret).expect("failed to hash secret");

        // A random salt yields a different hash for the same input each time.
        assert_ne!(first.expose_secret(), second.expose_secret());
    }

    #[test]
    fn should_verify_a_matching_secret() {
        let secret = SecretString::from("super-secret");
        let hash = SecretHasher::hash(&secret).expect("failed to hash secret");

        assert!(SecretHasher::verify(&secret, &hash).expect("failed to verify secret"));
    }

    #[test]
    fn should_reject_a_non_matching_secret() {
        let secret = SecretString::from("super-secret");
        let hash = SecretHasher::hash(&secret).expect("failed to hash secret");
        let wrong = SecretString::from("wrong-secret");

        assert!(!SecretHasher::verify(&wrong, &hash).expect("failed to verify secret"));
    }

    #[test]
    fn should_error_on_a_malformed_hash() {
        let secret = SecretString::from("super-secret");
        let malformed = SecretString::from("not-a-valid-argon2-hash");

        assert!(SecretHasher::verify(&secret, &malformed).is_err());
    }
}
