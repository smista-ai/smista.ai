//! Session tool call tables.
//!
//! A [`SessionToolCall`] holds the queryable metadata of a tool request; its
//! sensitive arguments, result and error live in the paired
//! [`SessionToolCallContent`] table under the same record id and are sanitised
//! for secrets before persistence.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;
use crate::types::SecretContent;

/// Execution status of a tool call.
///
/// **Provisional**: these variants are a placeholder until the private spec
/// pins the tool-call lifecycle. Serialized as `snake_case`.
#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
#[surreal(rename_all = "snake_case", untagged)]
pub enum ToolCallStatus {
    /// Requested, not yet started.
    Pending,
    /// Currently executing.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
}

/// Records a tool request and its execution result.
///
/// Arguments, result and error are sensitive and live in
/// [`SessionToolCallContent`]; only queryable metadata stays here.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionToolCall {
    /// Unique identifier for the tool call.
    pub id: RecordId,
    /// Session the tool call belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Name of the invoked tool.
    pub tool_name: String,
    /// Execution status.
    pub status: ToolCallStatus,
    /// When the tool call was requested.
    pub created_at: DateTime<Utc>,
    /// When the tool call completed, if applicable.
    pub completed_at: Option<DateTime<Utc>>,
}

impl Table for SessionToolCall {
    fn name() -> &'static str {
        "session_tool_call"
    }
}

/// The sanitised payload of a [`SessionToolCall`], paired 1:1 by record id.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionToolCallContent {
    /// Record id, identical to the owning [`SessionToolCall`].
    pub id: RecordId,
    /// Tool-call arguments (sanitised), in clear or sealed for an encrypted session.
    pub arguments: SecretContent,
    /// Tool-call result (sanitised), in clear or sealed for an encrypted session.
    pub result: Option<SecretContent>,
    /// Error, if the tool call failed, in clear or sealed for an encrypted session.
    pub error: Option<SecretContent>,
}

impl Table for SessionToolCallContent {
    fn name() -> &'static str {
        "session_tool_call_content"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_session_tool_call() {
        let id = RecordId::new(SessionToolCall::name(), uuid::Uuid::now_v7().to_string());
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let tool_call = SessionToolCall {
            id,
            session: session.clone(),
            user: user.clone(),
            tool_name: "read_file".to_string(),
            status: ToolCallStatus::Completed,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), tool_call).await;
    }

    #[tokio::test]
    async fn should_store_and_read_session_tool_call_content() {
        let id = RecordId::new(
            SessionToolCallContent::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let content = SessionToolCallContent {
            id,
            arguments: SecretContent::plaintext("{\"path\":\"src/main.rs\"}"),
            result: Some(SecretContent::plaintext("file contents")),
            error: None,
        };

        crate::tests::roundtrip(content).await;
    }
}
