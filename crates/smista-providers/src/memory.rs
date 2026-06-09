//! Agent memory: long-term user facts and per-session context.
//!
//! Two things live here, sharing one backend:
//!
//! - [`MemoryStorage`] — the storage seam. A trait so the concrete backend
//!   (the router's database) is injected rather than hard-wired, keeping this
//!   crate free of any storage dependency.
//! - [`MemoryTool`] — the model-facing tool that lets an agent record and
//!   forget memories during a turn, backed by a [`MemoryStorage`].
//!
//! The same [`MemoryStorage`] also serves the caller directly: before a turn it
//! loads stored memories to build the agent's preamble, so what the model can
//! recall and what it can edit go through one interface.
//!
//! Memory is scoped two ways: user facts persist across sessions, while session
//! facts are isolated to a single session. A backend
//! is created already scoped to the user (and session) it serves, so the trait
//! methods carry no owner identifiers.

mod preamble;
mod storage;
mod tool;

use serde::{Deserialize, Serialize};

pub use self::preamble::build_preamble;
pub use self::storage::MemoryStorage;
pub use self::tool::{MemoryTool, MemoryToolError};

/// A single memory entry as surfaced to the caller.
///
/// Mirrors what the backend persists, minus bookkeeping the caller never needs
/// (timestamps, ownership): an opaque `handle` to target the entry later, an
/// optional `key` topic, and the remembered `content`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MemoryRecord {
    /// Opaque, backend-defined handle identifying this entry. Pass it back to
    /// `forget_*` to remove exactly this entry.
    pub handle: String,
    /// Optional topic. A keyed entry upserts on its key, so a later record with
    /// the same key replaces this one; keyless entries accumulate freely.
    pub key: Option<String>,
    /// The remembered fact.
    pub content: String,
}
