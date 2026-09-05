//! The model-facing memory tool.
//!
//! [`MemoryTool`] exposes record and forget operations to an agent during a
//! turn, translating the model's request into calls on a [`MemoryStorage`]
//! backend. Recall is intentionally absent: stored memories are injected into
//! the agent preamble up front (see [`build_preamble`](super::build_preamble)),
//! so the model reads them as context rather than fetching them through a tool.
//!
//! Both operations are addressed by `key`. A memory is recorded under a key and
//! later forgotten by that same key, so the model never deals with the
//! backend's opaque handle: `forget` resolves the key to a handle internally.

use std::sync::Arc;

use rig_core::tool::PortableTool;
use serde::Deserialize;

use crate::memory::MemoryScope;
use crate::memory::storage::MemoryStorage;

/// Operation the model asks the [`MemoryTool`] to perform on memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOp {
    /// Record a new fact, or replace the keyed fact on the same topic.
    Record,
    /// Forget a previously recorded fact.
    Forget,
}

/// Which memory store an operation targets: the user's or the session's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStore {
    /// A long-term fact about the user, shared across all of their sessions.
    User,
    /// A fact local to one session, invisible to other sessions of the same user.
    Session,
}

impl MemoryStore {
    /// Lower-case label used when reporting an operation back to the model.
    const fn label(self) -> &'static str {
        match self {
            MemoryStore::User => "user",
            MemoryStore::Session => "session",
        }
    }
}

/// Arguments accepted by the [`MemoryTool`], deserialized from the model's tool
/// call.
///
/// `value` is required for [`MemoryOp::Record`] and ignored for
/// [`MemoryOp::Forget`].
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryArgs {
    /// The operation to perform.
    op: MemoryOp,
    /// Which store the operation targets.
    scope: MemoryStore,
    /// Topic the fact is filed under; also how it is later forgotten.
    key: String,
    /// The fact to record. Required for `record`, ignored for `forget`.
    #[serde(default)]
    value: Option<String>,
}

/// Errors raised by the [`MemoryTool`].
#[derive(Debug, thiserror::Error)]
pub enum MemoryToolError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    /// A `record` operation was requested without a `value`.
    #[error("a value is required to record a memory")]
    MissingValue,

    /// The backing [`MemoryStorage`] failed.
    #[error(transparent)]
    Storage(#[from] E),
}

/// A tool that lets an agent record and forget memories during a turn.
///
/// Generic over the [`MemoryStorage`] backend, which is a shared, long-lived
/// handle. The tool carries the [`MemoryScope`] it serves and passes it on
/// every call; the same backend can therefore also back the caller's preamble
/// loading for any scope.
pub struct MemoryTool<S>
where
    S: MemoryStorage,
{
    /// The shared backend this tool records into and forgets from.
    storage: Arc<S>,
    /// The user and session this tool's operations are scoped to.
    scope: MemoryScope,
}

impl<S> MemoryTool<S>
where
    S: MemoryStorage,
{
    /// Creates a new [`MemoryTool`] backed by `storage`, scoped to `scope`.
    pub fn new(storage: Arc<S>, scope: MemoryScope) -> Self {
        Self { storage, scope }
    }

    /// Resolves a `key` to its backend handle within `store`, if present.
    async fn handle_for(
        &self,
        store: MemoryStore,
        key: String,
    ) -> Result<Option<String>, S::Error> {
        let memory = match store {
            MemoryStore::User => self.storage.get_user_memory_by_key(self.scope, key).await?,
            MemoryStore::Session => {
                self.storage
                    .get_session_memory_by_key(self.scope, key)
                    .await?
            }
        };

        Ok(memory.map(|memory| memory.handle))
    }
}

impl<S> PortableTool for MemoryTool<S>
where
    S: MemoryStorage + 'static,
{
    const NAME: &'static str = "memory";

    type Args = MemoryArgs;
    type Output = String;
    type Error = MemoryToolError<S::Error>;

    fn description(&self) -> String {
        concat!(
            "Record or forget a memory addressed by `key`.\n\n",
            "Use scope `user` for durable facts about the user that should ",
            "persist across sessions (preferences, identity, long-lived ",
            "context). Use scope `session` for working memory tied to the ",
            "current session only.\n\n",
            "Operations:\n",
            "- `record`: store `value` under `key`, replacing any existing ",
            "fact filed under the same key.\n",
            "- `forget`: remove the fact filed under `key`.\n\n",
            "You do not need to recall memories: everything recorded is ",
            "already provided to you as context at the start of the turn."
        )
        .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "op": {
                    "type": "string",
                    "enum": ["record", "forget"],
                    "description": "The operation to perform."
                },
                "scope": {
                    "type": "string",
                    "enum": ["user", "session"],
                    "description": "Which store to target: durable user memory or session-only working memory."
                },
                "key": {
                    "type": "string",
                    "description": "Topic the fact is filed under; reuse the same key to replace or forget it."
                },
                "value": {
                    "type": "string",
                    "description": "The fact to record. Required for `record`, ignored for `forget`."
                }
            },
            "required": ["op", "scope", "key"]
        })
    }

    async fn call(&self, args: MemoryArgs) -> Result<String, Self::Error> {
        let MemoryArgs {
            op,
            scope: store,
            key,
            value,
        } = args;

        match op {
            MemoryOp::Record => {
                let value = value.ok_or(MemoryToolError::MissingValue)?;
                match store {
                    MemoryStore::User => {
                        self.storage
                            .put_user_memory(self.scope, Some(key.clone()), value)
                            .await?;
                    }
                    MemoryStore::Session => {
                        self.storage
                            .put_session_memory(self.scope, Some(key.clone()), value)
                            .await?;
                    }
                }
                Ok(format!("Recorded {} memory \"{key}\".", store.label()))
            }
            MemoryOp::Forget => match self.handle_for(store, key.clone()).await? {
                Some(handle) => {
                    match store {
                        MemoryStore::User => {
                            self.storage.forget_user_memory(self.scope, handle).await?;
                        }
                        MemoryStore::Session => {
                            self.storage
                                .forget_session_memory(self.scope, handle)
                                .await?;
                        }
                    }
                    Ok(format!("Forgot {} memory \"{key}\".", store.label()))
                }
                None => Ok(format!("No {} memory found for \"{key}\".", store.label())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use uuid::Uuid;

    use super::*;
    use crate::memory::MemoryRecord;

    /// In-memory [`MemoryStorage`] fake keyed by `key` within each scope.
    #[derive(Default)]
    struct FakeStorage {
        user: Mutex<HashMap<String, String>>,
        session: Mutex<HashMap<String, String>>,
    }

    #[derive(Debug, thiserror::Error)]
    #[error("fake storage error")]
    struct FakeError;

    impl FakeStorage {
        fn records(store: &Mutex<HashMap<String, String>>) -> Vec<MemoryRecord> {
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

        fn record_by_key(
            store: &Mutex<HashMap<String, String>>,
            key: &str,
        ) -> Option<MemoryRecord> {
            store.lock().unwrap().get(key).map(|content| MemoryRecord {
                handle: format!("handle:{key}"),
                key: Some(key.to_string()),
                content: content.clone(),
            })
        }
    }

    impl MemoryStorage for FakeStorage {
        type Error = FakeError;

        async fn put_user_memory(
            &self,
            _scope: MemoryScope,
            key: Option<String>,
            content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            let key = key.unwrap_or_default();
            self.user
                .lock()
                .unwrap()
                .insert(key.clone(), content.clone());
            Ok(MemoryRecord {
                handle: format!("handle:{key}"),
                key: Some(key),
                content,
            })
        }

        async fn forget_user_memory(
            &self,
            _scope: MemoryScope,
            handle: String,
        ) -> Result<(), Self::Error> {
            let key = handle.strip_prefix("handle:").unwrap_or(&handle);
            self.user.lock().unwrap().remove(key);
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Self::records(&self.user))
        }

        async fn get_user_memory_by_key(
            &self,
            _scope: MemoryScope,
            key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(Self::record_by_key(&self.user, &key))
        }

        async fn put_session_memory(
            &self,
            _scope: MemoryScope,
            key: Option<String>,
            content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            let key = key.unwrap_or_default();
            self.session
                .lock()
                .unwrap()
                .insert(key.clone(), content.clone());
            Ok(MemoryRecord {
                handle: format!("handle:{key}"),
                key: Some(key),
                content,
            })
        }

        async fn forget_session_memory(
            &self,
            _scope: MemoryScope,
            handle: String,
        ) -> Result<(), Self::Error> {
            let key = handle.strip_prefix("handle:").unwrap_or(&handle);
            self.session.lock().unwrap().remove(key);
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Self::records(&self.session))
        }

        async fn get_session_memory_by_key(
            &self,
            _scope: MemoryScope,
            key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(Self::record_by_key(&self.session, &key))
        }
    }

    /// A throwaway scope; `FakeStorage` ignores it.
    fn scope() -> MemoryScope {
        MemoryScope {
            user_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
        }
    }

    fn tool() -> MemoryTool<FakeStorage> {
        MemoryTool::new(Arc::new(FakeStorage::default()), scope())
    }

    #[tokio::test]
    async fn should_record_user_memory() {
        let tool = tool();
        let out = tool
            .call(MemoryArgs {
                op: MemoryOp::Record,
                scope: MemoryStore::User,
                key: "editor".to_string(),
                value: Some("neovim".to_string()),
            })
            .await
            .expect("record");

        assert_eq!(out, "Recorded user memory \"editor\".");
        assert_eq!(
            tool.storage
                .user
                .lock()
                .unwrap()
                .get("editor")
                .map(String::as_str),
            Some("neovim")
        );
    }

    #[tokio::test]
    async fn should_isolate_scopes() {
        let tool = tool();
        tool.call(MemoryArgs {
            op: MemoryOp::Record,
            scope: MemoryStore::Session,
            key: "cwd".to_string(),
            value: Some("/tmp".to_string()),
        })
        .await
        .expect("record");

        assert!(tool.storage.user.lock().unwrap().is_empty());
        assert_eq!(
            tool.storage
                .session
                .lock()
                .unwrap()
                .get("cwd")
                .map(String::as_str),
            Some("/tmp")
        );
    }

    #[tokio::test]
    async fn should_error_when_recording_without_value() {
        let tool = tool();
        let err = tool
            .call(MemoryArgs {
                op: MemoryOp::Record,
                scope: MemoryStore::User,
                key: "editor".to_string(),
                value: None,
            })
            .await
            .expect_err("missing value");

        assert!(matches!(err, MemoryToolError::MissingValue));
    }

    #[tokio::test]
    async fn should_forget_user_memory_by_key() {
        let tool = tool();
        tool.call(MemoryArgs {
            op: MemoryOp::Record,
            scope: MemoryStore::User,
            key: "editor".to_string(),
            value: Some("neovim".to_string()),
        })
        .await
        .expect("record");

        let out = tool
            .call(MemoryArgs {
                op: MemoryOp::Forget,
                scope: MemoryStore::User,
                key: "editor".to_string(),
                value: None,
            })
            .await
            .expect("forget");

        assert_eq!(out, "Forgot user memory \"editor\".");
        assert!(tool.storage.user.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn should_report_when_forgetting_unknown_key() {
        let tool = tool();
        let out = tool
            .call(MemoryArgs {
                op: MemoryOp::Forget,
                scope: MemoryStore::Session,
                key: "ghost".to_string(),
                value: None,
            })
            .await
            .expect("forget");

        assert_eq!(out, "No session memory found for \"ghost\".");
    }

    #[test]
    fn should_deserialize_args_from_model_json() {
        let args: MemoryArgs = serde_json::from_value(serde_json::json!({
            "op": "record",
            "scope": "user",
            "key": "editor",
            "value": "neovim"
        }))
        .expect("valid args");

        assert!(matches!(args.op, MemoryOp::Record));
        assert!(matches!(args.scope, MemoryStore::User));
        assert_eq!(args.key, "editor");
        assert_eq!(args.value.as_deref(), Some("neovim"));
    }

    #[test]
    fn should_expose_memory_tool_definition() {
        let tool = tool();

        assert!(tool.description().contains("Record or forget a memory"));
        assert_eq!(tool.parameters()["type"], "object");
        assert_eq!(
            tool.parameters()["required"],
            serde_json::json!(["op", "scope", "key"])
        );
    }
}
