//! Types for issuing and validating API keys for smista-router

use rand::distr::{Alphanumeric, SampleString};
use secrecy::{ExposeSecret as _, SecretString};
use uuid::Uuid;

use crate::auth::{AuthenticationResult, AuthenticatorError};

const API_KEY_PREFIX_V1: &str = "sk-smista-api01-";
const API_KEY_RANDOM_LEN_V1: usize = 96;

/// The [`ApiKeyIssuer`] struct is responsible for generating and parsing API keys for users.
pub struct ApiKeyIssuer;

impl ApiKeyIssuer {
    /// Generates a new API key for the user identified by `user_id`.
    ///
    /// The API key format is:
    ///
    /// `sk-smista-api01-<user-id>-<96 alphanumeric>`
    ///
    /// where `<user-id>` is the user's UUID in its simple (32 hex digits, no
    /// hyphens) form and the trailing alphanumeric secret is generated with a
    /// CSPRNG. Embedding the user id lets the authentication layer identify the
    /// owner from the key alone, load that user and verify the key against the
    /// stored hash, without a secondary lookup by hash.
    pub fn generate_api_key_v1(user_id: &Uuid) -> SecretString {
        let random_string = Self::generate_alphanumeric_len(API_KEY_RANDOM_LEN_V1);
        let api_key = format!(
            "{API_KEY_PREFIX_V1}{user_id}-{random_string}",
            user_id = user_id.simple()
        );

        SecretString::from(api_key)
    }

    /// Parses the user id embedded in a v1 API key.
    ///
    /// # Errors
    ///
    /// Returns an error if the key does not carry the expected v1 prefix, is
    /// missing its secret segment, or its embedded user id is not a valid UUID.
    pub fn parse_user_id(api_key: &SecretString) -> AuthenticationResult<Uuid> {
        let body = api_key
            .expose_secret()
            .strip_prefix(API_KEY_PREFIX_V1)
            .ok_or(AuthenticatorError::InvalidApiKey)?;

        let (user_id, secret) = body
            .split_once('-')
            .ok_or(AuthenticatorError::InvalidApiKey)?;

        if secret.is_empty() {
            return Err(AuthenticatorError::InvalidApiKey);
        }

        Uuid::try_parse(user_id).map_err(|_| AuthenticatorError::InvalidApiKey)
    }

    /// Generates a random alphanumeric string of the specified length using a cryptographically secure random number generator (CSPRNG).
    #[inline(always)]
    fn generate_alphanumeric_len(len: usize) -> String {
        Alphanumeric.sample_string(&mut rand::rng(), len)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn should_generate_api_key_with_v1_prefix_user_id_and_random_secret() {
        let user_id = Uuid::now_v7();
        let api_key = ApiKeyIssuer::generate_api_key_v1(&user_id);
        let api_key = api_key.expose_secret();

        let expected_prefix = format!("{API_KEY_PREFIX_V1}{}-", user_id.simple());
        assert!(api_key.starts_with(&expected_prefix));

        let secret = &api_key[expected_prefix.len()..];
        assert_eq!(secret.len(), API_KEY_RANDOM_LEN_V1);
        assert!(secret.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn should_generate_unique_api_keys() {
        let user_id = Uuid::now_v7();
        let first = ApiKeyIssuer::generate_api_key_v1(&user_id);
        let second = ApiKeyIssuer::generate_api_key_v1(&user_id);

        assert_ne!(first.expose_secret(), second.expose_secret());
    }

    #[test]
    fn should_parse_the_user_id_back_from_a_generated_key() {
        let user_id = Uuid::now_v7();
        let api_key = ApiKeyIssuer::generate_api_key_v1(&user_id);

        let parsed = ApiKeyIssuer::parse_user_id(&api_key).expect("failed to parse user id");
        assert_eq!(parsed, user_id);
    }

    #[test]
    fn should_reject_a_key_without_the_v1_prefix() {
        let api_key = SecretString::from("sk-other-deadbeef-secret");

        assert!(ApiKeyIssuer::parse_user_id(&api_key).is_err());
    }

    #[test]
    fn should_reject_a_key_missing_its_secret_segment() {
        let user_id = Uuid::now_v7();
        let api_key = SecretString::from(format!("{API_KEY_PREFIX_V1}{}", user_id.simple()));

        assert!(ApiKeyIssuer::parse_user_id(&api_key).is_err());
    }

    #[test]
    fn should_reject_a_key_with_an_invalid_user_id() {
        let api_key = SecretString::from(format!("{API_KEY_PREFIX_V1}not-a-uuid-secret"));

        assert!(ApiKeyIssuer::parse_user_id(&api_key).is_err());
    }

    #[test]
    fn should_generate_alphanumeric_string_of_requested_length() {
        let random = ApiKeyIssuer::generate_alphanumeric_len(32);

        assert_eq!(random.len(), 32);
        assert!(random.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
