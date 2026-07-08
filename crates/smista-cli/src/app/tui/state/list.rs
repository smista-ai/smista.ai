//! Selectable list state shared by replacement views.

const PAGE_STEP: usize = 8;

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

    /// Selects the first entry.
    pub fn first(&mut self) {
        self.current_index = 0;
    }

    /// Selects the last entry.
    pub fn last(&mut self) {
        self.current_index = self.clamp_index(usize::MAX);
    }

    /// Moves selection one page toward the start.
    pub fn page_previous(&mut self) {
        self.current_index = self.current_index.saturating_sub(PAGE_STEP);
    }

    /// Moves selection one page toward the end.
    pub fn page_next(&mut self) {
        self.select(self.current_index.saturating_add(PAGE_STEP));
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
    fn home_end_and_page_selection_clamp_to_existing_entries() {
        let mut state = ListState::new((0..20).collect::<Vec<_>>());

        state.last();
        assert_eq!(state.current_index(), 19);

        state.page_previous();
        assert_eq!(state.current_index(), 11);

        state.first();
        assert_eq!(state.current_index(), 0);

        state.page_next();
        assert_eq!(state.current_index(), 8);

        state.page_next();
        state.page_next();
        assert_eq!(state.current_index(), 19);
    }

    #[test]
    fn empty_list_has_no_selected_entry() {
        let mut state = ListState::<String>::default();

        state.select(10);
        state.next();
        state.last();
        state.page_next();

        assert_eq!(state.current_index(), 0);
        assert_eq!(state.selected(), None);
    }
}
