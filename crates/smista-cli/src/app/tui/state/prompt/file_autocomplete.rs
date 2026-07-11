//! File-autocomplete state for the interactive prompt.

use std::path::PathBuf;

use super::{VerticalDirection, char_len, char_to_byte_index, move_vertical};

/// State for an active file mention at the end of a text prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAutocompleteState {
    pub(super) input: String,
    pub(super) cursor: usize,
    pub(super) mention_start: usize,
    pub(super) command: bool,
    matches: Vec<PathBuf>,
    selected: usize,
    suggestion: Option<String>,
}

impl FileAutocompleteState {
    pub(super) fn new(input: String, mention_start: usize, command: bool) -> Self {
        let cursor = char_len(&input);
        Self {
            input,
            cursor,
            mention_start,
            command,
            matches: Vec::new(),
            selected: 0,
            suggestion: None,
        }
    }

    pub(super) fn text(&self) -> &str {
        &self.input
    }

    pub(super) fn query(&self) -> &str {
        &self.input[self.mention_start..]
    }

    pub(super) fn insert(&mut self, char: char) {
        let byte_index = self.cursor_byte_index();
        self.input.insert(byte_index, char);
        self.cursor += 1;
        self.clear_matches();
    }

    pub(super) fn backspace(&mut self) -> Option<char> {
        if self.cursor == 0 {
            return None;
        }

        self.cursor -= 1;
        let removed = self.remove_at_cursor();
        self.clear_matches();
        removed
    }

    pub(super) fn delete(&mut self) -> Option<char> {
        let removed = self.remove_at_cursor();
        if removed.is_some() {
            self.clear_matches();
        }
        removed
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

    pub(super) fn replace_matches(&mut self, mut matches: Vec<PathBuf>) {
        let selected = self.matches.get(self.selected).cloned();
        matches.sort();
        matches.dedup();
        self.selected = selected
            .and_then(|selected| matches.iter().position(|path| path == &selected))
            .unwrap_or_default();
        self.matches = matches;
        self.refresh_suggestion();
    }

    pub(super) fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
            self.refresh_suggestion();
        }
    }

    pub(super) fn previous_match(&mut self) {
        if !self.matches.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or_else(|| self.matches.len() - 1);
            self.refresh_suggestion();
        }
    }

    pub(super) fn current_suggestion(&self) -> Option<&str> {
        self.suggestion.as_deref()
    }

    pub(super) fn accept_suggestion(&mut self) -> bool {
        let Some(path) = self.matches.get(self.selected) else {
            return false;
        };
        let path = path.to_string_lossy();
        self.input.truncate(self.mention_start);
        self.input.push_str(&path);
        self.cursor = char_len(&self.input);
        self.clear_matches();
        true
    }

    pub(super) fn cursor_byte_index(&self) -> usize {
        char_to_byte_index(&self.input, self.cursor)
    }

    fn clear_matches(&mut self) {
        self.matches.clear();
        self.selected = 0;
        self.suggestion = None;
    }

    fn refresh_suggestion(&mut self) {
        self.suggestion = self.matches.get(self.selected).map(|path| {
            format!(
                "{}{}",
                &self.input[..self.mention_start],
                path.to_string_lossy()
            )
        });
    }

    fn remove_at_cursor(&mut self) -> Option<char> {
        let byte_index = self.cursor_byte_index();
        let char = self.input[byte_index..].chars().next()?;
        self.input.remove(byte_index);
        Some(char)
    }
}
