use crate::book_list::{CurrentBookInfo, TocItem};
use crate::theme::Base16Palette;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct TableOfContents {
    pub selected_index: usize,
    pub list_state: ListState,
}

impl TableOfContents {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            selected_index: 0,
            list_state,
        }
    }

    pub fn move_selection_down(&mut self, current_book_info: &CurrentBookInfo) {
        let total_items = self.count_visible_toc_items(&current_book_info.toc_items);
        // Add 1 for the "<< books list" item
        if self.selected_index < total_items {
            self.selected_index += 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    /// Get the selected item (either back button or TOC item)
    pub fn get_selected_item<'a>(
        &self,
        current_book_info: &'a CurrentBookInfo,
    ) -> SelectedTocItem<'a> {
        if self.selected_index == 0 {
            SelectedTocItem::BackToBooks
        } else {
            // Subtract 1 to account for the back button
            if let Some(toc_item) =
                self.get_toc_item_by_index(&current_book_info.toc_items, self.selected_index - 1)
            {
                SelectedTocItem::TocItem(toc_item)
            } else {
                SelectedTocItem::BackToBooks
            }
        }
    }

    /// Count visible TOC items (considering expansion state)
    fn count_visible_toc_items(&self, toc_items: &[TocItem]) -> usize {
        let mut count = 0;
        for item in toc_items {
            count += 1; // Count the item itself
            match item {
                TocItem::Section {
                    children,
                    is_expanded,
                    ..
                } => {
                    if *is_expanded {
                        count += self.count_visible_toc_items(children);
                    }
                }
                TocItem::Chapter { .. } => {}
            }
        }
        count
    }

    /// Get TOC item by flat index
    fn get_toc_item_by_index<'a>(
        &self,
        toc_items: &'a [TocItem],
        target_index: usize,
    ) -> Option<&'a TocItem> {
        self.get_toc_item_by_index_helper(toc_items, target_index, &mut 0)
    }

    fn get_toc_item_by_index_helper<'a>(
        &self,
        toc_items: &'a [TocItem],
        target_index: usize,
        current_index: &mut usize,
    ) -> Option<&'a TocItem> {
        for item in toc_items {
            if *current_index == target_index {
                return Some(item);
            }
            *current_index += 1;

            match item {
                TocItem::Section {
                    children,
                    is_expanded,
                    ..
                } => {
                    if *is_expanded {
                        if let Some(child_item) =
                            self.get_toc_item_by_index_helper(children, target_index, current_index)
                        {
                            return Some(child_item);
                        }
                    }
                }
                TocItem::Chapter { .. } => {}
            }
        }

        None
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        is_focused: bool,
        palette: &Base16Palette,
        current_book_info: &CurrentBookInfo,
        book_display_name: &str,
    ) {
        // Get focus-aware colors
        let (_text_color, border_color, _bg_color) = palette.get_panel_colors(is_focused);
        let (selection_bg, selection_fg) = palette.get_selection_colors(is_focused);

        let mut items: Vec<ListItem> = Vec::new();
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "← Books List",
            Style::default().fg(palette.base_0b),
        )])));

        // Render TOC items
        if !current_book_info.toc_items.is_empty() {
            let mut toc_item_index = 0;
            self.render_toc_items(
                current_book_info,
                &mut items,
                palette,
                &current_book_info.toc_items,
                0,
                &mut toc_item_index,
                is_focused,
            );
        } else if !current_book_info.sections.is_empty() {
            self.render_hierarchical_chapters(current_book_info, &mut items, palette);
        } else {
            self.render_flat_chapters(current_book_info, &mut items, palette);
        }
        let title = format!("{} - Book", book_display_name);
        let toc_list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(palette.base_00)),
            )
            .highlight_style(Style::default().bg(selection_bg).fg(selection_fg))
            .style(Style::default().bg(palette.base_00));

        f.render_stateful_widget(toc_list, area, &mut self.list_state);
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

    /// Render TOC items using the new ADT structure
    fn render_toc_items(
        &self,
        current_book: &CurrentBookInfo,
        items: &mut Vec<ListItem>,
        palette: &Base16Palette,
        toc_items: &[TocItem],
        indent_level: usize,
        toc_item_index: &mut usize,
        is_focused: bool,
    ) {
        let (text_color, _border_color, _bg_color) = palette.get_panel_colors(is_focused);
        for item in toc_items {
            let is_selected_toc_item =
                self.selected_index > 0 && self.selected_index - 1 == *toc_item_index;

            match item {
                TocItem::Chapter { title, index, .. } => {
                    // Render a simple chapter
                    let chapter_style = if is_selected_toc_item {
                        Style::default().fg(palette.base_08).bg(palette.base_01)
                    // Highlight selected TOC item
                    } else if *index == current_book.current_chapter {
                        Style::default().fg(palette.base_08) // Highlight current chapter
                    } else {
                        Style::default().fg(text_color) // Dimmer for other chapters
                    };

                    let indent = "  ".repeat(indent_level + 1);
                    let chapter_content = Line::from(vec![Span::styled(
                        format!("{}{}", indent, title),
                        chapter_style,
                    )]);
                    items.push(ListItem::new(chapter_content));
                }
                TocItem::Section {
                    title,
                    index,
                    children,
                    is_expanded,
                    ..
                } => {
                    // Render section header with expand/collapse indicator
                    let section_icon = if *is_expanded { "▼" } else { "▶" };

                    // Determine the style based on selection and current chapter
                    let section_style = if is_selected_toc_item {
                        Style::default().fg(palette.base_0f).bg(palette.base_01)
                    // Highlight selected TOC item
                    } else if let Some(section_index) = index {
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

                    *toc_item_index += 1; // Increment for the section itself

                    // Render children if expanded
                    if *is_expanded {
                        self.render_toc_items(
                            current_book,
                            items,
                            palette,
                            children,
                            indent_level + 1,
                            toc_item_index,
                            is_focused,
                        );
                    }

                    continue; // Skip the increment at the end of the loop since we already did it
                }
            }

            *toc_item_index += 1;
        }
    }
}

pub enum SelectedTocItem<'a> {
    BackToBooks,
    TocItem(&'a TocItem),
}
