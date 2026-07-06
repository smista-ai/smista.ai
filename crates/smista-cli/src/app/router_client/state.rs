//! State machine phase of the router client.

/// The router client state machine phase during execution.
///
/// Buffered execute and continue calls block inside the worker until the router
/// responds, so there is no separate waiting-for-router-response state.
/// [`Streaming`](Self::Streaming) is reserved for streaming execution and
/// continuation, where the worker must poll stream events and incoming commands
/// together so [`Break`](crate::app::router_client::cmd::ContinueExecution::Break)
/// or [`Inject`](crate::app::router_client::cmd::ContinueExecution::Inject)
/// can interrupt the stream.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Non-idle states are scaffolded before router responses set them."
    )
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The router client is idle and ready to receive commands.
    Idle,
    /// The router is waiting for client-executed tool results.
    AwaitingTool,
    /// The router is waiting for a user approval decision.
    AwaitingApproval,
    /// The router is streaming a turn and can be interrupted.
    Streaming,
}
