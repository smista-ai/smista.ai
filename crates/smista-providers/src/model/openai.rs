//! OpenAI models.

mod gpt;

use std::sync::Arc;

use secrecy::SecretString;

#[doc(inline)]
pub use self::gpt::{Gpt_5_4, Gpt_5_4_Mini, Gpt_5_5, gpt_5_4, gpt_5_4_mini, gpt_5_5};
use crate::memory::MemoryStorage;

/// Arguments for creating a new OpenAI model.
///
/// Carries the credential, base system prompt and memory backend every OpenAI
/// model needs to construct its underlying agent.
pub struct OpenAIModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    /// The OpenAI API key authenticating requests, held as a [`SecretString`]
    /// so it is redacted from logs and never printed by [`Debug`].
    pub apikey: SecretString,
    /// The base system prompt; the model appends its memory preamble to this.
    pub preamble: String,
    /// The memory backend the model reads to build its preamble and writes
    /// through for the memory tool.
    pub storage: Arc<S>,
}

// Holds a credential, so `Debug` is implemented by hand to redact the API key
// rather than derived. A derive would also add a spurious `S: Debug` bound.
impl<S> std::fmt::Debug for OpenAIModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIModelArgs")
            .field("apikey", &"[redacted]")
            .field("preamble", &self.preamble)
            .field(
                "storage",
                &format_args!("Arc<{}>", std::any::type_name::<S>()),
            )
            .finish()
    }
}

// `#[derive(Clone)]` would add a spurious `S: Clone` bound; `Arc<S>` is `Clone`
// for any `S`, so implement it by hand without the bound.
impl<S> Clone for OpenAIModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    fn clone(&self) -> Self {
        Self {
            apikey: self.apikey.clone(),
            preamble: self.preamble.clone(),
            storage: Arc::clone(&self.storage),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use smista_core::model::Provider;

    use super::*;
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; only its type identity matters here.
    struct NoStorage;

    impl MemoryStorage for NoStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_user_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    #[test]
    fn should_redact_apikey_in_debug_output() {
        let args = OpenAIModelArgs {
            apikey: SecretString::from("sk-openai-api03-TOPSECRETVALUE"),
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        };

        let rendered = format!("{args:?}");
        assert!(!rendered.contains("sk-openai-api03-TOPSECRETVALUE"));
        assert!(rendered.contains("[redacted]"));
        assert!(rendered.contains("be helpful"));
    }

    #[test]
    fn should_clone_args_without_requiring_storage_clone() {
        let args = OpenAIModelArgs {
            apikey: SecretString::from("sk-openai-secret"),
            preamble: "preamble".to_string(),
            storage: Arc::new(NoStorage),
        };

        let cloned = args.clone();
        assert_eq!(cloned.preamble, "preamble");
        // The clone shares the same backend rather than duplicating it.
        assert!(Arc::ptr_eq(&args.storage, &cloned.storage));
    }

    #[test]
    fn should_expose_distinct_model_references() {
        let references = [gpt_5_4(), gpt_5_4_mini(), gpt_5_5()];

        // Every reference targets the OpenAI provider...
        assert!(
            references
                .iter()
                .all(|reference| reference.provider == Provider::OpenAI)
        );
        // ...and names a distinct model.
        let models: BTreeSet<&str> = references.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(models.len(), references.len());
    }
}
