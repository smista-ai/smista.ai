//! Entity definitions for the smista.ai storage layer.
//!
//! Each table in the schema maps to a dedicated type implementing [`Table`].
//! Content-bearing entities are split across two types that share the same
//! UUIDv7 record id — a queryable metadata table (e.g. [`SessionMessage`]) and a
//! paired `_content` table (e.g. [`SessionMessageContent`]) holding only the
//! encryptable payload. See [`crate`] for the design invariants and
//! `docs/technical/schema.md` for the authoritative schema.
//!
//! Relations are explicit references: a session-scoped row stores its `session`
//! and (redundantly) its owning `user` as [`surrealdb::types::RecordId`] values.
//! SurrealDB does not enforce foreign keys, so a reference is just a record id;
//! cascade and cleanup are performed explicitly by the storage operations,
//! not by the database.

mod auth_token;
mod context_memory;
mod session;
mod session_approval;
mod session_context_reference;
mod session_diff;
mod session_message;
mod session_plan;
mod session_routing_decision;
mod session_run_input;
mod session_run_state;
mod session_tool_call;
mod trace_event;
mod user;
mod user_memory;

pub use self::auth_token::AuthToken;
pub use self::context_memory::{ContextMemory, ContextMemoryContent};
pub use self::session::Session;
pub use self::session_approval::SessionApproval;
pub use self::session_context_reference::SessionContextReference;
pub use self::session_diff::{DiffStatus, SessionDiff, SessionDiffContent};
pub use self::session_message::{SessionMessage, SessionMessageContent};
pub use self::session_plan::{PlanStatus, SessionPlan, SessionPlanContent};
pub use self::session_routing_decision::SessionRoutingDecision;
pub use self::session_run_input::{SessionRunInput, SessionRunInputContent};
pub use self::session_run_state::{
    ActiveTurn, ApprovalKind, PendingWrite, ResumeStep, RunPhase, RunState, ToolApproval, ToolWait,
};
pub use self::session_tool_call::{SessionToolCall, SessionToolCallContent, ToolCallStatus};
pub use self::trace_event::{TraceEvent, TraceEventContent, TraceEventType};
pub use self::user::User;
pub use self::user_memory::{UserMemory, UserMemoryContent};

/// Extracts the UUIDv7 key of a record id as a [`Uuid`].
///
/// Every entity's key is a UUIDv7 stored as the record id's key (see the schema
/// invariants at the [crate] root). This recovers it so callers outside the
/// storage layer can address a memory by [`Uuid`] without depending on
/// SurrealDB's [`RecordId`](surrealdb::types::RecordId) type. A non-UUID key
/// yields the nil UUID, which only happens for an externally tampered store.
fn record_uuid(id: &surrealdb::types::RecordId) -> uuid::Uuid {
    match &id.key {
        surrealdb::types::RecordIdKey::String(value) => {
            uuid::Uuid::parse_str(value).unwrap_or_default()
        }
        surrealdb::types::RecordIdKey::Uuid(uuid) => **uuid,
        _ => uuid::Uuid::default(),
    }
}

/// A typed handle to a SurrealDB table.
///
/// Every entity implements this trait to expose the name of the table it is
/// stored in, so storage operations and tests can address a table generically
/// without hard-coding its name at each call site.
pub trait Table {
    /// Returns the name of the table backing this entity.
    fn name() -> &'static str;

    /// Returns the SurrealDB [`Table`] corresponding to this entity.
    ///
    /// This function must be used when binding the table name in a query.
    #[inline(always)]
    fn table() -> surrealdb::types::Table {
        surrealdb::types::Table::from(Self::name())
    }
}
