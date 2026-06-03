//! Session routing decision table.

use chrono::{DateTime, Utc};
use smista_core::intent::TaskIntent;
use smista_core::model::Provider;
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

/// Records which provider/model pair was selected for a task and why.
///
/// Metadata-only: there is no paired content table.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionRoutingDecision {
    /// Unique identifier for the decision.
    pub id: RecordId,
    /// Session the decision belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Task the decision routed.
    pub task_type: TaskIntent,
    /// Selected provider.
    pub provider: Provider,
    /// Selected model.
    pub model: String,
    /// Routing rule that matched, if any.
    pub matched_rule: Option<String>,
    /// Whether a fallback model was used.
    pub fallback_used: Option<bool>,
    /// Whether a manual override was used.
    pub override_used: Option<bool>,
    /// Why this provider/model was chosen.
    pub reason: String,
    /// When the decision was recorded.
    pub created_at: DateTime<Utc>,
}

impl Table for SessionRoutingDecision {
    fn name() -> &'static str {
        "session_routing_decision"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_session_routing_decision() {
        let id = RecordId::new(
            SessionRoutingDecision::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let decision = SessionRoutingDecision {
            id,
            session: session.clone(),
            user: user.clone(),
            task_type: TaskIntent::Edit,
            provider: Provider::OpenAI,
            model: "gpt".to_string(),
            matched_rule: Some("edit -> gpt".to_string()),
            fallback_used: Some(false),
            override_used: None,
            reason: "best for edits".to_string(),
            created_at: Utc::now(),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), decision).await;
    }
}
