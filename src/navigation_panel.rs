use crate::book_list::BookList;
use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::table_of_contents::{TableOfContents, TocItem};
use crate::theme::Base16Palette;
use ratatui::{layout::Rect, Frame};

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

    pub fn move_selection_down(&mut self, current_book_info: Option<&CurrentBookInfo>) {
        match self.mode {
            NavigationMode::BookSelection => {
                self.book_list.move_selection_down();
            }
            NavigationMode::TableOfContents => {
                if let Some(book_info) = current_book_info {
                    self.table_of_contents.move_selection_down(book_info);
                }
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

    pub fn switch_to_toc_mode(&mut self, book_index: usize) {
        self.mode = NavigationMode::TableOfContents;
        self.current_book_index = Some(book_index);
        self.table_of_contents = TableOfContents::new();
    }

    pub fn switch_to_book_mode(&mut self) {
        self.mode = NavigationMode::BookSelection;
        // Keep current_book_index so we can highlight the open book
    }

    pub fn get_selected_book_index(&self) -> usize {
        self.book_list.selected
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
}
