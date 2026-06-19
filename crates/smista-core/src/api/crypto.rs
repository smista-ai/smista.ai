//! End-to-end encryption payloads carried by the protocol.
//!
//! In an encrypted session the router holds no key and never encrypts or
//! decrypts: it asks the client to, as an ordinary continuation answered on the
//! same channel. [`EncryptedPayload`] is the sealed wire form of one content
//! payload — the same envelope storage persists at rest, but as a serialization
//! value the API can carry. It is opaque to the router; only a client holding
//! the session key can open it.
//!
//! Two record shapes pair a payload with the storage record it belongs to:
//! [`SealedRecord`] (ciphertext) and [`PlainRecord`] (cleartext). The router
//! sends sealed records for the client to open (an `awaiting_decrypt` turn) and
//! plain records for the client to seal (an `awaiting_encrypt` turn); the client
//! returns the opposite shape in the `/continue` bundle.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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

/// A storage record paired with its sealed payload.
///
/// The router sends these in an `awaiting_decrypt` turn (history to open); the
/// client returns them in the `/continue` bundle after sealing the content of
/// an `awaiting_encrypt` turn. Correlated by `record_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SealedRecord {
    /// Identifier of the storage record the envelope belongs to.
    pub record_id: String,
    /// The sealed envelope.
    pub envelope: EncryptedPayload,
}

/// A storage record paired with its cleartext.
///
/// The router sends these in an `awaiting_encrypt` turn (content to seal); the
/// client returns them in the `/continue` bundle after opening the records of
/// an `awaiting_decrypt` turn. Correlated by `record_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct PlainRecord {
    /// Identifier of the storage record the cleartext belongs to.
    pub record_id: String,
    /// The cleartext payload.
    pub plaintext: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> EncryptedPayload {
        EncryptedPayload {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        }
    }

    #[test]
    fn should_roundtrip_sealed_record() {
        let record = SealedRecord {
            record_id: "session_message:0194".to_string(),
            envelope: payload(),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<SealedRecord>(&json).unwrap(), record);
    }

    #[test]
    fn should_roundtrip_plain_record() {
        let record = PlainRecord {
            record_id: "session_message:0194".to_string(),
            plaintext: "the body".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<PlainRecord>(&json).unwrap(), record);
    }
}
