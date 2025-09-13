use crate::markdown_text_reader::ActiveSection;
use crate::navigation_panel::CurrentBookInfo;
use crate::theme::Base16Palette;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

/// New ADT-based model for TOC items
#[derive(Clone, Debug)]
pub enum TocItem {
    /// A leaf chapter that can be read
    Chapter {
        title: String,
        href: String,
        anchor: Option<String>, // Optional anchor/fragment within the chapter
    },
    /// A section that may have its own content and contains child items
    Section {
        title: String,
        href: Option<String>, // Some sections are readable, others are just containers
        anchor: Option<String>, // Optional anchor/fragment within the chapter
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

    /// Get the href for this item
    pub fn href(&self) -> Option<&str> {
        match self {
            TocItem::Chapter { href, .. } => Some(href),
            TocItem::Section { href, .. } => href.as_deref(),
        }
    }

    /// Get the anchor/fragment for this item
    pub fn anchor(&self) -> Option<&String> {
        match self {
            TocItem::Chapter { anchor, .. } => anchor.as_ref(),
            TocItem::Section { anchor, .. } => anchor.as_ref(),
        }
    }

    /// Toggle expansion state (only applies to sections)
    pub fn toggle_expansion(&mut self) {
        if let TocItem::Section { is_expanded, .. } = self {
            *is_expanded = !*is_expanded;
        }
    }
}

pub struct TableOfContents {
    pub selected_index: usize,
    pub list_state: ListState,
    current_book_info: Option<CurrentBookInfo>,
    active_item_index: Option<usize>, // Track the index of the currently reading item
    last_viewport_height: usize,      // Track viewport height for scroll calculations
    manual_navigation: bool,          // True when user is manually navigating TOC
}

impl TableOfContents {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            selected_index: 0,
            list_state,
            current_book_info: None,
            active_item_index: None,
            last_viewport_height: 0,
            manual_navigation: false,
        }
    }

    pub fn set_current_book_info(&mut self, book_info: CurrentBookInfo) {
        self.current_book_info = Some(book_info);
    }

    /// Update the active section and ensure it's visible in the viewport
    /// This is called when the active section changes due to scrolling in the reading area
    pub fn update_active_section(
        &mut self,
        active_section: &ActiveSection,
        viewport_height: usize,
    ) {
        self.last_viewport_height = viewport_height;

        if let Some(ref book_info) = self.current_book_info {
            // Find the index of the active item in the flattened list
            if let Some(active_index) =
                self.find_active_item_index(&book_info.toc_items, active_section)
            {
                // Add 1 to account for the "Books List" item at the top
                let active_index_with_header = active_index + 1;
                self.active_item_index = Some(active_index_with_header);

                // Only auto-scroll if user is not manually navigating
                if !self.manual_navigation {
                    // Ensure the active item is visible in the viewport
                    self.ensure_item_visible(active_index_with_header, viewport_height);
                }
            }
        }
    }

    /// Ensure a specific item is visible in the viewport
    fn ensure_item_visible(&mut self, target_index: usize, viewport_height: usize) {
        // Get the current offset from the list state
        let current_offset = self.list_state.offset();

        // Calculate visible range
        let visible_start = current_offset;
        let visible_end = current_offset + viewport_height.saturating_sub(3); // Account for borders

        // Check if target is outside visible range
        if target_index < visible_start {
            // Scroll up to show the item at the top
            *self.list_state.offset_mut() = target_index;
        } else if target_index >= visible_end {
            // Scroll down to show the item at the bottom
            let new_offset = target_index.saturating_sub(viewport_height.saturating_sub(4));
            *self.list_state.offset_mut() = new_offset;
        }
        // If item is already visible, don't change the offset
    }

    /// Find the index of the active item in the flattened TOC list
    fn find_active_item_index(
        &self,
        items: &[TocItem],
        active_section: &ActiveSection,
    ) -> Option<usize> {
        let mut current_index = 0;

        for item in items {
            // Check if this item matches the active section
            if self.is_item_active(item, active_section) {
                return Some(current_index);
            }

            current_index += 1;

            // If it's an expanded section, count its children
            if let TocItem::Section {
                children,
                is_expanded,
                ..
            } = item
            {
                if *is_expanded {
                    if let Some(child_index) = self.find_active_item_index(children, active_section)
                    {
                        return Some(current_index + child_index);
                    }
                    current_index += self.count_visible_toc_items(children);
                }
            }
        }

        None
    }

    pub fn move_selection_down(&mut self) {
        self.manual_navigation = true; // User is manually navigating
        if let Some(ref current_book_info) = self.current_book_info {
            let total_items = self.count_visible_toc_items(&current_book_info.toc_items);
            // Add 1 for the "<< books list" item
            if self.selected_index < total_items {
                self.selected_index += 1;
                self.list_state.select(Some(self.selected_index));
            }
        }
    }

    pub fn move_selection_up(&mut self) {
        self.manual_navigation = true; // User is manually navigating
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    /// Clear the manual navigation flag when focus returns to content
    pub fn clear_manual_navigation(&mut self) {
        self.manual_navigation = false;
    }

    /// Get the selected item (either back button or TOC item)
    pub fn get_selected_item(&self) -> Option<SelectedTocItem> {
        if let Some(ref current_book_info) = self.current_book_info {
            if self.selected_index == 0 {
                Some(SelectedTocItem::BackToBooks)
            } else {
                // Subtract 1 to account for the back button
                if let Some(toc_item) = self
                    .get_toc_item_by_index(&current_book_info.toc_items, self.selected_index - 1)
                {
                    Some(SelectedTocItem::TocItem(toc_item))
                } else {
                    Some(SelectedTocItem::BackToBooks)
                }
            }
        } else {
            None
        }
    }

    /// Toggle expansion state of the currently selected item if it's a section
    pub fn toggle_selected_expansion(&mut self) {
        if let Some(ref mut current_book_info) = self.current_book_info {
            if self.selected_index > 0 {
                // Subtract 1 to account for the back button
                let target_index = self.selected_index - 1;
                Self::toggle_expansion_at_index(
                    &mut current_book_info.toc_items,
                    target_index,
                    &mut 0,
                );
            }
        }
    }

    /// Helper to find and toggle expansion at a specific index
    fn toggle_expansion_at_index(
        toc_items: &mut [TocItem],
        target_index: usize,
        current_index: &mut usize,
    ) -> bool {
        for item in toc_items {
            if *current_index == target_index {
                item.toggle_expansion();
                return true;
            }
            *current_index += 1;

            match item {
                TocItem::Section {
                    children,
                    is_expanded,
                    ..
                } => {
                    if *is_expanded {
                        if Self::toggle_expansion_at_index(children, target_index, current_index) {
                            return true;
                        }
                    }
                }
                TocItem::Chapter { .. } => {}
            }
        }
        false
    }

    /// Get the total number of visible items in the table of contents (including the back button)
    pub fn get_total_items(&self) -> usize {
        if let Some(ref current_book_info) = self.current_book_info {
            // Add 1 for the "<< books list" item
            self.count_visible_toc_items(&current_book_info.toc_items) + 1
        } else {
            1 // Just the back button
        }
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
            let total_items = self.get_total_items();
            if new_index < total_items {
                self.selected_index = new_index;
                self.list_state.select(Some(new_index));
                return true;
            }
        }
        false
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

    /// Get the current book info for filename searches
    pub fn get_current_book_info(&self) -> Option<&CurrentBookInfo> {
        self.current_book_info.as_ref()
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        is_focused: bool,
        palette: &Base16Palette,
        book_display_name: &str,
    ) {
        // Store viewport height for scroll calculations
        self.last_viewport_height = area.height as usize;
        let Some(ref current_book_info) = self.current_book_info else {
            return;
        };
        // Get focus-aware colors
        let (_text_color, border_color, _bg_color) = palette.get_panel_colors(is_focused);
        let (selection_bg, selection_fg) = palette.get_selection_colors(is_focused);

        let mut items: Vec<ListItem> = Vec::new();
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "← Books List",
            Style::default().fg(palette.base_0b),
        )])));

        // Render TOC items
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
        let title = format!("{} - Book", book_display_name);
        let mut toc_list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(palette.base_00)),
            )
            .style(Style::default().bg(palette.base_00));

        if is_focused {
            toc_list = toc_list.highlight_style(Style::default().bg(selection_bg).fg(selection_fg))
        }

        f.render_stateful_widget(toc_list, area, &mut self.list_state);
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
            match item {
                TocItem::Chapter { title, .. } => {
                    // Render a simple chapter
                    let is_active = self.is_item_active(item, &current_book.active_section);
                    let chapter_style = if is_active {
                        Style::default().fg(palette.base_08)
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

                    children,
                    is_expanded,
                    ..
                } => {
                    let section_icon = if *is_expanded { "▼" } else { "▶" };

                    let is_active = self.is_item_active(item, &current_book.active_section);
                    let section_style = if is_active {
                        Style::default().fg(palette.base_08)
                    } else {
                        Style::default().fg(palette.base_0d) // Blue for sections
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

    /// Check if a TOC item is active based on the current active section
    fn is_item_active(&self, item: &TocItem, active_section: &ActiveSection) -> bool {
        match active_section {
            ActiveSection::Anchor(active_anchor) => {
                // Check if this item's anchor matches the active anchor
                if let Some(item_anchor) = item.anchor() {
                    item_anchor == active_anchor
                } else {
                    false
                }
            }
            ActiveSection::Chapter(_chapter_idx) => {
                // Compare by href - check if this item's href matches the current chapter's href
                if let Some(current_book) = &self.current_book_info {
                    if let Some(current_href) = &current_book.current_chapter_href {
                        if let Some(item_href) = item.href() {
                            // Normalize both hrefs for comparison
                            let current_normalized =
                                current_href.split('#').next().unwrap_or(current_href);
                            let item_normalized = item_href.split('#').next().unwrap_or(item_href);

                            // Check if they match
                            current_normalized == item_normalized
                                || current_normalized.ends_with(item_normalized)
                                || item_normalized.ends_with(current_normalized)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }
}

pub enum SelectedTocItem<'a> {
    BackToBooks,
    TocItem(&'a TocItem),
}
