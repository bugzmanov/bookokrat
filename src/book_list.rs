use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::table_of_contents::{TableOfContents, TocItem};
use crate::theme::Base16Palette;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

#[derive(Clone)]
pub struct CurrentBookInfo {
    pub path: String,
    pub toc_items: Vec<TocItem>, // ADT-based hierarchical structure
    pub current_chapter: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum BookListMode {
    BookSelection,
    TableOfContents,
}

pub struct BookList {
    pub selected: usize,
    pub list_state: ListState,
    pub mode: BookListMode,
    pub table_of_contents: TableOfContents,
    pub current_book_index: Option<usize>, // Index of currently open book
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
            mode: BookListMode::BookSelection,
            table_of_contents: TableOfContents::new(),
            current_book_index: None,
        }
    }

    pub fn move_selection_down(
        &mut self,
        book_manager: &BookManager,
        current_book_info: Option<&CurrentBookInfo>,
    ) {
        match self.mode {
            BookListMode::BookSelection => {
                if self.selected < book_manager.book_count().saturating_sub(1) {
                    self.selected += 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            BookListMode::TableOfContents => {
                if let Some(book_info) = current_book_info {
                    self.table_of_contents.move_selection_down(book_info);
                }
            }
        }
    }

    pub fn move_selection_up(&mut self, _current_book_info: Option<&CurrentBookInfo>) {
        match self.mode {
            BookListMode::BookSelection => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            BookListMode::TableOfContents => {
                self.table_of_contents.move_selection_up();
            }
        }
    }

    pub fn set_selection_to_index(&mut self, index: usize) {
        self.selected = index;
        self.list_state.select(Some(index));
    }

    pub fn switch_to_toc_mode(&mut self, book_index: usize) {
        self.mode = BookListMode::TableOfContents;
        self.current_book_index = Some(book_index);
        self.table_of_contents = TableOfContents::new();
    }

    pub fn switch_to_book_mode(&mut self) {
        self.mode = BookListMode::BookSelection;
        // Keep current_book_index so we can highlight the open book
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        is_focused: bool,
        palette: &Base16Palette,
        bookmarks: &Bookmarks,
        book_manager: &BookManager,
        current_book_info: Option<&CurrentBookInfo>,
    ) {
        match self.mode {
            BookListMode::BookSelection => {
                self.render_book_list(f, area, is_focused, palette, bookmarks, book_manager);
            }
            BookListMode::TableOfContents => {
                if let Some(book_info) = current_book_info {
                    if let Some(current_idx) = self.current_book_index {
                        if let Some(book) = book_manager.get_book_info(current_idx) {
                            self.table_of_contents.render(
                                f,
                                area,
                                is_focused,
                                palette,
                                book_info,
                                &book.display_name,
                            );
                        }
                    }
                }
            }
        }
    }

    fn render_book_list(
        &mut self,
        f: &mut Frame,
        area: Rect,
        is_focused: bool,
        palette: &Base16Palette,
        bookmarks: &Bookmarks,
        book_manager: &BookManager,
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
            let book_style = if Some(idx) == self.current_book_index {
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
        let highlight_style = if Some(self.selected) == self.current_book_index {
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
