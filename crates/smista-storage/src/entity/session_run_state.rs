//! Session run-state table.
//!
//! [`RunState`] persists the execution state machine of a session's in-flight
//! run, so the router can pause between protocol turns and resume on the next
//! request without holding the run in memory. A session has at most one
//! in-flight run, so the row is keyed by the session id and there is at most one
//! per session; a write overwrites it in place. It is metadata-only: every
//! [`RunPhase`] variant carries only references to rows already in storage,
//! never raw content, so the row never needs sealing. See
//! `docs/technical/execution-protocol.md` for the run lifecycle.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Session, Table, User};

/// The execution state of a session's in-flight run.
///
/// Keyed by the owning session's UUIDv7, so it is 1:1 with the session. Both
/// `session` and `user` are stored as explicit references; `user` redundantly,
/// so ownership checks never need a join.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct RunState {
    /// Record id, sharing the owning session's UUIDv7 key.
    pub id: RecordId,
    /// Session the run belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Id of the in-flight run, correlating its continuations.
    pub run_id: String,
    /// The phase the run is paused in.
    pub phase: RunPhase,
    /// When the state was last written.
    pub updated_at: DateTime<Utc>,
}

impl RunState {
    /// Builds a run state for `session_id`, owned by `user_id`, in `phase`.
    ///
    /// The record key is the session id, so the row is 1:1 with the session and
    /// a later write replaces it in place.
    #[must_use]
    pub fn new(session_id: Uuid, user_id: Uuid, run_id: Uuid, phase: RunPhase) -> Self {
        Self {
            id: RecordId::new(Self::name(), session_id.to_string()),
            session: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            run_id: run_id.to_string(),
            phase,
            updated_at: Utc::now(),
        }
    }
}

impl Table for RunState {
    fn name() -> &'static str {
        "session_run_state"
    }
}

/// A phase of the run execution state machine.
///
/// Each `Awaiting*` variant carries only references to rows already in storage,
/// never raw content, so the state stays plain metadata and never needs sealing.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub enum RunPhase {
    /// Nothing outstanding: the run completed, errored, or was interrupted with
    /// no follow-up. Reading no row at all means the same thing.
    Idle,
    /// A turn is in flight.
    Running {
        /// Zero-based index of the turn being served.
        turn: u32,
        /// When the turn started.
        started_at: DateTime<Utc>,
    },
    /// Blocked on one or more client-run tools. The outstanding calls are the
    /// `session_tool_call` rows still marked requested, so none are stored here.
    AwaitingTool,
    /// Blocked on a yes/no decision with no tool to run.
    AwaitingApproval {
        /// Identifier the client echoes back with its decision.
        approval_id: String,
        /// What kind of approval is being requested.
        kind: ApprovalKind,
        /// Opaque, non-secret detail the orchestrator re-emits to the client,
        /// serialized as JSON. Storage neither reads nor interprets it.
        detail: String,
    },
    /// Blocked on the client opening one sealed record before the prompt can be
    /// built. The reference locates the row, since it carries its table name.
    AwaitingDecrypt {
        /// The sealed record to open.
        record_id: RecordId,
    },
    /// Blocked on the client sealing one record before it can be persisted.
    AwaitingEncrypt {
        /// The record whose content must be sealed.
        record_id: RecordId,
    },
}

/// The kind of a standalone approval that has no tool to run.
#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
pub enum ApprovalKind {
    /// Disclosing context to a remote provider.
    RemoteDisclosure,
    /// Confirming a cost limit.
    CostLimit,
    /// Accepting or rejecting a generated plan before execution begins.
    Plan,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_roundtrip_idle_phase() {
        crate::tests::value_roundtrip(RunPhase::Idle).await;
    }

    #[tokio::test]
    async fn should_roundtrip_running_phase() {
        crate::tests::value_roundtrip(RunPhase::Running {
            turn: 3,
            started_at: Utc::now(),
        })
        .await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_tool_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingTool).await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_approval_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingApproval {
            approval_id: "a1".to_string(),
            kind: ApprovalKind::RemoteDisclosure,
            detail: r#"{"provider":"anthropic"}"#.to_string(),
        })
        .await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_decrypt_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingDecrypt {
            record_id: RecordId::new("session_message", uuid::Uuid::now_v7().to_string()),
        })
        .await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_encrypt_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingEncrypt {
            record_id: RecordId::new("session_message", uuid::Uuid::now_v7().to_string()),
        })
        .await;
    }

    #[tokio::test]
    async fn should_store_and_read_run_state() {
        let session_id = uuid::Uuid::now_v7();
        let user_id = uuid::Uuid::now_v7();
        let session = RecordId::new(Session::name(), session_id.to_string());
        let user = RecordId::new(User::name(), user_id.to_string());
        let state = RunState::new(
            session_id,
            user_id,
            uuid::Uuid::now_v7(),
            RunPhase::AwaitingTool,
        );

        crate::tests::fk_roundtrip(crate::tests::session(session, user), state).await;
    }
}
