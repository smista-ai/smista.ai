use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
pub mod mock;

/// Input events emitted by the input listener.
///
/// These variants describe the initial event vocabulary for the client
/// skeleton. Real terminal key decoding is intentionally left for a later task.
#[expect(
    dead_code,
    reason = "Input event variants are defined before real key handling is wired."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    /// Interrupt signal (i.e. Ctrl+C)
    Interrupt,
    /// Character input
    Char(char),
}

/// Ratatui keyboard input listener.
///
/// The current implementation only observes cancellation. Terminal event
/// decoding will use the sender once real key handling is implemented.
pub struct InputListener {
    exit: CancellationToken,
    #[expect(
        dead_code,
        reason = "The input sender is reserved for future terminal event forwarding."
    )]
    tx: Sender<InputEvent>,
}

impl InputListener {
    /// Creates an input listener bound to the shared cancellation token.
    #[must_use]
    pub fn new(exit: CancellationToken, tx: Sender<InputEvent>) -> Self {
        Self { exit, tx }
    }

    /// Spawns the input listener task.
    #[must_use]
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(self.run_loop())
    }

    async fn run_loop(self) {
        tracing::debug!("InputListener started");

        self.exit.cancelled().await;

        tracing::info!("InputListener stopped");
    }
}
