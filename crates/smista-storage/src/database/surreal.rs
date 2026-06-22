//! SurrealDB [`Database`] trait implementation.

mod backend;
mod options;
mod schema;
#[cfg(test)]
mod tests;

use chrono::{DateTime, Duration, Utc};
use smista_core::trace::{Payload, Trace, TraceEvent as CoreTraceEvent, TraceEventPayload};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Namespace;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

#[doc(inline)]
pub use self::backend::{RemoteOptions, SurrealBackend};
#[doc(inline)]
pub use self::options::SurrealOptions;
use crate::api::{MemoryRef, Pagination, SessionState};
use crate::database::Database;
use crate::entity::{
    AuthToken, ContextMemory, ContextMemoryContent, Session, SessionApproval,
    SessionContextReference, SessionDiff, SessionDiffContent, SessionMessage,
    SessionMessageContent, SessionPlan, SessionPlanContent, SessionRoutingDecision,
    SessionToolCall, SessionToolCallContent, Table, TraceEvent, TraceEventContent, User,
    UserMemory, UserMemoryContent,
};
use crate::types::SecretContent;
use crate::{StorageError, StorageResult};

/// [`Database`] implementation backed by SurrealDB.
#[derive(Debug, Clone)]
pub struct SurrealDatabase(Surreal<Any>);

impl SurrealDatabase {
    /// Connects to (or embeds) a SurrealDB database from the given options.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the embedded directory
    /// cannot be created, the connection or namespace selection fails,
    /// authentication is rejected, or the schema migration fails.
    pub async fn new(options: SurrealOptions) -> StorageResult<Self> {
        options.backend.prepare().await?;

        let endpoint = options.backend.endpoint();
        tracing::debug!(db.endpoint = %endpoint, "connecting to SurrealDB at {{db.endpoint}}");

        let db = surrealdb::engine::any::connect(&endpoint).await?;
        db.use_ns(&options.namespace).use_db(&options.db).await?;
        tracing::debug!(
            db.namespace = %options.namespace,
            db.name = %options.db,
            "using namespace {{db.namespace}} and database {{db.name}}"
        );

        if let Some((username, password)) = options.backend.credentials() {
            tracing::debug!(db.username = %username, "signing in to SurrealDB as {{db.username}}");
            db.signin(Namespace {
                namespace: options.namespace,
                username: username.to_string(),
                password: password.to_string(),
            })
            .await?;
        }

        schema::apply(&db).await?;

        Ok(Self(db))
    }
}

/// Constructs a SurrealDB [`RecordId`] for the given [`Table`] and an id (must be [`ToString`]).
#[inline(always)]
#[must_use]
fn record_id<T, ID>(id: ID) -> RecordId
where
    T: Table,
    ID: ToString,
{
    RecordId::new(T::name(), id.to_string())
}

/// Pairs each base row with its `_content` row, matched by shared record key.
///
/// `_content` rows carry the same record key as their base row, so they are
/// joined on `id.key`. The base ordering is preserved, and a base row whose
/// content is missing is dropped — a paired write is transactional, so this
/// only guards against an externally tampered store.
fn pair_by_key<M, C>(
    bases: Vec<M>,
    contents: &[C],
    base_id: impl Fn(&M) -> &RecordId,
    content_id: impl Fn(&C) -> &RecordId,
) -> Vec<(M, C)>
where
    C: Clone,
{
    bases
        .into_iter()
        .filter_map(|base| {
            contents
                .iter()
                .find(|content| content_id(content).key == base_id(&base).key)
                .map(|content| (base, content.clone()))
        })
        .collect()
}

/// Explicit ownership cascade for [`SurrealDatabase::delete_session`].
///
/// SurrealDB enforces no foreign-key cascade, so every session-scoped table and
/// its paired `_content` table is cleared here. `_content` rows share the base
/// row's record key, so they are matched with `record::id`. The statements run
/// inside an explicit transaction so the session and all of its children are
/// removed atomically.
const DELETE_SESSION_CASCADE: &str = r#"
DELETE session_message_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_message WHERE session = $sess);
DELETE session_message WHERE session = $sess;
DELETE session_tool_call_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_tool_call WHERE session = $sess);
DELETE session_tool_call WHERE session = $sess;
DELETE session_plan_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_plan WHERE session = $sess);
DELETE session_plan WHERE session = $sess;
DELETE session_diff_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_diff WHERE session = $sess);
DELETE session_diff WHERE session = $sess;
DELETE trace_event_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM trace_event WHERE session = $sess);
DELETE trace_event WHERE session = $sess;
DELETE context_memory_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM context_memory WHERE session = $sess);
DELETE context_memory WHERE session = $sess;
DELETE session_routing_decision WHERE session = $sess;
DELETE session_context_reference WHERE session = $sess;
DELETE session_approval WHERE session = $sess;
DELETE $sess;
"#;

/// Bulk ownership cascade for the retention purges.
///
/// Mirrors [`DELETE_SESSION_CASCADE`] but clears every session whose id is in
/// the bound `$targets` list rather than a single session, so a cleanup pass
/// removes many sessions and all of their child rows in one transaction. The
/// `_content` tables share their base row's record key and are matched with
/// `record::id`.
const PURGE_SESSIONS_CASCADE: &str = r#"
DELETE session_message_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_message WHERE session IN $targets);
DELETE session_message WHERE session IN $targets;
DELETE session_tool_call_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_tool_call WHERE session IN $targets);
DELETE session_tool_call WHERE session IN $targets;
DELETE session_plan_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_plan WHERE session IN $targets);
DELETE session_plan WHERE session IN $targets;
DELETE session_diff_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM session_diff WHERE session IN $targets);
DELETE session_diff WHERE session IN $targets;
DELETE trace_event_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM trace_event WHERE session IN $targets);
DELETE trace_event WHERE session IN $targets;
DELETE context_memory_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM context_memory WHERE session IN $targets);
DELETE context_memory WHERE session IN $targets;
DELETE session_routing_decision WHERE session IN $targets;
DELETE session_context_reference WHERE session IN $targets;
DELETE session_approval WHERE session IN $targets;
DELETE session WHERE id IN $targets;
"#;

/// Bulk cleanup for trace events past their retention window.
///
/// Removes every [`TraceEvent`] created before the bound `$cutoff` together
/// with its paired `_content` row, leaving the owning sessions intact.
const PURGE_TRACES: &str = r#"
DELETE trace_event_content WHERE record::id(id) IN (SELECT VALUE record::id(id) FROM trace_event WHERE created_at < $cutoff);
DELETE trace_event WHERE created_at < $cutoff;
"#;

impl SurrealDatabase {
    /// Asserts that `session` is owned by `user_id`, returning
    /// [`StorageError::NotFound`] otherwise.
    ///
    /// A row owned by another user is treated as absent rather than disclosed,
    /// so callers cannot tell an unowned session apart from a missing one.
    async fn assert_session_owned(&self, user_id: Uuid, session: &RecordId) -> StorageResult<()> {
        let user = record_id::<User, _>(user_id);
        let owned: Option<Session> = self
            .0
            .query("SELECT * FROM $sess WHERE user = $user")
            .bind(("sess", session.clone()))
            .bind(("user", user))
            .await?
            .take(0)?;

        owned.map(|_| ()).ok_or(StorageError::NotFound)
    }

    /// Asserts that a session-scoped write belongs to `user_id`: both the
    /// target `session` and the row's redundant `user` reference must name
    /// `user_id`, returning [`StorageError::NotFound`] otherwise.
    ///
    /// The redundant `user` column is what every ownership-scoped read filters
    /// on, so a write whose `user` disagrees with the authenticated caller
    /// would persist a row the owner could never reach. Rejecting the mismatch
    /// keeps ownership unforgeable through that column.
    async fn assert_write_owned(
        &self,
        user_id: Uuid,
        session: &RecordId,
        row_user: &RecordId,
    ) -> StorageResult<()> {
        if row_user != &record_id::<User, _>(user_id) {
            return Err(StorageError::NotFound);
        }
        self.assert_session_owned(user_id, session).await
    }

    /// Creates a single metadata-only row, returning the stored row.
    async fn create_one<T>(&self, value: T) -> StorageResult<T>
    where
        T: SurrealValue + Table + Send + 'static,
    {
        match self.0.create(T::name()).content(value).await? {
            Some(created) => Ok(created),
            None => Err(StorageError::AlreadyExists),
        }
    }

    /// Creates a metadata row and its paired `_content` row in one transaction.
    ///
    /// Both rows share the same record key; if either write fails the
    /// transaction is cancelled, so a base row is never left without its
    /// content.
    async fn create_paired<M, C>(&self, meta: M, content: C) -> StorageResult<M>
    where
        M: SurrealValue + Table + Send + 'static,
        C: SurrealValue + Table + Send + 'static,
    {
        let tx = self.0.clone().begin().await?;

        let created: Option<M> = match tx.create(M::name()).content(meta).await {
            Ok(created) => created,
            Err(err) => {
                let _ = tx.cancel().await;
                return Err(err.into());
            }
        };

        let created_content: surrealdb::Result<Option<C>> =
            tx.create(C::name()).content(content).await;
        if let Err(err) = created_content {
            let _ = tx.cancel().await;
            return Err(err.into());
        }

        tx.commit().await?;
        created.ok_or(StorageError::AlreadyExists)
    }

    /// Lists every row of table `T` belonging to `session`, oldest first.
    async fn list_session_rows<T>(&self, session: &RecordId) -> StorageResult<Vec<T>>
    where
        T: SurrealValue + Table + Send + 'static,
    {
        self.0
            .query("SELECT * FROM $table WHERE session = $sess ORDER BY created_at")
            .bind(("table", T::table()))
            .bind(("sess", session.clone()))
            .await?
            .take(0)
            .map_err(StorageError::from)
    }

    /// Replaces the payload of the `_content` row paired with `base`, bumps the
    /// base `updated_at`, and returns the refreshed base row.
    ///
    /// Both writes run in one transaction so the metadata and its content never
    /// drift apart. Returns [`StorageError::NotFound`] if the base row is gone.
    async fn replace_paired_content<M, C>(
        &self,
        base: RecordId,
        content_table: &'static str,
        updated_at: DateTime<Utc>,
        content: C,
    ) -> StorageResult<M>
    where
        M: SurrealValue + Send + 'static,
        C: SurrealValue + Send + 'static,
    {
        let content_id = RecordId::new(content_table, base.key.clone());
        let tx = self.0.clone().begin().await?;

        let content_written = tx
            .query("UPDATE $content SET content = $payload")
            .bind(("content", content_id))
            .bind(("payload", content))
            .await
            .and_then(|response| response.check());
        if let Err(err) = content_written {
            let _ = tx.cancel().await;
            return Err(err.into());
        }

        let base_written = tx
            .query("UPDATE $base SET updated_at = $updated_at")
            .bind(("base", base))
            .bind(("updated_at", updated_at))
            .await
            .and_then(|response| response.check())
            .and_then(|mut response| response.take::<Option<M>>(0));
        let updated = match base_written {
            Ok(updated) => updated,
            Err(err) => {
                let _ = tx.cancel().await;
                return Err(err.into());
            }
        };

        tx.commit().await?;
        updated.ok_or(StorageError::NotFound)
    }

    /// Purges every session selected by `select_targets` together with its
    /// child rows, via [`PURGE_SESSIONS_CASCADE`].
    ///
    /// `select_targets` is a `SELECT VALUE id FROM session WHERE …` expression
    /// that may reference the bound `$cutoff`; it is evaluated into `$targets`
    /// inside the same transaction as the cascade, so selection and deletion
    /// are atomic with no read-modify-write window. An empty selection deletes
    /// nothing.
    async fn purge_sessions(
        &self,
        select_targets: &str,
        cutoff: DateTime<Utc>,
    ) -> StorageResult<()> {
        let query = format!("LET $targets = ({select_targets});{PURGE_SESSIONS_CASCADE}");

        let tx = self.0.clone().begin().await?;
        let outcome = tx
            .query(query)
            .bind(("cutoff", cutoff))
            .await
            .and_then(|response| response.check());

        match outcome {
            Ok(_) => {
                tx.commit().await?;
                Ok(())
            }
            Err(err) => {
                let _ = tx.cancel().await;
                Err(err.into())
            }
        }
    }

    /// Deletes a base row and its paired `_content` row in one transaction.
    async fn forget_paired(
        &self,
        base: RecordId,
        content_table: &'static str,
    ) -> StorageResult<()> {
        let content_id = RecordId::new(content_table, base.key.clone());
        let tx = self.0.clone().begin().await?;

        let deleted = tx
            .query("DELETE $base; DELETE $content")
            .bind(("base", base))
            .bind(("content", content_id))
            .await
            .and_then(|response| response.check());
        match deleted {
            Ok(_) => {
                tx.commit().await?;
                Ok(())
            }
            Err(err) => {
                let _ = tx.cancel().await;
                Err(err.into())
            }
        }
    }
}

impl Database for SurrealDatabase {
    async fn create_user(&self, user: User) -> StorageResult<User> {
        let id = format!("{:?}", user.id);
        tracing::debug!("creating user with id {id}");

        match self.0.create(User::name()).content(user).await? {
            Some(user) => {
                tracing::debug!("created user with id {id}");
                Ok(user)
            }
            None => {
                tracing::error!("failed to create user {id}: no record returned");
                Err(StorageError::AlreadyExists)
            }
        }
    }

    async fn get_user(&self, id: Uuid) -> StorageResult<Option<User>> {
        tracing::debug!("getting user with id {id}");
        let id = record_id::<User, _>(id);

        self.0.select(id).await.map_err(StorageError::from)
    }

    async fn create_token(&self, token: AuthToken) -> StorageResult<AuthToken> {
        let id = format!("{:?}", token.id);
        let user_id = format!("{:?}", token.user);
        tracing::debug!(
            "creating auth token for user {user_id}: {hash}",
            user_id = user_id,
            hash = token.token_hash
        );

        match self.0.create(AuthToken::name()).content(token).await? {
            Some(token) => {
                tracing::debug!("created auth token with id {id} for user {user_id}",);
                Ok(token)
            }
            None => {
                tracing::error!(
                    "failed to create auth token for user {user_id}: no record returned",
                );
                Err(StorageError::AlreadyExists)
            }
        }
    }

    async fn get_active_token(&self, token_id: Uuid) -> StorageResult<Option<AuthToken>> {
        tracing::debug!("getting active token {token_id}");

        self.0
            .query(
                "SELECT * FROM $table WHERE id = $id \
                 AND expires_at > time::now() AND revoked_at IS NONE",
            )
            .bind(("table", AuthToken::table()))
            .bind(("id", record_id::<AuthToken, _>(token_id)))
            .await?
            .take::<Option<AuthToken>>(0)
            .map_err(StorageError::from)
    }

    async fn get_token(&self, token_id: Uuid) -> StorageResult<Option<AuthToken>> {
        tracing::debug!("getting token {token_id}");

        self.0
            .query("SELECT * FROM $table WHERE id = $id")
            .bind(("table", AuthToken::table()))
            .bind(("id", record_id::<AuthToken, _>(token_id)))
            .await?
            .take::<Option<AuthToken>>(0)
            .map_err(StorageError::from)
    }

    async fn revoke_token(&self, id: Uuid) -> StorageResult<()> {
        tracing::debug!("revoking auth token {id}");

        let revoked: Option<AuthToken> = self
            .0
            .query("UPDATE $tok SET revoked_at = time::now()")
            .bind(("tok", record_id::<AuthToken, _>(id)))
            .await?
            .take(0)?;

        if revoked.is_some() {
            tracing::debug!("revoked auth token {id}");
            Ok(())
        } else {
            tracing::error!("failed to revoke auth token {id}: token not found");
            Err(StorageError::NotFound)
        }
    }

    async fn create_session(&self, session: Session) -> StorageResult<Session> {
        tracing::debug!("creating session");
        self.create_one(session).await
    }

    async fn get_session(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Session>> {
        tracing::debug!("getting session {id} for user {user_id}");

        let session: Option<Session> = self.0.select(record_id::<Session, _>(id)).await?;
        Ok(session.filter(|session| session.user == record_id::<User, _>(user_id)))
    }

    async fn list_sessions(&self, user_id: Uuid) -> StorageResult<Vec<Session>> {
        tracing::debug!("listing sessions for user {user_id}");

        self.0
            .query("SELECT * FROM $table WHERE user = $user ORDER BY updated_at DESC")
            .bind(("table", Session::table()))
            .bind(("user", record_id::<User, _>(user_id)))
            .await?
            .take(0)
            .map_err(StorageError::from)
    }

    async fn update_session(&self, user_id: Uuid, session: Session) -> StorageResult<Session> {
        tracing::debug!("updating session for user {user_id}");
        let user = record_id::<User, _>(user_id);

        // Ownership: both the stored row and the incoming row must belong to the
        // user; ownership can never be transferred through an update.
        let stored: Option<Session> = self.0.select(session.id.clone()).await?;
        match stored {
            Some(stored) if stored.user == user && session.user == user => {}
            _ => {
                tracing::error!("refusing to update session not owned by user {user_id}");
                return Err(StorageError::NotFound);
            }
        }

        let updated: Option<Session> = self.0.update(session.id.clone()).content(session).await?;
        updated.ok_or(StorageError::NotFound)
    }

    async fn archive_session(&self, user_id: Uuid, id: Uuid) -> StorageResult<()> {
        tracing::debug!("archiving session {id} for user {user_id}");

        if self.get_session(user_id, id).await?.is_none() {
            tracing::error!("refusing to archive session {id} not owned by user {user_id}");
            return Err(StorageError::NotFound);
        }

        self.0
            .query("UPDATE $sess SET archived_at = time::now()")
            .bind(("sess", record_id::<Session, _>(id)))
            .await?
            .check()?;

        tracing::debug!("archived session {id}");
        Ok(())
    }

    async fn delete_session(&self, user_id: Uuid, id: Uuid) -> StorageResult<()> {
        tracing::debug!("deleting session {id} for user {user_id}");

        if self.get_session(user_id, id).await?.is_none() {
            tracing::error!("refusing to delete session {id} not owned by user {user_id}");
            return Err(StorageError::NotFound);
        }

        let tx = self.0.clone().begin().await?;
        let outcome = tx
            .query(DELETE_SESSION_CASCADE)
            .bind(("sess", record_id::<Session, _>(id)))
            .await
            .and_then(|response| response.check());

        match outcome {
            Ok(_) => {
                tx.commit().await?;
                tracing::debug!("deleted session {id} and its child rows");
                Ok(())
            }
            Err(err) => {
                let _ = tx.cancel().await;
                tracing::error!("failed to delete session {id}: {err}");
                Err(err.into())
            }
        }
    }

    async fn append_message(
        &self,
        user_id: Uuid,
        message: SessionMessage,
        content: SessionMessageContent,
    ) -> StorageResult<SessionMessage> {
        tracing::debug!("appending message for user {user_id}");
        self.assert_write_owned(user_id, &message.session, &message.user)
            .await?;
        self.create_paired(message, content).await
    }

    async fn append_routing_decision(
        &self,
        user_id: Uuid,
        decision: SessionRoutingDecision,
    ) -> StorageResult<SessionRoutingDecision> {
        tracing::debug!("appending routing decision for user {user_id}");
        self.assert_write_owned(user_id, &decision.session, &decision.user)
            .await?;
        self.create_one(decision).await
    }

    async fn append_context_reference(
        &self,
        user_id: Uuid,
        reference: SessionContextReference,
    ) -> StorageResult<SessionContextReference> {
        tracing::debug!("appending context reference for user {user_id}");
        self.assert_write_owned(user_id, &reference.session, &reference.user)
            .await?;
        self.create_one(reference).await
    }

    async fn append_tool_call(
        &self,
        user_id: Uuid,
        tool_call: SessionToolCall,
        content: SessionToolCallContent,
    ) -> StorageResult<SessionToolCall> {
        tracing::debug!("appending tool call for user {user_id}");
        self.assert_write_owned(user_id, &tool_call.session, &tool_call.user)
            .await?;
        self.create_paired(tool_call, content).await
    }

    async fn append_plan(
        &self,
        user_id: Uuid,
        plan: SessionPlan,
        content: SessionPlanContent,
    ) -> StorageResult<SessionPlan> {
        tracing::debug!("appending plan for user {user_id}");
        self.assert_write_owned(user_id, &plan.session, &plan.user)
            .await?;
        self.create_paired(plan, content).await
    }

    async fn append_diff(
        &self,
        user_id: Uuid,
        diff: SessionDiff,
        content: SessionDiffContent,
    ) -> StorageResult<SessionDiff> {
        tracing::debug!("appending diff for user {user_id}");
        self.assert_write_owned(user_id, &diff.session, &diff.user)
            .await?;
        self.create_paired(diff, content).await
    }

    async fn append_approval(
        &self,
        user_id: Uuid,
        approval: SessionApproval,
    ) -> StorageResult<SessionApproval> {
        tracing::debug!("appending approval for user {user_id}");
        self.assert_write_owned(user_id, &approval.session, &approval.user)
            .await?;
        self.create_one(approval).await
    }

    async fn append_trace_event(
        &self,
        user_id: Uuid,
        event: TraceEvent,
        content: TraceEventContent,
    ) -> StorageResult<TraceEvent> {
        tracing::debug!("appending trace event for user {user_id}");
        self.assert_write_owned(user_id, &event.session, &event.user)
            .await?;
        self.create_paired(event, content).await
    }

    async fn seal_content<C>(&self, id: Uuid, content: C) -> StorageResult<()>
    where
        C: Table + SurrealValue + Send + 'static,
    {
        tracing::debug!("sealing {table} content {id}", table = C::name());

        let updated: Option<C> = self
            .0
            .update(record_id::<C, _>(id))
            .content(content)
            .await?;
        updated.map(|_| ()).ok_or(StorageError::NotFound)
    }

    async fn get_session_state(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<SessionState>> {
        tracing::debug!("loading session state {id} for user {user_id}");

        let Some(session) = self.get_session(user_id, id).await? else {
            return Ok(None);
        };
        let session_id = session.id.clone();

        Ok(Some(SessionState {
            messages: self.list_session_rows(&session_id).await?,
            routing_decisions: self.list_session_rows(&session_id).await?,
            context_references: self.list_session_rows(&session_id).await?,
            tool_calls: self.list_session_rows(&session_id).await?,
            plans: self.list_session_rows(&session_id).await?,
            diffs: self.list_session_rows(&session_id).await?,
            approvals: self.list_session_rows(&session_id).await?,
            session,
        }))
    }

    async fn get_session_trace_events(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        pagination: Pagination,
    ) -> StorageResult<Option<Trace>> {
        tracing::debug!("loading trace events for session {session_id} and user {user_id}");

        let Some(session) = self.get_session(user_id, session_id).await? else {
            return Ok(None);
        };
        let owned_session = session.id.clone();

        // Window the events oldest first; an out-of-range page reads back empty
        // while the session itself still resolves, so the caller gets a 200 with
        // no events rather than a not-found.
        let events: Vec<TraceEvent> = self
            .0
            .query(
                "SELECT * FROM $table WHERE session = $sess \
                 ORDER BY created_at LIMIT $limit START $offset",
            )
            .bind(("table", TraceEvent::table()))
            .bind(("sess", owned_session))
            .bind(("limit", pagination.limit))
            .bind(("offset", pagination.offset))
            .await?
            .take(0)?;

        // The `_content` payloads share each event's record key; fetch only the
        // rows paired with this page rather than the whole session.
        let content_ids: Vec<RecordId> = events
            .iter()
            .map(|event| RecordId::new(TraceEventContent::name(), event.id.key.clone()))
            .collect();
        let contents: Vec<TraceEventContent> = self
            .0
            .query("SELECT * FROM $content_table WHERE id IN $ids")
            .bind(("content_table", TraceEventContent::table()))
            .bind(("ids", content_ids))
            .await?
            .take(0)?;

        // Map each storage row (metadata + paired content) to the core read-model
        // event. A plaintext payload is parsed into the typed `Payload`; an
        // encrypted one is surfaced as its sealed envelope, since storage holds
        // no key and so cannot open it.
        let mut read = Vec::with_capacity(events.len());
        for event in events {
            let Some(content) = contents
                .iter()
                .find(|content| content.id.key == event.id.key)
            else {
                // A paired write is transactional, so a base row without its
                // content only happens on an externally tampered store; skip it.
                continue;
            };
            let payload = match &content.payload {
                SecretContent::Plaintext(json) => {
                    TraceEventPayload::Plaintext(serde_json::from_str::<Payload>(json)?)
                }
                SecretContent::Encrypted(envelope) => {
                    TraceEventPayload::Encrypted(envelope.clone().into())
                }
            };
            read.push(CoreTraceEvent {
                event_type: event.event_type,
                task_type: event.task_type,
                provider: event.provider,
                model: event.model,
                matched_rule: event.matched_rule,
                created_at: event.created_at,
                payload,
            });
        }

        Ok(Some(Trace {
            session_id,
            events: read,
        }))
    }

    async fn record_user_memory(
        &self,
        user_id: Uuid,
        memory: UserMemory,
        content: UserMemoryContent,
    ) -> StorageResult<UserMemory> {
        tracing::debug!("recording user memory for user {user_id}");
        let user = record_id::<User, _>(user_id);
        if memory.user != user {
            tracing::error!("refusing to record user memory not owned by user {user_id}");
            return Err(StorageError::NotFound);
        }

        // A keyed memory upserts on `(user, key)`; a keyless one always inserts.
        if let Some(key) = memory.key.clone() {
            let existing: Option<UserMemory> = self
                .0
                .query("SELECT * FROM $table WHERE user = $user AND key = $key")
                .bind(("table", UserMemory::table()))
                .bind(("user", user))
                .bind(("key", key))
                .await?
                .take(0)?;
            if let Some(existing) = existing {
                tracing::debug!("replacing keyed user memory in place");
                return self
                    .replace_paired_content::<UserMemory, _>(
                        existing.id,
                        UserMemoryContent::name(),
                        memory.updated_at,
                        content.content,
                    )
                    .await;
            }
        }

        self.create_paired(memory, content).await
    }

    async fn update_user_memory(
        &self,
        user_id: Uuid,
        target: MemoryRef,
        content: UserMemoryContent,
    ) -> StorageResult<UserMemory> {
        tracing::debug!("updating user memory for user {user_id}");

        let existing: Option<UserMemory> = match &target {
            MemoryRef::Id(id) => self.get_user_memory(user_id, *id).await?,
            MemoryRef::Key(key) => self
                .0
                .query("SELECT * FROM $table WHERE user = $user AND key = $key")
                .bind(("table", UserMemory::table()))
                .bind(("user", record_id::<User, _>(user_id)))
                .bind(("key", key.clone()))
                .await?
                .take(0)?,
        };
        let Some(existing) = existing else {
            tracing::error!("user memory to update not found for user {user_id}");
            return Err(StorageError::NotFound);
        };

        self.replace_paired_content::<UserMemory, _>(
            existing.id,
            UserMemoryContent::name(),
            Utc::now(),
            content.content,
        )
        .await
    }

    async fn get_user_memory(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<UserMemory>> {
        tracing::debug!("getting user memory {id} for user {user_id}");

        let memory: Option<UserMemory> = self.0.select(record_id::<UserMemory, _>(id)).await?;
        Ok(memory.filter(|memory| memory.user == record_id::<User, _>(user_id)))
    }

    async fn list_user_memory(&self, user_id: Uuid) -> StorageResult<Vec<UserMemory>> {
        tracing::debug!("listing user memory for user {user_id}");

        self.0
            .query("SELECT * FROM $table WHERE user = $user ORDER BY updated_at DESC")
            .bind(("table", UserMemory::table()))
            .bind(("user", record_id::<User, _>(user_id)))
            .await?
            .take(0)
            .map_err(StorageError::from)
    }

    async fn list_user_memory_with_content(
        &self,
        user_id: Uuid,
    ) -> StorageResult<Vec<(UserMemory, UserMemoryContent)>> {
        tracing::debug!("listing user memory with content for user {user_id}");
        let user = record_id::<User, _>(user_id);

        let bases: Vec<UserMemory> = self
            .0
            .query("SELECT * FROM $table WHERE user = $user ORDER BY updated_at DESC")
            .bind(("table", UserMemory::table()))
            .bind(("user", user.clone()))
            .await?
            .take(0)?;

        // The `_content` payloads share each memory's record key; pair them by key.
        let contents: Vec<UserMemoryContent> = self
            .0
            .query(
                "SELECT * FROM $content_table WHERE record::id(id) IN \
                 (SELECT VALUE record::id(id) FROM $base_table WHERE user = $user)",
            )
            .bind(("content_table", UserMemoryContent::table()))
            .bind(("base_table", UserMemory::table()))
            .bind(("user", user))
            .await?
            .take(0)?;

        Ok(pair_by_key(
            bases,
            &contents,
            |memory| &memory.id,
            |content| &content.id,
        ))
    }

    async fn forget_user_memory(&self, user_id: Uuid, id: Uuid) -> StorageResult<()> {
        tracing::debug!("forgetting user memory {id} for user {user_id}");

        if self.get_user_memory(user_id, id).await?.is_none() {
            tracing::error!("refusing to forget user memory {id} not owned by user {user_id}");
            return Err(StorageError::NotFound);
        }

        self.forget_paired(record_id::<UserMemory, _>(id), UserMemoryContent::name())
            .await
    }

    async fn record_context_memory(
        &self,
        user_id: Uuid,
        memory: ContextMemory,
        content: ContextMemoryContent,
    ) -> StorageResult<ContextMemory> {
        tracing::debug!("recording context memory for user {user_id}");
        self.assert_write_owned(user_id, &memory.session, &memory.user)
            .await?;

        // A keyed memory upserts on `(session, key)`; a keyless one always inserts.
        if let Some(key) = memory.key.clone() {
            let existing: Option<ContextMemory> = self
                .0
                .query("SELECT * FROM $table WHERE session = $sess AND key = $key")
                .bind(("table", ContextMemory::table()))
                .bind(("sess", memory.session.clone()))
                .bind(("key", key))
                .await?
                .take(0)?;
            if let Some(existing) = existing {
                tracing::debug!("replacing keyed context memory in place");
                return self
                    .replace_paired_content::<ContextMemory, _>(
                        existing.id,
                        ContextMemoryContent::name(),
                        memory.updated_at,
                        content.content,
                    )
                    .await;
            }
        }

        self.create_paired(memory, content).await
    }

    async fn update_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        target: MemoryRef,
        content: ContextMemoryContent,
    ) -> StorageResult<ContextMemory> {
        tracing::debug!("updating context memory for user {user_id} in session {session_id}");

        let existing: Option<ContextMemory> = match &target {
            MemoryRef::Id(id) => self.get_context_memory(user_id, session_id, *id).await?,
            MemoryRef::Key(key) => self
                .0
                .query(
                    "SELECT * FROM $table WHERE session = $sess \
                         AND user = $user AND key = $key",
                )
                .bind(("table", ContextMemory::table()))
                .bind(("sess", record_id::<Session, _>(session_id)))
                .bind(("user", record_id::<User, _>(user_id)))
                .bind(("key", key.clone()))
                .await?
                .take(0)?,
        };
        let Some(existing) = existing else {
            tracing::error!("context memory to update not found for user {user_id}");
            return Err(StorageError::NotFound);
        };

        self.replace_paired_content::<ContextMemory, _>(
            existing.id,
            ContextMemoryContent::name(),
            Utc::now(),
            content.content,
        )
        .await
    }

    async fn get_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<ContextMemory>> {
        tracing::debug!("getting context memory {id} for user {user_id} in session {session_id}");

        let memory: Option<ContextMemory> =
            self.0.select(record_id::<ContextMemory, _>(id)).await?;
        Ok(memory.filter(|memory| {
            memory.user == record_id::<User, _>(user_id)
                && memory.session == record_id::<Session, _>(session_id)
        }))
    }

    async fn list_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> StorageResult<Vec<ContextMemory>> {
        tracing::debug!("listing context memory for user {user_id} in session {session_id}");

        self.0
            .query(
                "SELECT * FROM $table WHERE session = $sess \
                 AND user = $user ORDER BY updated_at DESC",
            )
            .bind(("table", ContextMemory::table()))
            .bind(("sess", record_id::<Session, _>(session_id)))
            .bind(("user", record_id::<User, _>(user_id)))
            .await?
            .take(0)
            .map_err(StorageError::from)
    }

    async fn list_context_memory_with_content(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> StorageResult<Vec<(ContextMemory, ContextMemoryContent)>> {
        tracing::debug!(
            "listing context memory with content for user {user_id} in session {session_id}"
        );
        let session = record_id::<Session, _>(session_id);
        let user = record_id::<User, _>(user_id);

        let bases: Vec<ContextMemory> = self
            .0
            .query(
                "SELECT * FROM $table WHERE session = $sess \
                 AND user = $user ORDER BY updated_at DESC",
            )
            .bind(("table", ContextMemory::table()))
            .bind(("sess", session.clone()))
            .bind(("user", user.clone()))
            .await?
            .take(0)?;

        // The `_content` payloads share each memory's record key; pair them by key.
        let contents: Vec<ContextMemoryContent> = self
            .0
            .query(
                "SELECT * FROM $content_table WHERE record::id(id) IN \
                 (SELECT VALUE record::id(id) FROM $base_table \
                  WHERE session = $sess AND user = $user)",
            )
            .bind(("content_table", ContextMemoryContent::table()))
            .bind(("base_table", ContextMemory::table()))
            .bind(("sess", session))
            .bind(("user", user))
            .await?
            .take(0)?;

        Ok(pair_by_key(
            bases,
            &contents,
            |memory| &memory.id,
            |content| &content.id,
        ))
    }

    async fn forget_context_memory(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        id: Uuid,
    ) -> StorageResult<()> {
        tracing::debug!(
            "forgetting context memory {id} for user {user_id} in session {session_id}"
        );

        if self
            .get_context_memory(user_id, session_id, id)
            .await?
            .is_none()
        {
            tracing::error!("refusing to forget context memory {id} not owned by user {user_id}");
            return Err(StorageError::NotFound);
        }

        self.forget_paired(
            record_id::<ContextMemory, _>(id),
            ContextMemoryContent::name(),
        )
        .await
    }

    async fn delete_expired_tokens(&self) -> StorageResult<()> {
        tracing::debug!("deleting expired and revoked auth tokens");

        self.0
            .query("DELETE auth_token WHERE expires_at <= time::now() OR revoked_at IS NOT NONE")
            .await?
            .check()?;

        Ok(())
    }

    async fn purge_old_sessions(&self, session_retention_days: u32) -> StorageResult<()> {
        tracing::debug!("purging sessions older than {session_retention_days} days");
        let cutoff = Utc::now() - Duration::days(i64::from(session_retention_days));

        // Archived sessions are governed by their own retention window, so the
        // age-based purge skips them.
        self.purge_sessions(
            "SELECT VALUE id FROM session WHERE archived_at IS NONE AND updated_at < $cutoff",
            cutoff,
        )
        .await
    }

    async fn purge_archived_sessions(
        &self,
        archived_session_retention_days: u32,
    ) -> StorageResult<()> {
        tracing::debug!(
            "purging sessions archived more than {archived_session_retention_days} days ago"
        );
        let cutoff = Utc::now() - Duration::days(i64::from(archived_session_retention_days));

        self.purge_sessions(
            "SELECT VALUE id FROM session WHERE archived_at IS NOT NONE AND archived_at < $cutoff",
            cutoff,
        )
        .await
    }

    async fn purge_traces(&self, trace_retention_days: u32) -> StorageResult<()> {
        tracing::debug!("purging trace events older than {trace_retention_days} days");
        let cutoff = Utc::now() - Duration::days(i64::from(trace_retention_days));

        let tx = self.0.clone().begin().await?;
        let outcome = tx
            .query(PURGE_TRACES)
            .bind(("cutoff", cutoff))
            .await
            .and_then(|response| response.check());

        match outcome {
            Ok(_) => {
                tx.commit().await?;
                Ok(())
            }
            Err(err) => {
                let _ = tx.cancel().await;
                Err(err.into())
            }
        }
    }
}
