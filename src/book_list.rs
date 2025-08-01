use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::theme::Base16Palette;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct BookList {
    pub selected: usize,
    pub list_state: ListState,
}

impl BookList {
    pub fn new(book_manager: &BookManager) -> Self {
        let has_files = !book_manager.is_empty();

        let mut list_state = ListState::default();
        if has_files {
            list_state.select(Some(0));
        }

        Self {
            selected: 0,
            list_state,
        }
    }

    pub fn move_selection_down(&mut self, book_manager: &BookManager) {
        if self.selected < book_manager.book_count().saturating_sub(1) {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn set_selection_to_index(&mut self, index: usize) {
        self.selected = index;
        self.list_state.select(Some(index));
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        is_focused: bool,
        palette: &Base16Palette,
        bookmarks: &Bookmarks,
        book_manager: &BookManager,
        current_book_index: Option<usize>,
    ) {
        // Get focus-aware colors
        let (text_color, border_color, _bg_color) = palette.get_panel_colors(is_focused);
        let (selection_bg, selection_fg) = palette.get_selection_colors(is_focused);
        let timestamp_color = if is_focused {
            palette.base_04 // Brighter timestamp for focused
        } else {
            palette.base_01 // Much dimmer timestamp for unfocused
        };

        // Create list items with last read timestamps
        let mut items: Vec<ListItem> = Vec::new();

        for (idx, book_info) in book_manager.books.iter().enumerate() {
            let bookmark = bookmarks.get_bookmark(&book_info.path);
            let last_read = bookmark
                .map(|b| b.last_read.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "Never".to_string());

            // Highlight the currently open book in red
            let book_style = if Some(idx) == current_book_index {
                Style::default().fg(palette.base_08) // Red for currently open book
            } else {
                Style::default().fg(text_color)
            };

            let content = Line::from(vec![
                Span::styled(book_info.display_name.clone(), book_style),
                Span::styled(
                    format!(" ({})", last_read),
                    Style::default().fg(timestamp_color),
                ),
            ]);
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

#[cfg(test)]
mod tests {
    use super::*;
}
