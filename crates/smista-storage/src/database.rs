//! The storage [`Database`] interface.
//!
//! Application code depends on this trait, never on SurrealDB directly; the
//! SurrealDB-backed implementation lands behind this boundary (see the crate
//! root for the schema invariants). Every method is asynchronous and its
//! returned future is `Send`, so the trait composes with `axum` and other
//! Tokio-based services.
//!
//! The trait is intentionally a single interface rather than one trait per
//! entity: some operations span several tables in one transaction (for example
//! [`Database::delete_session`] removes a session together with its messages,
//! tool calls and per-session memory), so they cannot be expressed as
//! independent per-table traits.
//!
//! ## Identity and ownership
//!
//! Records are addressed by their [`Uuid`] key — the router owns id generation
//! (`Uuid::now_v7`), so ids are portable and time-sortable. Every user-scoped
//! operation takes the authenticated `user_id`; the implementation rejects
//! cross-user access by treating a record owned by another user as absent —
//! a read returns `None` and a write returns [`StorageError::NotFound`](crate::StorageError::NotFound),
//! never disclosing that the record exists.
//!
//! ## Trace events
//!
//! Trace events are persisted like any other entity:
//! [`Database::append_trace_event`] records a [`TraceEvent`] with its paired
//! content, and [`Database::get_session_trace_events`] returns the assembled
//! [`Trace`] read view defined in `smista-core`.

pub mod surreal;

use std::future::Future;

use smista_core::trace::Trace;
use uuid::Uuid;

use crate::StorageResult;
use crate::api::{MemoryRef, SessionState};
use crate::entity::{
    AuthToken, ContextMemory, ContextMemoryContent, Session, SessionApproval,
    SessionContextReference, SessionDiff, SessionDiffContent, SessionMessage,
    SessionMessageContent, SessionPlan, SessionPlanContent, SessionRoutingDecision,
    SessionToolCall, SessionToolCallContent, TraceEvent, TraceEventContent, User, UserMemory,
    UserMemoryContent,
};

/// The storage interface for smista.ai.
///
/// Implementors persist the domain entities defined in [`crate::entity`] and
/// enforce the schema invariants documented at the crate root. All methods are
/// asynchronous; their futures are `Send` so they can be awaited from any
/// Tokio task.
///
/// See the [crate documentation](crate) for the identity, ownership and trace
/// conventions that apply to every method.
pub trait Database: Send + Sync {
    // -- Users ---------------------------------------------------------------

    /// Persists a new user and returns the stored row.
    fn create_user(&self, user: User) -> impl Future<Output = StorageResult<User>> + Send;

    /// Loads a user by id, or `None` if no such user exists.
    ///
    /// API keys embed their owner's id, so the authentication layer parses the
    /// id from the presented key, loads the user here and verifies the key
    /// against the stored hash; there is no lookup by hash.
    fn get_user(&self, id: Uuid) -> impl Future<Output = StorageResult<Option<User>>> + Send;

    // -- Tokens --------------------------------------------------------------

    /// Persists a new authentication token and returns the stored row.
    fn create_token(
        &self,
        token: AuthToken,
    ) -> impl Future<Output = StorageResult<AuthToken>> + Send;

    /// Returns the active token with the given id, if any.
    ///
    /// A token is active when it exists, has not expired and has not been
    /// revoked; otherwise `None` is returned. The full row is returned so the
    /// caller can verify the presented secret against the stored hash and scope
    /// the request to the owning user.
    fn get_active_token(
        &self,
        token_id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<AuthToken>>> + Send;

    /// Returns the token with the given id whatever its state, if it exists.
    ///
    /// Unlike [`Database::get_active_token`], this does not filter out expired or
    /// revoked tokens. The authentication layer uses it to tell an expired or
    /// revoked token apart from one that was never issued, so each can be
    /// reported with its own error.
    fn get_token(
        &self,
        token_id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<AuthToken>>> + Send;

    /// Revokes the token with the given id.
    fn revoke_token(&self, id: Uuid) -> impl Future<Output = StorageResult<()>> + Send;

    // -- Sessions ------------------------------------------------------------

    /// Persists a new session and returns the stored row.
    fn create_session(
        &self,
        session: Session,
    ) -> impl Future<Output = StorageResult<Session>> + Send;

    /// Loads a session owned by `user_id`, or `None` if absent.
    ///
    /// A session owned by a different user is treated as absent rather than
    /// disclosed.
    fn get_session(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<Session>>> + Send;

    /// Lists the sessions owned by `user_id`, most recently updated first.
    fn list_sessions(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = StorageResult<Vec<Session>>> + Send;

    /// Updates a session owned by `user_id` and returns the stored row.
    fn update_session(
        &self,
        user_id: Uuid,
        session: Session,
    ) -> impl Future<Output = StorageResult<Session>> + Send;

    /// Marks a session owned by `user_id` as archived.
    fn archive_session(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Deletes a session owned by `user_id` and all of its child rows.
    ///
    /// The cascade is explicit: messages, routing decisions, context
    /// references, tool calls, plans, diffs and per-session [`ContextMemory`]
    /// (each with its paired `_content` row) are removed in the same
    /// transaction. SurrealDB does not enforce foreign keys, so nothing
    /// cascades automatically.
    fn delete_session(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    // -- Append --------------------------------------------------------------

    /// Appends a message together with its paired content body.
    ///
    /// The `content` row shares the message's record id; both are written in
    /// the same transaction.
    fn append_message(
        &self,
        user_id: Uuid,
        message: SessionMessage,
        content: SessionMessageContent,
    ) -> impl Future<Output = StorageResult<SessionMessage>> + Send;

    /// Appends a routing decision to its session.
    fn append_routing_decision(
        &self,
        user_id: Uuid,
        decision: SessionRoutingDecision,
    ) -> impl Future<Output = StorageResult<SessionRoutingDecision>> + Send;

    /// Appends a context reference to its session.
    fn append_context_reference(
        &self,
        user_id: Uuid,
        reference: SessionContextReference,
    ) -> impl Future<Output = StorageResult<SessionContextReference>> + Send;

    /// Appends a tool call together with its paired, sanitised content.
    ///
    /// The `content` row shares the tool call's record id; both are written in
    /// the same transaction.
    fn append_tool_call(
        &self,
        user_id: Uuid,
        tool_call: SessionToolCall,
        content: SessionToolCallContent,
    ) -> impl Future<Output = StorageResult<SessionToolCall>> + Send;

    /// Appends a plan together with its paired snapshot content.
    fn append_plan(
        &self,
        user_id: Uuid,
        plan: SessionPlan,
        content: SessionPlanContent,
    ) -> impl Future<Output = StorageResult<SessionPlan>> + Send;

    /// Appends a diff together with its paired, secret-filtered body.
    fn append_diff(
        &self,
        user_id: Uuid,
        diff: SessionDiff,
        content: SessionDiffContent,
    ) -> impl Future<Output = StorageResult<SessionDiff>> + Send;

    /// Appends an approval decision to its session.
    fn append_approval(
        &self,
        user_id: Uuid,
        approval: SessionApproval,
    ) -> impl Future<Output = StorageResult<SessionApproval>> + Send;

    /// Appends a trace event together with its paired payload.
    ///
    /// The `content` row shares the event's record id; both are written in the
    /// same transaction.
    fn append_trace_event(
        &self,
        user_id: Uuid,
        event: TraceEvent,
        content: TraceEventContent,
    ) -> impl Future<Output = StorageResult<TraceEvent>> + Send;

    // -- Reads ---------------------------------------------------------------

    /// Loads the full state of a session owned by `user_id`, or `None`.
    ///
    /// Returns the session metadata together with every child row; see
    /// [`SessionState`]. Content payloads are not included.
    fn get_session_state(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<SessionState>>> + Send;

    /// Loads every trace event for a session owned by `user_id`, assembled into
    /// a [`Trace`].
    ///
    /// Returns the [`Trace`] read view built from all of the session's trace
    /// events, oldest first, or `None` if the session has no events or is not
    /// owned by `user_id`.
    fn get_session_trace_events(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<Trace>>> + Send;

    // -- User memory ---------------------------------------------------------

    /// Records a user memory together with its paired content.
    ///
    /// A keyed memory upserts on `(user, key)`: an existing fact with the same
    /// key is replaced. A keyless memory is always inserted as a new row.
    fn record_user_memory(
        &self,
        user_id: Uuid,
        memory: UserMemory,
        content: UserMemoryContent,
    ) -> impl Future<Output = StorageResult<UserMemory>> + Send;

    /// Updates the content of a user memory owned by `user_id`.
    ///
    /// The target row is selected by id or by key (see [`MemoryRef`]); the
    /// `content` payload replaces the existing fact.
    fn update_user_memory(
        &self,
        user_id: Uuid,
        target: MemoryRef,
        content: UserMemoryContent,
    ) -> impl Future<Output = StorageResult<UserMemory>> + Send;

    /// Loads a user memory owned by `user_id`, or `None` if absent.
    fn get_user_memory(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<UserMemory>>> + Send;

    /// Lists the memories owned by `user_id`.
    fn list_user_memory(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = StorageResult<Vec<UserMemory>>> + Send;

    /// Lists the memories owned by `user_id`, each paired with its stored
    /// content, most recently updated first.
    ///
    /// Unlike [`Self::list_user_memory`], this also loads the paired
    /// `_content` row, so a caller can render the remembered facts without a
    /// second read per memory.
    fn list_user_memory_with_content(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = StorageResult<Vec<(UserMemory, UserMemoryContent)>>> + Send;

    /// Forgets a user memory owned by `user_id`, with its paired content.
    fn forget_user_memory(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    // -- Context memory ------------------------------------------------------

    /// Records a per-session context memory together with its paired content.
    ///
    /// A keyed memory upserts on `(session, key)`; a keyless memory is always
    /// inserted as a new row.
    fn record_context_memory(
        &self,
        user_id: Uuid,
        memory: ContextMemory,
        content: ContextMemoryContent,
    ) -> impl Future<Output = StorageResult<ContextMemory>> + Send;

    /// Updates the content of a context memory owned by `user_id` in `session_id`.
    ///
    /// The target row is selected by id or by key (see [`MemoryRef`]).
    fn update_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        target: MemoryRef,
        content: ContextMemoryContent,
    ) -> impl Future<Output = StorageResult<ContextMemory>> + Send;

    /// Loads a context memory owned by `user_id` in `session_id`, or `None`.
    fn get_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<ContextMemory>>> + Send;

    /// Lists the context memories owned by `user_id` in `session_id`.
    fn list_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> impl Future<Output = StorageResult<Vec<ContextMemory>>> + Send;

    /// Lists the context memories owned by `user_id` in `session_id`, each
    /// paired with its stored content, most recently updated first.
    ///
    /// The content of an encrypted session is returned sealed as a
    /// [`SecretContent::Encrypted`](crate::types::SecretContent::Encrypted)
    /// envelope; storage holds no key and never decrypts it.
    fn list_context_memory_with_content(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> impl Future<Output = StorageResult<Vec<(ContextMemory, ContextMemoryContent)>>> + Send;

    /// Forgets a context memory owned by `user_id`, with its paired content.
    fn forget_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    // -- Cleanup ------------------------------------------------------------

    /// Deletes all expired or revoked auth tokens.
    fn delete_expired_tokens(&self) -> impl Future<Output = StorageResult<()>> + Send;

    /// Purge all sessions older than `session_retention_days` and their child rows.
    ///
    /// Skips archived sessions, which are purged separately by [`Database::purge_archived_sessions`].
    fn purge_old_sessions(
        &self,
        session_retention_days: u32,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Purge all the archived sessions older than `archived_session_retention_days` and their child rows.
    fn purge_archived_sessions(
        &self,
        archived_session_retention_days: u32,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Purge all the deleted sessions older than `deleted_session_retention_days` and their child rows.
    fn purge_traces(
        &self,
        trace_retention_days: u32,
    ) -> impl Future<Output = StorageResult<()>> + Send;
}
