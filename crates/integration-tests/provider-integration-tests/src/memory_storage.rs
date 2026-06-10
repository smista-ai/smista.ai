//! In-memory storage implementation for provider integration tests.

use std::collections::HashMap;
use std::sync::Mutex;

use smista_providers::memory::{MemoryRecord, MemoryStorage};

/// In-memory [`MemoryStorage`], keyed by `key` within each scope.
///
/// Mirrors how a real backend resolves a key to a `handle`: the handle is just
/// `handle:{key}`, so `forget` can strip the prefix to find the entry. Suitable
/// for any suite that needs a working memory backend without a database —
/// whether it stores facts (the memory-tool suite) or never writes at all (the
/// model-completion suite).
#[derive(Debug, Default)]
pub struct InMemoryStorage {
    user: Mutex<HashMap<String, String>>,
    session: Mutex<HashMap<String, String>>,
}

/// Error type for [`InMemoryStorage`]; the fake never actually fails.
#[derive(Debug, thiserror::Error)]
#[error("in-memory storage error")]
pub struct InMemoryError;

impl InMemoryStorage {
    fn put(
        store: &Mutex<HashMap<String, String>>,
        key: Option<String>,
        content: String,
    ) -> MemoryRecord {
        let key = key.unwrap_or_default();
        store.lock().unwrap().insert(key.clone(), content.clone());
        MemoryRecord {
            handle: format!("handle:{key}"),
            key: Some(key),
            content,
        }
    }

    fn forget(store: &Mutex<HashMap<String, String>>, handle: &str) {
        let key = handle.strip_prefix("handle:").unwrap_or(handle);
        store.lock().unwrap().remove(key);
    }

    fn list(store: &Mutex<HashMap<String, String>>) -> Vec<MemoryRecord> {
        store
            .lock()
            .unwrap()
            .iter()
            .map(|(key, content)| MemoryRecord {
                handle: format!("handle:{key}"),
                key: Some(key.clone()),
                content: content.clone(),
            })
            .collect()
    }

    fn by_key(store: &Mutex<HashMap<String, String>>, key: &str) -> Option<MemoryRecord> {
        store.lock().unwrap().get(key).map(|content| MemoryRecord {
            handle: format!("handle:{key}"),
            key: Some(key.to_string()),
            content: content.clone(),
        })
    }
}

impl MemoryStorage for InMemoryStorage {
    type Error = InMemoryError;

    async fn put_user_memory(
        &self,
        key: Option<String>,
        content: String,
    ) -> Result<MemoryRecord, Self::Error> {
        Ok(Self::put(&self.user, key, content))
    }

    async fn forget_user_memory(&self, handle: String) -> Result<(), Self::Error> {
        Self::forget(&self.user, &handle);
        Ok(())
    }

    async fn get_user_memories(
        &self,
        _limit: Option<usize>,
    ) -> Result<Vec<MemoryRecord>, Self::Error> {
        Ok(Self::list(&self.user))
    }

    async fn get_user_memory_by_key(
        &self,
        key: String,
    ) -> Result<Option<MemoryRecord>, Self::Error> {
        Ok(Self::by_key(&self.user, &key))
    }

    async fn put_session_memory(
        &self,
        key: Option<String>,
        content: String,
    ) -> Result<MemoryRecord, Self::Error> {
        Ok(Self::put(&self.session, key, content))
    }

    async fn forget_session_memory(&self, handle: String) -> Result<(), Self::Error> {
        Self::forget(&self.session, &handle);
        Ok(())
    }

    async fn get_session_memories(
        &self,
        _limit: Option<usize>,
    ) -> Result<Vec<MemoryRecord>, Self::Error> {
        Ok(Self::list(&self.session))
    }

    async fn get_session_memory_by_key(
        &self,
        key: String,
    ) -> Result<Option<MemoryRecord>, Self::Error> {
        Ok(Self::by_key(&self.session, &key))
    }
}
