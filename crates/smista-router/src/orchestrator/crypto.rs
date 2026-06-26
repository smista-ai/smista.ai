//! Mapping between a [`ContentRef`] and the storage row it addresses.
//!
//! The end-to-end-encryption fold keys every sealed payload by a [`ContentRef`]
//! of the form `kind:id`. This module is the single source of truth translating
//! that reference into the base (metadata) table plus the record uuid it
//! addresses, and back. The sealed payload itself lives in the paired
//! `<table>_content` row under the same uuid key.
#![allow(
    dead_code,
    reason = "the crypto fold helpers are wired into the turn loop in a later orchestrator task"
)]

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

/// A router-authored row whose content must be sealed before persistence.
pub(crate) struct Authored {
    /// The base content table the row lives under.
    pub(crate) table: &'static str,
    /// The row's record uuid.
    pub(crate) id: Uuid,
    /// The plaintext content to seal.
    pub(crate) plaintext: String,
}

/// Builds the `to_encrypt` response fold from router-authored rows.
///
/// Each authored row becomes a `ContentRef -> plaintext` entry the client seals
/// and returns. A row whose table is not sealable is skipped.
pub(crate) fn to_encrypt_map(authored: &[Authored]) -> BTreeMap<ContentRef, String> {
    authored
        .iter()
        .filter_map(|row| {
            record_id_to_content_ref(row.table, row.id)
                .map(|reference| (reference, row.plaintext.clone()))
        })
        .collect()
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

/// Applies the client-sealed content to storage, one row per entry.
///
/// For each `(ContentRef, EncryptedPayload)` the paired `_content` row is sealed
/// via [`UserSession::set_content`]. A reference with a malformed uuid is
/// rejected.
///
/// # Errors
///
/// Returns [`SessionError`](crate::session::SessionError) if a row cannot be
/// written, is not owned by the session's user, or carries a malformed
/// reference.
pub(crate) async fn apply_sealed(
    session: &UserSession,
    sealed: &BTreeMap<ContentRef, EncryptedPayload>,
) -> SessionResult<()> {
    for (reference, payload) in sealed {
        let table = content_ref_table(reference);
        let id = match content_ref_uuid(reference) {
            Ok(id) => id,
            Err(error) => {
                tracing::error!(%error, "rejecting sealed row with a malformed reference");
                return Err(crate::session::SessionError::NotFound);
            }
        };
        session
            .set_content(
                table,
                id,
                SecretContent::Encrypted(payload_to_envelope(payload)),
            )
            .await?;
    }
    tracing::debug!(rows = sealed.len(), "applied sealed content");
    Ok(())
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

    #[test]
    fn should_build_to_encrypt_from_authored_rows() {
        let id = Uuid::now_v7();
        let map = to_encrypt_map(&[Authored {
            table: "session_message",
            id,
            plaintext: "hi".to_string(),
        }]);
        assert_eq!(
            map.get(&ContentRef::Message(id.to_string()))
                .map(String::as_str),
            Some("hi")
        );
    }

    #[tokio::test]
    async fn should_apply_sealed_content_to_storage() {
        use chrono::Utc;
        use smista_core::message::MessageRole;
        use smista_core::model::Provider;
        use smista_storage::database::Database as _;
        use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
        use smista_storage::entity::{Session, SessionMessage, SessionMessageContent, Table, User};
        use smista_storage::surrealdb::RecordId;

        use crate::session::Sessions;

        let db = SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database");
        let user_id = Uuid::now_v7();
        db.create_user(User {
            id: RecordId::new(User::name(), user_id.to_string()),
            api_key_hash: format!("hash-{user_id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("failed to create user");

        let sessions = Sessions::new(db, user_id);
        let session_id = Uuid::now_v7();
        let session = sessions
            .create(Session {
                id: RecordId::new(Session::name(), session_id.to_string()),
                user: RecordId::new(User::name(), user_id.to_string()),
                title: None,
                encrypted: true,
                key_id: Some("kf_test".to_string()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                archived_at: None,
            })
            .await
            .expect("failed to create session");

        let id = Uuid::now_v7();
        session
            .append_message(
                SessionMessage {
                    id: RecordId::new(SessionMessage::name(), id.to_string()),
                    session: RecordId::new(Session::name(), session_id.to_string()),
                    user: RecordId::new(User::name(), user_id.to_string()),
                    role: MessageRole::Assistant,
                    provider: Provider::Anthropic,
                    model: "claude".to_string(),
                    created_at: Utc::now(),
                },
                SessionMessageContent {
                    id: RecordId::new(SessionMessageContent::name(), id.to_string()),
                    content: SecretContent::plaintext("placeholder"),
                },
            )
            .await
            .expect("failed to append message");

        let mut sealed = BTreeMap::new();
        sealed.insert(
            ContentRef::Message(id.to_string()),
            EncryptedPayload {
                version: 1,
                algorithm: "xchacha20poly1305".to_string(),
                key_id: "kf_test".to_string(),
                nonce: "bm9uY2U".to_string(),
                ciphertext: "Y2lwaGVydGV4dA".to_string(),
            },
        );
        apply_sealed(&session, &sealed)
            .await
            .expect("failed to apply sealed content");

        let got = session
            .get_content("session_message", id)
            .await
            .expect("failed to read content")
            .expect("content row missing");
        assert!(got.is_encrypted());
    }
}
