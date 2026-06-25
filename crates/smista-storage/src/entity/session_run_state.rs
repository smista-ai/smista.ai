//! Session run-state table.
//!
//! [`RunState`] persists the execution state machine of a session's in-flight
//! run, so the router can pause between protocol turns and resume on the next
//! request without holding the run in memory. A session has at most one
//! in-flight run, so the row is keyed by the session id and there is at most one
//! per session; a write overwrites it in place.
//!
//! [`phase`](RunState::phase) is the durable checkpoint the run resumes from;
//! [`active`](RunState::active) is the orthogonal processing lock, present while
//! a turn is being served. Because the phase is never overwritten by the lock, a
//! crash mid-turn loses nothing: startup clears `active` and the run resumes from
//! its checkpoint. The row is metadata-only: every [`RunPhase`] variant carries
//! only references to rows already in storage, never raw content, so the row
//! never needs sealing. See `docs/technical/run-state-machine.md` for the run
//! lifecycle.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Session, Table, User};

/// The execution state of a session's in-flight run.
///
/// Keyed by the owning session's UUIDv7, so it is 1:1 with the session. `phase`
/// is the durable checkpoint the run resumes from; `active` is the processing
/// lock (`Some` while a turn is being served), orthogonal to the phase so a
/// crash never loses the checkpoint. Both `session` and `user` are stored as
/// explicit references; `user` redundantly, so ownership checks never need a
/// join.
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
    /// Zero-based count of turns served so far in this run.
    pub turn: u32,
    /// The durable checkpoint the run is paused at.
    pub phase: RunPhase,
    /// The processing lock: `Some` while a turn is being served. Cleared on
    /// startup, so a crashed lock never wedges the run.
    pub active: Option<ActiveTurn>,
    /// When the state was last written.
    pub updated_at: DateTime<Utc>,
}

impl RunState {
    /// Builds a run state for `session_id`, owned by `user_id`, in `phase`.
    ///
    /// Starts at turn zero with no processing lock; the record key is the
    /// session id, so the row is 1:1 with the session and a later write replaces
    /// it in place.
    #[must_use]
    pub fn new(session_id: Uuid, user_id: Uuid, run_id: Uuid, phase: RunPhase) -> Self {
        Self {
            id: RecordId::new(Self::name(), session_id.to_string()),
            session: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            run_id: run_id.to_string(),
            turn: 0,
            phase,
            active: None,
            updated_at: Utc::now(),
        }
    }
}

impl Table for RunState {
    fn name() -> &'static str {
        "session_run_state"
    }
}

/// The processing lock on a run: present while a turn is being served.
///
/// The presence of this value is the run's *Running* state. `lease` identifies
/// the worker that holds it, so a restarted or multi-instance router can tell a
/// live lock from a stale one and clear the stale ones.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct ActiveTurn {
    /// When the in-flight turn started.
    pub started_at: DateTime<Utc>,
    /// Identifier of the worker holding the lock (a UUIDv7 string).
    pub lease: String,
}

/// Where a run resumes once a wait is answered — the turn-loop program counter.
#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
pub enum ResumeStep {
    /// Plaintext is now available; finish building the prompt and invoke.
    BuildPrompt,
    /// The model is already selected and a gate is cleared; invoke it.
    Invoke,
    /// Ingest the just-delivered results, decision or input, then start the next
    /// turn (re-classify, resolve, invoke).
    NextTurn,
    /// Write the sealed content rows, then go idle.
    Finalize,
}

/// One outstanding tool call a run is blocked on.
///
/// Carries only the reference and the approval requirement; the arguments live
/// in the `session_tool_call` row, sealed when the session is encrypted.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct ToolWait {
    /// Reference to the `session_tool_call` row, correlating the later result.
    pub call_id: String,
    /// Whether the user must approve the call before it runs.
    pub requires_approval: ToolApproval,
}

/// Whether a client-executed tool needs the user's approval first.
///
/// A tool denied by policy never reaches the client, so `deny` is not a value
/// here. Mirrors the core API type, kept storage-local so the storage crate
/// stays self-contained.
#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
pub enum ToolApproval {
    /// Run the tool without confirmation.
    Allow,
    /// Confirm with the user before running the tool.
    Ask,
}

/// A checkpoint of the run execution state machine.
///
/// Each `Awaiting*` variant records what the run waits for and, via `resume`,
/// where it continues once the wait is answered. Every variant carries only
/// references to rows already in storage, never raw content, so the state stays
/// plain metadata and never needs sealing.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub enum RunPhase {
    /// Nothing outstanding: the run completed, errored, or was interrupted with
    /// no follow-up. Reading no row at all means the same thing.
    Idle,
    /// Blocked on the client opening sealed records before the prompt can be
    /// built. Each reference locates a row by its table name.
    AwaitingDecrypt {
        /// The sealed records to open (one or more).
        records: Vec<RecordId>,
        /// Where the run resumes once the plaintext is supplied.
        resume: ResumeStep,
    },
    /// Blocked on one or more client-run tools.
    AwaitingTool {
        /// The outstanding `allow`/`ask` calls; `deny` never pauses.
        calls: Vec<ToolWait>,
        /// Where the run resumes once the results arrive.
        resume: ResumeStep,
        /// Router-authored rows the client must seal alongside the results
        /// (assistant message, tool arguments). Empty unless encrypted.
        to_seal: Vec<RecordId>,
    },
    /// Blocked on a yes/no decision with no tool to run.
    AwaitingApproval {
        /// Identifier the client echoes back with its decision.
        approval_id: String,
        /// What kind of approval is being requested.
        kind: ApprovalKind,
        /// Opaque, non-secret detail the orchestrator re-emits to the client,
        /// serialized as JSON. Storage neither reads nor interprets it.
        detail: String,
        /// Where the run resumes once the decision arrives.
        resume: ResumeStep,
        /// Router-authored rows the client must seal alongside the decision
        /// (for example a plan snapshot). Empty unless encrypted.
        to_seal: Vec<RecordId>,
    },
    /// Blocked on the client sealing router-authored content before it can be
    /// persisted, with no other outstanding work.
    AwaitingEncrypt {
        /// The records whose content must be sealed (one or more).
        records: Vec<RecordId>,
        /// Where the run resumes once the ciphertext arrives.
        resume: ResumeStep,
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

    fn record(table: &str) -> RecordId {
        RecordId::new(table, uuid::Uuid::now_v7().to_string())
    }

    #[tokio::test]
    async fn should_roundtrip_idle_phase() {
        crate::tests::value_roundtrip(RunPhase::Idle).await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_decrypt_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingDecrypt {
            records: vec![record("session_message"), record("session_message")],
            resume: ResumeStep::BuildPrompt,
        })
        .await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_tool_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingTool {
            calls: vec![ToolWait {
                call_id: "c1".to_string(),
                requires_approval: ToolApproval::Ask,
            }],
            resume: ResumeStep::NextTurn,
            to_seal: vec![record("session_message")],
        })
        .await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_approval_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingApproval {
            approval_id: "a1".to_string(),
            kind: ApprovalKind::Plan,
            detail: r#"{"plan":"p1"}"#.to_string(),
            resume: ResumeStep::NextTurn,
            to_seal: vec![record("session_plan")],
        })
        .await;
    }

    #[tokio::test]
    async fn should_roundtrip_awaiting_encrypt_phase() {
        crate::tests::value_roundtrip(RunPhase::AwaitingEncrypt {
            records: vec![record("session_message")],
            resume: ResumeStep::Finalize,
        })
        .await;
    }

    #[tokio::test]
    async fn should_roundtrip_active_turn() {
        crate::tests::value_roundtrip(ActiveTurn {
            started_at: Utc::now(),
            lease: uuid::Uuid::now_v7().to_string(),
        })
        .await;
    }

    #[tokio::test]
    async fn should_default_new_run_state_to_idle_lock_free_turn_zero() {
        let session_id = uuid::Uuid::now_v7();
        let user_id = uuid::Uuid::now_v7();
        let state = RunState::new(session_id, user_id, uuid::Uuid::now_v7(), RunPhase::Idle);
        assert_eq!(state.turn, 0);
        assert_eq!(state.active, None);
        assert_eq!(state.phase, RunPhase::Idle);
    }

    #[tokio::test]
    async fn should_store_and_read_run_state() {
        let session_id = uuid::Uuid::now_v7();
        let user_id = uuid::Uuid::now_v7();
        let session = RecordId::new(Session::name(), session_id.to_string());
        let user = RecordId::new(User::name(), user_id.to_string());
        let mut state = RunState::new(
            session_id,
            user_id,
            uuid::Uuid::now_v7(),
            RunPhase::AwaitingTool {
                calls: Vec::new(),
                resume: ResumeStep::NextTurn,
                to_seal: Vec::new(),
            },
        );
        state.active = Some(ActiveTurn {
            started_at: Utc::now(),
            lease: uuid::Uuid::now_v7().to_string(),
        });

        crate::tests::fk_roundtrip(crate::tests::session(session, user), state).await;
    }
}
