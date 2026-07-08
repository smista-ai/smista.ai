//! Console state

use crate::app::tui::state::PromptState;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ConsoleState {
    /// Prompt input state
    pub prompt: PromptState,
    approval_option_index: usize,
}

impl ConsoleState {
    /// Advances the selected approval option.
    pub fn next_approval_option(&mut self, option_count: usize) {
        if option_count > 0 {
            self.approval_option_index = (self.approval_option_index + 1) % option_count;
        }
    }

    /// Moves to the previous approval option.
    pub fn previous_approval_option(&mut self, option_count: usize) {
        if option_count == 0 {
            return;
        }

        self.approval_option_index = if self.approval_option_index == 0 {
            option_count.saturating_sub(1)
        } else {
            self.approval_option_index.saturating_sub(1)
        };
    }

    /// Returns the selected approval option index clamped to `option_count`.
    #[must_use]
    pub fn approval_option_index(&self, option_count: usize) -> usize {
        self.approval_option_index
            .min(option_count.saturating_sub(1))
    }

    /// Resets the selected approval option.
    pub fn reset_approval_option(&mut self) {
        self.approval_option_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_option_cursor_wraps_clamps_and_resets() {
        let mut state = ConsoleState::default();

        state.previous_approval_option(3);
        assert_eq!(state.approval_option_index(3), 2);

        state.next_approval_option(3);
        assert_eq!(state.approval_option_index(3), 0);

        state.next_approval_option(2);
        state.next_approval_option(2);
        assert_eq!(state.approval_option_index(2), 0);

        state.previous_approval_option(0);
        assert_eq!(state.approval_option_index(0), 0);

        state.next_approval_option(3);
        assert_eq!(state.approval_option_index(1), 0);

        state.reset_approval_option();
        assert_eq!(state.approval_option_index(3), 0);
    }
}
