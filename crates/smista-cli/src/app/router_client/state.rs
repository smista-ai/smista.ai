//! State machine phase of the router client.

/// The router client state machine phase during execution.
///
/// [`Streaming`](Self::Streaming) covers opening and draining execute or
/// continuation streams. The worker polls router progress and incoming commands
/// together so [`Break`](crate::app::router_client::cmd::ContinueExecution::Break)
/// or [`Inject`](crate::app::router_client::cmd::ContinueExecution::Inject) can
/// interrupt an active request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The router client is idle and ready to receive commands.
    Idle,
    /// The router is waiting for client-executed tool results.
    AwaitingTool,
    /// The router is waiting for a user approval decision.
    AwaitingApproval,
    /// The router is opening or streaming a turn and can be interrupted.
    Streaming,
}
