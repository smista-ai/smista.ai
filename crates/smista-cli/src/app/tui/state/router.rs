//! Router activity state for the TUI.

const ROUTER_IDLE: &str = "idle";
const ROUTER_INTERRUPTED: &str = "interrupted";
const ROUTER_THINKING: &str = "thinking";

/// Current router activity.
#[derive(Debug, Default)]
pub enum RouterState {
    /// The router has no active turn.
    #[default]
    Idle,
    /// The active turn was interrupted.
    Interrupted,
    /// The router is processing a turn.
    Thinking,
}

impl RouterState {
    /// Returns a stable label for tracing and diagnostics.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Idle => ROUTER_IDLE,
            Self::Interrupted => ROUTER_INTERRUPTED,
            Self::Thinking => ROUTER_THINKING,
        }
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
        assert_eq!(RouterState::Thinking.kind(), ROUTER_THINKING);
    }
}
