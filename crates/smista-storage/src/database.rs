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

use smista_core::trace::{Trace, TraceEvent as CoreTraceEvent};
use uuid::Uuid;

use crate::StorageResult;
use crate::api::{MemoryRef, Pagination, SessionState};
use crate::entity::{
    AuthToken, ContextMemory, ContextMemoryContent, DiffStatus, PlanStatus, RunState, Session,
    SessionApproval, SessionContextReference, SessionDiff, SessionDiffContent, SessionMessage,
    SessionMessageContent, SessionPlan, SessionPlanContent, SessionRoutingDecision,
    SessionRunInput, SessionRunInputContent, SessionToolCall, SessionToolCallContent,
    ToolCallStatus, TraceEvent, TraceEventContent, User, UserMemory, UserMemoryContent,
};
use crate::types::SecretContent;

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

    // -- Run state -----------------------------------------------------------

    /// Sets the in-flight run state for a session, overwriting any existing row.
    ///
    /// The row is keyed by the session id, so each call replaces the session's
    /// single run-state row in place; the stored row is returned. Fails with
    /// [`StorageError::NotFound`](crate::StorageError::NotFound) when the session
    /// is not owned by `user_id`.
    fn set_run_state(
        &self,
        user_id: Uuid,
        state: RunState,
    ) -> impl Future<Output = StorageResult<RunState>> + Send;

    /// Atomically acquires the run lock for a session.
    ///
    /// Writes `state` — which must carry an `active` lock — only if no lock is
    /// currently held: the session has no run-state row, or the row's `active`
    /// is `None`. Returns the stored state when the lock is granted, or `None`
    /// when a turn already holds it. The check and the write commit as one
    /// transaction, so two concurrent requests can never both acquire. Fails
    /// with [`StorageError::NotFound`](crate::StorageError::NotFound) when the
    /// session is not owned by `user_id`.
    fn acquire_run_lock(
        &self,
        user_id: Uuid,
        state: RunState,
    ) -> impl Future<Output = StorageResult<Option<RunState>>> + Send;

    /// Loads the in-flight run state for a session owned by `user_id`.
    ///
    /// Returns `None` when the session has no in-flight run, which is the idle
    /// state. A row owned by a different user is treated as absent.
    fn get_run_state(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<RunState>>> + Send;

    /// Persists a run's request context, overwriting any existing row.
    ///
    /// The metadata row and its paired content row are written in one
    /// transaction, both keyed by the session id, so each call replaces the
    /// session's single run-input row in place; the stored metadata row is
    /// returned. Fails with
    /// [`StorageError::NotFound`](crate::StorageError::NotFound) when the session
    /// is not owned by `user_id`.
    fn set_run_input(
        &self,
        user_id: Uuid,
        input: SessionRunInput,
        content: SessionRunInputContent,
    ) -> impl Future<Output = StorageResult<SessionRunInput>> + Send;

    /// Loads a run's request context together with its content, or `None`.
    ///
    /// A row owned by a different user is treated as absent. Used to re-run the
    /// deterministic resolver on every turn of a multi-turn run.
    fn get_run_input(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<(SessionRunInput, SessionRunInputContent)>>> + Send;

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

    // -- Status transitions --------------------------------------------------

    /// Moves a stored plan owned by `user_id` into `status`, returning the row.
    ///
    /// The plan keeps every other field; only its `status` and `updated_at`
    /// change, and `approved_at` is stamped when `status` is
    /// [`PlanStatus::Approved`] and cleared otherwise. An append leaves a plan
    /// at [`PlanStatus::Draft`]; this is the only way to reach the approved or
    /// rejected state. Fails with
    /// [`StorageError::NotFound`](crate::StorageError::NotFound) when the plan is
    /// absent or owned by another user.
    fn set_plan_status(
        &self,
        user_id: Uuid,
        plan_id: Uuid,
        status: PlanStatus,
    ) -> impl Future<Output = StorageResult<SessionPlan>> + Send;

    /// Moves a stored diff owned by `user_id` into `status`, returning the row.
    ///
    /// The diff keeps every other field; only its `status` changes, and
    /// `applied_at` is stamped when `status` is [`DiffStatus::Applied`] and
    /// cleared otherwise. An append leaves a diff at [`DiffStatus::Proposed`];
    /// this is the only way to reach the applied or rejected state. Fails with
    /// [`StorageError::NotFound`](crate::StorageError::NotFound) when the diff is
    /// absent or owned by another user.
    fn set_diff_status(
        &self,
        user_id: Uuid,
        diff_id: Uuid,
        status: DiffStatus,
    ) -> impl Future<Output = StorageResult<SessionDiff>> + Send;

    /// Records the outcome of a tool call owned by `user_id`, returning the row.
    ///
    /// Moves the call into `status` and stamps `completed_at` for a terminal
    /// status ([`ToolCallStatus::Completed`] or [`ToolCallStatus::Failed`]),
    /// while writing the sanitised `result` and `error` onto the paired content
    /// row and preserving its arguments. An append leaves a call at
    /// [`ToolCallStatus::Pending`]; this is how the orchestrator records the
    /// result the client returned. Fails with
    /// [`StorageError::NotFound`](crate::StorageError::NotFound) when the call is
    /// absent or owned by another user.
    fn set_tool_call_outcome(
        &self,
        user_id: Uuid,
        call_id: Uuid,
        status: ToolCallStatus,
        result: Option<SecretContent>,
        error: Option<SecretContent>,
    ) -> impl Future<Output = StorageResult<SessionToolCall>> + Send;

    /// Overwrites the `<table>_content` payload for the row keyed by `id`.
    ///
    /// The base `<table>` row must be owned by `user_id`; ownership is verified
    /// against its `user` column before the write. This is how a router-authored
    /// row whose metadata was written up front is sealed once its ciphertext
    /// returns on a later continuation. `table` must name one of the content
    /// tables; any other value fails with
    /// [`StorageError::NotFound`](crate::StorageError::NotFound), as does a base
    /// row that is absent or owned by another user.
    fn set_content(
        &self,
        user_id: Uuid,
        table: &str,
        id: Uuid,
        content: SecretContent,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Reads the `<table>_content` payload for the row keyed by `id`, or `None`.
    ///
    /// Used to build the `to_decrypt` map. The base `<table>` row must be owned
    /// by `user_id`; a row that is absent or owned by another user reads as
    /// `None`, so the read never reveals it exists. `table` must name one of the
    /// content tables; any other value fails with
    /// [`StorageError::NotFound`](crate::StorageError::NotFound).
    fn get_content(
        &self,
        user_id: Uuid,
        table: &str,
        id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<SecretContent>>> + Send;

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

    /// Lists the messages of a session owned by `user_id`, each paired with its
    /// stored content, oldest first.
    ///
    /// The chronological order is the session's conversation history, the shape
    /// the router's context selection recalls. Like
    /// [`Self::list_context_memory_with_content`], the content of an encrypted
    /// session is returned sealed as a
    /// [`SecretContent::Encrypted`]
    /// envelope; storage holds no key and never decrypts it. A session owned by
    /// a different user, or one with no messages, yields an empty list.
    fn list_session_messages_with_content(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> impl Future<Output = StorageResult<Vec<(SessionMessage, SessionMessageContent)>>> + Send;

    /// Loads a page of trace events for a session owned by `user_id`, assembled
    /// into a [`Trace`].
    ///
    /// Returns the [`Trace`] read view built from the session's trace events,
    /// oldest first, windowed by `pagination`. A session that exists but has no
    /// event in the requested window yields a [`Trace`] with no events; `None`
    /// is returned when the session is absent, archived, or not owned by
    /// `user_id`, the three reported alike so existence stays private.
    fn get_session_trace_events(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        pagination: Pagination,
    ) -> impl Future<Output = StorageResult<Option<Trace>>> + Send;

    /// Loads every [`TraceEventType::Cost`](smista_core::trace::TraceEventType::Cost)
    /// event for a session owned by `user_id`, oldest first.
    ///
    /// Returns the session's cost events as the core read-model
    /// [`TraceEvent`](smista_core::trace::TraceEvent)s, so usage can be
    /// aggregated from a single query rather than paging the whole trace. A cost
    /// event of a plaintext session carries its decoded
    /// [`Payload::Cost`](smista_core::trace::Payload::Cost); one of an encrypted
    /// session carries its sealed envelope, since storage holds no key and never
    /// decrypts it. `None` is returned when the session is absent, archived, or
    /// not owned by `user_id`, the three reported alike so existence stays
    /// private.
    fn get_session_cost_events(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> impl Future<Output = StorageResult<Option<Vec<CoreTraceEvent>>>> + Send;

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
    /// [`SecretContent::Encrypted`]
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
