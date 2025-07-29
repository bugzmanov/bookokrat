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

#[derive(Clone)]
pub struct ChapterInfo {
    pub title: String,
    pub index: usize,
    pub is_section_header: bool,
}

#[derive(Clone)]
pub struct SectionInfo {
    pub title: String,
    pub start_chapter: usize,
    pub chapters: Vec<ChapterInfo>,
    pub is_expanded: bool,
}

#[derive(Clone, Debug)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub children: Vec<TocEntry>,
}

#[derive(Clone)]
pub struct CurrentBookInfo {
    pub path: String,
    pub chapters: Vec<ChapterInfo>, // Keep for backward compatibility
    pub sections: Vec<SectionInfo>, // New hierarchical structure
    pub current_chapter: usize,
}

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
        current_book_info: Option<&CurrentBookInfo>,
    ) {
        // Get focus-aware colors
        let (text_color, border_color, _bg_color) = palette.get_panel_colors(is_focused);
        let (selection_bg, selection_fg) = palette.get_selection_colors(is_focused);
        let timestamp_color = if is_focused {
            palette.base_04 // Brighter timestamp for focused
        } else {
            palette.base_01 // Much dimmer timestamp for unfocused
        };

        // Create list items with last read timestamps and chapter info
        let mut items: Vec<ListItem> = Vec::new();

        for book_info in &book_manager.books {
            let bookmark = bookmarks.get_bookmark(&book_info.path);
            let last_read = bookmark
                .map(|b| b.last_read.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "Never".to_string());

            let content = Line::from(vec![
                Span::styled(
                    book_info.display_name.clone(),
                    Style::default().fg(text_color),
                ),
                Span::styled(
                    format!(" ({})", last_read),
                    Style::default().fg(timestamp_color),
                ),
            ]);
            items.push(ListItem::new(content));

            // If this is the currently opened book, show its hierarchical structure
            if let Some(current_book) = current_book_info {
                if current_book.path == book_info.path {
                    // Use sections if available, otherwise fall back to flat chapter list
                    if !current_book.sections.is_empty() {
                        self.render_hierarchical_chapters(current_book, &mut items, palette);
                    } else {
                        self.render_flat_chapters(current_book, &mut items, palette);
                    }
                }
            }
        }

        let files = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Books")
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(palette.base_00)),
            )
            .highlight_style(Style::default().bg(selection_bg).fg(selection_fg))
            .style(Style::default().bg(palette.base_00));

        f.render_stateful_widget(files, area, &mut self.list_state);
    }

    /// Render chapters in hierarchical format with collapsible sections
    fn render_hierarchical_chapters(
        &self,
        current_book: &CurrentBookInfo,
        items: &mut Vec<ListItem>,
        palette: &Base16Palette,
    ) {
        for section in &current_book.sections {
            // Render section header
            let section_icon = if section.is_expanded { "▼" } else { "▶" };
            let section_style = Style::default().fg(palette.base_0d); // Blue for sections

            let section_content = Line::from(vec![Span::styled(
                format!("  {} {}", section_icon, section.title),
                section_style,
            )]);
            items.push(ListItem::new(section_content));

            // Render chapters in section if expanded
            if section.is_expanded {
                for chapter in &section.chapters {
                    let chapter_style = if chapter.index == current_book.current_chapter {
                        Style::default().fg(palette.base_08) // Highlight current chapter
                    } else {
                        Style::default().fg(palette.base_03) // Dimmer for other chapters
                    };

                    let chapter_content = Line::from(vec![Span::styled(
                        format!("    {}", chapter.title),
                        chapter_style,
                    )]);
                    items.push(ListItem::new(chapter_content));
                }
            }
        }
    }

    /// Render chapters in flat format (fallback for books without sections)
    fn render_flat_chapters(
        &self,
        current_book: &CurrentBookInfo,
        items: &mut Vec<ListItem>,
        palette: &Base16Palette,
    ) {
        for chapter in &current_book.chapters {
            let chapter_style = if chapter.index == current_book.current_chapter {
                Style::default().fg(palette.base_08) // Highlight current chapter
            } else {
                Style::default().fg(palette.base_03) // Dimmer for other chapters
            };

            let chapter_content = Line::from(vec![Span::styled(
                format!("  {}", chapter.title),
                chapter_style,
            )]);
            items.push(ListItem::new(chapter_content));
        }
    }
}
