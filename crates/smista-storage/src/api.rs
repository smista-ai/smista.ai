//! Storage API types, which are types used by the storage, but are not themselves persisted entities.

use uuid::Uuid;

use crate::entity::{
    Session, SessionApproval, SessionContextReference, SessionDiff, SessionMessage, SessionPlan,
    SessionRoutingDecision, SessionToolCall,
};

/// Selects an existing memory row to update, either by its id or by its key.
///
/// Keyed memory rows are unique per owner, so an update may target the key
/// directly without first resolving its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryRef {
    /// Target the row with this record id.
    Id(Uuid),
    /// Target the owner's keyed row with this key.
    Key(String),
}

/// The full persisted state of a session.
///
/// Returned by [`Database::get_session_state`](crate::Database::get_session_state);
/// aggregates the session row with
/// every child row that belongs to it. Content payloads stored in the paired
/// `_content` tables are not included — this is the queryable metadata view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionState {
    /// The session itself.
    pub session: Session,
    /// Messages exchanged during the session, oldest first.
    pub messages: Vec<SessionMessage>,
    /// Routing decisions recorded during the session.
    pub routing_decisions: Vec<SessionRoutingDecision>,
    /// Context references selected or excluded during the session.
    pub context_references: Vec<SessionContextReference>,
    /// Tool calls requested during the session.
    pub tool_calls: Vec<SessionToolCall>,
    /// Plans generated or approved during the session.
    pub plans: Vec<SessionPlan>,
    /// Diffs proposed or applied during the session.
    pub diffs: Vec<SessionDiff>,
    /// Approval decisions recorded during the session.
    pub approvals: Vec<SessionApproval>,
}
