//! Idempotent schema migration applied when a [`SurrealDatabase`] connects.
//!
//! [`apply`] runs once on connect, after namespace/database selection and any
//! sign-in. Every statement uses `IF NOT EXISTS`, so re-running it on an
//! already-initialized database (including a pre-provisioned remote) is a
//! no-op. Tables are declared `SCHEMALESS`: field typing is enforced in Rust
//! and deferred here, while the indexes below enforce the invariants that the
//! storage layer relies on — uniqueness of secret hashes and fast,
//! ownership-scoped lookups (there are no foreign keys to lean on).
//!
//! [`SurrealDatabase`]: super::SurrealDatabase

use surrealdb::Surreal;
use surrealdb::engine::any::Any;

/// The schema migration, run verbatim on every connect.
const MIGRATION: &str = r#"
-- Tables. SCHEMALESS: field shapes are validated in Rust; SCHEMAFULL typing is
-- deferred until the CRUD layer can verify each field mapping.
DEFINE TABLE IF NOT EXISTS user SCHEMALESS;
DEFINE TABLE IF NOT EXISTS auth_token SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_message SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_message_content SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_routing_decision SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_context_reference SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_tool_call SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_tool_call_content SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_plan SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_plan_content SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_diff SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_diff_content SCHEMALESS;
DEFINE TABLE IF NOT EXISTS session_approval SCHEMALESS;
DEFINE TABLE IF NOT EXISTS trace_event SCHEMALESS;
DEFINE TABLE IF NOT EXISTS trace_event_content SCHEMALESS;
DEFINE TABLE IF NOT EXISTS user_memory SCHEMALESS;
DEFINE TABLE IF NOT EXISTS user_memory_content SCHEMALESS;
DEFINE TABLE IF NOT EXISTS context_memory SCHEMALESS;
DEFINE TABLE IF NOT EXISTS context_memory_content SCHEMALESS;

-- Uniqueness of secret hashes. Both fields are always present, so a plain
-- UNIQUE index is safe and surfaces duplicates as a constraint violation.
DEFINE INDEX IF NOT EXISTS user_api_key_hash ON TABLE user FIELDS api_key_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS auth_token_token_hash ON TABLE auth_token FIELDS token_hash UNIQUE;

-- Ownership and child-lookup indexes. Every session-scoped query filters by
-- owner or parent; these keep those reads off a full table scan.
DEFINE INDEX IF NOT EXISTS session_user ON TABLE session FIELDS user;
DEFINE INDEX IF NOT EXISTS auth_token_user ON TABLE auth_token FIELDS user;
DEFINE INDEX IF NOT EXISTS session_message_session ON TABLE session_message FIELDS session;
DEFINE INDEX IF NOT EXISTS session_routing_decision_session ON TABLE session_routing_decision FIELDS session;
DEFINE INDEX IF NOT EXISTS session_context_reference_session ON TABLE session_context_reference FIELDS session;
DEFINE INDEX IF NOT EXISTS session_tool_call_session ON TABLE session_tool_call FIELDS session;
DEFINE INDEX IF NOT EXISTS session_plan_session ON TABLE session_plan FIELDS session;
DEFINE INDEX IF NOT EXISTS session_diff_session ON TABLE session_diff FIELDS session;
DEFINE INDEX IF NOT EXISTS session_approval_session ON TABLE session_approval FIELDS session;
DEFINE INDEX IF NOT EXISTS trace_event_session ON TABLE trace_event FIELDS session;

-- Keyed-memory lookups. Not UNIQUE: keyless memories carry a NULL key and may
-- repeat, so per-owner key uniqueness is enforced by the upsert logic, not the
-- index. The composite key still accelerates the by-key resolution.
DEFINE INDEX IF NOT EXISTS user_memory_user_key ON TABLE user_memory FIELDS user, key;
DEFINE INDEX IF NOT EXISTS context_memory_session_key ON TABLE context_memory FIELDS session, key;
"#;

/// Applies the schema migration to the connected database.
///
/// # Errors
///
/// Returns [`StorageError::Backend`](crate::StorageError::Backend) if any
/// `DEFINE` statement fails.
pub(super) async fn apply(db: &Surreal<Any>) -> crate::StorageResult<()> {
    tracing::debug!("applying SurrealDB schema migration");
    db.query(MIGRATION).await?.check()?;
    tracing::debug!("SurrealDB schema migration applied successfully");

    Ok(())
}
