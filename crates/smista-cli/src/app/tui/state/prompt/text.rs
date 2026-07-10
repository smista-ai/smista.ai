//! Ordinary text state for the interactive prompt.

use super::{VerticalDirection, char_len, char_to_byte_index, move_vertical};

/// State for a text prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPromptState {
    pub(super) input: String,
    pub(super) cursor: usize,
}

impl TextPromptState {
    pub(super) fn new(input: String) -> Self {
        let cursor = input.chars().count();
        Self { input, cursor }
    }

    pub fn text(&self) -> &str {
        &self.input
    }

    pub(super) fn insert(&mut self, char: char) {
        let byte_index = char_to_byte_index(&self.input, self.cursor);
        self.input.insert(byte_index, char);
        self.cursor += 1;
    }

    pub(super) fn backspace(&mut self) -> Option<char> {
        if self.cursor == 0 {
            return None;
        }

        self.cursor -= 1;
        self.remove_at_cursor()
    }

    pub(super) fn delete(&mut self) -> Option<char> {
        self.remove_at_cursor()
    }

    pub(super) fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(super) fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(char_len(&self.input));
    }

    pub(super) fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub(super) fn move_end(&mut self) {
        self.cursor = char_len(&self.input);
    }

    pub(super) fn move_up(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Up);
    }

    pub(super) fn move_down(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Down);
    }

    fn remove_at_cursor(&mut self) -> Option<char> {
        let byte_index = char_to_byte_index(&self.input, self.cursor);
        let char = self.input[byte_index..].chars().next()?;
        self.input.remove(byte_index);
        Some(char)
    }
}
