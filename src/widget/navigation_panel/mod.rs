pub mod book_list;
pub mod table_of_contents;

pub use book_list::BookList;
pub use table_of_contents::{SelectedTocItem, TableOfContents, TocItem};

use crate::book_manager::BookManager;
use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::markdown_text_reader::ActiveSection;
use crate::search::{SearchMode, SearchState, SearchablePanel};
use crate::theme::Base16Palette;
use ratatui::{Frame, layout::Rect};

pub enum NavigationPanelAction {
    SelectBook {
        book_index: usize,
    },
    NavigateToChapter {
        href: String,
        anchor: Option<String>,
    },
    ToggleSection,
    SwitchToBookList,
    Bypass, // when the component assumes the upper layer should handle the action
}

pub enum SelectedActionOwned {
    None,
    BookIndex(usize),
    BackToBooks,
    TocItem(TocItem),
}

#[derive(Clone)]
pub struct CurrentBookInfo {
    pub path: String,
    pub toc_items: Vec<TocItem>,
    pub current_chapter: usize,
    pub current_chapter_href: Option<String>, // The href of the current chapter
    pub active_section: ActiveSection,
}

#[derive(Clone, PartialEq, Debug)]
pub enum NavigationMode {
    BookSelection,
    TableOfContents,
}

pub struct NavigationPanel {
    pub mode: NavigationMode,
    pub book_list: BookList,
    pub table_of_contents: TableOfContents,
    pub current_book_index: Option<usize>,
}

impl NavigationPanel {
    pub fn new(book_manager: &BookManager) -> Self {
        Self {
            mode: NavigationMode::BookSelection,
            book_list: BookList::new(book_manager),
            table_of_contents: TableOfContents::new(),
            current_book_index: None,
        }
    }

    pub fn move_selection_down(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list.move_selection_down();
            }
            NavigationMode::TableOfContents => {
                self.table_of_contents.move_selection_down();
            }
        }
    }

    pub fn move_selection_up(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list.move_selection_up();
            }
            NavigationMode::TableOfContents => {
                self.table_of_contents.move_selection_up();
            }
        }
    }

    /// Scroll the view down (for mouse scroll) while keeping cursor position stable
    pub fn scroll_down(&mut self, area_height: u16) {
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list.scroll_down(area_height);
            }
            NavigationMode::TableOfContents => {
                self.table_of_contents.scroll_down(area_height);
            }
        }
    }

    /// Scroll the view up (for mouse scroll) while keeping cursor position stable
    pub fn scroll_up(&mut self, area_height: u16) {
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list.scroll_up(area_height);
            }
            NavigationMode::TableOfContents => {
                self.table_of_contents.scroll_up(area_height);
            }
        }
    }

    pub fn switch_to_toc_mode(&mut self, book_info: CurrentBookInfo) {
        self.mode = NavigationMode::TableOfContents;

        if self.table_of_contents.get_current_book_info().is_none() {
            self.table_of_contents = TableOfContents::new();
            self.table_of_contents.set_current_book_info(book_info);
        } else {
            self.table_of_contents
                .update_current_book_info_preserve_state(book_info);
        }
    }

    pub fn switch_to_book_mode(&mut self) {
        self.mode = NavigationMode::BookSelection;
        // Keep current_book_index so we can highlight the open book
    }

    pub fn is_in_book_mode(&self) -> bool {
        matches!(self.mode, NavigationMode::BookSelection)
    }

    pub fn get_selected_book_index(&self) -> usize {
        self.book_list.selected
    }

    /// Handle mouse click at the given position
    /// Returns true if an item was selected (for double-click handling)
    pub fn handle_mouse_click(&mut self, x: u16, y: u16, area: Rect) -> bool {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.handle_mouse_click(x, y, area),
            NavigationMode::TableOfContents => {
                self.table_of_contents.handle_mouse_click(x, y, area)
            }
        }
    }

    /// Handle uppercase H to collapse all TOC items
    pub fn handle_shift_h(&mut self) {
        if self.mode == NavigationMode::TableOfContents {
            self.table_of_contents.collapse_all();
        }
    }

    /// Handle uppercase L to expand all TOC items
    pub fn handle_shift_l(&mut self) {
        if self.mode == NavigationMode::TableOfContents {
            self.table_of_contents.expand_all();
        }
    }

    /// Get the currently selected index based on the mode
    pub fn get_selected_action(&self) -> SelectedActionOwned {
        match self.mode {
            NavigationMode::BookSelection => {
                SelectedActionOwned::BookIndex(self.book_list.selected)
            }
            NavigationMode::TableOfContents => {
                if let Some(item) = self.table_of_contents.get_selected_item() {
                    match item {
                        SelectedTocItem::BackToBooks => SelectedActionOwned::BackToBooks,
                        SelectedTocItem::TocItem(toc_item) => {
                            // Clone the TocItem to avoid lifetime issues
                            SelectedActionOwned::TocItem(toc_item.clone())
                        }
                    }
                } else {
                    SelectedActionOwned::None
                }
            }
        }
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        is_focused: bool,
        palette: &Base16Palette,
        book_manager: &BookManager,
    ) {
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list
                    .render(f, area, is_focused, palette, self.current_book_index);
            }
            NavigationMode::TableOfContents => {
                if let Some(current_idx) = self.current_book_index {
                    if let Some(book) = book_manager.get_book_info(current_idx) {
                        self.table_of_contents.render(
                            f,
                            area,
                            is_focused,
                            palette,
                            &book.display_name,
                        );
                    }
                }
            }
        }
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<NavigationPanelAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Tab => Some(NavigationPanelAction::Bypass),
            KeyCode::Char('/') => {
                self.start_search();
                None
            }
            KeyCode::Char('j') => {
                self.handle_j();
                None
            }
            KeyCode::Char('k') => {
                self.move_selection_up();
                None
            }
            KeyCode::Char('h') => {
                self.handle_h();
                None
            }
            KeyCode::Char('l') => {
                self.handle_l();
                None
            }
            KeyCode::Char('H') => {
                self.handle_shift_h();
                None
            }
            KeyCode::Char('L') => {
                self.handle_shift_l();
                None
            }
            KeyCode::Char('g') if key_seq.handle_key('g') == "gg" => {
                self.handle_gg();
                None
            }
            KeyCode::Char('G') => {
                self.handle_upper_g();
                None
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_d();
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_u();
                None
            }
            KeyCode::Char('n') if self.is_searching() => {
                let search_state = self.get_search_state();
                if search_state.mode == SearchMode::NavigationMode {
                    self.next_match();
                }
                None
            }
            KeyCode::Char('N') if self.is_searching() => {
                let search_state = self.get_search_state();
                if search_state.mode == SearchMode::NavigationMode {
                    self.previous_match();
                }
                None
            }
            KeyCode::Enter => {
                // Handle Enter key based on current mode
                match self.mode {
                    NavigationMode::BookSelection => {
                        let book_index = self.get_selected_book_index();
                        Some(NavigationPanelAction::SelectBook { book_index })
                    }
                    NavigationMode::TableOfContents => {
                        match self.table_of_contents.get_selected_item() {
                            Some(SelectedTocItem::BackToBooks) => {
                                Some(NavigationPanelAction::SwitchToBookList)
                            }
                            Some(SelectedTocItem::TocItem(toc_item)) => match toc_item {
                                TocItem::Chapter { href, anchor, .. } => {
                                    Some(NavigationPanelAction::NavigateToChapter {
                                        href: href.clone(),
                                        anchor: anchor.clone(),
                                    })
                                }
                                TocItem::Section { href, anchor, .. } => {
                                    if let Some(href_str) = href {
                                        Some(NavigationPanelAction::NavigateToChapter {
                                            href: href_str.clone(),
                                            anchor: anchor.clone(),
                                        })
                                    } else {
                                        None
                                        // // Section has no content - just toggle expansion
                                        // Some(NavigationPanelAction::ToggleSection)
                                    }
                                }
                            },
                            None => None,
                        }
                    }
                }
            }
            _ => None,
        }
    }
}

impl VimNavMotions for NavigationPanel {
    fn handle_h(&mut self) {
        // Left movement - collapse section in TOC mode, switch to book mode otherwise
        if self.mode == NavigationMode::TableOfContents {
            // Try to collapse the selected section first
            self.table_of_contents.collapse_selected();
            // Note: We don't switch to book mode here anymore - 'h' is for folding
        } else {
            // In book selection mode, h doesn't do anything special
        }
    }

    fn handle_j(&mut self) {
        // Down movement - move selection down
        self.move_selection_down();
    }

    fn handle_k(&mut self) {
        // Up movement - move selection up
        self.move_selection_up();
    }

    fn handle_l(&mut self) {
        // Right movement - expand section in TOC mode
        if self.mode == NavigationMode::TableOfContents {
            // Expand the selected section if it's collapsed
            self.table_of_contents.expand_selected();
        }
        // In book selection mode, l doesn't do anything special
    }

    fn handle_ctrl_d(&mut self) {
        // Page down - move selection down by half page
        match self.mode {
            NavigationMode::BookSelection => {
                // Move down by multiple items (e.g., 10 items or half visible page)
                for _ in 0..10 {
                    self.book_list.move_selection_down();
                }
            }
            NavigationMode::TableOfContents => {
                // Move down by multiple items in TOC
                for _ in 0..10 {
                    self.table_of_contents.move_selection_down();
                }
            }
        }
    }

    fn handle_ctrl_u(&mut self) {
        // Page up - move selection up by half page
        match self.mode {
            NavigationMode::BookSelection => {
                // Move up by multiple items (e.g., 10 items or half visible page)
                for _ in 0..10 {
                    self.book_list.move_selection_up();
                }
            }
            NavigationMode::TableOfContents => {
                // Move up by multiple items in TOC
                for _ in 0..10 {
                    self.table_of_contents.move_selection_up();
                }
            }
        }
    }

    fn handle_gg(&mut self) {
        // Go to top - move selection to first item
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list.selected = 0;
                self.book_list.list_state.select(Some(0));
            }
            NavigationMode::TableOfContents => {
                self.table_of_contents.selected_index = 0;
                self.table_of_contents.list_state.select(Some(0));
            }
        }
    }

    fn handle_upper_g(&mut self) {
        // Go to bottom - move selection to last item
        match self.mode {
            NavigationMode::BookSelection => {
                if !self.book_list.is_empty() {
                    self.book_list.selected = self.book_list.book_count() - 1;
                    self.book_list
                        .list_state
                        .select(Some(self.book_list.book_count() - 1));
                }
            }
            NavigationMode::TableOfContents => {
                // Get the total count and go to the last item
                let total_items = self.table_of_contents.get_total_items();
                if total_items > 0 {
                    let last_index = total_items - 1;
                    self.table_of_contents.selected_index = last_index;
                    self.table_of_contents.list_state.select(Some(last_index));
                }
            }
        }
    }
}

impl SearchablePanel for NavigationPanel {
    fn start_search(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.start_search(),
            NavigationMode::TableOfContents => self.table_of_contents.start_search(),
        }
    }

    fn cancel_search(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.cancel_search(),
            NavigationMode::TableOfContents => self.table_of_contents.cancel_search(),
        }
    }

    fn confirm_search(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.confirm_search(),
            NavigationMode::TableOfContents => self.table_of_contents.confirm_search(),
        }
    }

    fn exit_search(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.exit_search(),
            NavigationMode::TableOfContents => self.table_of_contents.exit_search(),
        }
    }

    fn update_search_query(&mut self, query: &str) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.update_search_query(query),
            NavigationMode::TableOfContents => self.table_of_contents.update_search_query(query),
        }
    }

    fn next_match(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.next_match(),
            NavigationMode::TableOfContents => self.table_of_contents.next_match(),
        }
    }

    fn previous_match(&mut self) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.previous_match(),
            NavigationMode::TableOfContents => self.table_of_contents.previous_match(),
        }
    }

    fn get_search_state(&self) -> &SearchState {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.get_search_state(),
            NavigationMode::TableOfContents => self.table_of_contents.get_search_state(),
        }
    }

    fn is_searching(&self) -> bool {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.is_searching(),
            NavigationMode::TableOfContents => self.table_of_contents.is_searching(),
        }
    }

    fn has_matches(&self) -> bool {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.has_matches(),
            NavigationMode::TableOfContents => self.table_of_contents.has_matches(),
        }
    }

    fn jump_to_match(&mut self, match_index: usize) {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.jump_to_match(match_index),
            NavigationMode::TableOfContents => self.table_of_contents.jump_to_match(match_index),
        }
    }

    fn get_searchable_content(&self) -> Vec<String> {
        match self.mode {
            NavigationMode::BookSelection => self.book_list.get_searchable_content(),
            NavigationMode::TableOfContents => self.table_of_contents.get_searchable_content(),
        }
    }
}
