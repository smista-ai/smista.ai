//! Router activity state for the TUI.

use std::time::Instant;

const ROUTER_IDLE: &str = "Idle";
const ROUTER_INTERRUPTED: &str = "Interrupted";
const ROUTER_THINKING: &str = "Thinking";

/// Current router activity.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum RouterState {
    /// The router has no active turn.
    #[default]
    Idle,
    /// The active turn was interrupted.
    Interrupted,
    /// The router is processing a turn.
    ///
    /// Contains the time at which the turn started, for calculating how long the router has been thinking.
    Thinking(Instant),
}

impl RouterState {
    /// Returns a stable label for tracing and diagnostics.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Idle => ROUTER_IDLE,
            Self::Interrupted => ROUTER_INTERRUPTED,
            Self::Thinking(_) => ROUTER_THINKING,
        }
    }

    /// Returns whether the state contains transient display content.
    #[must_use]
    pub fn needs_refresh(&self) -> bool {
        matches!(self, Self::Thinking(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_state_reports_stable_kind() {
        assert_eq!(RouterState::default().kind(), ROUTER_IDLE);
        assert_eq!(RouterState::Idle.kind(), ROUTER_IDLE);
        assert_eq!(RouterState::Interrupted.kind(), ROUTER_INTERRUPTED);
        assert_eq!(
            RouterState::Thinking(Instant::now()).kind(),
            ROUTER_THINKING
        );
    }

    #[test]
    fn only_thinking_state_needs_refresh() {
        assert!(!RouterState::Idle.needs_refresh());
        assert!(!RouterState::Interrupted.needs_refresh());
        assert!(RouterState::Thinking(Instant::now()).needs_refresh());
    }
}
