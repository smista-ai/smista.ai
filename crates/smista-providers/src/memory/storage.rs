//! The [`MemoryStorage`] backend seam.
//!
//! Defines the trait the [`MemoryTool`](super::MemoryTool) writes through and
//! the caller reads from when building an agent preamble. Concrete backends
//! (the router's database, an in-memory fake for tests, an external service)
//! live elsewhere; keeping the seam here lets this crate stay free of any
//! storage dependency.

use super::{MemoryRecord, MemoryScope};

/// Stores and retrieves an agent's memories, scoped per call.
///
/// Implementors back the memory operations the model drives via the
/// [`MemoryTool`](super::MemoryTool) and the reads the caller performs to build
/// the agent preamble. The error type is left associated so a backend reports
/// its own failures without this crate depending on it.
///
/// A backend is a **long-lived, shared handle**, not one created per session,
/// so every method takes a [`MemoryScope`] identifying the user and session it
/// acts within. User-scoped methods read only the scope's user; session-scoped
/// methods are confined to its session.
pub trait MemoryStorage: Send + Sync {
    /// The error type returned by storage operations. Kept generic so the trait
    /// stays backend-agnostic: implementors map their own failures into it.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Records a user memory in `scope`. A `key` upserts on that topic,
    /// replacing any existing keyed entry; a keyless entry is always inserted.
    /// Returns the stored entry, whose `handle` targets it for later removal.
    fn put_user_memory(
        &self,
        scope: MemoryScope,
        key: Option<String>,
        content: String,
    ) -> impl Future<Output = Result<MemoryRecord, Self::Error>> + Send;

    /// Forgets the user memory in `scope` identified by `handle`. A missing
    /// entry is not an error.
    fn forget_user_memory(
        &self,
        scope: MemoryScope,
        handle: String,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Lists the scope's user memories, most relevant first, capped at `limit`
    /// when set.
    fn get_user_memories(
        &self,
        scope: MemoryScope,
        limit: Option<usize>,
    ) -> impl Future<Output = Result<Vec<MemoryRecord>, Self::Error>> + Send;

    /// Loads the scope's user memory filed under `key`, or [`None`] if there is
    /// none. Lets a caller resolve a key to its entry (and `handle`) without
    /// listing every memory.
    fn get_user_memory_by_key(
        &self,
        scope: MemoryScope,
        key: String,
    ) -> impl Future<Output = Result<Option<MemoryRecord>, Self::Error>> + Send;

    /// Records a session memory in `scope`. A `key` upserts on that topic; a
    /// keyless entry is always inserted. Returns the stored entry.
    fn put_session_memory(
        &self,
        scope: MemoryScope,
        key: Option<String>,
        content: String,
    ) -> impl Future<Output = Result<MemoryRecord, Self::Error>> + Send;

    /// Forgets the session memory in `scope` identified by `handle`. A missing
    /// entry is not an error.
    fn forget_session_memory(
        &self,
        scope: MemoryScope,
        handle: String,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Lists the scope's session memories, most relevant first, capped at
    /// `limit` when set.
    fn get_session_memories(
        &self,
        scope: MemoryScope,
        limit: Option<usize>,
    ) -> impl Future<Output = Result<Vec<MemoryRecord>, Self::Error>> + Send;

    /// Loads the scope's session memory filed under `key`, or [`None`] if there
    /// is none. Lets a caller resolve a key to its entry (and `handle`) without
    /// listing every memory.
    fn get_session_memory_by_key(
        &self,
        scope: MemoryScope,
        key: String,
    ) -> impl Future<Output = Result<Option<MemoryRecord>, Self::Error>> + Send;
}
