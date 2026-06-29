//! Router credential formats shared by the server and its clients.
//!
//! `smista-router` separates **router authentication** (who you are) from
//! **provider credentials** (keys for the upstream models). This module owns the
//! two router-authentication credentials and their on-the-wire formats:
//!
//! - [`ApiKey`] — the long-lived key minted at bootstrap,
//!   `sk-smista-api01-<user-id>-<secret>`, exchanged for a session token.
//! - [`SessionToken`] — the short-lived bearer token, `<token-id>-<secret>`,
//!   sent on every authenticated request.
//!
//! The format is a contract between the router that issues these credentials and
//! every client that presents them, so it lives here, in the shared core, rather
//! than being reimplemented on either side. Issuing a credential — generating its
//! random secret and hashing it for storage — is the router's responsibility and
//! is deliberately **not** part of this module; only the format, its structural
//! validation and the identifiers it embeds are shared.
//!
//! Both types wrap a [`SecretString`], so the secret never appears in a `Debug`
//! rendering and is read out only through the explicit `expose` accessor.
//! Construction with [`ApiKey::new`]/[`SessionToken::new`] or `From<SecretString>`
//! is unchecked, for credentials a caller already trusts; parsing untrusted input
//! goes through [`FromStr`], which validates the structure.

use std::str::FromStr;

use secrecy::{ExposeSecret as _, SecretString};
use uuid::Uuid;

use crate::error::ParseError;

/// Prefix of a version 1 API key.
///
/// The `api01` segment versions the format: a future revision can introduce a
/// new prefix while older keys remain recognizable by this one.
const API_KEY_PREFIX_V1: &str = "sk-smista-api01-";

/// The header name that carries the API key in requests.
const API_KEY_HEADER: &str = "X-Smista-Api-Key";

/// A router API key, formatted `sk-smista-api01-<user-id>-<secret>`.
///
/// The key embeds the owner's user id in its simple (32 hex digits, no hyphens)
/// form, so the router identifies the owner from the key alone. The trailing
/// secret is opaque. Presented in the `X-Smista-Api-Key` header to obtain a
/// session token; never logged or persisted in the clear.
#[derive(Clone, Debug)]
pub struct ApiKey(SecretString);

impl ApiKey {
    /// Assembles a version 1 API key for `user_id` from a generated `secret`.
    ///
    /// Produces `sk-smista-api01-<user-id>-<secret>`, with the user id in its
    /// simple form. Generating the random `secret` and hashing the key for
    /// storage are the caller's concern; this only fixes the wire format. Pass a
    /// non-empty `secret`, or the result will not [parse](ApiKey::from_str) back.
    #[must_use]
    pub fn from_parts(user_id: &Uuid, secret: &str) -> Self {
        Self(SecretString::from(format!(
            "{API_KEY_PREFIX_V1}{}-{secret}",
            user_id.simple()
        )))
    }

    /// Wraps an already-trusted secret as an API key without validating it.
    ///
    /// Use [`FromStr`] instead to parse and validate untrusted input.
    pub fn new<S>(secret: S) -> Self
    where
        S: Into<SecretString>,
    {
        Self(secret.into())
    }

    /// Exposes the raw key so it can be placed in a request header.
    ///
    /// The returned string is the secret in the clear; never log or persist it.
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Returns the user id embedded in the key.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidApiKey`] when the key lacks the version 1
    /// prefix, is missing its secret segment, or its embedded id is not a valid
    /// UUID — possible only for a key built through the unchecked constructors,
    /// since [`FromStr`] rejects such values up front.
    pub fn user_id(&self) -> Result<Uuid, ParseError> {
        parse_api_key_user_id(self.expose())
    }

    /// Returns the name of the request header that carries the API key.
    pub const fn header_name() -> &'static str {
        API_KEY_HEADER
    }
}

impl From<SecretString> for ApiKey {
    fn from(secret: SecretString) -> Self {
        Self(secret)
    }
}

impl FromStr for ApiKey {
    type Err = ParseError;

    /// Parses and validates a key in `sk-smista-api01-<user-id>-<secret>` form.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidApiKey`] when the value lacks the version 1
    /// prefix, is missing its secret segment, or its embedded user id is not a
    /// valid UUID. The malformed value is never included in the error.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Validate the structure, then keep the original secret verbatim so it
        // can be presented back to the router exactly as issued.
        parse_api_key_user_id(s)?;
        Ok(Self(SecretString::from(s)))
    }
}

/// A router session token, formatted `<token-id>-<secret>`.
///
/// The token id is a UUID in its simple (32 hex digits, no hyphens) form; the
/// trailing secret is opaque. Treated as an opaque bearer credential by clients
/// and presented as `Authorization: Bearer <token>`; never logged or persisted
/// in the clear.
#[derive(Clone, Debug)]
pub struct SessionToken(SecretString);

impl SessionToken {
    /// Assembles a session token for `token_id` from a generated `secret`.
    ///
    /// Produces `<token-id>-<secret>`, with the id in its simple form.
    /// Generating the random `secret` and hashing the token for storage are the
    /// caller's concern; this only fixes the wire format. Pass a non-empty
    /// `secret`, or the result will not [parse](SessionToken::from_str) back.
    #[must_use]
    pub fn from_parts(token_id: &Uuid, secret: &str) -> Self {
        Self(SecretString::from(format!(
            "{}-{secret}",
            token_id.simple()
        )))
    }

    /// Wraps an already-trusted secret as a session token without validating it.
    ///
    /// Use [`FromStr`] instead to parse and validate untrusted input.
    pub fn new<S>(secret: S) -> Self
    where
        S: Into<SecretString>,
    {
        Self(secret.into())
    }

    /// Exposes the raw token so it can be placed in a request header.
    ///
    /// The returned string is the secret in the clear; never log or persist it.
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Returns the token id embedded in the token.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidSessionToken`] when the token lacks its
    /// separator, is missing its secret segment, or its id is not a valid UUID —
    /// possible only for a token built through the unchecked constructors, since
    /// [`FromStr`] rejects such values up front.
    pub fn token_id(&self) -> Result<Uuid, ParseError> {
        parse_session_token_id(self.expose())
    }
}

impl From<SecretString> for SessionToken {
    fn from(secret: SecretString) -> Self {
        Self(secret)
    }
}

impl FromStr for SessionToken {
    type Err = ParseError;

    /// Parses and validates a token in `<token-id>-<secret>` form.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidSessionToken`] when the value lacks its
    /// separator, is missing its secret segment, or its id is not a valid UUID.
    /// The malformed value is never included in the error.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Validate the structure, then keep the original secret verbatim so it
        // can be presented back to the router exactly as issued.
        parse_session_token_id(s)?;
        Ok(Self(SecretString::from(s)))
    }
}

/// Parses the user id out of a version 1 API key, validating its structure.
fn parse_api_key_user_id(key: &str) -> Result<Uuid, ParseError> {
    let body = key
        .strip_prefix(API_KEY_PREFIX_V1)
        .ok_or(ParseError::InvalidApiKey)?;
    let (user_id, secret) = body.split_once('-').ok_or(ParseError::InvalidApiKey)?;
    if secret.is_empty() {
        return Err(ParseError::InvalidApiKey);
    }
    Uuid::try_parse(user_id).map_err(|_| ParseError::InvalidApiKey)
}

/// Parses the token id out of a session token, validating its structure.
fn parse_session_token_id(token: &str) -> Result<Uuid, ParseError> {
    let (token_id, secret) = token
        .split_once('-')
        .ok_or(ParseError::InvalidSessionToken)?;
    if secret.is_empty() {
        return Err(ParseError::InvalidSessionToken);
    }
    Uuid::try_parse(token_id).map_err(|_| ParseError::InvalidSessionToken)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_user_id() -> Uuid {
        Uuid::parse_str("018f9c3e-7a2b-7c4d-8e5f-1a2b3c4d5e6f").expect("invalid sample uuid")
    }

    #[test]
    fn should_parse_a_well_formed_api_key_and_recover_its_user_id() {
        let user_id = sample_user_id();
        let raw = format!("{API_KEY_PREFIX_V1}{}-deadbeefsecret", user_id.simple());

        let key: ApiKey = raw.parse().expect("a well-formed key should parse");
        assert_eq!(key.expose(), raw);
        assert_eq!(key.user_id().expect("user id should parse"), user_id);
    }

    #[test]
    fn should_assemble_an_api_key_that_parses_back_to_its_user_id() {
        let user_id = sample_user_id();
        let key = ApiKey::from_parts(&user_id, "deadbeefsecret");

        assert!(key.expose().starts_with(API_KEY_PREFIX_V1));
        // The assembled key validates and yields the embedded user id.
        let reparsed: ApiKey = key.expose().parse().expect("assembled key should parse");
        assert_eq!(reparsed.user_id().expect("user id should parse"), user_id);
    }

    #[test]
    fn should_reject_an_api_key_without_the_v1_prefix() {
        assert_eq!(
            "sk-other-deadbeef-secret".parse::<ApiKey>().unwrap_err(),
            ParseError::InvalidApiKey
        );
    }

    #[test]
    fn should_reject_an_api_key_missing_its_secret_segment() {
        let raw = format!("{API_KEY_PREFIX_V1}{}", sample_user_id().simple());
        assert_eq!(
            raw.parse::<ApiKey>().unwrap_err(),
            ParseError::InvalidApiKey
        );
    }

    #[test]
    fn should_reject_an_api_key_with_an_empty_secret() {
        let raw = format!("{API_KEY_PREFIX_V1}{}-", sample_user_id().simple());
        assert_eq!(
            raw.parse::<ApiKey>().unwrap_err(),
            ParseError::InvalidApiKey
        );
    }

    #[test]
    fn should_reject_an_api_key_with_an_invalid_user_id() {
        let raw = format!("{API_KEY_PREFIX_V1}not-a-uuid-secret");
        assert_eq!(
            raw.parse::<ApiKey>().unwrap_err(),
            ParseError::InvalidApiKey
        );
    }

    #[test]
    fn should_keep_an_unchecked_api_key_verbatim() {
        // `new` does not validate, so a malformed value is wrapped as-is and
        // only surfaces as an error when the embedded id is read.
        let key = ApiKey::new("not-a-key");
        assert_eq!(key.expose(), "not-a-key");
        assert_eq!(key.user_id().unwrap_err(), ParseError::InvalidApiKey);
    }

    #[test]
    fn should_parse_a_well_formed_session_token_and_recover_its_id() {
        let token_id = sample_user_id();
        let raw = format!("{}-loweralphanumericsecret", token_id.simple());

        let token: SessionToken = raw.parse().expect("a well-formed token should parse");
        assert_eq!(token.expose(), raw);
        assert_eq!(token.token_id().expect("token id should parse"), token_id);
    }

    #[test]
    fn should_assemble_a_session_token_that_parses_back_to_its_id() {
        let token_id = sample_user_id();
        let token = SessionToken::from_parts(&token_id, "loweralphanumericsecret");

        // The assembled token validates and yields the embedded token id.
        let reparsed: SessionToken = token
            .expose()
            .parse()
            .expect("assembled token should parse");
        assert_eq!(
            reparsed.token_id().expect("token id should parse"),
            token_id
        );
    }

    #[test]
    fn should_reject_a_session_token_without_a_separator() {
        assert_eq!(
            "noseparatorhere".parse::<SessionToken>().unwrap_err(),
            ParseError::InvalidSessionToken
        );
    }

    #[test]
    fn should_reject_a_session_token_with_an_invalid_id() {
        assert_eq!(
            "not-a-valid-secret".parse::<SessionToken>().unwrap_err(),
            ParseError::InvalidSessionToken
        );
    }

    #[test]
    fn should_reject_a_session_token_missing_its_secret() {
        let raw = format!("{}-", sample_user_id().simple());
        assert_eq!(
            raw.parse::<SessionToken>().unwrap_err(),
            ParseError::InvalidSessionToken
        );
    }
}
