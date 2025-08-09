use crate::book_list::BookList;
use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::main_app::VimNavMotions;
use crate::table_of_contents::{SelectedTocItem, TableOfContents, TocItem};
use crate::theme::Base16Palette;
use ratatui::{Frame, layout::Rect};

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

    pub fn switch_to_toc_mode(&mut self, book_index: usize, book_info: CurrentBookInfo) {
        self.mode = NavigationMode::TableOfContents;
        self.current_book_index = Some(book_index);
        self.table_of_contents = TableOfContents::new();
        self.table_of_contents.set_current_book_info(book_info);
    }

    pub fn switch_to_book_mode(&mut self) {
        self.mode = NavigationMode::BookSelection;
        // Keep current_book_index so we can highlight the open book
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
        bookmarks: &Bookmarks,
        book_manager: &BookManager,
    ) {
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list.render(
                    f,
                    area,
                    is_focused,
                    palette,
                    bookmarks,
                    self.current_book_index,
                );
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
}

impl VimNavMotions for NavigationPanel {
    fn handle_h(&mut self) {
        // Left movement - switch back to book selection mode
        if self.mode == NavigationMode::TableOfContents {
            self.switch_to_book_mode();
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
        // Right movement - could be used to enter/select or expand sections
        // This could be implemented to expand/collapse sections in TOC mode
        // or to enter a book in book selection mode
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

    fn handle_G(&mut self) {
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
