#![doc(html_playground_url = "https://play.rust-lang.org")]
#![doc(html_favicon_url = "https://smista.ai/logo-150.png")]
#![doc(html_logo_url = "https://smista.ai/logo.png")]
//! # smista-storage
//!
//! Storage layer for smista.ai. Defines the persistence entities (users,
//! sessions, tokens, messages, routing decisions, context references, tool
//! calls, approvals, plans, diffs and memory) and the storage traits used to
//! access them. The entity types live in [`entity`].
//!
//! Application code depends on the storage traits rather than on SurrealDB
//! directly; SurrealDB-specific code stays behind this boundary and supports
//! both embedded (local-first) and remote (SaaS) deployments.
//!
//! ## Schema invariants
//!
//! These rules hold for every entity; see `docs/technical/schema.md` for the
//! authoritative reference.
//!
//! - **The record id is the id.** Each entity's [`surrealdb::types::RecordId`]
//!   (`table:⟨key⟩`) *is* its identity — there is no separate id column. Keys are
//!   UUIDv7 generated in Rust, so ids are portable and time-sortable.
//! - **Metadata and content are physically separated.** Every entity carrying
//!   sensitive free-form content has a paired 1:1 `<entity>_content` type sharing
//!   the same record id. The base type holds queryable metadata; the `_content`
//!   type holds only the payload that may later be encrypted.
//! - **Ownership is enforced at the query boundary.** Every session-scoped row
//!   stores `user` redundantly, so ownership checks never need a join.
//! - **Secrets are never stored raw.** Only hashes of API keys and tokens are
//!   persisted; tool-call payloads and diffs are sanitised before persistence.
//!
//! ## References, not foreign keys
//!
//! Relations are explicit [`surrealdb::types::RecordId`] references. SurrealDB
//! does not enforce foreign keys, so a reference is just a record id — there is
//! no automatic cascade. Deleting a session cascades to its rows explicitly in
//! the storage operations, not in the database.

pub mod api;
mod database;
pub mod entity;
mod error;
#[cfg(test)]
mod tests;

pub use self::database::Database;
pub use self::error::{StorageError, StorageResult};
