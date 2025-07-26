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
        _is_active: bool,
        palette: &Base16Palette,
        bookmarks: &Bookmarks,
        book_manager: &BookManager,
    ) {
        let (interface_color, _, border_color, highlight_bg, highlight_fg) =
            palette.get_interface_colors(false);

        // Create list items with last read timestamps
        let items: Vec<ListItem> = book_manager
            .books
            .iter()
            .map(|book_info| {
                let bookmark = bookmarks.get_bookmark(&book_info.path);
                let last_read = bookmark
                    .map(|b| b.last_read.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Never".to_string());

                let content = Line::from(vec![
                    Span::styled(
                        book_info.display_name.clone(),
                        Style::default().fg(interface_color),
                    ),
                    Span::styled(
                        format!(" ({})", last_read),
                        Style::default().fg(palette.base_03),
                    ),
                ]);
                ListItem::new(content)
            })
            .collect();

        let files = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Books")
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(palette.base_00)),
            )
            .highlight_style(Style::default().bg(highlight_bg).fg(highlight_fg))
            .style(Style::default().bg(palette.base_00));

        f.render_stateful_widget(files, area, &mut self.list_state);
    }
}
