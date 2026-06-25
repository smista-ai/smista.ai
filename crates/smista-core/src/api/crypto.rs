//! End-to-end encryption payloads carried by the protocol.
//!
//! In an encrypted session the router holds no key and never encrypts or
//! decrypts: it asks the client to, as an ordinary continuation answered on the
//! same channel. [`EncryptedPayload`] is the sealed wire form of one content
//! payload — the same envelope storage persists at rest, but as a serialization
//! value the API can carry. It is opaque to the router; only a client holding
//! the session key can open it.
//!
//! Records are correlated by their storage `record_id`. The router sends sealed
//! payloads for the client to open (an `awaiting_decrypt` turn carries a
//! `record_id` -> [`EncryptedPayload`] map) and plaintext for the client to seal
//! (the encrypt request folded onto a data response carries a `record_id` ->
//! plaintext map); the client returns the opposite map in the `/continue`
//! message.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::EncryptedPayload;
//!
//! let payload = EncryptedPayload {
//!     version: 1,
//!     algorithm: "xchacha20poly1305".to_string(),
//!     key_id: "kf_ab12".to_string(),
//!     nonce: "bm9uY2U".to_string(),
//!     ciphertext: "Y2lwaGVydGV4dA".to_string(),
//! };
//! let json = serde_json::to_string(&payload).unwrap();
//! assert!(json.contains("\"key_id\":\"kf_ab12\""));
//! ```

use serde::{Deserialize, Serialize};

/// The sealed wire form of one content payload: an AEAD ciphertext envelope.
///
/// Self-describing — it names the algorithm and the key that sealed it, and
/// carries the one-time nonce and the ciphertext (including its authentication
/// tag) — and never carries the key itself. It mirrors the envelope storage
/// persists but is a distinct serialization-first type; the router converts
/// between the two when it persists or returns content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct EncryptedPayload {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_roundtrip_encrypted_payload() {
        let payload = EncryptedPayload {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert_eq!(
            serde_json::from_str::<EncryptedPayload>(&json).unwrap(),
            payload
        );
    }
}
