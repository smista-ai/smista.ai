use std::collections::HashMap;
use std::path::PathBuf;

/// Commands are sent by the UI to the router client to be executed on the smista-router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    /// Execute a user prompt through the router.
    Execute {
        /// User prompt to be executed by the router. This is the main input for the router to process.
        prompt: String,
        /// Files context loaded by the user. Maps file paths to their content.
        files: HashMap<PathBuf, String>,
    },
}

/// Messages are sent by the router client to the UI to notify about the status of the execution.
#[expect(
    dead_code,
    reason = "Router message variants are defined before real router execution is wired."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// The router is waiting for the user to approve or reject an action.
    AwaitingApproval {
        /// Human-readable approval prompt.
        message: String,
    },
    /// The router has no active turn.
    Idle,
    /// The router is processing a turn.
    Thinking,
}
