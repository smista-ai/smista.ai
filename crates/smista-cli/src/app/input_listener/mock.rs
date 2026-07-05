use std::time::Duration;

use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::app::input_listener::InputEvent;

/// Deterministic input listener used by app lifecycle tests.
pub struct MockInputListener {
    events: Vec<InputEvent>,
    exit: CancellationToken,
    interval: Duration,
    tx: Sender<InputEvent>,
}

impl MockInputListener {
    /// Creates a mock listener that emits `events` at `interval`.
    #[must_use]
    pub fn new(
        events: Vec<InputEvent>,
        exit: CancellationToken,
        interval: Duration,
        tx: Sender<InputEvent>,
    ) -> Self {
        Self {
            events,
            exit,
            interval,
            tx,
        }
    }

    /// Spawns the mock input listener task.
    #[must_use]
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(self.run_loop())
    }

    async fn run_loop(self) {
        let mut index = 0;

        loop {
            tokio::select! {
                _ = self.exit.cancelled() => {
                    break;
                }
                _ = tokio::time::sleep(self.interval) => {
                    if let Some(event) = self.events.get(index) {
                        if let Err(e) = self.tx.send(event.clone()).await {
                            eprintln!("Failed to send event: {}", e);
                        }
                        index += 1;
                    }
                }
            }
        }
    }
}
