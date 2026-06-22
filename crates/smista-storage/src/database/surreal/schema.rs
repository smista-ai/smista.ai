//! Idempotent schema migration applied when a [`SurrealDatabase`] connects.
//!
//! [`apply`] runs once on connect, after namespace/database selection and any
//! sign-in. Every statement uses `IF NOT EXISTS`, so re-running it on an
//! already-initialized database (including a pre-provisioned remote) is a
//! no-op. Tables are declared `SCHEMAFULL`: every field is typed here, so the
//! database rejects unknown or mistyped fields. Reference fields are typed as
//! `record<…>` and carry an `ASSERT record::exists($value)` guard, giving a
//! foreign-key-like existence check on write. SurrealDB still performs no
//! cascade or restrict on delete, so ownership cascade and cleanup stay the
//! storage layer's responsibility. The indexes below enforce the remaining
//! invariants: uniqueness of secret hashes and fast, ownership-scoped lookups.
//!
//! [`SurrealDatabase`]: super::SurrealDatabase

use surrealdb::Surreal;
use surrealdb::engine::any::Any;

/// The schema migration, run verbatim on every connect.
const MIGRATION: &str = r#"
-- Tables. SCHEMAFULL: every field is typed below, so the database rejects
-- unknown or mistyped fields. The record id (`id`) is implicit and never
-- declared as a field.
DEFINE TABLE IF NOT EXISTS user SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS auth_token SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_message SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_message_content SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_routing_decision SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_context_reference SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_tool_call SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_tool_call_content SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_plan SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_plan_content SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_diff SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_diff_content SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_approval SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS session_run_state SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS trace_event SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS trace_event_content SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS user_memory SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS user_memory_content SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS context_memory SCHEMAFULL;
DEFINE TABLE IF NOT EXISTS context_memory_content SCHEMAFULL;

-- Fields. Reference fields are typed `record<…>` and assert the referenced
-- row exists at write time — a foreign-key-like guard. SurrealDB does not
-- cascade or restrict on delete, so cascade and cleanup stay in Rust. The
-- core enums (role, provider, task_type, decision, status, event_type)
-- serialize to plain strings, so they are typed `string`.
--
-- Encryptable content fields hold a `SecretContent`, which serializes to an
-- externally-tagged object: `{ "Plaintext": "…" }` for a normal session or
-- `{ "Encrypted": { version, algorithm, key_id, nonce, ciphertext } }` for an
-- end-to-end encrypted one. They are typed `TYPE object FLEXIBLE` so either
-- shape is accepted. `user_memory_content` is not yet encryptable (it is
-- user-scoped, outside the per-session flag) and stays `string`.

-- user
DEFINE FIELD IF NOT EXISTS api_key_hash ON TABLE user TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE user TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE user TYPE datetime;
DEFINE FIELD IF NOT EXISTS disabled_at ON TABLE user TYPE option<datetime>;

-- auth_token
DEFINE FIELD IF NOT EXISTS user ON TABLE auth_token TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE auth_token TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE auth_token TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE auth_token TYPE datetime;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE auth_token TYPE option<datetime>;

-- session
DEFINE FIELD IF NOT EXISTS user ON TABLE session TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS title ON TABLE session TYPE option<string>;
DEFINE FIELD IF NOT EXISTS encrypted ON TABLE session TYPE bool;
DEFINE FIELD IF NOT EXISTS key_id ON TABLE session TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE session TYPE datetime;
DEFINE FIELD IF NOT EXISTS archived_at ON TABLE session TYPE option<datetime>;

-- session_message
DEFINE FIELD IF NOT EXISTS session ON TABLE session_message TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_message TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS role ON TABLE session_message TYPE string;
DEFINE FIELD IF NOT EXISTS provider ON TABLE session_message TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE session_message TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session_message TYPE datetime;

-- session_message_content
DEFINE FIELD IF NOT EXISTS content ON TABLE session_message_content TYPE object FLEXIBLE;

-- session_routing_decision
DEFINE FIELD IF NOT EXISTS session ON TABLE session_routing_decision TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_routing_decision TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS task_type ON TABLE session_routing_decision TYPE string;
DEFINE FIELD IF NOT EXISTS provider ON TABLE session_routing_decision TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE session_routing_decision TYPE string;
DEFINE FIELD IF NOT EXISTS matched_rule ON TABLE session_routing_decision TYPE option<string>;
DEFINE FIELD IF NOT EXISTS fallback_used ON TABLE session_routing_decision TYPE option<bool>;
DEFINE FIELD IF NOT EXISTS override_used ON TABLE session_routing_decision TYPE option<bool>;
DEFINE FIELD IF NOT EXISTS reason ON TABLE session_routing_decision TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session_routing_decision TYPE datetime;

-- session_context_reference
DEFINE FIELD IF NOT EXISTS session ON TABLE session_context_reference TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_context_reference TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS path ON TABLE session_context_reference TYPE option<string>;
DEFINE FIELD IF NOT EXISTS kind ON TABLE session_context_reference TYPE string;
DEFINE FIELD IF NOT EXISTS included ON TABLE session_context_reference TYPE bool;
DEFINE FIELD IF NOT EXISTS reason ON TABLE session_context_reference TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session_context_reference TYPE datetime;

-- session_tool_call
DEFINE FIELD IF NOT EXISTS session ON TABLE session_tool_call TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_tool_call TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS tool_name ON TABLE session_tool_call TYPE string;
DEFINE FIELD IF NOT EXISTS status ON TABLE session_tool_call TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session_tool_call TYPE datetime;
DEFINE FIELD IF NOT EXISTS completed_at ON TABLE session_tool_call TYPE option<datetime>;

-- session_tool_call_content
DEFINE FIELD IF NOT EXISTS arguments ON TABLE session_tool_call_content TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS result ON TABLE session_tool_call_content TYPE option<object> FLEXIBLE;
DEFINE FIELD IF NOT EXISTS error ON TABLE session_tool_call_content TYPE option<object> FLEXIBLE;

-- session_approval
DEFINE FIELD IF NOT EXISTS session ON TABLE session_approval TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_approval TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS target_type ON TABLE session_approval TYPE string;
DEFINE FIELD IF NOT EXISTS target_id ON TABLE session_approval TYPE string;
DEFINE FIELD IF NOT EXISTS decision ON TABLE session_approval TYPE string;
DEFINE FIELD IF NOT EXISTS reason ON TABLE session_approval TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session_approval TYPE datetime;

-- session_run_state. Metadata-only: `phase` is the `RunPhase` enum, which the
-- SurrealValue derive encodes as an externally-tagged object (every variant,
-- including the payload-free ones), so it is typed `object FLEXIBLE`.
DEFINE FIELD IF NOT EXISTS session ON TABLE session_run_state TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_run_state TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS run_id ON TABLE session_run_state TYPE string;
DEFINE FIELD IF NOT EXISTS phase ON TABLE session_run_state TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE session_run_state TYPE datetime;

-- session_plan
DEFINE FIELD IF NOT EXISTS session ON TABLE session_plan TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_plan TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS path ON TABLE session_plan TYPE string;
DEFINE FIELD IF NOT EXISTS status ON TABLE session_plan TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session_plan TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE session_plan TYPE datetime;
DEFINE FIELD IF NOT EXISTS approved_at ON TABLE session_plan TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS content_hash ON TABLE session_plan TYPE option<string>;

-- session_plan_content
DEFINE FIELD IF NOT EXISTS content_snapshot ON TABLE session_plan_content TYPE option<object> FLEXIBLE;

-- session_diff
DEFINE FIELD IF NOT EXISTS session ON TABLE session_diff TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE session_diff TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS path ON TABLE session_diff TYPE string;
DEFINE FIELD IF NOT EXISTS status ON TABLE session_diff TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE session_diff TYPE datetime;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE session_diff TYPE option<datetime>;

-- session_diff_content
DEFINE FIELD IF NOT EXISTS diff ON TABLE session_diff_content TYPE object FLEXIBLE;

-- trace_event
DEFINE FIELD IF NOT EXISTS session ON TABLE trace_event TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE trace_event TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS event_type ON TABLE trace_event TYPE string;
DEFINE FIELD IF NOT EXISTS task_type ON TABLE trace_event TYPE string;
DEFINE FIELD IF NOT EXISTS provider ON TABLE trace_event TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE trace_event TYPE string;
DEFINE FIELD IF NOT EXISTS matched_rule ON TABLE trace_event TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE trace_event TYPE datetime;

-- trace_event_content
DEFINE FIELD IF NOT EXISTS payload ON TABLE trace_event_content TYPE object FLEXIBLE;

-- user_memory
DEFINE FIELD IF NOT EXISTS user ON TABLE user_memory TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS key ON TABLE user_memory TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE user_memory TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE user_memory TYPE datetime;

-- user_memory_content
DEFINE FIELD IF NOT EXISTS content ON TABLE user_memory_content TYPE string;

-- context_memory
DEFINE FIELD IF NOT EXISTS session ON TABLE context_memory TYPE record<session> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS user ON TABLE context_memory TYPE record<user> ASSERT record::exists($value);
DEFINE FIELD IF NOT EXISTS key ON TABLE context_memory TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE context_memory TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE context_memory TYPE datetime;

-- context_memory_content
DEFINE FIELD IF NOT EXISTS content ON TABLE context_memory_content TYPE object FLEXIBLE;

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
DEFINE INDEX IF NOT EXISTS session_run_state_session ON TABLE session_run_state FIELDS session UNIQUE;
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
