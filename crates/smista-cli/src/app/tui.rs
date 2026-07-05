use crate::app::AppContext;
use crate::app::input_listener::InputEvent;
use crate::app::router_client::{Cmd, Msg};

/// Terminal UI facade for the interactive client.
///
/// The TUI currently accepts events and router messages as no-ops so the worker
/// topology can be tested before rendering and key handling are implemented.
pub struct Tui {
    #[expect(
        dead_code,
        reason = "The TUI context is reserved for future rendering and command handling."
    )]
    context: AppContext,
}

impl Tui {
    /// Creates a TUI scaffold.
    ///
    /// `initial_prompt` is accepted now so the public constructor already has
    /// the shape needed by the future renderer.
    #[must_use]
    pub fn new(context: AppContext, _initial_prompt: Option<String>) -> Self {
        // TODO: handle initial prompt; push to ui
        Self { context }
    }

    /// Handles one input event and optionally produces a router command.
    ///
    /// This scaffold does not map keys to commands yet.
    #[must_use]
    pub fn handle_input_event(&self, event: InputEvent) -> Option<Cmd> {
        tracing::debug!("Handling input event: {event:?}");
        None
    }

    /// Handles one router message and optionally produces a follow-up command.
    ///
    /// This scaffold does not update UI state yet.
    #[must_use]
    pub fn handle_client_msg(&self, msg: Msg) -> Option<Cmd> {
        tracing::debug!("Handling client message: {msg:?}");
        None
    }
}
