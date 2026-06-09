//! Integration test for the [`MemoryTool`] driven by a live Anthropic model.
//!
//! Exercises the full memory loop end to end against the real Anthropic API:
//! the model records a user and a session memory through the tool, those facts
//! are surfaced into a fresh agent's preamble and recalled, and the model then
//! forgets both through the tool.
//!
//! The tool can only be invoked the way it is in production — by the model
//! issuing tool calls — so the test gives the model explicit, deterministic
//! instructions and asserts against the resulting storage state rather than the
//! model's prose. The only assertion on model output is the recall step, which
//! checks for two rare verbatim tokens the model cannot reasonably paraphrase.
//!
//! Requires `ANTHROPIC_API_KEY`; when it is absent the test skips so it never
//! fails on a machine without credentials. It runs only under
//! `just provider_integration_test`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rig_core::completion::Prompt;
use rig_core::prelude::{CompletionClient, ProviderClient};
use rig_core::providers::anthropic;
use rig_core::providers::anthropic::completion::CLAUDE_HAIKU_4_5;
use smista_providers::memory::{MemoryRecord, MemoryStorage, MemoryTool, build_preamble};

/// A long-term user fact and a rare value the model will echo verbatim.
const USER_KEY: &str = "favorite_color";
const USER_VALUE: &str = "chartreuse";
/// A session-scoped fact and an equally distinctive value.
const SESSION_KEY: &str = "active_ticket";
const SESSION_VALUE: &str = "SMISTA-4242";

/// In-memory [`MemoryStorage`], keyed by `key` within each scope.
///
/// Mirrors how a real backend resolves a key to a `handle`: the handle is just
/// `handle:{key}`, so `forget` can strip the prefix to find the entry.
#[derive(Default)]
struct InMemoryStorage {
    user: Mutex<HashMap<String, String>>,
    session: Mutex<HashMap<String, String>>,
}

/// Error type for [`InMemoryStorage`]; the fake never actually fails.
#[derive(Debug, thiserror::Error)]
#[error("in-memory storage error")]
struct InMemoryError;

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

#[tokio::test]
async fn should_record_recall_and_forget_user_and_session_memories() {
    // ensure the apikey is set
    let Ok(client) = anthropic::Client::from_env() else {
        panic!("ANTHROPIC_API_KEY is not set or invalid");
    };

    let storage = Arc::new(InMemoryStorage::default());

    // Steps 1 & 2: the model records a user memory and a session memory by
    // calling the tool. We assert against storage, not the model's reply.
    let recorder = client
        .agent(CLAUDE_HAIKU_4_5)
        .preamble(
            "You manage the user's memory by calling the `memory` tool. \
             Follow the user's instructions exactly, then reply with `done`.",
        )
        .temperature(0.0)
        .max_tokens(1024)
        .tool(MemoryTool::new(storage.clone()))
        .build();

    recorder
        .prompt(format!(
            "Record exactly two memories with the memory tool, one tool call each.\n\
             1. op=record, scope=user, key={USER_KEY}, value={USER_VALUE}\n\
             2. op=record, scope=session, key={SESSION_KEY}, value={SESSION_VALUE}\n\
             After both calls succeed, reply with `done`."
        ))
        .max_turns(6)
        .await
        .expect("record prompt failed");

    let stored_user = storage
        .get_user_memory_by_key(USER_KEY.to_string())
        .await
        .expect("read user memory");
    assert_eq!(
        stored_user.map(|record| record.content).as_deref(),
        Some(USER_VALUE),
        "the model did not record the user memory through the tool"
    );

    let stored_session = storage
        .get_session_memory_by_key(SESSION_KEY.to_string())
        .await
        .expect("read session memory");
    assert_eq!(
        stored_session.map(|record| record.content).as_deref(),
        Some(SESSION_VALUE),
        "the model did not record the session memory through the tool"
    );

    // Step 3: a fresh agent gets the stored memories injected into its preamble
    // and must recall them. No tool is attached: recall is preamble-only.
    let user = storage.get_user_memories(None).await.expect("list user");
    let session = storage
        .get_session_memories(None)
        .await
        .expect("list session");
    let preamble = build_preamble(&user, &session).expect("preamble is non-empty");
    assert!(preamble.contains(USER_VALUE));
    assert!(preamble.contains(SESSION_VALUE));

    let recaller = client
        .agent(CLAUDE_HAIKU_4_5)
        .preamble(&preamble)
        .temperature(0.0)
        .max_tokens(256)
        .build();

    let answer = recaller
        .prompt(
            "Answer in one short sentence, using only what you already know about \
             me and this session: what is my favorite color, and which ticket am I \
             working on right now?",
        )
        .await
        .expect("recall prompt failed");

    let answer = answer.to_lowercase();
    assert!(
        answer.contains(&USER_VALUE.to_lowercase()),
        "recall did not surface the user memory; answer was: {answer}"
    );
    assert!(
        answer.contains(&SESSION_VALUE.to_lowercase()),
        "recall did not surface the session memory; answer was: {answer}"
    );

    // Steps 4 & 5: the model forgets both memories through the tool.
    let forgetter = client
        .agent(CLAUDE_HAIKU_4_5)
        .preamble(
            "You manage the user's memory by calling the `memory` tool. \
             Follow the user's instructions exactly, then reply with `done`.",
        )
        .temperature(0.0)
        .max_tokens(1024)
        .tool(MemoryTool::new(storage.clone()))
        .build();

    forgetter
        .prompt(format!(
            "Forget exactly two memories with the memory tool, one tool call each.\n\
             1. op=forget, scope=user, key={USER_KEY}\n\
             2. op=forget, scope=session, key={SESSION_KEY}\n\
             After both calls succeed, reply with `done`."
        ))
        .max_turns(6)
        .await
        .expect("forget prompt failed");

    assert!(
        storage
            .get_user_memory_by_key(USER_KEY.to_string())
            .await
            .expect("read user memory")
            .is_none(),
        "the model did not forget the user memory through the tool"
    );
    assert!(
        storage
            .get_session_memory_by_key(SESSION_KEY.to_string())
            .await
            .expect("read session memory")
            .is_none(),
        "the model did not forget the session memory through the tool"
    );
}
