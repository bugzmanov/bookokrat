use crate::book_manager::{BookInfo, BookManager};
use crate::search::{SearchMode, SearchState, SearchablePanel, find_matches_in_text};
use crate::theme::Base16Palette;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

pub struct BookList {
    pub selected: usize,
    pub list_state: ListState,
    book_infos: Vec<BookInfo>,
    search_state: SearchState,
}

impl BookList {
    pub fn new(book_manager: &BookManager) -> Self {
        let books = book_manager.books.clone();
        let has_files = !books.is_empty();
        let mut list_state = ListState::default();
        if has_files {
            list_state.select(Some(0));
        }

        Self {
            selected: 0,
            list_state,
            book_infos: books,
            search_state: SearchState::new(),
        }
    }

    pub fn move_selection_down(&mut self) {
        if self.selected < self.book_infos.len().saturating_sub(1) {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
            // Clear current match when manually navigating so next 'n' finds from new position
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
            // Clear current match when manually navigating so next 'n' finds from new position
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    pub fn set_selection_to_index(&mut self, index: usize) {
        self.selected = index;
        self.list_state.select(Some(index));
    }

    pub fn scroll_down(&mut self, area_height: u16) {
        if self.book_infos.is_empty() {
            return;
        }

        let visible_height = area_height.saturating_sub(2) as usize; // Account for borders
        let total_items = self.book_infos.len();
        let current_offset = self.list_state.offset();

        let cursor_viewport_pos = self.selected.saturating_sub(current_offset);

        if current_offset + visible_height < total_items {
            let new_offset = current_offset + 1;

            let new_selected = (new_offset + cursor_viewport_pos).min(total_items - 1);

            self.selected = new_selected;
            self.list_state.select(Some(self.selected));
            self.list_state = ListState::default()
                .with_selected(Some(self.selected))
                .with_offset(new_offset);
        } else if self.selected < total_items - 1 {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
        }
    }

    /// Scroll the view up while keeping cursor at same screen position if possible
    pub fn scroll_up(&mut self, area_height: u16) {
        if self.book_infos.is_empty() {
            return;
        }

        let visible_height = area_height.saturating_sub(2) as usize; // Account for borders
        let current_offset = self.list_state.offset();

        let cursor_viewport_pos = self.selected.saturating_sub(current_offset);

        if current_offset > 0 {
            let new_offset = current_offset - 1;
            let new_selected = new_offset + cursor_viewport_pos;

            self.selected = new_selected;
            self.list_state.select(Some(self.selected));
            self.list_state = ListState::default()
                .with_selected(Some(self.selected))
                .with_offset(new_offset);
        } else if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
        }

        // Keep selection inside the visible viewport when we cannot scroll up further
        if visible_height > 0 {
            let current_offset = self.list_state.offset();
            let max_visible_index = current_offset.saturating_add(visible_height.saturating_sub(1));
            if self.selected > max_visible_index {
                self.selected = max_visible_index.min(self.book_infos.len().saturating_sub(1));
                self.list_state = ListState::default()
                    .with_selected(Some(self.selected))
                    .with_offset(current_offset);
            }
        }
    }

    pub fn get_selected_book(&self) -> Option<&BookInfo> {
        self.book_infos.get(self.selected)
    }

    pub fn book_count(&self) -> usize {
        self.book_infos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.book_infos.is_empty()
    }

    /// Handle mouse click at the given position
    /// Returns true if an item was clicked
    pub fn handle_mouse_click(&mut self, _x: u16, y: u16, area: Rect) -> bool {
        // Account for the border (1 line at top and bottom)
        if y > area.y && y < area.y + area.height - 1 {
            let relative_y = y - area.y - 1; // Subtract 1 for the top border

            // Get the current scroll offset from the list_state
            let offset = self.list_state.offset();

            // Calculate the actual index in the list
            let new_index = offset + relative_y as usize;

            // Check if the click is within the valid range
            if new_index < self.book_infos.len() {
                self.selected = new_index;
                self.list_state.select(Some(new_index));
                return true;
            }
        }
        false
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        is_focused: bool,
        palette: &Base16Palette,
        current_book_index: Option<usize>,
    ) {
        // Get focus-aware colors
        let (text_color, border_color, _bg_color) = palette.get_panel_colors(is_focused);
        let (selection_bg, selection_fg) = palette.get_selection_colors(is_focused);

        // Create list items
        let mut items: Vec<ListItem> = Vec::new();

        for (idx, book_info) in self.book_infos.iter().enumerate() {
            // Determine base style for this book
            let base_style = if Some(idx) == current_book_index {
                Style::default().fg(palette.base_08) // Red for currently open book
            } else {
                Style::default().fg(text_color)
            };

            // Check if this item is a search match
            let is_search_match = self.search_state.is_match(idx);
            let is_current_search_match = self.search_state.is_current_match(idx);

            // Build the line with potential search highlights
            let content = if self.search_state.active && is_search_match {
                // Find the highlight ranges for this match
                let empty_vec = vec![];
                let highlight_ranges = self
                    .search_state
                    .matches
                    .iter()
                    .find(|m| m.index == idx)
                    .map(|m| &m.highlight_ranges)
                    .unwrap_or(&empty_vec);

                let mut spans = Vec::new();
                let text = &book_info.display_name;
                let mut last_end = 0;

                for (start, end) in highlight_ranges {
                    // Add non-highlighted text before this match
                    if *start > last_end {
                        spans.push(Span::styled(text[last_end..*start].to_string(), base_style));
                    }

                    // Add highlighted match text
                    let highlight_style = if is_current_search_match {
                        // Current match: bright yellow background with black text
                        Style::default().bg(Color::Yellow).fg(Color::Black)
                    } else {
                        // Other matches: dim yellow background
                        Style::default().bg(Color::Rgb(100, 100, 0)).fg(text_color)
                    };

                    spans.push(Span::styled(
                        text[*start..*end].to_string(),
                        highlight_style,
                    ));

                    last_end = *end;
                }

                // Add remaining non-highlighted text
                if last_end < text.len() {
                    spans.push(Span::styled(text[last_end..].to_string(), base_style));
                }

                Line::from(spans)
            } else {
                // No search active or not a match - render normally
                Line::from(vec![Span::styled(
                    book_info.display_name.clone(),
                    base_style,
                )])
            };

            items.push(ListItem::new(content));
        }

        // For the currently open book, we want to keep the red color even when selected
        let highlight_style = if Some(self.selected) == current_book_index {
            // Currently open book is selected - keep red foreground, add selection background
            Style::default().bg(selection_bg).fg(palette.base_08) // Keep red text
        } else {
            // Normal selection highlighting
            Style::default().bg(selection_bg).fg(selection_fg)
        };

        let files = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Books")
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(palette.base_00)),
            )
            .highlight_style(highlight_style)
            .style(Style::default().bg(palette.base_00));

        f.render_stateful_widget(files, area, &mut self.list_state);
    }
}

impl SearchablePanel for BookList {
    fn start_search(&mut self) {
        self.search_state.start_search(self.selected);
    }

    fn cancel_search(&mut self) {
        let original_position = self.search_state.cancel_search();
        self.set_selection_to_index(original_position);
    }

    fn confirm_search(&mut self) {
        self.search_state.confirm_search();
        // If search was cancelled (empty query), restore position
        if !self.search_state.active {
            let original_position = self.search_state.original_position;
            self.set_selection_to_index(original_position);
        }
    }

    fn exit_search(&mut self) {
        self.search_state.exit_search();
        // Keep current position
    }

    fn update_search_query(&mut self, query: &str) {
        self.search_state.update_query(query.to_string());

        // Find matches in book names
        let searchable = self.get_searchable_content();
        let matches = find_matches_in_text(query, &searchable);
        self.search_state.set_matches(matches);

        // Jump to match if found
        if let Some(match_index) = self.search_state.get_current_match() {
            self.jump_to_match(match_index);
        }
    }

    fn next_match(&mut self) {
        if self.search_state.matches.is_empty() {
            return;
        }

        // If we have a current match index, go to the next one
        if let Some(current_idx) = self.search_state.current_match_index {
            // Move to next match
            let next_idx = (current_idx + 1) % self.search_state.matches.len();
            self.search_state.current_match_index = Some(next_idx);

            if let Some(search_match) = self.search_state.matches.get(next_idx) {
                self.jump_to_match(search_match.index);
            }
        } else {
            // No current match, find the first match after current selected position
            let current_position = self.selected;

            // Find the first match that's after the current position
            let mut next_match_idx = None;
            for (idx, search_match) in self.search_state.matches.iter().enumerate() {
                if search_match.index > current_position {
                    next_match_idx = Some(idx);
                    break;
                }
            }

            // If no match found after current position, wrap to beginning
            let target_idx = next_match_idx.unwrap_or(0);
            self.search_state.current_match_index = Some(target_idx);

            if let Some(search_match) = self.search_state.matches.get(target_idx) {
                self.jump_to_match(search_match.index);
            }
        }
    }

    fn previous_match(&mut self) {
        if self.search_state.matches.is_empty() {
            return;
        }

        // If we have a current match index, go to the previous one
        if let Some(current_idx) = self.search_state.current_match_index {
            // Move to previous match
            let prev_idx = if current_idx == 0 {
                self.search_state.matches.len() - 1
            } else {
                current_idx - 1
            };
            self.search_state.current_match_index = Some(prev_idx);

            if let Some(search_match) = self.search_state.matches.get(prev_idx) {
                self.jump_to_match(search_match.index);
            }
        } else {
            // No current match, find the last match before current selected position
            let current_position = self.selected;

            // Find the last match that's before the current position
            let mut prev_match_idx = None;
            for (idx, search_match) in self.search_state.matches.iter().enumerate().rev() {
                if search_match.index < current_position {
                    prev_match_idx = Some(idx);
                    break;
                }
            }

            // If no match found before current position, wrap to end
            let target_idx = prev_match_idx.unwrap_or(self.search_state.matches.len() - 1);
            self.search_state.current_match_index = Some(target_idx);

            if let Some(search_match) = self.search_state.matches.get(target_idx) {
                self.jump_to_match(search_match.index);
            }
        }
    }

    fn get_search_state(&self) -> &SearchState {
        &self.search_state
    }

    fn is_searching(&self) -> bool {
        self.search_state.active
    }

    fn has_matches(&self) -> bool {
        !self.search_state.matches.is_empty()
    }

    fn jump_to_match(&mut self, match_index: usize) {
        if match_index < self.book_infos.len() {
            self.set_selection_to_index(match_index);
        }
    }

    fn get_searchable_content(&self) -> Vec<String> {
        self.book_infos
            .iter()
            .map(|book| book.display_name.clone())
            .collect()
    }

    // Note: These are placeholder methods for search input modes
    // They exist in the upstream version but we use query string mode for now
}

impl BookList {
    pub fn handle_search_char(&mut self, _c: char) {
        // Stub: character-by-character input for book list search not implemented
        // This is called when in InputMode, but we use traditional query string mode
    }

    pub fn handle_search_backspace(&mut self) {
        // Stub: backspace handling for book list search not implemented
        // This is called when in InputMode, but we use traditional query string mode
    }
}

#[cfg(test)]
mod tests {}
