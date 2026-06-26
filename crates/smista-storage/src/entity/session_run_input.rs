//! Session run-input tables.
//!
//! [`SessionRunInput`] persists the request context of a session's in-flight
//! run — the policy, local preferences and workspace snapshot — so every turn
//! of a multi-turn run can re-run the deterministic resolver without the client
//! re-sending it. A `continue` carries only the pause answer, so this row is the
//! cross-turn carrier of the original request.
//!
//! The non-secret metadata (policy, preferences, workspace paths and flags) is
//! stored in clear as JSON on [`SessionRunInput`]; the secret half (the input
//! text, attachments and git diff) lives in the paired
//! [`SessionRunInputContent`] table, which shares the same record id and may be
//! sealed for an encrypted session. A session has at most one in-flight run, so
//! the row is keyed by the session id and is 1:1 with the session; a write
//! overwrites it in place. Storage never interprets either payload.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Session, Table, User};
use crate::types::SecretContent;

/// The request context of a session's in-flight run, keyed by the session id.
///
/// The JSON-encoded `policy`, `local_preferences` and `workspace` fields are
/// non-secret metadata stored in clear; the secret half lives in
/// [`SessionRunInputContent`]. Both `session` and `user` are stored as explicit
/// references; `user` redundantly, so ownership checks never need a join.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionRunInput {
    /// Record id, sharing the owning session's UUIDv7 key.
    pub id: RecordId,
    /// Session the run belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Id of the run this context belongs to, correlating its continuations.
    pub run_id: String,
    /// The deterministic policy snapshot, serialized as JSON.
    pub policy: String,
    /// The client's local execution preferences, serialized as JSON.
    pub local_preferences: String,
    /// The workspace paths and flags (no git diff), serialized as JSON.
    pub workspace: String,
    /// Whether the run is in plan mode, persisted so it survives across turns
    /// without the client re-sending the plan command. Set when the run starts
    /// from a plan command and cleared when the plan is accepted.
    pub plan_active: bool,
    /// When the context was recorded.
    pub created_at: DateTime<Utc>,
}

impl SessionRunInput {
    /// Builds a run-input row for `session_id`, owned by `user_id`.
    ///
    /// The record key is the session id, so the row is 1:1 with the session and
    /// a later write replaces it in place. `plan_active` records whether the run
    /// began in plan mode.
    #[must_use]
    pub fn new(
        session_id: Uuid,
        user_id: Uuid,
        run_id: Uuid,
        policy: String,
        local_preferences: String,
        workspace: String,
        plan_active: bool,
    ) -> Self {
        Self {
            id: RecordId::new(Self::name(), session_id.to_string()),
            session: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            run_id: run_id.to_string(),
            policy,
            local_preferences,
            workspace,
            plan_active,
            created_at: Utc::now(),
        }
    }
}

impl Table for SessionRunInput {
    fn name() -> &'static str {
        "session_run_input"
    }
}

/// The secret half of a [`SessionRunInput`], paired 1:1 by record id.
///
/// Holds the run-input bundle (input text, attachments, instructions, skills and
/// git diff), serialized as JSON and stored in clear or sealed for an encrypted
/// session.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionRunInputContent {
    /// Record id, identical to the owning [`SessionRunInput`].
    pub id: RecordId,
    /// The run-input bundle, stored in clear or sealed for an encrypted session.
    pub content: SecretContent,
}

impl Table for SessionRunInputContent {
    fn name() -> &'static str {
        "session_run_input_content"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn should_store_and_read_run_input() {
        let session_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let session = RecordId::new(Session::name(), session_id.to_string());
        let user = RecordId::new(User::name(), user_id.to_string());
        let input = SessionRunInput::new(
            session_id,
            user_id,
            Uuid::now_v7(),
            r#"{"version":1}"#.to_string(),
            "{}".to_string(),
            "{}".to_string(),
            false,
        );

        crate::tests::fk_roundtrip(crate::tests::session(session, user), input).await;
    }

    #[tokio::test]
    async fn should_store_and_read_run_input_content() {
        let id = RecordId::new(SessionRunInputContent::name(), Uuid::now_v7().to_string());
        let content = SessionRunInputContent {
            id,
            content: SecretContent::plaintext(r#"{"input":"hi"}"#),
        };

        crate::tests::roundtrip(content).await;
    }
}
