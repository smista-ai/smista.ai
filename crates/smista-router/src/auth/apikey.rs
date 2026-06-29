//! Types for issuing and validating API keys for smista-router

use rand::distr::{Alphanumeric, SampleString};
use secrecy::{ExposeSecret as _, SecretString};
use smista_core::credential::ApiKey;
use uuid::Uuid;

use crate::auth::{AuthenticationResult, AuthenticatorError};

const API_KEY_RANDOM_LEN_V1: usize = 96;

/// The [`ApiKeyIssuer`] struct is responsible for generating and parsing API keys for users.
pub struct ApiKeyIssuer;

impl ApiKeyIssuer {
    /// Generates a new API key for the user identified by `user_id`.
    ///
    /// The trailing alphanumeric secret is generated with a CSPRNG; the rest of
    /// the key — its versioned prefix and the embedded user id — is assembled by
    /// [`ApiKey::from_parts`], the shared owner of the format. Embedding the user
    /// id lets the authentication layer identify the owner from the key alone,
    /// load that user and verify the key against the stored hash, without a
    /// secondary lookup by hash.
    pub fn generate_api_key_v1(user_id: &Uuid) -> SecretString {
        let random_string = Self::generate_alphanumeric_len(API_KEY_RANDOM_LEN_V1);
        SecretString::from(ApiKey::from_parts(user_id, &random_string).expose())
    }

    /// Parses the user id embedded in a v1 API key.
    ///
    /// The key's format is validated by [`ApiKey`], the shared owner of the
    /// format; a malformed key is reported as [`AuthenticatorError::InvalidApiKey`].
    ///
    /// # Errors
    ///
    /// Returns [`AuthenticatorError::InvalidApiKey`] if the key does not carry
    /// the expected v1 prefix, is missing its secret segment, or its embedded
    /// user id is not a valid UUID.
    pub fn parse_user_id(api_key: &SecretString) -> AuthenticationResult<Uuid> {
        api_key
            .expose_secret()
            .parse::<ApiKey>()
            .map_err(|_| AuthenticatorError::InvalidApiKey)?
            .user_id()
            .map_err(|_| AuthenticatorError::InvalidApiKey)
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

        let expected_prefix = format!("sk-smista-api01-{}-", user_id.simple());
        assert!(api_key.starts_with(&expected_prefix));

        // The CSPRNG-generated secret is `API_KEY_RANDOM_LEN_V1` alphanumerics.
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
    fn should_reject_a_malformed_key() {
        let api_key = SecretString::from("sk-other-deadbeef-secret");

        assert!(matches!(
            ApiKeyIssuer::parse_user_id(&api_key),
            Err(AuthenticatorError::InvalidApiKey)
        ));
    }

    #[test]
    fn should_generate_alphanumeric_string_of_requested_length() {
        let random = ApiKeyIssuer::generate_alphanumeric_len(32);

        assert_eq!(random.len(), 32);
        assert!(random.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
