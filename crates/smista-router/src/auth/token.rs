//! Token module contains the logic for generating and verifying session tokens

use rand::RngExt as _;
use secrecy::SecretString;
use smista_core::credential::SessionToken;
use uuid::Uuid;

use crate::auth::{AuthenticationResult, AuthenticatorError};

/// Length, in characters, of the random secret appended to a session token.
///
/// 64 characters drawn from [`TOKEN_CHARSET`] give 36^64 bits of entropy, far
/// beyond brute-force reach; the secret is verified against a stored hash on
/// every request, so it must be unguessable.
const TOKEN_RANDOM_LEN: usize = 64;

/// Alphabet the random token secret is sampled from: lowercase letters and
/// digits, matching the documented `<token-id>-<64 lowercase alphanumeric>`
/// token format.
const TOKEN_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Session token issuer.
///
/// Generates the random secret and parses presented tokens, while the
/// `<token-id>-<secret>` wire format itself is owned by
/// [`SessionToken`](smista_core::credential::SessionToken), shared with every
/// client that presents a token.
pub struct TokenIssuer;

impl TokenIssuer {
    /// Generates a new session token, returning its id and the sealed string.
    ///
    /// The id is a fresh UUIDv7 and the secret is `TOKEN_RANDOM_LEN` characters
    /// drawn from [`TOKEN_CHARSET`]; the two are assembled into the
    /// `<token-id>-<secret>` form by [`SessionToken`], the shared owner of the
    /// format. The id is returned alongside the token so the caller can persist
    /// the token row without reparsing the secret.
    pub fn generate_token() -> (Uuid, SecretString) {
        let token_id = Uuid::now_v7();
        let secret = Self::generate_secret(TOKEN_RANDOM_LEN);
        let token = SessionToken::from_parts(&token_id, &secret);

        (token_id, SecretString::from(token.expose()))
    }

    /// Parses the token id out of a presented session token.
    ///
    /// The token's format is validated by [`SessionToken`], the shared owner of
    /// the format.
    ///
    /// # Errors
    ///
    /// Returns [`AuthenticatorError::InvalidToken`] when the string lacks its
    /// `-` separator, its id is not a valid UUID, or its secret segment is empty.
    pub fn parse_token_id(token: &str) -> AuthenticationResult<Uuid> {
        token
            .parse::<SessionToken>()
            .map_err(|_| AuthenticatorError::InvalidToken)?
            .token_id()
            .map_err(|_| AuthenticatorError::InvalidToken)
    }

    /// Generates a random lowercase-alphanumeric string of length `len`.
    ///
    /// Characters are sampled uniformly from [`TOKEN_CHARSET`] using the
    /// thread-local CSPRNG.
    #[inline(always)]
    fn generate_secret(len: usize) -> String {
        let mut rng = rand::rng();
        (0..len)
            .map(|_| char::from(TOKEN_CHARSET[rng.random_range(0..TOKEN_CHARSET.len())]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret as _;

    use super::*;

    #[test]
    fn should_generate_token_in_the_documented_format() {
        let (token_id, token) = TokenIssuer::generate_token();
        let rendered = token.expose_secret();

        // `<token-id>-<random>`: the id renders as 32 hex digits with no hyphens.
        let (id, random) = rendered
            .split_once('-')
            .expect("token is missing its separator");
        assert_eq!(id, token_id.simple().to_string());
        assert_eq!(id.len(), 32);
        assert_eq!(random.len(), TOKEN_RANDOM_LEN);
        assert!(
            random
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "the random secret must be lowercase alphanumeric"
        );
    }

    #[test]
    fn should_generate_unique_tokens() {
        let (_, first) = TokenIssuer::generate_token();
        let (_, second) = TokenIssuer::generate_token();

        assert_ne!(first.expose_secret(), second.expose_secret());
    }

    #[test]
    fn should_parse_the_token_id_back_from_a_generated_token() {
        let (token_id, token) = TokenIssuer::generate_token();

        let parsed = TokenIssuer::parse_token_id(token.expose_secret())
            .expect("failed to parse a freshly generated token");
        assert_eq!(parsed, token_id);
    }

    #[test]
    fn should_reject_a_malformed_token() {
        assert!(matches!(
            TokenIssuer::parse_token_id("noseparatorhere"),
            Err(AuthenticatorError::InvalidToken)
        ));
    }
}
