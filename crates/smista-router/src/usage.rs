//! Session usage aggregation.
//!
//! [`SessionUsage`] answers `GET /sessions/{id}/usage`. It reads every cost
//! trace event of a session in one query and folds them into a
//! [`SessionUsageResponse`]: the session total plus per-model and per-task-type
//! breakdowns. Token counts a provider never reported — including those sealed
//! in an encrypted session the router cannot open — are omitted rather than
//! guessed.

use rust_decimal::Decimal;
use smista_core::api::{ModelUsage, SessionUsageResponse, TaskTypeUsage};
use smista_core::intent::TaskIntent;
use smista_core::model::Provider;
use smista_core::trace::{CostPayload, Payload, TraceEvent, TraceEventPayload};
use smista_core::usage::Usage;
use smista_storage::StorageResult;
use smista_storage::database::Database as _;
use smista_storage::database::surreal::SurrealDatabase;
use uuid::Uuid;

/// Aggregates a single session's token and cost usage from its cost events.
#[derive(Debug)]
pub(crate) struct SessionUsage {
    /// Storage backend the cost events are read from.
    database: SurrealDatabase,
    /// Session whose usage is reported.
    session_id: Uuid,
    /// Owner the session must belong to.
    user_id: Uuid,
}

impl SessionUsage {
    /// Creates a [`SessionUsage`] scoped to `session_id` and its owner `user_id`.
    pub(crate) fn new(database: SurrealDatabase, session_id: Uuid, user_id: Uuid) -> Self {
        Self {
            database,
            session_id,
            user_id,
        }
    }

    /// Aggregates the session's usage from its cost trace events.
    ///
    /// Returns `None` when the session is absent, archived or owned by another
    /// user, so the caller answers `404` alike and never reveals it exists. A
    /// session that resolves but has no cost event yields an empty breakdown,
    /// not a `None`.
    pub(crate) async fn usage(&self) -> StorageResult<Option<SessionUsageResponse>> {
        let Some(events) = self
            .database
            .get_session_cost_events(self.user_id, self.session_id)
            .await?
        else {
            return Ok(None);
        };

        tracing::debug!(
            "aggregating usage from {count} cost events for session {session_id}",
            count = events.len(),
            session_id = self.session_id,
        );
        Ok(Some(aggregate(events)))
    }
}

/// Folds the session's cost events into the total and both breakdowns.
///
/// Breakdown entries keep first-seen order, which is oldest-first since the
/// events arrive ordered by `created_at`, so the result is deterministic.
fn aggregate(events: Vec<TraceEvent>) -> SessionUsageResponse {
    let mut total = UsageTally::default();
    let mut by_model: Vec<(Provider, String, UsageTally)> = Vec::new();
    let mut by_task_type: Vec<(TaskIntent, UsageTally)> = Vec::new();

    for event in &events {
        let cost = cost_payload(event);
        total.add(cost);

        match by_model
            .iter_mut()
            .find(|(provider, model, _)| *provider == event.provider && *model == event.model)
        {
            Some((_, _, tally)) => tally.add(cost),
            None => {
                let mut tally = UsageTally::default();
                tally.add(cost);
                by_model.push((event.provider.clone(), event.model.clone(), tally));
            }
        }

        match by_task_type
            .iter_mut()
            .find(|(task_type, _)| *task_type == event.task_type)
        {
            Some((_, tally)) => tally.add(cost),
            None => {
                let mut tally = UsageTally::default();
                tally.add(cost);
                by_task_type.push((event.task_type, tally));
            }
        }
    }

    SessionUsageResponse {
        total: total.into_usage(),
        by_model: by_model
            .into_iter()
            .map(|(provider, model, tally)| ModelUsage {
                provider,
                model,
                request_count: tally.request_count,
                usage: tally.into_usage(),
            })
            .collect(),
        by_task_type: by_task_type
            .into_iter()
            .map(|(task_type, tally)| TaskTypeUsage {
                task_type,
                request_count: tally.request_count,
                usage: tally.into_usage(),
            })
            .collect(),
    }
}

/// Currency every reported cost is priced in. The router prices in USD, which
/// `CostPayload` does not carry, so it is stamped on alongside a known cost.
const USAGE_CURRENCY: &str = "USD";

/// Returns the decoded cost payload of a cost event, or `None` when the payload
/// is sealed (an encrypted session) and so unreadable by the router.
fn cost_payload(event: &TraceEvent) -> Option<&CostPayload> {
    match &event.payload {
        TraceEventPayload::Plaintext(Payload::Cost(cost)) => Some(cost),
        _ => None,
    }
}

/// Running token and cost totals for one aggregation bucket.
#[derive(Debug, Default)]
struct UsageTally {
    /// Reported input tokens summed across the bucket's cost events.
    input_tokens: u64,
    /// Reported output tokens summed across the bucket's cost events.
    output_tokens: u64,
    /// Summed estimated cost, present only when at least one event priced it.
    estimated_cost: Option<Decimal>,
    /// Whether any readable cost event contributed token counts, so an
    /// all-encrypted bucket omits tokens rather than reporting a bare zero.
    has_tokens: bool,
    /// Number of cost events folded into the bucket.
    request_count: u32,
}

impl UsageTally {
    /// Folds one cost event into the tally.
    ///
    /// `cost` is `None` for an encrypted event the router cannot open, so only
    /// its request is counted and its tokens stay omitted rather than guessed.
    fn add(&mut self, cost: Option<&CostPayload>) {
        self.request_count += 1;
        let Some(cost) = cost else { return };

        self.input_tokens += cost.input_tokens;
        self.output_tokens += cost.output_tokens;
        self.has_tokens = true;
        if let Some(raw) = &cost.cost {
            match raw.parse::<Decimal>() {
                Ok(amount) => {
                    self.estimated_cost = Some(self.estimated_cost.unwrap_or_default() + amount);
                }
                // Stored costs are written as decimal strings; an unparsable one
                // is tampered data, so skip it rather than abort the report.
                Err(err) => tracing::warn!("skipping unparsable cost {raw:?}: {err}"),
            }
        }
    }

    /// Renders the tally as a [`Usage`], omitting tokens never reported. The
    /// currency is present exactly when a cost is, since it only qualifies one.
    fn into_usage(self) -> Usage {
        Usage {
            input_tokens: self.has_tokens.then_some(self.input_tokens),
            output_tokens: self.has_tokens.then_some(self.output_tokens),
            total_tokens: self
                .has_tokens
                .then_some(self.input_tokens + self.output_tokens),
            currency: self
                .estimated_cost
                .is_some()
                .then(|| USAGE_CURRENCY.to_string()),
            estimated_cost: self.estimated_cost,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use smista_core::model::Provider;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
    use smista_storage::entity::{Session, Table as _, User};
    use smista_storage::surrealdb::RecordId;
    use smista_storage::types::{ContentEnvelope, SecretContent};
    use uuid::Uuid;

    use super::*;
    use crate::trace::{SerializedPayload, TraceContext, Tracer};

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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            disabled_at: None,
        }
    }

    fn session_entity(id: Uuid, user_id: Uuid, key_id: Option<String>) -> Session {
        Session {
            id: RecordId::new(Session::name(), id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: Some("session".to_string()),
            encrypted: key_id.is_some(),
            key_id,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            archived_at: None,
        }
    }

    /// Builds a database with a user and a session already persisted.
    async fn seeded_db(key_id: Option<String>) -> (SurrealDatabase, Uuid, Uuid) {
        let db = memory_db().await;
        let user_id = Uuid::now_v7();
        db.create_user(user_entity(user_id))
            .await
            .expect("failed to create user");
        let session_id = Uuid::now_v7();
        db.create_session(session_entity(session_id, user_id, key_id))
            .await
            .expect("failed to create session");
        (db, user_id, session_id)
    }

    fn context(task_type: TaskIntent, provider: Provider, model: &str) -> TraceContext {
        TraceContext {
            task_type,
            provider,
            model: model.to_string(),
            matched_rule: None,
        }
    }

    fn plaintext_cost(
        provider: Provider,
        model: &str,
        input: u64,
        output: u64,
        cost: &str,
    ) -> SecretContent {
        let payload = SerializedPayload::cost(CostPayload {
            provider,
            model: model.to_string(),
            input_tokens: input,
            output_tokens: output,
            cost: Some(cost.to_string()),
        })
        .expect("failed to serialize cost payload");
        SecretContent::plaintext(payload.into_string())
    }

    fn envelope() -> ContentEnvelope {
        ContentEnvelope {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        }
    }

    /// Records a cost event carrying `content` for the session.
    async fn record_cost(
        db: &SurrealDatabase,
        user_id: Uuid,
        session_id: Uuid,
        context: TraceContext,
        content: SecretContent,
    ) {
        Tracer::new(db.clone(), session_id, user_id)
            .record_cost(context, content)
            .await
            .expect("failed to record cost");
    }

    #[tokio::test]
    async fn should_aggregate_usage_across_models_and_task_types() {
        let (db, user_id, session_id) = seeded_db(None).await;
        record_cost(
            &db,
            user_id,
            session_id,
            context(TaskIntent::Edit, Provider::OpenAI, "gpt-5.5-thinking"),
            plaintext_cost(Provider::OpenAI, "gpt-5.5-thinking", 5_000, 1_000, "0.30"),
        )
        .await;
        record_cost(
            &db,
            user_id,
            session_id,
            context(TaskIntent::Edit, Provider::OpenAI, "gpt-5.5-thinking"),
            plaintext_cost(Provider::OpenAI, "gpt-5.5-thinking", 3_000, 1_200, "0.01"),
        )
        .await;
        record_cost(
            &db,
            user_id,
            session_id,
            context(TaskIntent::Plan, Provider::Anthropic, "claude-sonnet"),
            plaintext_cost(Provider::Anthropic, "claude-sonnet", 4_000, 1_200, "0.18"),
        )
        .await;

        let response = SessionUsage::new(db, session_id, user_id)
            .usage()
            .await
            .expect("failed to aggregate usage")
            .expect("session not found");

        // Total folds every event.
        assert_eq!(response.total.input_tokens, Some(12_000));
        assert_eq!(response.total.output_tokens, Some(3_400));
        assert_eq!(response.total.total_tokens, Some(15_400));
        assert_eq!(response.total.estimated_cost, Some("0.49".parse().unwrap()));
        assert_eq!(response.total.currency.as_deref(), Some("USD"));

        // Two cost events of the same model collapse into one entry.
        assert_eq!(response.by_model.len(), 2);
        let openai = &response.by_model[0];
        assert_eq!(openai.provider, Provider::OpenAI);
        assert_eq!(openai.model, "gpt-5.5-thinking");
        assert_eq!(openai.request_count, 2);
        assert_eq!(openai.usage.input_tokens, Some(8_000));
        assert_eq!(openai.usage.total_tokens, Some(10_200));
        assert_eq!(openai.usage.estimated_cost, Some("0.31".parse().unwrap()));
        assert_eq!(response.by_model[1].provider, Provider::Anthropic);
        assert_eq!(response.by_model[1].request_count, 1);

        // Task-type breakdown groups by intent.
        assert_eq!(response.by_task_type.len(), 2);
        assert_eq!(response.by_task_type[0].task_type, TaskIntent::Edit);
        assert_eq!(response.by_task_type[0].request_count, 2);
        assert_eq!(response.by_task_type[1].task_type, TaskIntent::Plan);
        assert_eq!(
            response.by_task_type[1].usage.estimated_cost,
            Some("0.18".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn should_omit_tokens_for_a_sealed_cost_event() {
        let (db, user_id, session_id) = seeded_db(Some("kf_ab12".to_string())).await;
        record_cost(
            &db,
            user_id,
            session_id,
            context(TaskIntent::Edit, Provider::Anthropic, "claude-sonnet"),
            SecretContent::from(envelope()),
        )
        .await;

        let response = SessionUsage::new(db, session_id, user_id)
            .usage()
            .await
            .expect("failed to aggregate usage")
            .expect("session not found");

        // The router holds no key, so tokens and cost are omitted, but the
        // request is still counted and grouped by its plaintext metadata.
        assert_eq!(response.total.input_tokens, None);
        assert_eq!(response.total.total_tokens, None);
        assert_eq!(response.total.estimated_cost, None);
        assert_eq!(response.total.currency, None);
        assert_eq!(response.by_model.len(), 1);
        assert_eq!(response.by_model[0].provider, Provider::Anthropic);
        assert_eq!(response.by_model[0].request_count, 1);
        assert_eq!(response.by_model[0].usage.input_tokens, None);
        assert_eq!(response.by_task_type[0].request_count, 1);
    }

    #[tokio::test]
    async fn should_ignore_non_cost_events() {
        let (db, user_id, session_id) = seeded_db(None).await;
        // A non-cost event must not enter the usage report.
        Tracer::new(db.clone(), session_id, user_id)
            .record_classification(
                context(TaskIntent::Edit, Provider::OpenAI, "gpt-5.5-thinking"),
                SecretContent::plaintext("{\"type\":\"classification\"}".to_string()),
            )
            .await
            .expect("failed to record classification");
        record_cost(
            &db,
            user_id,
            session_id,
            context(TaskIntent::Edit, Provider::OpenAI, "gpt-5.5-thinking"),
            plaintext_cost(Provider::OpenAI, "gpt-5.5-thinking", 1_000, 200, "0.05"),
        )
        .await;

        let response = SessionUsage::new(db, session_id, user_id)
            .usage()
            .await
            .expect("failed to aggregate usage")
            .expect("session not found");

        assert_eq!(response.by_model.len(), 1);
        assert_eq!(response.by_model[0].request_count, 1);
        assert_eq!(response.total.input_tokens, Some(1_000));
    }

    #[tokio::test]
    async fn should_return_empty_breakdowns_for_a_session_without_usage() {
        let (db, user_id, session_id) = seeded_db(None).await;

        let response = SessionUsage::new(db, session_id, user_id)
            .usage()
            .await
            .expect("failed to aggregate usage")
            .expect("session not found");

        assert_eq!(response.total, Usage::default());
        assert!(response.by_model.is_empty());
        assert!(response.by_task_type.is_empty());
    }

    #[tokio::test]
    async fn should_return_none_for_an_unknown_session() {
        let (db, user_id, _session_id) = seeded_db(None).await;

        let response = SessionUsage::new(db, Uuid::now_v7(), user_id)
            .usage()
            .await
            .expect("failed to aggregate usage");

        assert!(response.is_none());
    }

    #[tokio::test]
    async fn should_return_none_for_another_users_session() {
        let (db, _user_id, session_id) = seeded_db(None).await;
        let other_user = Uuid::now_v7();
        db.create_user(user_entity(other_user))
            .await
            .expect("failed to create other user");

        // The session exists but is owned by someone else: reported as absent.
        let response = SessionUsage::new(db, session_id, other_user)
            .usage()
            .await
            .expect("failed to aggregate usage");

        assert!(response.is_none());
    }

    #[tokio::test]
    async fn should_return_none_for_an_archived_session() {
        let (db, user_id, session_id) = seeded_db(None).await;
        record_cost(
            &db,
            user_id,
            session_id,
            context(TaskIntent::Edit, Provider::OpenAI, "gpt-5.5-thinking"),
            plaintext_cost(Provider::OpenAI, "gpt-5.5-thinking", 1_000, 200, "0.05"),
        )
        .await;
        crate::session::Sessions::new(db.clone(), user_id)
            .open(session_id)
            .await
            .expect("failed to open session")
            .archive()
            .await
            .expect("failed to archive session");

        let response = SessionUsage::new(db, session_id, user_id)
            .usage()
            .await
            .expect("failed to aggregate usage");

        // An archived session is treated as gone.
        assert!(response.is_none());
    }
}
