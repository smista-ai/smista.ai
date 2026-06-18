//! Token module contains the logic for generating and verifying session tokens

use core::fmt;
use std::str::FromStr;

use rand::RngExt as _;
use secrecy::{ExposeSecret as _, SecretString};
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

/// A session token, consisting of a UUID and a random alphanumeric string.
///
/// The token is represented as a string in the format `<uuid>-<random>`,
/// where `<uuid>` is a valid UUID and `<random>` is a non-empty string.
/// The UUID serves as the stable identifier for the token, while the random part provides additional entropy for security.
pub struct Token {
    token_id: Uuid,
    random: SecretString,
}

impl Token {
    /// Returns the token's UUID, which serves as its stable identifier in the database and API.
    pub fn token_id(&self) -> Uuid {
        self.token_id
    }

    /// Parses a token from its `<token-id>-<random>` string representation.
    ///
    /// `<token-id>` must be a valid UUID and `<random>` a non-empty secret.
    ///
    /// # Errors
    ///
    /// Returns [`AuthenticatorError::InvalidToken`] when the string lacks the
    /// `-` separator, its id is not a valid UUID, or its secret segment is
    /// empty.
    pub fn parse(token_str: &str) -> AuthenticationResult<Self> {
        let (token_id_str, random) = token_str
            .split_once('-')
            .ok_or(AuthenticatorError::InvalidToken)?;

        let token_id =
            Uuid::try_parse(token_id_str).map_err(|_| AuthenticatorError::InvalidToken)?;

        if random.is_empty() {
            return Err(AuthenticatorError::InvalidToken);
        }

        Ok(Self {
            token_id,
            random: SecretString::from(random),
        })
    }
}

impl FromStr for Token {
    type Err = AuthenticatorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}",
            self.token_id.simple(),
            self.random.expose_secret()
        )
    }
}

/// Session token issuer
pub struct TokenIssuer;

impl TokenIssuer {
    /// Generates a new session token with a UUIDv7 id and a fresh random secret.
    ///
    /// The secret is `TOKEN_RANDOM_LEN` characters drawn from [`TOKEN_CHARSET`].
    pub fn generate_token() -> Token {
        let token_id = Uuid::now_v7();
        let random = SecretString::from(Self::generate_secret(TOKEN_RANDOM_LEN));

        Token { token_id, random }
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
    use super::*;

    #[test]
    fn should_generate_token_in_the_documented_format() {
        let token = TokenIssuer::generate_token();
        let rendered = token.to_string();

        // `<token-id>-<random>`: the id renders as 32 hex digits with no hyphens.
        let (id, random) = rendered
            .split_once('-')
            .expect("token is missing its separator");
        assert_eq!(id, token.token_id().simple().to_string());
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
        let first = TokenIssuer::generate_token().to_string();
        let second = TokenIssuer::generate_token().to_string();

        assert_ne!(first, second);
    }

    #[test]
    fn should_round_trip_a_token_through_parse() {
        let token = TokenIssuer::generate_token();
        let rendered = token.to_string();

        let parsed = Token::parse(&rendered).expect("failed to parse a freshly generated token");

        assert_eq!(parsed.token_id(), token.token_id());
        // The reparsed token renders identically, secret included.
        assert_eq!(parsed.to_string(), rendered);
    }

    #[test]
    fn should_parse_via_from_str() {
        let rendered = TokenIssuer::generate_token().to_string();

        let parsed: Token = rendered.parse().expect("failed to parse token via FromStr");

        assert_eq!(parsed.to_string(), rendered);
    }

    #[test]
    fn should_reject_a_token_without_a_separator() {
        assert!(matches!(
            Token::parse("noseparatorhere"),
            Err(AuthenticatorError::InvalidToken)
        ));
    }

    #[test]
    fn should_reject_a_token_with_an_invalid_id() {
        assert!(matches!(
            Token::parse("not-a-valid-secret"),
            Err(AuthenticatorError::InvalidToken)
        ));
    }

    #[test]
    fn should_reject_a_token_missing_its_secret() {
        let id = Uuid::now_v7().simple().to_string();
        let token_str = format!("{id}-");

        assert!(matches!(
            Token::parse(&token_str),
            Err(AuthenticatorError::InvalidToken)
        ));
    }
}
