//! Mapping between a [`ContentRef`] and the storage row it addresses.
//!
//! The end-to-end-encryption fold keys every sealed payload by a [`ContentRef`]
//! of the form `kind:id`. This module is the single source of truth translating
//! that reference into the base (metadata) table plus the record uuid it
//! addresses, and back. The sealed payload itself lives in the paired
//! `<table>_content` row under the same uuid key.
use std::collections::BTreeMap;

use smista_core::api::{ContentRef, EncryptedPayload};
use smista_storage::types::{ContentEnvelope, SecretContent};
use uuid::Uuid;

use crate::session::{SessionResult, UserSession};

/// An error mapping a [`ContentRef`] to a storage row.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum CryptoMapError {
    /// The reference id is not a valid uuid.
    #[error("content reference id is not a uuid: {0}")]
    BadUuid(String),
}

/// The base (metadata) table a content reference addresses.
///
/// The sealed payload lives in the paired `<table>_content` row under the same
/// uuid key.
pub(crate) fn content_ref_table(reference: &ContentRef) -> &'static str {
    match reference {
        ContentRef::Message(_) => "session_message",
        ContentRef::ToolCall(_) => "session_tool_call",
        ContentRef::Diff(_) => "session_diff",
        ContentRef::Plan(_) => "session_plan",
        ContentRef::Memory(_) => "context_memory",
        ContentRef::Trace(_) => "trace_event",
        ContentRef::RunInput(_) => "session_run_input",
    }
}

/// Parses the record uuid a content reference addresses.
///
/// # Errors
///
/// Returns [`CryptoMapError::BadUuid`] when the reference id is not a uuid.
pub(crate) fn content_ref_uuid(reference: &ContentRef) -> Result<Uuid, CryptoMapError> {
    Uuid::parse_str(reference.id()).map_err(|_| CryptoMapError::BadUuid(reference.id().to_string()))
}

/// Builds the [`ContentRef`] for a base `table` row keyed by `id`.
///
/// This is the inverse of [`content_ref_table`], used when emitting the
/// `to_encrypt`/`to_decrypt` folds for router-authored rows. Returns `None`
/// when `table` is not one of the sealable content tables.
pub(crate) fn record_id_to_content_ref(table: &str, id: Uuid) -> Option<ContentRef> {
    let id = id.to_string();
    Some(match table {
        "session_message" => ContentRef::Message(id),
        "session_tool_call" => ContentRef::ToolCall(id),
        "session_diff" => ContentRef::Diff(id),
        "session_plan" => ContentRef::Plan(id),
        "context_memory" => ContentRef::Memory(id),
        "trace_event" => ContentRef::Trace(id),
        "session_run_input" => ContentRef::RunInput(id),
        _ => return None,
    })
}

/// Reads the sealed payload of each referenced row into a `to_decrypt` map.
///
/// The orchestrator hands the map to the client, which opens each entry with the
/// session key. A reference whose row is missing or already in clear is skipped,
/// so only genuinely sealed rows are requested.
///
/// # Errors
///
/// Returns [`SessionError`](crate::session::SessionError) if a row cannot be read
/// or carries a malformed reference.
pub(crate) async fn build_to_decrypt(
    session: &UserSession,
    references: &[ContentRef],
) -> SessionResult<BTreeMap<ContentRef, EncryptedPayload>> {
    let mut map = BTreeMap::new();
    for reference in references {
        let table = content_ref_table(reference);
        let id = content_ref_uuid(reference).map_err(|error| {
            tracing::error!(%error, "rejecting decrypt request with a malformed reference");
            crate::session::SessionError::NotFound
        })?;
        if let Some(SecretContent::Encrypted(envelope)) = session.get_content(table, id).await? {
            map.insert(reference.clone(), envelope_to_payload(&envelope));
        }
    }
    tracing::debug!(rows = map.len(), "built decrypt request");
    Ok(map)
}

/// Converts a wire [`EncryptedPayload`] into a stored [`ContentEnvelope`].
pub(crate) fn payload_to_envelope(payload: &EncryptedPayload) -> ContentEnvelope {
    ContentEnvelope {
        version: payload.version,
        algorithm: payload.algorithm.clone(),
        key_id: payload.key_id.clone(),
        nonce: payload.nonce.clone(),
        ciphertext: payload.ciphertext.clone(),
    }
}

/// Converts a stored [`ContentEnvelope`] into a wire [`EncryptedPayload`].
pub(crate) fn envelope_to_payload(envelope: &ContentEnvelope) -> EncryptedPayload {
    EncryptedPayload {
        version: envelope.version,
        algorithm: envelope.algorithm.clone(),
        key_id: envelope.key_id.clone(),
        nonce: envelope.nonce.clone(),
        ciphertext: envelope.ciphertext.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_map_content_ref_to_table_and_uuid() {
        let id = Uuid::now_v7();
        let reference = ContentRef::Message(id.to_string());
        assert_eq!(content_ref_table(&reference), "session_message");
        assert_eq!(content_ref_uuid(&reference).unwrap(), id);
    }

    #[test]
    fn should_map_run_input_ref() {
        let id = Uuid::now_v7();
        let reference = ContentRef::RunInput(id.to_string());
        assert_eq!(content_ref_table(&reference), "session_run_input");
    }

    #[test]
    fn should_round_trip_record_id_to_content_ref() {
        let id = Uuid::now_v7();
        let reference = record_id_to_content_ref("session_tool_call", id).unwrap();
        assert_eq!(reference, ContentRef::ToolCall(id.to_string()));
    }

    #[test]
    fn should_reject_unknown_table() {
        assert!(record_id_to_content_ref("widget", Uuid::now_v7()).is_none());
    }

    #[test]
    fn should_reject_non_uuid_id() {
        let reference = ContentRef::Plan("not-a-uuid".to_string());
        assert!(matches!(
            content_ref_uuid(&reference),
            Err(CryptoMapError::BadUuid(_))
        ));
    }
}
