//! Selectable list state shared by replacement views.

/// State for a selectable list of entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListState<T> {
    entries: Vec<T>,
    current_index: usize,
}

impl<T> ListState<T> {
    /// Creates a list with its first entry selected.
    #[must_use]
    pub fn new(entries: Vec<T>) -> Self {
        Self {
            entries,
            current_index: 0,
        }
    }

    /// Creates a list with `current_index` selected when possible.
    #[must_use]
    pub fn with_current_index(entries: Vec<T>, current_index: usize) -> Self {
        let mut state = Self::new(entries);
        state.select(current_index);
        state
    }

    /// Returns the list entries in render order.
    #[must_use]
    pub fn entries(&self) -> &[T] {
        &self.entries
    }

    /// Returns the selected index.
    #[must_use]
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Returns the selected entry.
    #[must_use]
    pub fn selected(&self) -> Option<&T> {
        self.entries.get(self.current_index)
    }

    /// Returns `true` when the list has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Selects an entry, clamping to the last valid entry.
    pub fn select(&mut self, index: usize) {
        self.current_index = self.clamp_index(index);
    }

    /// Selects the next entry.
    pub fn next(&mut self) {
        self.select(self.current_index.saturating_add(1));
    }

    /// Selects the previous entry.
    pub fn previous(&mut self) {
        self.current_index = self.current_index.saturating_sub(1);
    }

    /// Replaces entries and preserves the nearest valid selection.
    pub fn replace_entries(&mut self, entries: Vec<T>) {
        self.entries = entries;
        self.current_index = self.clamp_index(self.current_index);
    }

    fn clamp_index(&self, index: usize) -> usize {
        if self.entries.is_empty() {
            0
        } else {
            index.min(self.entries.len() - 1)
        }
    }
}

impl<T> Default for ListState<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::ListState;

    #[test]
    fn selection_clamps_to_existing_entries() {
        let mut state = ListState::with_current_index(vec!["first", "second"], 10);

        assert_eq!(state.current_index(), 1);
        assert_eq!(state.selected(), Some(&"second"));

        state.next();
        assert_eq!(state.current_index(), 1);

        state.previous();
        assert_eq!(state.current_index(), 0);
    }

    #[test]
    fn empty_list_has_no_selected_entry() {
        let mut state = ListState::<String>::default();

        state.select(10);
        state.next();

        assert_eq!(state.current_index(), 0);
        assert_eq!(state.selected(), None);
    }
}
