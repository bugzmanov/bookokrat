use super::CurrentBookInfo;
use crate::markdown_text_reader::ActiveSection;
use crate::search::{SearchMode, SearchState, SearchablePanel, find_matches_in_text};
use crate::theme::Base16Palette;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
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

    /// Collapse/fold this section (only applies to sections)
    pub fn collapse(&mut self) {
        if let TocItem::Section { is_expanded, .. } = self {
            *is_expanded = false;
        }
    }

    /// Expand/unfold this section (only applies to sections)
    pub fn expand(&mut self) {
        if let TocItem::Section { is_expanded, .. } = self {
            *is_expanded = true;
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
    manual_navigation_cooldown: u8,   // Grace period counter after manual navigation
    search_state: SearchState,
}

impl Default for TableOfContents {
    fn default() -> Self {
        Self::new()
    }
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
            manual_navigation_cooldown: 0,
            search_state: SearchState::new(),
        }
    }

    pub fn set_current_book_info(&mut self, book_info: CurrentBookInfo) {
        self.current_book_info = Some(book_info);
    }

    /// Update book info while preserving expansion states from existing ToC items
    pub fn update_current_book_info_preserve_state(&mut self, mut new_book_info: CurrentBookInfo) {
        // If we have existing book info with the same ToC structure, preserve expansion states
        if let Some(ref current_info) = self.current_book_info {
            Self::copy_expansion_states(&current_info.toc_items, &mut new_book_info.toc_items);
        }
        self.current_book_info = Some(new_book_info);
    }

    /// Update only navigation-related fields without touching ToC structure
    pub fn update_navigation_info(
        &mut self,
        chapter: usize,
        chapter_href: Option<String>,
        active_section: ActiveSection,
    ) {
        if let Some(ref mut info) = self.current_book_info {
            info.current_chapter = chapter;
            info.current_chapter_href = chapter_href;
            info.active_section = active_section;
        }
    }

    pub fn set_active_from_hint(
        &mut self,
        chapter_href: &str,
        anchor: Option<&str>,
        viewport_height: Option<usize>,
    ) {
        let current_chapter = if let Some(ref info) = self.current_book_info {
            info.current_chapter
        } else {
            return;
        };

        let mut active = ActiveSection::new(
            current_chapter,
            chapter_href.to_string(),
            anchor.map(|a| a.to_string()),
        );

        if let Some(ref mut info) = self.current_book_info {
            info.current_chapter_href = Some(active.chapter_href.clone());
            info.active_section = active.clone();
        }

        let mut active_index = if let Some(ref info) = self.current_book_info {
            self.find_active_item_index(&info.toc_items, &active)
        } else {
            None
        };

        if active_index.is_none() && active.anchor.is_some() {
            active.anchor = None;
            if let Some(ref mut info) = self.current_book_info {
                info.active_section = active.clone();
            }
            active_index = if let Some(ref info) = self.current_book_info {
                self.find_active_item_index(&info.toc_items, &active)
            } else {
                None
            };
        }

        if let Some(idx) = active_index {
            let index_with_header = idx + 1;
            self.active_item_index = Some(index_with_header);
            if let Some(height) = viewport_height.or(Some(self.last_viewport_height)) {
                if height > 0 {
                    self.ensure_item_visible(index_with_header, height);
                }
            }
        }
    }

    pub fn anchors_for_chapter(&self, chapter_href: Option<&str>) -> Vec<String> {
        if let Some(ref info) = self.current_book_info {
            Self::anchors_for_items(&info.toc_items, chapter_href)
        } else {
            Vec::new()
        }
    }

    pub fn anchors_for_items(items: &[TocItem], chapter_href: Option<&str>) -> Vec<String> {
        let mut anchors = Vec::new();
        if let Some(href) = chapter_href {
            let target_base = ActiveSection::base_href(href);
            Self::collect_anchors_for_href(items, &target_base, &mut anchors);
        }
        anchors
    }

    fn collect_anchors_for_href(items: &[TocItem], target_base: &str, output: &mut Vec<String>) {
        for item in items {
            if let Some(item_href) = item.href() {
                let item_base = ActiveSection::base_href(item_href);
                if item_base == target_base {
                    if let Some(anchor) = item.anchor() {
                        let normalized = ActiveSection::normalize_anchor(anchor);
                        if !output.contains(&normalized) {
                            output.push(normalized);
                        }
                    }
                }
            }

            if let TocItem::Section { children, .. } = item {
                Self::collect_anchors_for_href(children, target_base, output);
            }
        }
    }

    /// Recursively copy expansion states from old items to new items based on matching titles/hrefs
    fn copy_expansion_states(old_items: &[TocItem], new_items: &mut [TocItem]) {
        for new_item in new_items.iter_mut() {
            // Find matching old item by title and href
            if let Some(old_item) = old_items
                .iter()
                .find(|old| old.title() == new_item.title() && old.href() == new_item.href())
            {
                if let (
                    TocItem::Section {
                        is_expanded: old_expanded,
                        children: old_children,
                        ..
                    },
                    TocItem::Section {
                        is_expanded: new_expanded,
                        children: new_children,
                        ..
                    },
                ) = (old_item, new_item)
                {
                    *new_expanded = *old_expanded;
                    // Recursively copy expansion states for children
                    Self::copy_expansion_states(old_children, new_children);
                }
            }
        }
    }

    /// Update the active section and ensure it's visible in the viewport
    /// This is called when the active section changes due to scrolling in the reading area
    pub fn update_active_section(
        &mut self,
        active_section: &ActiveSection,
        viewport_height: usize,
    ) {
        self.last_viewport_height = viewport_height;

        if self.manual_navigation_cooldown > 0 {
            self.manual_navigation_cooldown = self.manual_navigation_cooldown.saturating_sub(1);
            self.manual_navigation = self.manual_navigation_cooldown > 0;
        }

        if let Some(ref book_info) = self.current_book_info {
            if let Some(active_index) =
                self.find_active_item_index(&book_info.toc_items, active_section)
            {
                let active_index_with_header = active_index + 1;
                self.active_item_index = Some(active_index_with_header);

                if !self.manual_navigation && self.manual_navigation_cooldown == 0 {
                    self.ensure_item_visible(active_index_with_header, viewport_height);
                }
            }
        }
    }

    /// Ensure a specific item is visible in the viewport
    fn ensure_item_visible(&mut self, target_index: usize, viewport_height: usize) {
        let current_offset = self.list_state.offset();

        let visible_start = current_offset;
        let visible_end = current_offset + viewport_height.saturating_sub(3); // Account for borders

        if target_index < visible_start {
            *self.list_state.offset_mut() = target_index;
        } else if target_index >= visible_end {
            let new_offset = target_index.saturating_sub(viewport_height.saturating_sub(4));
            *self.list_state.offset_mut() = new_offset;
        }
    }

    /// Find the index of the active item in the flattened TOC list
    fn find_active_item_index(
        &self,
        items: &[TocItem],
        active_section: &ActiveSection,
    ) -> Option<usize> {
        let mut current_index = 0;

        for item in items {
            if self.is_item_active(item, active_section) {
                return Some(current_index);
            }

            current_index += 1;

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
                    current_index += Self::count_visible_toc_items(children);
                }
            }
        }

        None
    }

    pub fn move_selection_down(&mut self) {
        self.manual_navigation = true; // User is manually navigating
        self.manual_navigation_cooldown = 5; // Set grace period
        if let Some(ref current_book_info) = self.current_book_info {
            let total_items = Self::count_visible_toc_items(&current_book_info.toc_items);
            // Add 1 for the "<< books list" item
            if self.selected_index < total_items {
                self.selected_index += 1;
                self.list_state.select(Some(self.selected_index));
                // Clear current match when manually navigating so next 'n' finds from new position
                if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode
                {
                    self.search_state.current_match_index = None;
                }
            }
        }
    }

    pub fn move_selection_up(&mut self) {
        self.manual_navigation = true; // User is manually navigating
        self.manual_navigation_cooldown = 5; // Set grace period
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.list_state.select(Some(self.selected_index));
            // Clear current match when manually navigating so next 'n' finds from new position
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    /// Scroll the view down while keeping cursor at same screen position if possible
    pub fn scroll_down(&mut self, area_height: u16) {
        self.manual_navigation = true;
        self.manual_navigation_cooldown = 5; // Set grace period
        if let Some(ref current_book_info) = self.current_book_info {
            let visible_height = area_height.saturating_sub(2) as usize; // Account for borders
            let total_items = Self::count_visible_toc_items(&current_book_info.toc_items) + 1; // +1 for "<< books list"
            let current_offset = self.list_state.offset();

            let cursor_viewport_pos = self.selected_index.saturating_sub(current_offset);

            if current_offset + visible_height < total_items {
                let new_offset = current_offset + 1;

                let new_selected = (new_offset + cursor_viewport_pos).min(total_items - 1);

                self.selected_index = new_selected;
                self.list_state.select(Some(self.selected_index));

                self.list_state = ListState::default()
                    .with_selected(Some(self.selected_index))
                    .with_offset(new_offset);
            } else if self.selected_index < total_items - 1 {
                self.selected_index += 1;
                self.list_state.select(Some(self.selected_index));
            }
        }
    }

    /// Scroll the view up while keeping cursor at same screen position if possible
    pub fn scroll_up(&mut self, _area_height: u16) {
        self.manual_navigation = true;
        self.manual_navigation_cooldown = 5; // Set grace period
        let current_offset = self.list_state.offset();
        let cursor_viewport_pos = self.selected_index.saturating_sub(current_offset);

        if current_offset > 0 {
            let new_offset = current_offset - 1;

            let new_selected = new_offset + cursor_viewport_pos;

            self.selected_index = new_selected;
            self.list_state.select(Some(self.selected_index));
            self.list_state = ListState::default()
                .with_selected(Some(self.selected_index))
                .with_offset(new_offset);
        } else if self.selected_index > 0 {
            self.selected_index -= 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    /// Clear the manual navigation flag when focus returns to content
    pub fn clear_manual_navigation(&mut self) {
        // Don't clear if cooldown is active - let update_active_section handle it
        if self.manual_navigation_cooldown == 0 {
            self.manual_navigation = false;
        }
    }

    /// Get the selected item (either back button or TOC item)
    pub fn get_selected_item(&self) -> Option<SelectedTocItem> {
        if let Some(ref current_book_info) = self.current_book_info {
            if self.selected_index == 0 {
                Some(SelectedTocItem::BackToBooks)
            } else {
                // Subtract 1 to account for the back button
                if let Some(toc_item) = Self::get_toc_item_by_index(
                    &current_book_info.toc_items,
                    self.selected_index - 1,
                ) {
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
                // Set cooldown to prevent viewport jumping
                self.manual_navigation = true;
                self.manual_navigation_cooldown = 5;
            }
        }
    }

    /// Collapse/fold the currently selected item if it's an expanded section
    pub fn collapse_selected(&mut self) {
        if let Some(ref mut current_book_info) = self.current_book_info {
            if self.selected_index > 0 {
                // Subtract 1 to account for the back button
                let target_index = self.selected_index - 1;
                Self::set_expansion_at_index(
                    &mut current_book_info.toc_items,
                    target_index,
                    &mut 0,
                    false,
                );
                // Set cooldown to prevent viewport jumping
                self.manual_navigation = true;
                self.manual_navigation_cooldown = 5;
            }
        }
    }

    /// Expand/unfold the currently selected item if it's a collapsed section
    pub fn expand_selected(&mut self) {
        if let Some(ref mut current_book_info) = self.current_book_info {
            if self.selected_index > 0 {
                // Subtract 1 to account for the back button
                let target_index = self.selected_index - 1;
                Self::set_expansion_at_index(
                    &mut current_book_info.toc_items,
                    target_index,
                    &mut 0,
                    true,
                );
                // Set cooldown to prevent viewport jumping
                self.manual_navigation = true;
                self.manual_navigation_cooldown = 5;
            }
        }
    }

    /// Collapse/fold all sections in the table of contents
    pub fn collapse_all(&mut self) {
        if let Some(ref mut current_book_info) = self.current_book_info {
            Self::set_all_expansion_state(&mut current_book_info.toc_items, false);
            // Set cooldown to prevent viewport jumping
            self.manual_navigation = true;
            self.manual_navigation_cooldown = 5;
        }
    }

    /// Expand/unfold all sections in the table of contents
    pub fn expand_all(&mut self) {
        if let Some(ref mut current_book_info) = self.current_book_info {
            Self::set_all_expansion_state(&mut current_book_info.toc_items, true);
            // Set cooldown to prevent viewport jumping
            self.manual_navigation = true;
            self.manual_navigation_cooldown = 5;
        }
    }

    /// Helper to set expansion state for all sections
    fn set_all_expansion_state(toc_items: &mut [TocItem], expand: bool) {
        for item in toc_items {
            match item {
                TocItem::Section {
                    is_expanded,
                    children,
                    ..
                } => {
                    *is_expanded = expand;
                    // Recursively set expansion state for child sections
                    Self::set_all_expansion_state(children, expand);
                }
                TocItem::Chapter { .. } => {}
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
                    if *is_expanded
                        && Self::toggle_expansion_at_index(children, target_index, current_index)
                    {
                        return true;
                    }
                }
                TocItem::Chapter { .. } => {}
            }
        }
        false
    }

    /// Helper to find and set expansion state at a specific index
    fn set_expansion_at_index(
        toc_items: &mut [TocItem],
        target_index: usize,
        current_index: &mut usize,
        expand: bool,
    ) -> bool {
        for item in toc_items {
            if *current_index == target_index {
                if expand {
                    item.expand();
                } else {
                    item.collapse();
                }
                return true;
            }
            *current_index += 1;

            match item {
                TocItem::Section {
                    children,
                    is_expanded,
                    ..
                } => {
                    if *is_expanded
                        && Self::set_expansion_at_index(
                            children,
                            target_index,
                            current_index,
                            expand,
                        )
                    {
                        return true;
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
            Self::count_visible_toc_items(&current_book_info.toc_items) + 1
        } else {
            1 // Just the back button
        }
    }

    /// Handle mouse click at the given position
    /// Returns true if an item was clicked
    pub fn handle_mouse_click(&mut self, x: u16, y: u16, area: Rect) -> bool {
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
                // Check if this is a click on an expand/collapse arrow
                if new_index > 0 {
                    // Skip the "Books List" item at index 0
                    let toc_index = new_index - 1;
                    if let Some(ref current_book_info) = self.current_book_info {
                        if let Some((item, indent_level)) =
                            Self::get_toc_item_with_indent(&current_book_info.toc_items, toc_index)
                        {
                            // Check if this is a section with an arrow
                            if matches!(item, TocItem::Section { .. }) {
                                // Calculate arrow position: border + indent spaces + 1 for arrow
                                // Each indent level adds 2 spaces
                                let arrow_x = area.x + 1 + ((indent_level + 1) * 2) as u16;

                                // Check if click is on or near the arrow (±1 position for error margin)
                                if x >= arrow_x.saturating_sub(1) && x <= arrow_x + 1 {
                                    // Toggle expansion instead of selecting
                                    Self::toggle_expansion_at_index(
                                        &mut self.current_book_info.as_mut().unwrap().toc_items,
                                        toc_index,
                                        &mut 0,
                                    );
                                    // Set cooldown to prevent viewport jumping
                                    self.manual_navigation = true;
                                    self.manual_navigation_cooldown = 5;
                                    return true;
                                }
                            }
                        }
                    }
                }

                // Not an arrow click, select the item normally
                self.selected_index = new_index;
                self.list_state.select(Some(new_index));
                self.manual_navigation = true;
                self.manual_navigation_cooldown = 5; // Set grace period
                return true;
            }
        }
        false
    }

    /// Count visible TOC items (considering expansion state)
    fn count_visible_toc_items(toc_items: &[TocItem]) -> usize {
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
                        count += Self::count_visible_toc_items(children);
                    }
                }
                TocItem::Chapter { .. } => {}
            }
        }
        count
    }

    /// Get TOC item by flat index with its indent level
    fn get_toc_item_with_indent(
        toc_items: &[TocItem],
        target_index: usize,
    ) -> Option<(&TocItem, usize)> {
        Self::get_toc_item_with_indent_helper(toc_items, target_index, &mut 0, 0)
    }

    fn get_toc_item_with_indent_helper<'a>(
        toc_items: &'a [TocItem],
        target_index: usize,
        current_index: &mut usize,
        indent_level: usize,
    ) -> Option<(&'a TocItem, usize)> {
        for item in toc_items {
            if *current_index == target_index {
                return Some((item, indent_level));
            }
            *current_index += 1;

            match item {
                TocItem::Section {
                    children,
                    is_expanded,
                    ..
                } => {
                    if *is_expanded {
                        if let Some(result) = Self::get_toc_item_with_indent_helper(
                            children,
                            target_index,
                            current_index,
                            indent_level + 1,
                        ) {
                            return Some(result);
                        }
                    }
                }
                TocItem::Chapter { .. } => {}
            }
        }

        None
    }

    /// Get TOC item by flat index
    fn get_toc_item_by_index(toc_items: &[TocItem], target_index: usize) -> Option<&TocItem> {
        Self::get_toc_item_by_index_helper(toc_items, target_index, &mut 0)
    }

    fn get_toc_item_by_index_helper<'a>(
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
                        if let Some(child_item) = Self::get_toc_item_by_index_helper(
                            children,
                            target_index,
                            current_index,
                        ) {
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

        // Add the back button - check if it matches search
        let back_text = "← Books List";
        let back_line = if self.search_state.active && self.search_state.is_match(0) {
            self.create_highlighted_line(back_text, 0, palette.base_0b, palette)
        } else {
            Line::from(vec![Span::styled(
                back_text,
                Style::default().fg(palette.base_0b),
            )])
        };
        items.push(ListItem::new(back_line));

        // Render TOC items
        let mut toc_item_index = 1; // Start at 1 because 0 is the back button
        self.render_toc_items(
            current_book_info,
            &mut items,
            palette,
            &current_book_info.toc_items,
            0,
            &mut toc_item_index,
            is_focused,
        );
        let title = format!("{book_display_name} - Book");
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
    #[allow(clippy::too_many_arguments)]
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
                    let should_highlight =
                        self.should_highlight_item(item, &current_book.active_section);
                    let base_color = if should_highlight {
                        palette.base_08
                    } else {
                        text_color // Dimmer for other chapters
                    };

                    let indent = "  ".repeat(indent_level + 1);
                    let full_text = format!("{indent}{title}");

                    // Check if this item matches search
                    let chapter_content = if self.search_state.active
                        && self.search_state.is_match(*toc_item_index)
                    {
                        self.create_highlighted_line_with_indent(
                            &full_text,
                            *toc_item_index,
                            base_color,
                            palette,
                            indent.len(),
                        )
                    } else {
                        Line::from(vec![Span::styled(
                            full_text,
                            Style::default().fg(base_color),
                        )])
                    };
                    items.push(ListItem::new(chapter_content));
                }
                TocItem::Section {
                    title,

                    children,
                    is_expanded,
                    ..
                } => {
                    let section_icon = if *is_expanded { "⌄" } else { "›" };

                    let should_highlight =
                        self.should_highlight_item(item, &current_book.active_section);
                    let base_color = if should_highlight {
                        palette.base_08
                    } else {
                        palette.base_0d // Blue for sections
                    };

                    let indent = "  ".repeat(indent_level + 1);
                    let full_text = format!("{indent}{section_icon} {title}");

                    // Check if this item matches search
                    let section_content = if self.search_state.active
                        && self.search_state.is_match(*toc_item_index)
                    {
                        self.create_highlighted_line_with_indent(
                            &full_text,
                            *toc_item_index,
                            base_color,
                            palette,
                            indent.len(),
                        )
                    } else {
                        Line::from(vec![Span::styled(
                            full_text,
                            Style::default().fg(base_color),
                        )])
                    };
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

    /// Check if this item or any of its collapsed descendants contains the active section
    /// This ensures that collapsed sections containing the active item get highlighted
    fn should_highlight_item(&self, item: &TocItem, active_section: &ActiveSection) -> bool {
        // First check if this exact item is active
        if self.is_item_active(item, active_section) {
            return true;
        }

        // If this is a collapsed section, check if any descendant would be active
        if let TocItem::Section {
            children,
            is_expanded,
            ..
        } = item
        {
            if !is_expanded {
                // Section is collapsed - check if active item is inside
                return self.contains_active_item(children, active_section);
            }
        }

        false
    }

    /// Recursively check if any item in the tree contains the active section
    fn contains_active_item(&self, items: &[TocItem], active_section: &ActiveSection) -> bool {
        for item in items {
            // Check if this item is active
            if self.is_item_active(item, active_section) {
                return true;
            }

            // Recursively check children
            if let TocItem::Section { children, .. } = item {
                if self.contains_active_item(children, active_section) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a TOC item is active based on the current active section
    fn is_item_active(&self, item: &TocItem, active_section: &ActiveSection) -> bool {
        if let Some(item_href) = item.href() {
            let item_base_href = item_href.split('#').next().unwrap_or(item_href);

            if item_base_href == active_section.chapter_base_href {
                return match (&active_section.anchor, item.anchor()) {
                    (Some(active_anchor), Some(item_anchor)) => {
                        ActiveSection::normalize_anchor(item_anchor) == *active_anchor
                    }
                    (Some(_), None) => true,
                    (None, Some(_)) => false,
                    (None, None) => true,
                };
            }
        }

        false
    }

    /// Create a line with search highlighting
    fn create_highlighted_line(
        &self,
        text: &str,
        index: usize,
        base_color: Color,
        palette: &Base16Palette,
    ) -> Line<'static> {
        self.create_highlighted_line_with_indent(text, index, base_color, palette, 0)
    }

    /// Create a line with search highlighting, accounting for indent
    fn create_highlighted_line_with_indent(
        &self,
        text: &str,
        index: usize,
        base_color: Color,
        _palette: &Base16Palette,
        _indent_len: usize,
    ) -> Line<'static> {
        let empty_vec = vec![];
        let highlight_ranges = self
            .search_state
            .matches
            .iter()
            .find(|m| m.index == index)
            .map(|m| &m.highlight_ranges)
            .unwrap_or(&empty_vec);

        let mut spans = Vec::new();
        let mut last_end = 0;

        let is_current_match = self.search_state.is_current_match(index);

        for (start, end) in highlight_ranges {
            // The highlight ranges are already calculated for the full text including indent
            // No need to adjust them

            // Add non-highlighted text before this match
            if *start > last_end {
                spans.push(Span::styled(
                    text[last_end..*start].to_string(),
                    Style::default().fg(base_color),
                ));
            }

            // Add highlighted match text
            let highlight_style = if is_current_match {
                // Current match: bright yellow background with black text
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                // Other matches: dim yellow background
                Style::default().bg(Color::Rgb(100, 100, 0)).fg(base_color)
            };

            spans.push(Span::styled(
                text[*start..*end].to_string(),
                highlight_style,
            ));

            last_end = *end;
        }

        // Add remaining non-highlighted text
        if last_end < text.len() {
            spans.push(Span::styled(
                text[last_end..].to_string(),
                Style::default().fg(base_color),
            ));
        }

        Line::from(spans)
    }

    /// Helper method to collect all visible TOC items with their display text
    fn collect_visible_items(&self) -> Vec<String> {
        let mut items = Vec::new();

        // Add the back button
        items.push("← Books List".to_string());

        // Add TOC items
        if let Some(ref book_info) = self.current_book_info {
            Self::collect_toc_items_text(&book_info.toc_items, &mut items, 0);
        }

        items
    }

    /// Recursively collect text from TOC items
    fn collect_toc_items_text(toc_items: &[TocItem], items: &mut Vec<String>, indent_level: usize) {
        for item in toc_items {
            match item {
                TocItem::Chapter { title, .. } => {
                    let indent = "  ".repeat(indent_level + 1);
                    items.push(format!("{indent}{title}"));
                }
                TocItem::Section {
                    title,
                    children,
                    is_expanded,
                    ..
                } => {
                    let section_icon = if *is_expanded { "⌄" } else { "›" };
                    let indent = "  ".repeat(indent_level + 1);
                    items.push(format!("{indent}{section_icon} {title}"));

                    // Only collect children if expanded
                    if *is_expanded {
                        Self::collect_toc_items_text(children, items, indent_level + 1);
                    }
                }
            }
        }
    }

    /// Helper to set selection to a specific index
    fn set_selection_to_index(&mut self, index: usize) {
        self.selected_index = index;
        self.list_state.select(Some(index));
        self.manual_navigation = true; // Mark as manual navigation
        self.manual_navigation_cooldown = 5; // Set grace period
    }
}

impl SearchablePanel for TableOfContents {
    fn start_search(&mut self) {
        self.search_state.start_search(self.selected_index);
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

        // Find matches in visible TOC items
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
            let current_position = self.selected_index;

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
            let current_position = self.selected_index;

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
        if match_index < self.get_total_items() {
            self.set_selection_to_index(match_index);
        }
    }

    fn get_searchable_content(&self) -> Vec<String> {
        self.collect_visible_items()
    }
}

pub enum SelectedTocItem<'a> {
    BackToBooks,
    TocItem(&'a TocItem),
}
