//! Shared persisted value types.
//!
//! These types are stored as fields of the content entities but are not entities
//! themselves: they carry no record id and implement no [`Table`](crate::entity::Table).
//!
//! [`SecretContent`] is the payload every encryptable `_content` field holds:
//! either [`Plaintext`](SecretContent::Plaintext) for a normal session or
//! [`Encrypted`](SecretContent::Encrypted) for an end-to-end encrypted one. The
//! choice is fixed per session by the owning [`Session`](crate::entity::Session)'s
//! `encrypted` flag.
//!
//! The router and the storage layer treat an [`Encrypted`](SecretContent::Encrypted)
//! payload as opaque bytes. They never hold a key, never encrypt and never
//! decrypt: a [`ContentEnvelope`] is produced and opened only by a client that
//! holds the session key. Storage just persists and returns whichever form it is
//! given. See `docs/technical/e2e.md` for the end-to-end design.

use surrealdb::types::SurrealValue;

/// An AEAD ciphertext envelope sealing one content payload.
///
/// An envelope is self-describing: it names the algorithm and the key that
/// sealed it, carries the one-time `nonce` used, and holds the `ciphertext`
/// (including the authentication tag). It never carries the key itself. The
/// fields are opaque to storage; only a client holding the session key can open
/// one.
#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
pub struct ContentEnvelope {
    /// Envelope schema version, so the format can evolve.
    pub version: u8,
    /// AEAD algorithm identifier, for example `xchacha20poly1305`.
    pub algorithm: String,
    /// Fingerprint of the per-session key that sealed this payload.
    pub key_id: String,
    /// Base64-encoded one-time nonce used for this payload.
    pub nonce: String,
    /// Base64-encoded ciphertext, including the AEAD authentication tag.
    pub ciphertext: String,
}

/// A content payload stored either in clear or sealed as a [`ContentEnvelope`].
///
/// A non-encrypted session stores [`Plaintext`](Self::Plaintext); an encrypted
/// session stores [`Encrypted`](Self::Encrypted). The variant is self-describing,
/// so a reader can tell the two apart without consulting the session.
#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
pub enum SecretContent {
    /// The payload stored in clear, for a non-encrypted session.
    Plaintext(String),
    /// The payload sealed as an AEAD envelope, for an encrypted session.
    Encrypted(ContentEnvelope),
}

impl SecretContent {
    /// Wraps a plaintext payload.
    pub fn plaintext(text: impl Into<String>) -> Self {
        Self::Plaintext(text.into())
    }

    /// Returns the plaintext, or `None` when the payload is encrypted.
    pub fn as_plaintext(&self) -> Option<&str> {
        match self {
            Self::Plaintext(text) => Some(text),
            Self::Encrypted(_) => None,
        }
    }

    /// Returns the envelope, or `None` when the payload is plaintext.
    pub fn as_envelope(&self) -> Option<&ContentEnvelope> {
        match self {
            Self::Encrypted(envelope) => Some(envelope),
            Self::Plaintext(_) => None,
        }
    }

    /// Returns whether the payload is sealed as an envelope.
    pub fn is_encrypted(&self) -> bool {
        matches!(self, Self::Encrypted(_))
    }
}

impl From<ContentEnvelope> for SecretContent {
    fn from(envelope: ContentEnvelope) -> Self {
        Self::Encrypted(envelope)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn envelope() -> ContentEnvelope {
        ContentEnvelope {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        }
    }

    #[test]
    fn should_expose_plaintext_only_when_clear() {
        let clear = SecretContent::plaintext("hello");
        assert_eq!(clear.as_plaintext(), Some("hello"));
        assert_eq!(clear.as_envelope(), None);
        assert!(!clear.is_encrypted());
    }

    #[test]
    fn should_expose_envelope_only_when_encrypted() {
        let sealed = SecretContent::from(envelope());
        assert_eq!(sealed.as_plaintext(), None);
        assert_eq!(sealed.as_envelope(), Some(&envelope()));
        assert!(sealed.is_encrypted());
    }

    #[tokio::test]
    async fn should_store_and_read_plaintext_content() {
        crate::tests::value_roundtrip(SecretContent::plaintext("the body")).await;
    }

    #[tokio::test]
    async fn should_store_and_read_encrypted_content() {
        crate::tests::value_roundtrip(SecretContent::from(envelope())).await;
    }
}
