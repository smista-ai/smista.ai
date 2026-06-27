//! Session lifecycle access over the storage layer.
//!
//! The router never touches the database directly for session work: it goes
//! through [`Sessions`], the per-user entry point, and [`UserSession`], the
//! handle for one session. This mirrors the [`Tracer`](crate::trace::Tracer):
//! both wrap the storage handle and add nothing the storage does not already
//! do, they only scope each call to the authenticated user and, for
//! [`UserSession`], to a single session.
//!
//! [`Sessions`] works with a user's sessions as a group (create, list, get) and
//! opens one session into a [`UserSession`]. [`UserSession`] gathers everything
//! done with that session once it is open: reading its state, messages, trace
//! and run state; appending the rows a running session produces; moving plans
//! and diffs between statuses; managing the session's context memory; and
//! renaming, archiving or deleting it.
//!
//! A metadata row and its paired content are always written together, sharing
//! one record id, in a single append. Content is never re-encrypted in place
//! after that write: for an encrypted session the router pauses the run (see the
//! run state machine), the client returns the sealed content, and only then is
//! the already-encrypted row appended. So the append methods here take content
//! exactly as it is to be stored, in clear or sealed, and never touch a row
//! again to seal it.
//!
//! Ownership is enforced exactly as the storage layer enforces it: a session
//! owned by another user is treated as absent, never disclosed. Reading or
//! writing across users is impossible through these handles.

use smista_core::trace::Trace;
use smista_storage::StorageError;
use smista_storage::api::{MemoryRef, Pagination, SessionState};
use smista_storage::database::Database as _;
use smista_storage::database::surreal::SurrealDatabase;
use smista_storage::entity::{
    ContextMemory, ContextMemoryContent, DiffStatus, PlanStatus, RunState, Session,
    SessionApproval, SessionContextReference, SessionDiff, SessionDiffContent, SessionMessage,
    SessionMessageContent, SessionPlan, SessionPlanContent, SessionRoutingDecision,
    SessionRunInput, SessionRunInputContent, SessionToolCall, SessionToolCallContent,
    ToolCallStatus, UserMemory, UserMemoryContent,
};
use smista_storage::types::SecretContent;
use uuid::Uuid;

/// Error type for [`Sessions`] and [`UserSession`].
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Database error: {0}")]
    Database(#[from] StorageError),
    #[error("Session not found")]
    NotFound,
}

/// Result type for [`Sessions`] and [`UserSession`].
pub type SessionResult<T> = Result<T, SessionError>;

/// The entry point for working with one user's sessions.
///
/// Scoped to a single `user_id`, it creates, lists and loads that user's
/// sessions, and opens one of them into a [`UserSession`]. It wraps the storage
/// handle so the router never queries sessions directly.
#[derive(Debug, Clone)]
pub struct Sessions {
    /// The database backing the sessions.
    database: SurrealDatabase,
    /// The user whose sessions this handle exposes.
    user_id: Uuid,
}

impl Sessions {
    /// Creates a new [`Sessions`] handle for `user_id` over `database`.
    pub fn new(database: SurrealDatabase, user_id: Uuid) -> Self {
        Self { database, user_id }
    }

    /// Persists a new session and returns a handle to it.
    ///
    /// The session is known to exist once created, so the returned
    /// [`UserSession`] is ready to use without the existence check
    /// [`open`](Self::open) performs.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the session cannot be persisted.
    pub async fn create(&self, session: Session) -> SessionResult<UserSession> {
        let session_id = session.uuid();
        self.database
            .create_session(session)
            .await
            .map_err(SessionError::Database)?;
        Ok(self.handle(session_id))
    }

    /// Lists the user's sessions, most recently updated first.
    ///
    /// Archived sessions are included; the caller decides whether to surface
    /// them.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the sessions cannot be read.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn list(&self) -> SessionResult<Vec<Session>> {
        self.database
            .list_sessions(self.user_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Loads one of the user's sessions, or `None` if absent.
    ///
    /// A session owned by another user is treated as absent.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the session cannot be read.
    pub async fn get(&self, session_id: Uuid) -> SessionResult<Option<Session>> {
        self.database
            .get_session(self.user_id, session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Opens a handle to one session of this user.
    ///
    /// Existence and ownership are verified here, once: a session that is
    /// absent or owned by another user yields [`SessionError::NotFound`] rather
    /// than a handle whose every call would fail. This keeps the router from
    /// starting work — routing, provider calls, tool execution — against a
    /// session that does not exist. The storage layer still re-checks ownership
    /// on every write, so a session deleted after `open` cannot be written to.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotFound`] if the session is absent or owned by
    /// another user, or [`SessionError`] if it cannot be read.
    pub async fn open(&self, session_id: Uuid) -> SessionResult<UserSession> {
        if self.get(session_id).await?.is_none() {
            return Err(SessionError::NotFound);
        }
        Ok(self.handle(session_id))
    }

    /// Binds a [`UserSession`] to `session_id` without verifying existence.
    ///
    /// Internal helper shared by [`create`](Self::create), where the session is
    /// known to exist, and [`open`](Self::open), which checks first.
    fn handle(&self, session_id: Uuid) -> UserSession {
        UserSession {
            database: self.database.clone(),
            user_id: self.user_id,
            session_id,
        }
    }
}

/// A handle to a single session owned by one user.
///
/// Scoped to a `(user_id, session_id)` pair, it gathers every operation on that
/// session behind one type so the router never queries the session's rows
/// directly. Obtain one with [`Sessions::open`].
#[derive(Debug, Clone)]
pub struct UserSession {
    /// The database backing the session.
    database: SurrealDatabase,
    /// The user that owns the session.
    user_id: Uuid,
    /// The session this handle is bound to.
    session_id: Uuid,
}

impl UserSession {
    /// The owning user's id.
    pub(crate) fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// The bound session's id.
    pub(crate) fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Loads the session metadata, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the session cannot be read.
    pub async fn session(&self) -> SessionResult<Option<Session>> {
        self.database
            .get_session(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Loads the full state of the session, or `None` if absent.
    ///
    /// Returns the session metadata together with every child row; content
    /// payloads are not included.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the state cannot be read.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn state(&self) -> SessionResult<Option<SessionState>> {
        self.database
            .get_session_state(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Lists the session's messages, each paired with its content, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the messages cannot be read.
    pub async fn messages(&self) -> SessionResult<Vec<(SessionMessage, SessionMessageContent)>> {
        self.database
            .list_session_messages_with_content(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Returns a page of the session's trace, oldest event first.
    ///
    /// `None` is returned only when the session is absent or not owned by this
    /// user.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the trace cannot be read.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn traces(&self, pagination: Pagination) -> SessionResult<Option<Trace>> {
        self.database
            .get_session_trace_events(self.user_id, self.session_id, pagination)
            .await
            .map_err(SessionError::Database)
    }

    /// Loads the in-flight run state, or `None` when the session is idle.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the run state cannot be read.
    pub async fn run_state(&self) -> SessionResult<Option<RunState>> {
        self.database
            .get_run_state(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Updates the session and returns the stored row.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the session cannot be updated.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn update(&self, session: Session) -> SessionResult<Session> {
        self.database
            .update_session(self.user_id, session)
            .await
            .map_err(SessionError::Database)
    }

    /// Marks the session as archived.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the session cannot be archived.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn archive(&self) -> SessionResult<()> {
        self.database
            .archive_session(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Deletes the session and all of its child rows.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the session cannot be deleted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn delete(&self) -> SessionResult<()> {
        self.database
            .delete_session(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Loads the run's request-context snapshot with its content, or `None`.
    ///
    /// Re-read every turn of a multi-turn run so the deterministic resolver can
    /// be replayed without the client re-sending the original request.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the run input cannot be read.
    pub async fn run_input(
        &self,
    ) -> SessionResult<Option<(SessionRunInput, SessionRunInputContent)>> {
        self.database
            .get_run_input(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Writes the run's request-context snapshot, overwriting any existing row.
    ///
    /// Used to update the persisted plan-mode flag without re-sending the
    /// original request.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the run input cannot be written.
    pub async fn set_run_input(
        &self,
        input: SessionRunInput,
        content: SessionRunInputContent,
    ) -> SessionResult<SessionRunInput> {
        self.database
            .set_run_input(self.user_id, input, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Sets the in-flight run state, overwriting any existing row.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the run state cannot be written.
    pub async fn set_run_state(&self, state: RunState) -> SessionResult<RunState> {
        self.database
            .set_run_state(self.user_id, state)
            .await
            .map_err(SessionError::Database)
    }

    /// Atomically acquires the run lock, returning the stored state when granted
    /// or `None` when a turn already holds it.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the lock cannot be written.
    pub async fn acquire_run_lock(&self, state: RunState) -> SessionResult<Option<RunState>> {
        self.database
            .acquire_run_lock(self.user_id, state)
            .await
            .map_err(SessionError::Database)
    }

    /// Appends a message together with its paired content.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the message cannot be appended.
    pub async fn append_message(
        &self,
        message: SessionMessage,
        content: SessionMessageContent,
    ) -> SessionResult<SessionMessage> {
        self.database
            .append_message(self.user_id, message, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Appends a routing decision.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the decision cannot be appended.
    pub async fn append_routing_decision(
        &self,
        decision: SessionRoutingDecision,
    ) -> SessionResult<SessionRoutingDecision> {
        self.database
            .append_routing_decision(self.user_id, decision)
            .await
            .map_err(SessionError::Database)
    }

    /// Appends a context reference.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the reference cannot be appended.
    pub async fn append_context_reference(
        &self,
        reference: SessionContextReference,
    ) -> SessionResult<SessionContextReference> {
        self.database
            .append_context_reference(self.user_id, reference)
            .await
            .map_err(SessionError::Database)
    }

    /// Appends a tool call together with its paired content.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the tool call cannot be appended.
    pub async fn append_tool_call(
        &self,
        tool_call: SessionToolCall,
        content: SessionToolCallContent,
    ) -> SessionResult<SessionToolCall> {
        self.database
            .append_tool_call(self.user_id, tool_call, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Appends a plan together with its paired snapshot content.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the plan cannot be appended.
    pub async fn append_plan(
        &self,
        plan: SessionPlan,
        content: SessionPlanContent,
    ) -> SessionResult<SessionPlan> {
        self.database
            .append_plan(self.user_id, plan, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Appends a diff together with its paired content.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the diff cannot be appended.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn append_diff(
        &self,
        diff: SessionDiff,
        content: SessionDiffContent,
    ) -> SessionResult<SessionDiff> {
        self.database
            .append_diff(self.user_id, diff, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Appends an approval decision.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the approval cannot be appended.
    pub async fn append_approval(
        &self,
        approval: SessionApproval,
    ) -> SessionResult<SessionApproval> {
        self.database
            .append_approval(self.user_id, approval)
            .await
            .map_err(SessionError::Database)
    }

    /// Moves a stored plan into `status`, returning the updated row.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the plan cannot be updated.
    pub async fn set_plan_status(
        &self,
        plan_id: Uuid,
        status: PlanStatus,
    ) -> SessionResult<SessionPlan> {
        self.database
            .set_plan_status(self.user_id, plan_id, status)
            .await
            .map_err(SessionError::Database)
    }

    /// Moves a stored diff into `status`, returning the updated row.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the diff cannot be updated.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn set_diff_status(
        &self,
        diff_id: Uuid,
        status: DiffStatus,
    ) -> SessionResult<SessionDiff> {
        self.database
            .set_diff_status(self.user_id, diff_id, status)
            .await
            .map_err(SessionError::Database)
    }

    /// Records the outcome of a tool call, returning the updated row.
    ///
    /// Moves the call into `status`, stamping completion for a terminal status,
    /// and writes its sanitised result and error while preserving its arguments.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the tool call cannot be updated.
    pub async fn set_tool_call_outcome(
        &self,
        call_id: Uuid,
        status: ToolCallStatus,
        result: Option<SecretContent>,
        error: Option<SecretContent>,
    ) -> SessionResult<SessionToolCall> {
        self.database
            .set_tool_call_outcome(self.user_id, call_id, status, result, error)
            .await
            .map_err(SessionError::Database)
    }

    /// Records a context memory together with its paired content.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the memory cannot be recorded.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn record_context_memory(
        &self,
        memory: ContextMemory,
        content: ContextMemoryContent,
    ) -> SessionResult<ContextMemory> {
        self.database
            .record_context_memory(self.user_id, memory, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Updates the content of one of the session's context memories.
    ///
    /// The target row is selected by id or by key (see [`MemoryRef`]).
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the memory cannot be updated.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn update_context_memory(
        &self,
        target: MemoryRef,
        content: ContextMemoryContent,
    ) -> SessionResult<ContextMemory> {
        self.database
            .update_context_memory(self.user_id, self.session_id, target, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Loads one of the session's context memories, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the memory cannot be read.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn get_context_memory(&self, id: Uuid) -> SessionResult<Option<ContextMemory>> {
        self.database
            .get_context_memory(self.user_id, self.session_id, id)
            .await
            .map_err(SessionError::Database)
    }

    /// Lists the session's context memories.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the memories cannot be read.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn list_context_memory(&self) -> SessionResult<Vec<ContextMemory>> {
        self.database
            .list_context_memory(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Lists the session's context memories, each paired with its content, most
    /// recently updated first.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the memories cannot be read.
    pub async fn list_context_memory_with_content(
        &self,
    ) -> SessionResult<Vec<(ContextMemory, ContextMemoryContent)>> {
        self.database
            .list_context_memory_with_content(self.user_id, self.session_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Lists the owning user's memories, each paired with its content.
    ///
    /// User memory is user-scoped rather than session-scoped, so it spans the
    /// user's sessions; the orchestrator recalls it alongside the session
    /// history when building a prompt.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the memories cannot be read.
    pub async fn list_user_memory_with_content(
        &self,
    ) -> SessionResult<Vec<(UserMemory, UserMemoryContent)>> {
        self.database
            .list_user_memory_with_content(self.user_id)
            .await
            .map_err(SessionError::Database)
    }

    /// Forgets one of the session's context memories, with its paired content.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the memory cannot be forgotten.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the session endpoints and orchestrator later"
        )
    )]
    pub async fn forget_context_memory(&self, id: Uuid) -> SessionResult<()> {
        self.database
            .forget_context_memory(self.user_id, self.session_id, id)
            .await
            .map_err(SessionError::Database)
    }

    /// Seals (or overwrites) the content payload of a router-authored row.
    ///
    /// `table` is the base content table the row lives under; the orchestrator
    /// uses this to write a deferred row's content once the client returns its
    /// ciphertext.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the content cannot be written or the row is
    /// not owned by this session's user.
    pub async fn set_content(
        &self,
        table: &str,
        id: Uuid,
        content: SecretContent,
    ) -> SessionResult<()> {
        self.database
            .set_content(self.user_id, table, id, content)
            .await
            .map_err(SessionError::Database)
    }

    /// Reads the content payload of a row, or `None` when absent.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] if the content cannot be read.
    pub async fn get_content(&self, table: &str, id: Uuid) -> SessionResult<Option<SecretContent>> {
        self.database
            .get_content(self.user_id, table, id)
            .await
            .map_err(SessionError::Database)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use smista_core::api::ApprovalDecision;
    use smista_core::intent::TaskIntent;
    use smista_core::message::MessageRole;
    use smista_core::model::Provider;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
    use smista_storage::entity::{RunPhase, Table, ToolCallStatus, User};
    use smista_storage::surrealdb::RecordId;
    use smista_storage::types::SecretContent;

    use super::*;

    async fn memory_db() -> SurrealDatabase {
        SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database")
    }

    fn user_entity(id: Uuid) -> User {
        User {
            id: RecordId::new(User::name(), id.to_string()),
            api_key_hash: format!("hash-{id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        }
    }

    fn session_entity(id: Uuid, user_id: Uuid) -> Session {
        Session {
            id: RecordId::new(Session::name(), id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: Some("session".to_string()),
            encrypted: false,
            key_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        }
    }

    /// Builds a [`Sessions`] handle over a fresh in-memory database with its
    /// user already persisted, returning the handle and the user id.
    async fn sessions() -> (Sessions, Uuid) {
        let db = memory_db().await;
        let user_id = Uuid::now_v7();
        db.create_user(user_entity(user_id))
            .await
            .expect("failed to create user");

        (Sessions::new(db, user_id), user_id)
    }

    /// Persists a session for the handle's user and opens it.
    async fn open_session(sessions: &Sessions, user_id: Uuid) -> (UserSession, Uuid) {
        let session_id = Uuid::now_v7();
        let handle = sessions
            .create(session_entity(session_id, user_id))
            .await
            .expect("failed to create session");

        (handle, session_id)
    }

    #[tokio::test]
    async fn should_create_list_and_get_sessions() {
        let (sessions, user_id) = sessions().await;
        let session_id = Uuid::now_v7();

        let created = sessions
            .create(session_entity(session_id, user_id))
            .await
            .expect("failed to create session");
        let row = created
            .session()
            .await
            .expect("failed to read created session")
            .expect("created session not found");
        assert_eq!(
            row.id,
            RecordId::new(Session::name(), session_id.to_string())
        );

        let listed = sessions.list().await.expect("failed to list sessions");
        assert_eq!(listed.len(), 1);

        let fetched = sessions
            .get(session_id)
            .await
            .expect("failed to get session")
            .expect("session not found");
        assert_eq!(fetched.title, Some("session".to_string()));
    }

    #[tokio::test]
    async fn should_read_session_through_handle() {
        let (sessions, user_id) = sessions().await;
        let (session, session_id) = open_session(&sessions, user_id).await;

        assert!(session.session().await.expect("read failed").is_some());

        let state = session
            .state()
            .await
            .expect("state read failed")
            .expect("session state not found");
        assert_eq!(
            state.session.id,
            RecordId::new(Session::name(), session_id.to_string())
        );
        assert!(state.messages.is_empty());

        assert!(
            session
                .run_state()
                .await
                .expect("run state read failed")
                .is_none()
        );

        let trace = session
            .traces(Pagination::default())
            .await
            .expect("trace read failed")
            .expect("trace not found");
        assert!(trace.events.is_empty());
    }

    #[tokio::test]
    async fn should_append_message_and_read_it_back() {
        let (sessions, user_id) = sessions().await;
        let (session, session_id) = open_session(&sessions, user_id).await;

        let message_id = Uuid::now_v7();
        let message = SessionMessage {
            id: RecordId::new(SessionMessage::name(), message_id.to_string()),
            session: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            role: MessageRole::User,
            provider: Provider::Anthropic,
            model: "claude".to_string(),
            created_at: Utc::now(),
        };
        let content = SessionMessageContent {
            id: RecordId::new(SessionMessageContent::name(), message_id.to_string()),
            content: SecretContent::plaintext("hello".to_string()),
        };
        session
            .append_message(message, content)
            .await
            .expect("failed to append message");

        let messages = session.messages().await.expect("failed to read messages");
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn should_append_every_child_row() {
        let (sessions, user_id) = sessions().await;
        let (session, session_id) = open_session(&sessions, user_id).await;
        let session_ref = RecordId::new(Session::name(), session_id.to_string());
        let user_ref = RecordId::new(User::name(), user_id.to_string());

        session
            .append_routing_decision(SessionRoutingDecision {
                id: RecordId::new(SessionRoutingDecision::name(), Uuid::now_v7().to_string()),
                session: session_ref.clone(),
                user: user_ref.clone(),
                task_type: TaskIntent::Edit,
                provider: Provider::Anthropic,
                model: "claude".to_string(),
                matched_rule: Some("edit -> claude".to_string()),
                fallback_used: Some(false),
                override_used: Some(false),
                reason: "best for edit".to_string(),
                created_at: Utc::now(),
            })
            .await
            .expect("failed to append routing decision");

        session
            .append_context_reference(SessionContextReference {
                id: RecordId::new(SessionContextReference::name(), Uuid::now_v7().to_string()),
                session: session_ref.clone(),
                user: user_ref.clone(),
                path: Some("src/lib.rs".to_string()),
                kind: "file".to_string(),
                included: true,
                reason: "referenced".to_string(),
                created_at: Utc::now(),
            })
            .await
            .expect("failed to append context reference");

        let tool_call_id = Uuid::now_v7();
        session
            .append_tool_call(
                SessionToolCall {
                    id: RecordId::new(SessionToolCall::name(), tool_call_id.to_string()),
                    session: session_ref.clone(),
                    user: user_ref.clone(),
                    tool_name: "read_file".to_string(),
                    status: ToolCallStatus::Completed,
                    created_at: Utc::now(),
                    completed_at: Some(Utc::now()),
                },
                SessionToolCallContent {
                    id: RecordId::new(SessionToolCallContent::name(), tool_call_id.to_string()),
                    arguments: SecretContent::plaintext("src/lib.rs".to_string()),
                    result: None,
                    error: None,
                },
            )
            .await
            .expect("failed to append tool call");

        let plan_id = Uuid::now_v7();
        session
            .append_plan(
                SessionPlan {
                    id: RecordId::new(SessionPlan::name(), plan_id.to_string()),
                    session: session_ref.clone(),
                    user: user_ref.clone(),
                    path: "PLAN.md".to_string(),
                    status: PlanStatus::Draft,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    approved_at: None,
                    content_hash: None,
                },
                SessionPlanContent {
                    id: RecordId::new(SessionPlanContent::name(), plan_id.to_string()),
                    content: Some(SecretContent::plaintext("the plan".to_string())),
                },
            )
            .await
            .expect("failed to append plan");

        let diff_id = Uuid::now_v7();
        session
            .append_diff(
                SessionDiff {
                    id: RecordId::new(SessionDiff::name(), diff_id.to_string()),
                    session: session_ref.clone(),
                    user: user_ref.clone(),
                    path: "src/lib.rs".to_string(),
                    status: DiffStatus::Proposed,
                    created_at: Utc::now(),
                    applied_at: None,
                },
                SessionDiffContent {
                    id: RecordId::new(SessionDiffContent::name(), diff_id.to_string()),
                    content: SecretContent::plaintext("@@ -1 +1 @@".to_string()),
                },
            )
            .await
            .expect("failed to append diff");

        session
            .append_approval(SessionApproval {
                id: RecordId::new(SessionApproval::name(), Uuid::now_v7().to_string()),
                session: session_ref.clone(),
                user: user_ref.clone(),
                target_type: "plan".to_string(),
                target_id: plan_id.to_string(),
                decision: ApprovalDecision::Approved,
                reason: None,
                created_at: Utc::now(),
            })
            .await
            .expect("failed to append approval");

        // The status transitions move the appended plan and diff forward.
        let approved = session
            .set_plan_status(plan_id, PlanStatus::Approved)
            .await
            .expect("failed to set plan status");
        assert_eq!(approved.status, PlanStatus::Approved);
        let applied = session
            .set_diff_status(diff_id, DiffStatus::Applied)
            .await
            .expect("failed to set diff status");
        assert_eq!(applied.status, DiffStatus::Applied);

        let state = session
            .state()
            .await
            .expect("state read failed")
            .expect("session state not found");
        assert_eq!(state.routing_decisions.len(), 1);
        assert_eq!(state.context_references.len(), 1);
        assert_eq!(state.tool_calls.len(), 1);
        assert_eq!(state.plans.len(), 1);
        assert_eq!(state.diffs.len(), 1);
        assert_eq!(state.approvals.len(), 1);
    }

    #[tokio::test]
    async fn should_manage_run_state() {
        let (sessions, user_id) = sessions().await;
        let (session, session_id) = open_session(&sessions, user_id).await;

        session
            .set_run_state(RunState::new(
                session_id,
                user_id,
                Uuid::now_v7(),
                RunPhase::Idle,
            ))
            .await
            .expect("failed to set run state");

        let state = session
            .run_state()
            .await
            .expect("failed to read run state")
            .expect("run state not found");
        assert_eq!(state.phase, RunPhase::Idle);
    }

    #[tokio::test]
    async fn should_manage_context_memory() {
        let (sessions, user_id) = sessions().await;
        let (session, session_id) = open_session(&sessions, user_id).await;

        let memory_id = Uuid::now_v7();
        session
            .record_context_memory(
                ContextMemory::new(memory_id, session_id, user_id, Some("topic".to_string())),
                ContextMemoryContent::new(memory_id, SecretContent::plaintext("fact".to_string())),
            )
            .await
            .expect("failed to record context memory");

        session
            .update_context_memory(
                MemoryRef::Key("topic".to_string()),
                ContextMemoryContent::new(
                    memory_id,
                    SecretContent::plaintext("updated".to_string()),
                ),
            )
            .await
            .expect("failed to update context memory");

        assert!(
            session
                .get_context_memory(memory_id)
                .await
                .expect("failed to get context memory")
                .is_some()
        );
        assert_eq!(
            session
                .list_context_memory()
                .await
                .expect("failed to list context memory")
                .len(),
            1
        );
        assert_eq!(
            session
                .list_context_memory_with_content()
                .await
                .expect("failed to list context memory with content")
                .len(),
            1
        );

        session
            .forget_context_memory(memory_id)
            .await
            .expect("failed to forget context memory");
        assert!(
            session
                .list_context_memory()
                .await
                .expect("failed to list context memory")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn should_update_archive_and_delete_session() {
        let (sessions, user_id) = sessions().await;
        let (session, session_id) = open_session(&sessions, user_id).await;

        let mut row = session_entity(session_id, user_id);
        row.title = Some("renamed".to_string());
        let updated = session.update(row).await.expect("failed to update session");
        assert_eq!(updated.title, Some("renamed".to_string()));

        session.archive().await.expect("failed to archive session");
        let archived = sessions
            .get(session_id)
            .await
            .expect("failed to get session")
            .expect("session not found");
        assert!(archived.archived_at.is_some());

        session.delete().await.expect("failed to delete session");
        assert!(
            sessions
                .get(session_id)
                .await
                .expect("failed to get session")
                .is_none()
        );
    }

    #[tokio::test]
    async fn should_not_read_session_of_another_user() {
        let (owner, owner_id) = sessions().await;
        let (_, session_id) = open_session(&owner, owner_id).await;

        // A second user sharing the same database must not see the session.
        let intruder_id = Uuid::now_v7();
        owner
            .database
            .create_user(user_entity(intruder_id))
            .await
            .expect("failed to create intruder");
        let intruder = Sessions::new(owner.database.clone(), intruder_id);

        assert!(
            intruder
                .get(session_id)
                .await
                .expect("failed to get session")
                .is_none(),
            "session disclosed to a non-owner"
        );
        assert!(
            matches!(intruder.open(session_id).await, Err(SessionError::NotFound)),
            "session opened by a non-owner"
        );
    }

    #[tokio::test]
    async fn should_not_open_a_missing_session() {
        let (sessions, _) = sessions().await;

        assert!(
            matches!(
                sessions.open(Uuid::now_v7()).await,
                Err(SessionError::NotFound)
            ),
            "opened a session that does not exist"
        );
    }
}
