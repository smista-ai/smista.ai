//! The sink the turn loop forwards live stream events through.
//!
//! Wraps the sender half of the web layer's Server-Sent Events channel. Only the
//! incrementally produced events — text and reasoning deltas — flow through it;
//! the terminal events are appended by the web layer from the finished response.
//! A closed receiver (the client disconnected) is ignored, so the turn still
//! runs to completion and persists its work.

use smista_core::api::TurnEvent;
use tokio::sync::mpsc::UnboundedSender;

/// A best-effort sink for the live events a streamed turn produces.
#[derive(Clone)]
pub(crate) struct TurnSink {
    /// The sender half of the web layer's Server-Sent Events channel.
    tx: UnboundedSender<TurnEvent>,
}

impl TurnSink {
    /// Wraps the sender half of a Server-Sent Events channel.
    pub(crate) fn new(tx: UnboundedSender<TurnEvent>) -> Self {
        Self { tx }
    }

    /// Forwards one event, dropping it if the receiver is already gone.
    pub(crate) fn emit(&self, event: TurnEvent) {
        if self.tx.send(event).is_err() {
            tracing::debug!("stream receiver dropped; discarding a turn event");
        }
    }
}
