//! In-process registry of the live turn per session, for supersede.
//!
//! Storage holds the durable `active` flag of the run lock; this registry holds
//! the in-process cancellation handle that actually interrupts a generating
//! turn. A new request for a session supersedes the live turn by cancelling its
//! token; `break`/`inject` cancel it on demand. Each turn is tagged with a
//! monotonic nonce so a stale finisher never clobbers a newer turn.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// A handle to one turn's cancellation, tagged with a per-turn nonce.
///
/// The turn races a provider call against [`cancellation`](Self::cancellation) to
/// learn it was superseded; the orchestrator hands the same handle back to
/// [`InFlightRegistry::finish`] so only the turn that is still live is removed.
#[derive(Clone, Debug)]
pub(crate) struct TurnToken {
    nonce: u64,
    token: CancellationToken,
}

impl TurnToken {
    /// Whether this turn has been cancelled (superseded, broken or injected).
    #[cfg(test)]
    pub(crate) fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// The underlying cancellation token, to race a provider call against.
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.token
    }
}

/// Tracks the live turn per session so a new request can supersede it.
///
/// Keyed by session, because a session has at most one run. Cloning shares the
/// same registry, following the service `Arc<Inner>` pattern.
#[derive(Clone, Default)]
pub(crate) struct InFlightRegistry {
    live: Arc<Mutex<HashMap<Uuid, TurnToken>>>,
    counter: Arc<AtomicU64>,
}

impl InFlightRegistry {
    /// Begins a turn for `session_id`, cancelling and replacing any live turn.
    ///
    /// Returns a fresh [`TurnToken`] the turn selects on to detect supersede.
    pub(crate) fn begin(&self, session_id: Uuid) -> TurnToken {
        let token = TurnToken {
            nonce: self.counter.fetch_add(1, Ordering::Relaxed),
            token: CancellationToken::new(),
        };
        let mut live = self.live.lock().expect("in-flight registry poisoned");
        if let Some(previous) = live.insert(session_id, token.clone()) {
            tracing::debug!(%session_id, "superseding the in-flight turn");
            previous.token.cancel();
        }
        token
    }

    /// Cancels the live turn for `session_id`, if any.
    #[cfg(test)]
    pub(crate) fn cancel(&self, session_id: Uuid) {
        if let Some(live) = self
            .live
            .lock()
            .expect("in-flight registry poisoned")
            .get(&session_id)
        {
            live.token.cancel();
        }
    }

    /// Removes the entry for `session_id` iff it still points at `token`.
    ///
    /// A stale finisher whose turn was already superseded is a no-op, so it
    /// never removes a newer turn.
    pub(crate) fn finish(&self, session_id: Uuid, token: &TurnToken) {
        let mut live = self.live.lock().expect("in-flight registry poisoned");
        if live
            .get(&session_id)
            .is_some_and(|current| current.nonce == token.nonce)
        {
            live.remove(&session_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_cancel_previous_turn_when_a_new_one_begins() {
        let registry = InFlightRegistry::default();
        let session = Uuid::now_v7();
        let first = registry.begin(session);
        let second = registry.begin(session);
        assert!(
            first.is_cancelled(),
            "starting a new turn supersedes the old one"
        );
        assert!(!second.is_cancelled());
    }

    #[test]
    fn should_cancel_live_turn_on_demand() {
        let registry = InFlightRegistry::default();
        let session = Uuid::now_v7();
        let token = registry.begin(session);
        registry.cancel(session);
        assert!(token.is_cancelled());
    }

    #[test]
    fn should_not_remove_newer_turn_on_finish() {
        let registry = InFlightRegistry::default();
        let session = Uuid::now_v7();
        let old = registry.begin(session);
        let new = registry.begin(session);
        registry.finish(session, &old); // stale finisher
        registry.cancel(session); // must still reach `new`
        assert!(new.is_cancelled());
    }
}
