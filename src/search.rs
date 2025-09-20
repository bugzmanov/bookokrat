/// Search functionality for BookRat
/// Provides vim-like search with "/" input and "n"/"N" navigation

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub active: bool,
    pub mode: SearchMode,
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current_match_index: Option<usize>,
    pub original_position: usize, // Position to restore on cancel
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SearchMode {
    #[default]
    Inactive,
    InputMode,      // User is typing search query
    NavigationMode, // Query locked, navigating with n/N
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub index: usize, // Item index (book index, line number, etc.)
    pub score: f32,   // Match relevance score (1.0 for now)
    pub highlight_ranges: Vec<(usize, usize)>, // Character ranges to highlight in match
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_search(&mut self, current_position: usize) {
        self.active = true;
        self.mode = SearchMode::InputMode;
        self.query.clear();
        self.matches.clear();
        self.current_match_index = None;
        self.original_position = current_position;
    }

    pub fn cancel_search(&mut self) -> usize {
        self.active = false;
        self.mode = SearchMode::Inactive;
        self.query.clear();
        self.matches.clear();
        self.current_match_index = None;
        self.original_position
    }

    pub fn confirm_search(&mut self) {
        if self.query.is_empty() {
            // Empty query cancels search
            self.cancel_search();
        } else {
            self.mode = SearchMode::NavigationMode;
        }
    }

    pub fn exit_search(&mut self) {
        self.active = false;
        self.mode = SearchMode::Inactive;
        // Keep current position, don't restore original
    }

    pub fn update_query(&mut self, query: String) {
        self.query = query;
    }

    pub fn set_matches(&mut self, matches: Vec<SearchMatch>) {
        self.matches = matches;
        // Auto-jump to first match at or after current position
        if !self.matches.is_empty() {
            // Find first match at or after original position
            let start_index = self
                .matches
                .iter()
                .position(|m| m.index >= self.original_position)
                .unwrap_or(0); // Wrap to beginning if no match after current position
            self.current_match_index = Some(start_index);
        } else {
            self.current_match_index = None;
        }
    }

    pub fn next_match(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }

        self.current_match_index = Some(match self.current_match_index {
            Some(idx) => (idx + 1) % self.matches.len(), // Wrap around
            None => 0,
        });

        self.current_match_index
            .and_then(|idx| self.matches.get(idx))
            .map(|m| m.index)
    }

    pub fn previous_match(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }

        self.current_match_index = Some(match self.current_match_index {
            Some(0) => self.matches.len() - 1, // Wrap to end
            Some(idx) => idx - 1,
            None => self.matches.len() - 1,
        });

        self.current_match_index
            .and_then(|idx| self.matches.get(idx))
            .map(|m| m.index)
    }

    pub fn get_current_match(&self) -> Option<usize> {
        self.current_match_index
            .and_then(|idx| self.matches.get(idx))
            .map(|m| m.index)
    }

    pub fn is_match(&self, index: usize) -> bool {
        self.matches.iter().any(|m| m.index == index)
    }

    pub fn is_current_match(&self, index: usize) -> bool {
        self.current_match_index
            .and_then(|idx| self.matches.get(idx))
            .map(|m| m.index == index)
            .unwrap_or(false)
    }

    pub fn get_match_info(&self) -> String {
        if self.matches.is_empty() {
            "No matches".to_string()
        } else if let Some(current) = self.current_match_index {
            format!("[{}/{}]", current + 1, self.matches.len())
        } else {
            format!("[{} matches]", self.matches.len())
        }
    }
}

/// Trait for panels that support search functionality
pub trait SearchablePanel {
    // Lifecycle methods
    fn start_search(&mut self);
    fn cancel_search(&mut self);
    fn confirm_search(&mut self); // Enter key - lock search and enter navigation mode
    fn exit_search(&mut self); // Esc in navigation mode - exit but keep position

    // Search operations
    fn update_search_query(&mut self, query: &str);
    fn next_match(&mut self); // 'n' key
    fn previous_match(&mut self); // 'N' key

    // State access
    fn get_search_state(&self) -> &SearchState;
    fn is_searching(&self) -> bool;
    fn has_matches(&self) -> bool;

    // Panel-specific
    fn jump_to_match(&mut self, match_index: usize);
    fn get_searchable_content(&self) -> Vec<String>; // Extract searchable text
}

/// Helper function to find matches in text (case-insensitive)
pub fn find_matches_in_text(query: &str, items: &[String]) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    for (index, item) in items.iter().enumerate() {
        let item_lower = item.to_lowercase();

        // Find all occurrences of the query in this item
        let mut highlight_ranges = Vec::new();
        let mut search_start = 0;

        while let Some(pos) = item_lower[search_start..].find(&query_lower) {
            let actual_pos = search_start + pos;
            highlight_ranges.push((actual_pos, actual_pos + query.len()));
            search_start = actual_pos + 1; // Allow overlapping matches
        }

        if !highlight_ranges.is_empty() {
            matches.push(SearchMatch {
                index,
                score: 1.0, // Perfect match for now
                highlight_ranges,
            });
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_matches() {
        let items = vec![
            "The Great Gatsby".to_string(),
            "To Kill a Mockingbird".to_string(),
            "1984".to_string(),
            "The Catcher in the Rye".to_string(),
        ];

        let matches = find_matches_in_text("the", &items);
        assert_eq!(matches.len(), 2); // "The Great Gatsby" and "The Catcher in the Rye"

        let matches = find_matches_in_text("kill", &items);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].index, 1);

        let matches = find_matches_in_text("98", &items);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].index, 2);
    }

    #[test]
    fn test_search_state_navigation() {
        let mut state = SearchState::new();
        state.start_search(1);

        state.set_matches(vec![
            SearchMatch {
                index: 0,
                score: 1.0,
                highlight_ranges: vec![(0, 3)],
            },
            SearchMatch {
                index: 2,
                score: 1.0,
                highlight_ranges: vec![(0, 3)],
            },
            SearchMatch {
                index: 4,
                score: 1.0,
                highlight_ranges: vec![(0, 3)],
            },
        ]);

        // Should start at first match at or after position 1 (which is index 2)
        assert_eq!(state.get_current_match(), Some(2));

        // Next should go to index 4
        assert_eq!(state.next_match(), Some(4));

        // Next should wrap to index 0
        assert_eq!(state.next_match(), Some(0));

        // Previous should go back to index 4
        assert_eq!(state.previous_match(), Some(4));
    }
}
