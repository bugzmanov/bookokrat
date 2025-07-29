use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::text_generator::TextGenerator;
use crate::theme::Base16Palette;
use log::debug;
use std::collections::HashMap;
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
    pub chapters: Vec<ChapterInfo>, // Keep for backward compatibility
    pub sections: Vec<SectionInfo>, // Old hierarchical structure - keep for compatibility
    pub toc_items: Vec<TocItem>, // New ADT-based hierarchical structure
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
                    // Prefer new ADT-based structure, fall back to old structure, then flat chapters
                    if !current_book.toc_items.is_empty() {
                        self.render_toc_items(current_book, &mut items, palette, &current_book.toc_items, 0);
                    } else if !current_book.sections.is_empty() {
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

    /// Render TOC items using the new ADT structure without duplication
    fn render_toc_items(
        &self,
        current_book: &CurrentBookInfo,
        items: &mut Vec<ListItem>,
        palette: &Base16Palette,
        toc_items: &[TocItem],
        indent_level: usize,
    ) {
        for item in toc_items {
            match item {
                TocItem::Chapter { title, index, .. } => {
                    // Render a simple chapter
                    let chapter_style = if *index == current_book.current_chapter {
                        Style::default().fg(palette.base_08) // Highlight current chapter
                    } else {
                        Style::default().fg(palette.base_03) // Dimmer for other chapters
                    };

                    let indent = "  ".repeat(indent_level + 1);
                    let chapter_content = Line::from(vec![Span::styled(
                        format!("{}{}", indent, title),
                        chapter_style,
                    )]);
                    items.push(ListItem::new(chapter_content));
                },
                TocItem::Section { title, index, children, is_expanded, .. } => {
                    // Render section header with expand/collapse indicator
                    let section_icon = if *is_expanded { "▼" } else { "▶" };
                    
                    // Determine the style based on whether this section is also a readable chapter
                    let section_style = if let Some(section_index) = index {
                        if *section_index == current_book.current_chapter {
                            Style::default().fg(palette.base_08) // Highlight if current chapter
                        } else {
                            Style::default().fg(palette.base_0d) // Blue for readable sections
                        }
                    } else {
                        Style::default().fg(palette.base_0d) // Blue for non-readable sections
                    };

                    let indent = "  ".repeat(indent_level + 1);
                    let section_content = Line::from(vec![Span::styled(
                        format!("{}{} {}", indent, section_icon, title),
                        section_style,
                    )]);
                    items.push(ListItem::new(section_content));

                    // Render children if expanded
                    if *is_expanded {
                        self.render_toc_items(current_book, items, palette, children, indent_level + 1);
                    }
                }
            }
        }
    }

    /// Convert TOC entries to hierarchical sections with proper flat/hierarchical handling
    pub fn convert_toc_to_sections(
        text_generator: &TextGenerator,
        toc_entries: &[TocEntry],
        chapter_map: &HashMap<String, (usize, ChapterInfo)>,
    ) -> Vec<SectionInfo> {
        let mut sections = Vec::new();

        // Check if this is a flat structure (no TOC entries have children)
        let is_flat_structure = toc_entries.iter().all(|entry| entry.children.is_empty());
        debug!(
            "TOC structure type: {}",
            if is_flat_structure {
                "flat"
            } else {
                "hierarchical"
            }
        );

        if is_flat_structure && !toc_entries.is_empty() {
            // For flat structures, create a single "Chapters" section containing all chapters
            let mut all_chapters = Vec::new();
            let mut start_chapter_index = None;

            for toc_entry in toc_entries {
                let normalized_href = text_generator.normalize_href(&toc_entry.href);
                debug!(
                    "Adding flat TOC entry '{}' with href '{}' -> normalized: '{}'",
                    toc_entry.title, toc_entry.href, normalized_href
                );

                if let Some((chapter_index, chapter_info)) = chapter_map.get(&normalized_href) {
                    debug!(
                        "Found matching chapter {} for flat TOC entry '{}'",
                        chapter_index, toc_entry.title
                    );

                    // Use the first chapter as the start_chapter
                    if start_chapter_index.is_none() {
                        start_chapter_index = Some(*chapter_index);
                    }

                    all_chapters.push(chapter_info.clone());
                } else {
                    debug!(
                        "No matching chapter found for flat TOC entry '{}' with normalized href '{}'",
                        toc_entry.title, normalized_href
                    );
                }
            }

            if !all_chapters.is_empty() {
                let chapters_count = all_chapters.len();
                let section = SectionInfo {
                    title: "Chapters".to_string(),
                    start_chapter: start_chapter_index.unwrap_or(0),
                    chapters: all_chapters,
                    is_expanded: true,
                };
                sections.push(section);
                debug!(
                    "Created single flat section with {} chapters",
                    chapters_count
                );
            }

            return sections;
        }

        // Handle hierarchical structure (original logic)
        for toc_entry in toc_entries {
            let normalized_href = text_generator.normalize_href(&toc_entry.href);
            debug!(
                "Converting TOC entry '{}' with href '{}' -> normalized: '{}'",
                toc_entry.title, toc_entry.href, normalized_href
            );

            if let Some((chapter_index, chapter_info)) = chapter_map.get(&normalized_href) {
                debug!(
                    "Found matching chapter {} for TOC entry '{}'",
                    chapter_index, toc_entry.title
                );
                if !toc_entry.children.is_empty() {
                    // This is a section with children
                    let mut section_chapters = Vec::new();

                    // Add the section header chapter
                    section_chapters.push(chapter_info.clone());

                    // Add child chapters
                    for child in &toc_entry.children {
                        let child_normalized_href = text_generator.normalize_href(&child.href);
                        if let Some((_, child_chapter_info)) =
                            chapter_map.get(&child_normalized_href)
                        {
                            section_chapters.push(child_chapter_info.clone());
                        }
                    }

                    let section = SectionInfo {
                        title: toc_entry.title.clone(),
                        start_chapter: *chapter_index,
                        chapters: section_chapters,
                        is_expanded: true,
                    };
                    sections.push(section);
                } else {
                    // This is a standalone chapter - create a section for it
                    let section = SectionInfo {
                        title: toc_entry.title.clone(),
                        start_chapter: *chapter_index,
                        chapters: vec![chapter_info.clone()],
                        is_expanded: true,
                    };
                    sections.push(section);
                }
            } else {
                debug!(
                    "No matching chapter found for TOC entry '{}' with normalized href '{}'",
                    toc_entry.title, normalized_href
                );
            }
        }

        sections
    }

    /// Convert TocEntry tree to TocItem ADT
    pub fn convert_toc_to_items(
        text_generator: &TextGenerator,
        toc_entries: &[TocEntry],
        chapter_map: &HashMap<String, (usize, ChapterInfo)>,
    ) -> Vec<TocItem> {
        let mut items = Vec::new();

        for toc_entry in toc_entries {
            let normalized_href = text_generator.normalize_href(&toc_entry.href);
            debug!(
                "Converting TOC entry '{}' with href '{}' -> normalized: '{}'",
                toc_entry.title, toc_entry.href, normalized_href
            );

            if let Some((chapter_index, chapter_info)) = chapter_map.get(&normalized_href) {
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
                    let children = Self::convert_toc_to_items(text_generator, &toc_entry.children, chapter_map);
                    
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
                    let children = Self::convert_toc_to_items(text_generator, &toc_entry.children, chapter_map);
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
    fn test_convert_toc_to_sections_flat_vs_hierarchical() {
        let text_generator = TextGenerator::new();

        // Test Case 1: Flat structure (like careless.epub)
        let flat_toc_entries = vec![
            TocEntry {
                title: "Cover".to_string(),
                href: "xhtml/cover.xhtml".to_string(),
                children: vec![], // No children = flat
            },
            TocEntry {
                title: "Title Page".to_string(),
                href: "xhtml/title.xhtml#tit".to_string(),
                children: vec![], // No children = flat
            },
            TocEntry {
                title: "1. Simpleminded Hope".to_string(),
                href: "xhtml/chapter1.xhtml#ch1".to_string(),
                children: vec![], // No children = flat
            },
            TocEntry {
                title: "2. Pitching the Revolution".to_string(),
                href: "xhtml/chapter2.xhtml#ch2".to_string(),
                children: vec![], // No children = flat
            },
        ];

        // Test Case 2: Hierarchical structure (like yasno.epub)
        let hierarchical_toc_entries = vec![
            TocEntry {
                title: "Introduction".to_string(),
                href: "Text/intro.html".to_string(),
                children: vec![], // Standalone chapter
            },
            TocEntry {
                title: "Context".to_string(),
                href: "Text/Section0002.html".to_string(),
                children: vec![
                    // Has children = hierarchical
                    TocEntry {
                        title: "How context affects".to_string(),
                        href: "Text/content9.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "What it looks like".to_string(),
                        href: "Text/content11.html".to_string(),
                        children: vec![],
                    },
                ],
            },
            TocEntry {
                title: "Interest".to_string(),
                href: "Text/content21.html".to_string(),
                children: vec![
                    // Has children = hierarchical
                    TocEntry {
                        title: "Great school distortion".to_string(),
                        href: "Text/content23.html".to_string(),
                        children: vec![],
                    },
                ],
            },
        ];

        // Create chapter maps for both test cases
        let mut flat_chapter_map = HashMap::new();
        flat_chapter_map.insert(
            "xhtml/cover.xhtml".to_string(),
            (
                0,
                ChapterInfo {
                    title: "Cover".to_string(),
                    index: 0,
                    is_section_header: false,
                },
            ),
        );
        flat_chapter_map.insert(
            "xhtml/title.xhtml".to_string(),
            (
                1,
                ChapterInfo {
                    title: "Title Page".to_string(),
                    index: 1,
                    is_section_header: false,
                },
            ),
        );
        flat_chapter_map.insert(
            "xhtml/chapter1.xhtml".to_string(),
            (
                2,
                ChapterInfo {
                    title: "1. Simpleminded Hope".to_string(),
                    index: 2,
                    is_section_header: false,
                },
            ),
        );
        flat_chapter_map.insert(
            "xhtml/chapter2.xhtml".to_string(),
            (
                3,
                ChapterInfo {
                    title: "2. Pitching the Revolution".to_string(),
                    index: 3,
                    is_section_header: false,
                },
            ),
        );

        let mut hierarchical_chapter_map = HashMap::new();
        hierarchical_chapter_map.insert(
            "Text/intro.html".to_string(),
            (
                0,
                ChapterInfo {
                    title: "Introduction".to_string(),
                    index: 0,
                    is_section_header: false,
                },
            ),
        );
        hierarchical_chapter_map.insert(
            "Text/Section0002.html".to_string(),
            (
                1,
                ChapterInfo {
                    title: "Context".to_string(),
                    index: 1,
                    is_section_header: true,
                },
            ),
        );
        hierarchical_chapter_map.insert(
            "Text/content9.html".to_string(),
            (
                2,
                ChapterInfo {
                    title: "How context affects".to_string(),
                    index: 2,
                    is_section_header: false,
                },
            ),
        );
        hierarchical_chapter_map.insert(
            "Text/content11.html".to_string(),
            (
                3,
                ChapterInfo {
                    title: "What it looks like".to_string(),
                    index: 3,
                    is_section_header: false,
                },
            ),
        );
        hierarchical_chapter_map.insert(
            "Text/content21.html".to_string(),
            (
                4,
                ChapterInfo {
                    title: "Interest".to_string(),
                    index: 4,
                    is_section_header: true,
                },
            ),
        );
        hierarchical_chapter_map.insert(
            "Text/content23.html".to_string(),
            (
                5,
                ChapterInfo {
                    title: "Great school distortion".to_string(),
                    index: 5,
                    is_section_header: false,
                },
            ),
        );

        // Test flat structure conversion
        let flat_sections =
            BookList::convert_toc_to_sections(&text_generator, &flat_toc_entries, &flat_chapter_map);

        // Verify flat structure creates single "Chapters" section
        assert_eq!(
            flat_sections.len(),
            1,
            "Flat structure should create exactly 1 section, got {}",
            flat_sections.len()
        );
        assert_eq!(
            flat_sections[0].title, "Chapters",
            "Flat structure section should be titled 'Chapters'"
        );
        assert_eq!(
            flat_sections[0].chapters.len(),
            4,
            "Flat structure should contain all 4 chapters in single section"
        );

        // Verify chapter order and titles in flat structure
        let flat_chapter_titles: Vec<&String> = flat_sections[0]
            .chapters
            .iter()
            .map(|ch| &ch.title)
            .collect();
        assert_eq!(
            flat_chapter_titles,
            vec![
                &"Cover".to_string(),
                &"Title Page".to_string(),
                &"1. Simpleminded Hope".to_string(),
                &"2. Pitching the Revolution".to_string()
            ]
        );

        // Test hierarchical structure conversion
        let hierarchical_sections = BookList::convert_toc_to_sections(
            &text_generator,
            &hierarchical_toc_entries,
            &hierarchical_chapter_map,
        );

        // Verify hierarchical structure preserves multiple sections
        assert_eq!(
            hierarchical_sections.len(),
            3,
            "Hierarchical structure should create 3 sections (Introduction standalone + Context + Interest), got {}",
            hierarchical_sections.len()
        );

        // Verify standalone chapter becomes its own section
        assert_eq!(hierarchical_sections[0].title, "Introduction");
        assert_eq!(hierarchical_sections[0].chapters.len(), 1);

        // Verify section with children
        assert_eq!(hierarchical_sections[1].title, "Context");
        assert_eq!(
            hierarchical_sections[1].chapters.len(),
            3,
            "Context section should have section header + 2 child chapters"
        );

        // Verify another section with children
        assert_eq!(hierarchical_sections[2].title, "Interest");
        assert_eq!(
            hierarchical_sections[2].chapters.len(),
            2,
            "Interest section should have section header + 1 child chapter"
        );

        // Verify that hierarchical sections contain the expected chapters
        let context_chapter_titles: Vec<&String> = hierarchical_sections[1]
            .chapters
            .iter()
            .map(|ch| &ch.title)
            .collect();
        assert_eq!(
            context_chapter_titles,
            vec![
                &"Context".to_string(),
                &"How context affects".to_string(),
                &"What it looks like".to_string()
            ]
        );

        let interest_chapter_titles: Vec<&String> = hierarchical_sections[2]
            .chapters
            .iter()
            .map(|ch| &ch.title)
            .collect();
        assert_eq!(
            interest_chapter_titles,
            vec![
                &"Interest".to_string(),
                &"Great school distortion".to_string()
            ]
        );
    }

    #[test]
    fn test_yasno_epub_hierarchical_structure() {
        let text_generator = TextGenerator::new();

        // Replicate the actual yasno.epub TOC structure from the toc_inspector output
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
            TocEntry {
                title: "Коротко — не значит ясно".to_string(),
                href: "Text/content5.html".to_string(),
                children: vec![],
            },
            TocEntry {
                title: "Когда люди не читают текст".to_string(),
                href: "Text/content6.html".to_string(),
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
                    TocEntry {
                        title: "Почему читатель пришел".to_string(),
                        href: "Text/content13.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Шаблоны".to_string(),
                        href: "Text/content15.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Исправление контекста".to_string(),
                        href: "Text/content17.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Последнее слово о контексте".to_string(),
                        href: "Text/content20.html".to_string(),
                        children: vec![],
                    },
                ],
            },
            // Hierarchical section: Интерес with children
            TocEntry {
                title: "Интерес".to_string(),
                href: "Text/content21.html".to_string(),
                children: vec![
                    TocEntry {
                        title: "Великое школьное искажение".to_string(),
                        href: "Text/content23.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Главный секрет внимания".to_string(),
                        href: "Text/content25.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Ответственность".to_string(),
                        href: "Text/content26.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Полезное действие для читателя".to_string(),
                        href: "Text/content28.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Прагматика".to_string(),
                        href: "Text/content29.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Социальное".to_string(),
                        href: "Text/content30.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Эмоциональное".to_string(),
                        href: "Text/content31.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Всё о себе да о себе".to_string(),
                        href: "Text/content32.html".to_string(),
                        children: vec![],
                    },
                    TocEntry {
                        title: "Работа с темой статьи".to_string(),
                        href: "Text/content33.html".to_string(),
                        children: vec![],
                    },
                ],
            },
        ];

        // Create chapter map that matches yasno.epub structure
        let mut yasno_chapter_map = HashMap::new();
        
        // Add standalone chapters
        yasno_chapter_map.insert(
            "Text/content2.html".to_string(),
            (0, ChapterInfo { title: "Главное за пять минут".to_string(), index: 0, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content4.html".to_string(),
            (1, ChapterInfo { title: "Проклятие умных людей".to_string(), index: 1, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content5.html".to_string(),
            (2, ChapterInfo { title: "Коротко — не значит ясно".to_string(), index: 2, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content6.html".to_string(),
            (3, ChapterInfo { title: "Когда люди не читают текст".to_string(), index: 3, is_section_header: false }),
        );
        
        // Add Контекст section and its children
        yasno_chapter_map.insert(
            "Text/Section0002.html".to_string(),
            (4, ChapterInfo { title: "Контекст".to_string(), index: 4, is_section_header: true }),
        );
        yasno_chapter_map.insert(
            "Text/content9.html".to_string(),
            (5, ChapterInfo { title: "Как еще влияет контекст".to_string(), index: 5, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content11.html".to_string(),
            (6, ChapterInfo { title: "Как это выглядит".to_string(), index: 6, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content13.html".to_string(),
            (7, ChapterInfo { title: "Почему читатель пришел".to_string(), index: 7, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content15.html".to_string(),
            (8, ChapterInfo { title: "Шаблоны".to_string(), index: 8, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content17.html".to_string(),
            (9, ChapterInfo { title: "Исправление контекста".to_string(), index: 9, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content20.html".to_string(),
            (10, ChapterInfo { title: "Последнее слово о контексте".to_string(), index: 10, is_section_header: false }),
        );
        
        // Add Интерес section and its children
        yasno_chapter_map.insert(
            "Text/content21.html".to_string(),
            (11, ChapterInfo { title: "Интерес".to_string(), index: 11, is_section_header: true }),
        );
        yasno_chapter_map.insert(
            "Text/content23.html".to_string(),
            (12, ChapterInfo { title: "Великое школьное искажение".to_string(), index: 12, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content25.html".to_string(),
            (13, ChapterInfo { title: "Главный секрет внимания".to_string(), index: 13, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content26.html".to_string(),
            (14, ChapterInfo { title: "Ответственность".to_string(), index: 14, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content28.html".to_string(),
            (15, ChapterInfo { title: "Полезное действие для читателя".to_string(), index: 15, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content29.html".to_string(),
            (16, ChapterInfo { title: "Прагматика".to_string(), index: 16, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content30.html".to_string(),
            (17, ChapterInfo { title: "Социальное".to_string(), index: 17, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content31.html".to_string(),
            (18, ChapterInfo { title: "Эмоциональное".to_string(), index: 18, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content32.html".to_string(),
            (19, ChapterInfo { title: "Всё о себе да о себе".to_string(), index: 19, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content33.html".to_string(),
            (20, ChapterInfo { title: "Работа с темой статьи".to_string(), index: 20, is_section_header: false }),
        );

        // Test the conversion
        let sections = BookList::convert_toc_to_sections(&text_generator, &yasno_toc_entries, &yasno_chapter_map);

        // Print debug info to see what we're getting
        println!("Yasno.epub test - Generated {} sections:", sections.len());
        for (i, section) in sections.iter().enumerate() {
            println!("  Section {}: '{}' with {} chapters", i, section.title, section.chapters.len());
            for (j, chapter) in section.chapters.iter().enumerate() {
                println!("    Chapter {}.{}: '{}'", i, j, chapter.title);
            }
        }

        // Expected structure based on ClearView screenshot:
        // 1. Standalone chapters should each get their own section
        // 2. Hierarchical sections should be preserved with children
        
        // Should have 6 sections total:
        // 1. "Главное за пять минут" (standalone)
        // 2. "Проклятие умных людей" (standalone) 
        // 3. "Коротко — не значит ясно" (standalone)
        // 4. "Когда люди не читают текст" (standalone)
        // 5. "Контекст" (with 7 chapters: section header + 6 children)
        // 6. "Интерес" (with 10 chapters: section header + 9 children)
        
        assert_eq!(sections.len(), 6, "Should have 6 sections total");
        
        // Check standalone chapters
        assert_eq!(sections[0].title, "Главное за пять минут");
        assert_eq!(sections[0].chapters.len(), 1);
        
        assert_eq!(sections[1].title, "Проклятие умных людей");
        assert_eq!(sections[1].chapters.len(), 1);
        
        assert_eq!(sections[2].title, "Коротко — не значит ясно");
        assert_eq!(sections[2].chapters.len(), 1);
        
        assert_eq!(sections[3].title, "Когда люди не читают текст");
        assert_eq!(sections[3].chapters.len(), 1);
        
        // Check Контекст section (should have section header + 6 children = 7 total)
        assert_eq!(sections[4].title, "Контекст");
        assert_eq!(sections[4].chapters.len(), 7, "Контекст should have 7 chapters (header + 6 children)");
        assert_eq!(sections[4].chapters[0].title, "Контекст"); // Section header
        assert_eq!(sections[4].chapters[1].title, "Как еще влияет контекст"); // First child
        assert_eq!(sections[4].chapters[6].title, "Последнее слово о контексте"); // Last child
        
        // Check Интерес section (should have section header + 9 children = 10 total)
        assert_eq!(sections[5].title, "Интерес");
        assert_eq!(sections[5].chapters.len(), 10, "Интерес should have 10 chapters (header + 9 children)");
        assert_eq!(sections[5].chapters[0].title, "Интерес"); // Section header
        assert_eq!(sections[5].chapters[1].title, "Великое школьное искажение"); // First child
        assert_eq!(sections[5].chapters[9].title, "Работа с темой статьи"); // Last child
    }

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
        yasno_chapter_map.insert(
            "Text/content2.html".to_string(),
            (0, ChapterInfo { title: "Главное за пять минут".to_string(), index: 0, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content4.html".to_string(),
            (1, ChapterInfo { title: "Проклятие умных людей".to_string(), index: 1, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/Section0002.html".to_string(),
            (2, ChapterInfo { title: "Контекст".to_string(), index: 2, is_section_header: true }),
        );
        yasno_chapter_map.insert(
            "Text/content9.html".to_string(),
            (3, ChapterInfo { title: "Как еще влияет контекст".to_string(), index: 3, is_section_header: false }),
        );
        yasno_chapter_map.insert(
            "Text/content11.html".to_string(),
            (4, ChapterInfo { title: "Как это выглядит".to_string(), index: 4, is_section_header: false }),
        );

        // Test the ADT conversion
        let toc_items = BookList::convert_toc_to_items(&text_generator, &yasno_toc_entries, &yasno_chapter_map);

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
            TocItem::Section { title, index, children, .. } => {
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
                TocItem::Section { title, index, children, .. } => {
                    let index_str = if let Some(idx) = index {
                        format!("index: {}", idx)
                    } else {
                        "not readable".to_string()
                    };
                    println!("  {}: Section '{}' ({}) with {} children", i, title, index_str, children.len());
                    for (j, child) in children.iter().enumerate() {
                        match child {
                            TocItem::Chapter { title, index, .. } => {
                                println!("    {}.{}: Chapter '{}' (index: {})", i, j, title, index);
                            }
                            TocItem::Section { title, index, children, .. } => {
                                let index_str = if let Some(idx) = index {
                                    format!("index: {}", idx)
                                } else {
                                    "not readable".to_string()
                                };
                                println!("    {}.{}: Section '{}' ({}) with {} children", i, j, title, index_str, children.len());
                            }
                        }
                    }
                }
            }
        }
    }
}
