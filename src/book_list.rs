use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::table_of_contents::TableOfContents;
use crate::text_generator::TextGenerator;
use crate::theme::Base16Palette;
use log::debug;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashMap;

/// New ADT-based model for TOC items
#[derive(Clone, Debug)]
pub enum TocItem {
    /// A leaf chapter that can be read
    Chapter {
        title: String,
        href: String,
        index: usize,
    },
    /// A section that may have its own content and contains child items
    Section {
        title: String,
        href: Option<String>, // Some sections are readable, others are just containers
        index: Option<usize>, // Chapter index if this section is also readable
        children: Vec<TocItem>,
        is_expanded: bool,
    },
}

impl TocItem {
    /// Get the title of this TOC item
    pub fn title(&self) -> &str {
        match self {
            TocItem::Chapter { title, .. } => title,
            TocItem::Section { title, .. } => title,
        }
    }

    /// Get the chapter index if this item is readable
    pub fn chapter_index(&self) -> Option<usize> {
        match self {
            TocItem::Chapter { index, .. } => Some(*index),
            TocItem::Section { index, .. } => *index,
        }
    }

    /// Check if this item has children
    pub fn has_children(&self) -> bool {
        match self {
            TocItem::Chapter { .. } => false,
            TocItem::Section { children, .. } => !children.is_empty(),
        }
    }

    /// Get children if this is a section
    pub fn children(&self) -> &[TocItem] {
        match self {
            TocItem::Chapter { .. } => &[],
            TocItem::Section { children, .. } => children,
        }
    }

    /// Check if this section is expanded (only applies to sections)
    pub fn is_expanded(&self) -> bool {
        match self {
            TocItem::Chapter { .. } => false, // Chapters don't expand
            TocItem::Section { is_expanded, .. } => *is_expanded,
        }
    }

    /// Toggle expansion state (only applies to sections)
    pub fn toggle_expansion(&mut self) {
        if let TocItem::Section { is_expanded, .. } = self {
            *is_expanded = !*is_expanded;
        }
    }
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

    /// Convert TocEntry tree to TocItem ADT
    pub fn convert_toc_to_items(
        text_generator: &TextGenerator,
        toc_entries: &[TocEntry],
        chapter_map: &HashMap<String, usize>,
    ) -> Vec<TocItem> {
        let mut items = Vec::new();

        for toc_entry in toc_entries {
            let normalized_href = text_generator.normalize_href(&toc_entry.href);
            debug!(
                "Converting TOC entry '{}' with href '{}' -> normalized: '{}'",
                toc_entry.title, toc_entry.href, normalized_href
            );

            if let Some(chapter_index) = chapter_map.get(&normalized_href) {
                debug!(
                    "Found matching chapter {} for TOC entry '{}'",
                    chapter_index, toc_entry.title
                );

                if toc_entry.children.is_empty() {
                    // This is a leaf chapter
                    items.push(TocItem::Chapter {
                        title: toc_entry.title.clone(),
                        href: toc_entry.href.clone(),
                        index: *chapter_index,
                    });
                } else {
                    // This is a section with children - recursively convert children
                    let children = Self::convert_toc_to_items(
                        text_generator,
                        &toc_entry.children,
                        chapter_map,
                    );

                    items.push(TocItem::Section {
                        title: toc_entry.title.clone(),
                        href: Some(toc_entry.href.clone()),
                        index: Some(*chapter_index), // Section is also readable
                        children,
                        is_expanded: true, // Default to expanded
                    });
                }
            } else {
                debug!(
                    "No matching chapter found for TOC entry '{}' with normalized href '{}'",
                    toc_entry.title, normalized_href
                );

                // Even if we don't find a matching chapter, we should still include
                // sections that might contain valid children
                if !toc_entry.children.is_empty() {
                    let children = Self::convert_toc_to_items(
                        text_generator,
                        &toc_entry.children,
                        chapter_map,
                    );
                    if !children.is_empty() {
                        items.push(TocItem::Section {
                            title: toc_entry.title.clone(),
                            href: None, // Not readable, just a container
                            index: None,
                            children,
                            is_expanded: true,
                        });
                    }
                }
            }
        }

        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_generator::TextGenerator;
    use std::collections::HashMap;

    #[test]
    fn test_adt_based_toc_conversion() {
        let text_generator = TextGenerator::new();

        // Test the new ADT-based conversion with yasno.epub structure
        let yasno_toc_entries = vec![
            // Standalone chapters
            TocEntry {
                title: "Главное за пять минут".to_string(),
                href: "Text/content2.html".to_string(),
                children: vec![],
            },
            TocEntry {
                title: "Проклятие умных людей".to_string(),
                href: "Text/content4.html".to_string(),
                children: vec![],
            },
            // Hierarchical section: Контекст with children
            TocEntry {
                title: "Контекст".to_string(),
                href: "Text/Section0002.html".to_string(),
                children: vec![
                    TocEntry {
                        title: "Как еще влияет контекст".to_string(),
                        href: "Text/content9.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Как это выглядит".to_string(),
                        href: "Text/content11.html".to_string(),
                        children: vec![],
                    },
                ],
            },
        ];

        // Create chapter map
        let mut yasno_chapter_map = HashMap::new();
        yasno_chapter_map.insert("Text/content2.html".to_string(), 0);
        yasno_chapter_map.insert("Text/content4.html".to_string(), 1);
        yasno_chapter_map.insert("Text/Section0002.html".to_string(), 2);
        yasno_chapter_map.insert("Text/content9.html".to_string(), 3);
        yasno_chapter_map.insert("Text/content11.html".to_string(), 4);

        // Test the ADT conversion
        let toc_items =
            BookList::convert_toc_to_items(&text_generator, &yasno_toc_entries, &yasno_chapter_map);

        // Should have 3 items: 2 chapters + 1 section
        assert_eq!(toc_items.len(), 3);

        // First item: standalone chapter
        match &toc_items[0] {
            TocItem::Chapter { title, index, .. } => {
                assert_eq!(title, "Главное за пять минут");
                assert_eq!(*index, 0);
            }
            _ => panic!("Expected first item to be a Chapter"),
        }

        // Second item: standalone chapter
        match &toc_items[1] {
            TocItem::Chapter { title, index, .. } => {
                assert_eq!(title, "Проклятие умных людей");
                assert_eq!(*index, 1);
            }
            _ => panic!("Expected second item to be a Chapter"),
        }

        // Third item: section with children (should NOT duplicate the section as a child)
        match &toc_items[2] {
            TocItem::Section {
                title,
                index,
                children,
                ..
            } => {
                assert_eq!(title, "Контекст");
                assert_eq!(*index, Some(2)); // Section is readable
                assert_eq!(children.len(), 2); // Should have 2 children, NOT 3 (no duplication)

                // Check children
                match &children[0] {
                    TocItem::Chapter { title, index, .. } => {
                        assert_eq!(title, "Как еще влияет контекст");
                        assert_eq!(*index, 3);
                    }
                    _ => panic!("Expected first child to be a Chapter"),
                }

                match &children[1] {
                    TocItem::Chapter { title, index, .. } => {
                        assert_eq!(title, "Как это выглядит");
                        assert_eq!(*index, 4);
                    }
                    _ => panic!("Expected second child to be a Chapter"),
                }
            }
            _ => panic!("Expected third item to be a Section"),
        }

        // Print debug info to verify structure
        println!("ADT-based TOC structure:");
        for (i, item) in toc_items.iter().enumerate() {
            match item {
                TocItem::Chapter { title, index, .. } => {
                    println!("  {}: Chapter '{}' (index: {})", i, title, index);
                }
                TocItem::Section {
                    title,
                    index,
                    children,
                    ..
                } => {
                    let index_str = if let Some(idx) = index {
                        format!("index: {}", idx)
                    } else {
                        "not readable".to_string()
                    };
                    println!(
                        "  {}: Section '{}' ({}) with {} children",
                        i,
                        title,
                        index_str,
                        children.len()
                    );
                    for (j, child) in children.iter().enumerate() {
                        match child {
                            TocItem::Chapter { title, index, .. } => {
                                println!("    {}.{}: Chapter '{}' (index: {})", i, j, title, index);
                            }
                            TocItem::Section {
                                title,
                                index,
                                children,
                                ..
                            } => {
                                let index_str = if let Some(idx) = index {
                                    format!("index: {}", idx)
                                } else {
                                    "not readable".to_string()
                                };
                                println!(
                                    "    {}.{}: Section '{}' ({}) with {} children",
                                    i,
                                    j,
                                    title,
                                    index_str,
                                    children.len()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
