//! Recording and retrieval of session execution traces.
//!
//! The [`Tracer`] is the router's single entry point for the trace: the routing
//! stages call its `record_*` methods to append what they decided, and the
//! trace endpoint calls [`Tracer::traces`] to read the events back. It is
//! session-scoped — constructed once per `(session_id, user_id)` — and wraps the
//! storage handle, so a stage never touches the database directly.
//!
//! Each `record_*` method mirrors one [`TraceEventType`] variant. It takes the
//! shared [`TraceContext`] (the routing context every event carries) plus a
//! typed payload whose shape matches the `trace_event_content` row documented in
//! `docs/technical/schema.md`. The payload is serialized to JSON and stored as a
//! [`SecretContent`]; in clear for a normal session, and sealed in a follow-up
//! turn for an end-to-end encrypted one (see [`Tracer::encrypt_trace_event`]).
//!
//! Per the trace invariants, payloads never carry secret or restricted content:
//! context and tool payloads record references (paths, tool names, statuses),
//! not file bodies or raw tool output.

use chrono::Utc;
use smista_core::intent::TaskIntent;
use smista_core::model::Provider;
use smista_core::policy::Classification;
use smista_core::trace::{
    ApprovalPayload, ContextSelectionPayload, CostPayload, MessagePayload, Payload,
    RoutingDecisionPayload, ToolCallPayload, Trace, TraceEventType,
};
use smista_storage::StorageError;
use smista_storage::api::Pagination;
use smista_storage::database::Database as _;
use smista_storage::database::surreal::SurrealDatabase;
use smista_storage::entity::{Session, Table, TraceEvent, TraceEventContent, User};
use smista_storage::surrealdb::RecordId;
use smista_storage::types::{ContentEnvelope, SecretContent};
use uuid::Uuid;

/// The tracer is responsible for providing a simple interface to trace each event that must be tracked for each session.
/// It also provides a simple interface to query the traces for a given session.
#[derive(Debug, Clone)]
pub struct Tracer {
    /// The database used to store the traces.
    database: SurrealDatabase,
    /// The session id for which the traces are being tracked.
    session_id: Uuid,
    /// User id for which the traces are being tracked. This is used to filter the traces for a given user.
    user_id: Uuid,
}

/// Error type for [`Tracer`].
#[derive(Debug, thiserror::Error)]
pub enum TracerError {
    #[error("Database error: {0}")]
    Database(#[from] StorageError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for [`Tracer`].
pub type TracerResult<T> = Result<T, TracerError>;

/// The routing context every recorded event carries.
///
/// Mirrors the queryable metadata of a [`TraceEvent`]: the task that the
/// emitting stage served, the provider and model that served it, and the
/// routing rule that matched, if any. It is recorded alongside each event so a
/// [`Trace`] can be assembled from the events alone.
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// Task that the emitting routing context served.
    pub task_type: TaskIntent,
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Routing rule that matched, if any.
    pub matched_rule: Option<String>,
}

/// Arguments for [`Tracer::trace_event`].
struct TraceEventArgs {
    event_type: TraceEventType,
    task_type: TaskIntent,
    provider: Provider,
    model: String,
    matched_rule: Option<String>,
    content: SecretContent,
}

impl Tracer {
    /// Creates a new [`Tracer`] for the given `session_id` and `user_id`, using the provided `database`.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub fn new(database: SurrealDatabase, session_id: Uuid, user_id: Uuid) -> Self {
        Self {
            database,
            session_id,
            user_id,
        }
    }

    /// Records a [`TraceEventType::Message`] event.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the payload cannot be serialized or the event
    /// cannot be persisted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn record_message(
        &self,
        context: TraceContext,
        payload: MessagePayload,
    ) -> TracerResult<Uuid> {
        self.record(context, Payload::Message(payload)).await
    }

    /// Records a [`TraceEventType::Classification`] event.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the payload cannot be serialized or the event
    /// cannot be persisted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn record_classification(
        &self,
        context: TraceContext,
        classification: Classification,
    ) -> TracerResult<Uuid> {
        self.record(context, Payload::Classification(classification))
            .await
    }

    /// Records a [`TraceEventType::RoutingDecision`] event.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the payload cannot be serialized or the event
    /// cannot be persisted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn record_routing_decision(
        &self,
        context: TraceContext,
        payload: RoutingDecisionPayload,
    ) -> TracerResult<Uuid> {
        self.record(context, Payload::RoutingDecision(payload))
            .await
    }

    /// Records a [`TraceEventType::ContextSelection`] event.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the payload cannot be serialized or the event
    /// cannot be persisted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn record_context_selection(
        &self,
        context: TraceContext,
        payload: ContextSelectionPayload,
    ) -> TracerResult<Uuid> {
        self.record(context, Payload::ContextSelection(payload))
            .await
    }

    /// Records a [`TraceEventType::ToolCall`] event.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the payload cannot be serialized or the event
    /// cannot be persisted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn record_tool_call(
        &self,
        context: TraceContext,
        payload: ToolCallPayload,
    ) -> TracerResult<Uuid> {
        self.record(context, Payload::ToolCall(payload)).await
    }

    /// Records a [`TraceEventType::Approval`] event.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the payload cannot be serialized or the event
    /// cannot be persisted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn record_approval(
        &self,
        context: TraceContext,
        payload: ApprovalPayload,
    ) -> TracerResult<Uuid> {
        self.record(context, Payload::Approval(payload)).await
    }

    /// Records a [`TraceEventType::Cost`] event.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the payload cannot be serialized or the event
    /// cannot be persisted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn record_cost(
        &self,
        context: TraceContext,
        payload: CostPayload,
    ) -> TracerResult<Uuid> {
        self.record(context, Payload::Cost(payload)).await
    }

    /// Returns a page of the session's [`Trace`], oldest event first.
    ///
    /// The window is described by `pagination`. A session with no events in the
    /// window yields a [`Trace`] with no events; `None` is returned only when
    /// the session is absent or not owned by this tracer's user.
    ///
    /// # Errors
    ///
    /// Returns [`TracerError`] if the trace cannot be read.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    pub async fn traces(&self, pagination: Pagination) -> TracerResult<Option<Trace>> {
        self.database
            .get_session_trace_events(self.user_id, self.session_id, pagination)
            .await
            .map_err(TracerError::Database)
    }

    /// Serializes `payload` and appends an event with `context`; the event kind
    /// is taken from the [`Payload`] variant.
    async fn record(&self, context: TraceContext, payload: Payload) -> TracerResult<Uuid> {
        let event_type = payload.event_type();
        let content = SecretContent::plaintext(serde_json::to_string(&payload)?);
        self.trace_event(TraceEventArgs {
            event_type,
            task_type: context.task_type,
            provider: context.provider,
            model: context.model,
            matched_rule: context.matched_rule,
            content,
        })
        .await
    }

    /// Traces an event for the current session and user.
    ///
    /// Returns the shared record id of the persisted [`TraceEvent`] and its
    /// paired [`TraceEventContent`]. That id can later seal the content through
    /// [`Tracer::encrypt_trace_event`].
    async fn trace_event(
        &self,
        TraceEventArgs {
            event_type,
            task_type,
            provider,
            model,
            matched_rule,
            content,
        }: TraceEventArgs,
    ) -> TracerResult<Uuid> {
        // The event and its content share one record id, so the read path can
        // pair them by key (see `Database::get_session_trace_events`).
        let id = Uuid::now_v7();

        let event = TraceEvent {
            id: RecordId::new(TraceEvent::name(), id.to_string()),
            session: RecordId::new(Session::name(), self.session_id.to_string()),
            user: RecordId::new(User::name(), self.user_id.to_string()),
            event_type,
            task_type,
            provider,
            model,
            matched_rule,
            created_at: Utc::now(),
        };
        let content = TraceEventContent {
            id: RecordId::new(TraceEventContent::name(), id.to_string()),
            payload: content,
        };

        self.database
            .append_trace_event(self.user_id, event, content)
            .await
            .map_err(TracerError::Database)
            .map(|entity| {
                tracing::debug!(
                    "Traced event: {etype:?} with id: {id}, for session: {session_id} and user: {user_id}",
                    etype = entity.event_type,
                    session_id = self.session_id,
                    user_id = self.user_id
                );
                id
            })
    }

    /// Encrypts the content of a [`TraceEvent`] with the given `id` and [`ContentEnvelope`].
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired by the routing stages and trace endpoint later"
        )
    )]
    async fn encrypt_trace_event(&self, id: Uuid, content: ContentEnvelope) -> TracerResult<()> {
        self.database
            .seal_content(id, TraceEventContent::new(id, content.into()))
            .await
            .map_err(TracerError::Database)
    }
}

#[cfg(test)]
mod tests {
    use smista_core::api::ApprovalDecision;
    use smista_core::message::MessageRole;
    use smista_core::policy::{Confidence, IntentSource};
    use smista_core::tool::ToolCallStatus;
    use smista_core::trace::TraceEventPayload;
    use smista_storage::database::surreal::{SurrealBackend, SurrealOptions};

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

    /// Builds a tracer over a fresh in-memory database with its user and session
    /// already persisted, returning the tracer and both ids.
    async fn tracer() -> (Tracer, Uuid, Uuid) {
        let db = memory_db().await;
        let user_id = Uuid::now_v7();
        db.create_user(user_entity(user_id))
            .await
            .expect("failed to create user");
        let session_id = Uuid::now_v7();
        db.create_session(session_entity(session_id, user_id))
            .await
            .expect("failed to create session");

        (Tracer::new(db, session_id, user_id), session_id, user_id)
    }

    fn context() -> TraceContext {
        TraceContext {
            task_type: TaskIntent::Edit,
            provider: Provider::Anthropic,
            model: "claude".to_string(),
            matched_rule: Some("edit -> claude".to_string()),
        }
    }

    #[tokio::test]
    async fn should_seal_recorded_event_content() {
        let (tracer, _session_id, _user_id) = tracer().await;

        let id = tracer
            .record_message(
                context(),
                MessagePayload {
                    role: MessageRole::User,
                    provider: Provider::Anthropic,
                    model: "claude".to_string(),
                },
            )
            .await
            .expect("failed to record message");

        let envelope = ContentEnvelope {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        };
        tracer
            .encrypt_trace_event(id, envelope)
            .await
            .expect("failed to seal content");

        // After sealing, storage holds no key, so the read model surfaces the
        // sealed envelope rather than a readable payload.
        let trace = tracer
            .traces(Pagination::default())
            .await
            .expect("failed to read trace")
            .expect("trace not found");
        assert_eq!(trace.events.len(), 1);
        assert!(matches!(
            trace.events[0].payload,
            TraceEventPayload::Encrypted(_)
        ));
    }

    #[tokio::test]
    async fn should_record_every_event_type() {
        let (tracer, session_id, _user_id) = tracer().await;

        tracer
            .record_message(
                context(),
                MessagePayload {
                    role: MessageRole::User,
                    provider: Provider::Anthropic,
                    model: "claude".to_string(),
                },
            )
            .await
            .expect("failed to record message");
        tracer
            .record_classification(
                context(),
                Classification {
                    intent: TaskIntent::Review,
                    source: IntentSource::Inferred,
                    reason: "keyword 'review' matched".to_string(),
                    matched_rule: Some(0),
                    confidence: Some(Confidence::High),
                },
            )
            .await
            .expect("failed to record classification");
        tracer
            .record_routing_decision(
                context(),
                RoutingDecisionPayload {
                    provider: Provider::Anthropic,
                    model: "claude".to_string(),
                    matched_rule: Some("edit -> claude".to_string()),
                    fallback_used: false,
                    override_used: false,
                    reason: "best for edit".to_string(),
                },
            )
            .await
            .expect("failed to record routing decision");
        tracer
            .record_context_selection(
                context(),
                ContextSelectionPayload {
                    path: Some("src/auth/middleware.rs".to_string()),
                    kind: "file".to_string(),
                    included: true,
                    reason: "referenced by the prompt".to_string(),
                },
            )
            .await
            .expect("failed to record context selection");
        tracer
            .record_tool_call(
                context(),
                ToolCallPayload {
                    tool_name: "read_file".to_string(),
                    status: ToolCallStatus::Completed,
                    arguments: Some("src/auth/middleware.rs".to_string()),
                    result: None,
                    error: None,
                },
            )
            .await
            .expect("failed to record tool call");
        tracer
            .record_approval(
                context(),
                ApprovalPayload {
                    target_type: "tool_call".to_string(),
                    target_id: "c1".to_string(),
                    decision: ApprovalDecision::Approved,
                    reason: None,
                },
            )
            .await
            .expect("failed to record approval");
        tracer
            .record_cost(
                context(),
                CostPayload {
                    provider: Provider::Anthropic,
                    model: "claude".to_string(),
                    input_tokens: 1_200,
                    output_tokens: 500,
                    cost: Some("0.08".to_string()),
                },
            )
            .await
            .expect("failed to record cost");

        let trace = tracer
            .traces(Pagination::default())
            .await
            .expect("failed to read trace")
            .expect("trace not found");

        assert_eq!(trace.session_id, session_id);
        assert_eq!(trace.events.len(), 7);
        for event_type in [
            TraceEventType::Message,
            TraceEventType::Classification,
            TraceEventType::RoutingDecision,
            TraceEventType::ContextSelection,
            TraceEventType::ToolCall,
            TraceEventType::Approval,
            TraceEventType::Cost,
        ] {
            assert!(
                trace
                    .events
                    .iter()
                    .any(|event| event.event_type == event_type),
                "missing event of type {event_type:?}"
            );
        }
    }

    #[tokio::test]
    async fn should_serialize_classification_payload() {
        let (tracer, _session_id, _user_id) = tracer().await;

        tracer
            .record_classification(
                context(),
                Classification {
                    intent: TaskIntent::Review,
                    source: IntentSource::Inferred,
                    reason: "keyword 'review' matched".to_string(),
                    matched_rule: Some(2),
                    confidence: Some(Confidence::High),
                },
            )
            .await
            .expect("failed to record classification");

        let trace = tracer
            .traces(Pagination::default())
            .await
            .expect("failed to read trace")
            .expect("trace not found");

        let event = &trace.events[0];
        assert_eq!(event.event_type, TraceEventType::Classification);
        let TraceEventPayload::Plaintext(Payload::Classification(classification)) = &event.payload
        else {
            panic!(
                "expected a plaintext classification payload, got {:?}",
                event.payload
            );
        };
        assert_eq!(classification.intent, TaskIntent::Review);
        assert_eq!(classification.source, IntentSource::Inferred);
        assert_eq!(classification.confidence, Some(Confidence::High));
        assert_eq!(classification.matched_rule, Some(2));
    }

    #[tokio::test]
    async fn should_paginate_recorded_events() {
        let (tracer, _session_id, _user_id) = tracer().await;

        for index in 0..3u32 {
            tracer
                .record_message(
                    context(),
                    MessagePayload {
                        role: MessageRole::User,
                        provider: Provider::Anthropic,
                        model: format!("claude-{index}"),
                    },
                )
                .await
                .expect("failed to record message");
        }

        let first = tracer
            .traces(Pagination {
                limit: 2,
                offset: 0,
            })
            .await
            .expect("failed to read first page")
            .expect("trace not found");
        assert_eq!(first.events.len(), 2);

        let second = tracer
            .traces(Pagination {
                limit: 2,
                offset: 2,
            })
            .await
            .expect("failed to read second page")
            .expect("trace not found");
        assert_eq!(second.events.len(), 1);
    }

    #[tokio::test]
    async fn should_not_read_trace_of_unowned_session() {
        let (owner_tracer, session_id, _owner_id) = tracer().await;
        owner_tracer
            .record_message(
                context(),
                MessagePayload {
                    role: MessageRole::User,
                    provider: Provider::Anthropic,
                    model: "claude".to_string(),
                },
            )
            .await
            .expect("failed to record message");

        // A tracer for a different user sharing the same database must not see
        // the session's trace.
        let intruder_id = Uuid::now_v7();
        owner_tracer
            .database
            .create_user(user_entity(intruder_id))
            .await
            .expect("failed to create intruder");
        let intruder = Tracer::new(owner_tracer.database.clone(), session_id, intruder_id);

        assert!(
            intruder
                .traces(Pagination::default())
                .await
                .expect("failed to read trace")
                .is_none(),
            "trace disclosed to a non-owner"
        );
    }
}
