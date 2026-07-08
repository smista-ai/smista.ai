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
