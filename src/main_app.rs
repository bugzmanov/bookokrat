use crate::book_manager::BookManager;
use crate::book_search::{BookSearch, BookSearchAction};
use crate::book_stat::{BookStat, BookStatAction};
use crate::bookmarks::Bookmarks;
use crate::comments::BookComments;
use crate::event_source::EventSource;
use crate::images::book_images::BookImages;
use crate::images::image_popup::ImagePopup;
use crate::images::image_storage::ImageStorage;
use crate::inputs::{ClickType, KeySeq, MouseTracker, map_keys_to_input};
use crate::jump_list::{JumpList, JumpLocation};
use crate::markdown_text_reader::MarkdownTextReader;
use crate::navigation_panel::{CurrentBookInfo, NavigationPanel};
use crate::parsing::text_generator::TextGenerator;
use crate::parsing::toc_parser::TocParser;
use crate::reading_history::ReadingHistory;
use crate::search::{SearchMode, SearchablePanel};
use crate::search_engine::SearchEngine;
use crate::system_command::{RealSystemCommandExecutor, SystemCommandExecutor};
use crate::table_of_contents::TocItem;
use crate::theme::OCEANIC_NEXT;
use crate::types::LinkInfo;
use image::GenericImageView;
use log::warn;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChapterDirection {
    Next,
    Previous,
}

use std::io::BufReader;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use epub::doc::EpubDoc;
use log::{debug, error, info};
use ratatui::{
    Terminal,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

struct EpubBook {
    file: String,
    epub: EpubDoc<BufReader<std::fs::File>>,
}
impl EpubBook {
    fn new(file: String, doc: EpubDoc<BufReader<std::fs::File>>) -> Self {
        Self { file, epub: doc }
    }

    fn total_chapters(&self) -> usize {
        self.epub.get_num_pages()
    }

    fn current_chapter(&self) -> usize {
        self.epub.get_current_page()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Quit,
}

pub struct App {
    pub book_manager: BookManager,
    pub navigation_panel: NavigationPanel,
    text_reader: MarkdownTextReader,
    bookmarks: Bookmarks,
    book_images: BookImages,
    current_book: Option<EpubBook>,
    pub focused_panel: FocusedPanel,
    pub system_command_executor: Box<dyn SystemCommandExecutor>,
    last_bookmark_save: std::time::Instant,
    mouse_tracker: MouseTracker,
    key_sequence: KeySeq,
    reading_history: Option<ReadingHistory>,
    image_popup: Option<ImagePopup>,
    terminal_size: Rect,
    profiler: Arc<Mutex<Option<pprof::ProfilerGuard<'static>>>>,
    book_stat: BookStat,
    jump_list: JumpList,
    book_search: Option<BookSearch>,
}

pub trait VimNavMotions {
    fn handle_h(&mut self);
    fn handle_j(&mut self);
    fn handle_k(&mut self);
    fn handle_l(&mut self);
    fn handle_ctrl_d(&mut self);
    fn handle_ctrl_u(&mut self);
    fn handle_gg(&mut self);
    fn handle_upper_g(&mut self);
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum FocusedPanel {
    Main(MainPanel),
    Popup(PopupWindow),
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum MainPanel {
    NavigationList,
    Content,
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum PopupWindow {
    ReadingHistory,
    BookStats,
    ImagePopup,
    BookSearch,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::new_with_config(None, Some("bookmarks.json"), true)
    }

    /// Helper method to check if focus is on a main panel (not a popup)
    fn is_main_panel(&self, panel: MainPanel) -> bool {
        match self.focused_panel {
            FocusedPanel::Main(p) => p == panel,
            FocusedPanel::Popup(_) => false,
        }
    }

    /// Check if we're in search mode
    fn is_in_search_mode(&self) -> bool {
        self.navigation_panel.is_searching() || self.text_reader.is_searching()
    }

    /// Check if we're actively typing a search query (InputMode)
    fn is_search_input_mode(&self) -> bool {
        if self.navigation_panel.is_searching() {
            self.navigation_panel.get_search_state().mode == SearchMode::InputMode
        } else if self.text_reader.is_searching() {
            self.text_reader.get_search_state().mode == SearchMode::InputMode
        } else {
            false
        }
    }

    /// Handle search input
    fn handle_search_input(&mut self, c: char) {
        if self.navigation_panel.is_searching() {
            let mut query = self.navigation_panel.get_search_state().query.clone();
            query.push(c);
            self.navigation_panel.update_search_query(&query);
        } else if self.text_reader.is_searching() {
            let mut query = self.text_reader.get_search_state().query.clone();
            query.push(c);
            self.text_reader.update_search_query(&query);
        }
    }

    /// Handle search backspace
    fn handle_search_backspace(&mut self) {
        if self.navigation_panel.is_searching() {
            let mut query = self.navigation_panel.get_search_state().query.clone();
            query.pop();
            self.navigation_panel.update_search_query(&query);
        } else if self.text_reader.is_searching() {
            let mut query = self.text_reader.get_search_state().query.clone();
            query.pop();
            self.text_reader.update_search_query(&query);
        }
    }

    /// Cancel current search
    fn cancel_current_search(&mut self) {
        if self.navigation_panel.is_searching() {
            let search_state = self.navigation_panel.get_search_state();
            if search_state.mode == SearchMode::InputMode {
                self.navigation_panel.cancel_search();
            } else {
                self.navigation_panel.exit_search();
            }
        } else if self.text_reader.is_searching() {
            let search_state = self.text_reader.get_search_state();
            if search_state.mode == SearchMode::InputMode {
                self.text_reader.cancel_search();
            } else {
                self.text_reader.exit_search();
            }
        }
    }

    /// Helper method to check if any popup is active
    fn has_active_popup(&self) -> bool {
        matches!(self.focused_panel, FocusedPanel::Popup(_))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_with_mock_system_executor(
        book_directory: Option<&str>,
        bookmark_file: Option<&str>,
        auto_load_recent: bool,
        system_executor: crate::system_command::MockSystemCommandExecutor,
    ) -> Self {
        Self::new_with_config_and_executor(
            book_directory,
            bookmark_file,
            auto_load_recent,
            Box::new(system_executor),
        )
    }

    pub fn new_with_config(
        book_directory: Option<&str>,
        bookmark_file: Option<&str>,
        auto_load_recent: bool,
    ) -> Self {
        Self::new_with_config_and_executor(
            book_directory,
            bookmark_file,
            auto_load_recent,
            Box::new(RealSystemCommandExecutor),
        )
    }

    fn new_with_config_and_executor(
        book_directory: Option<&str>,
        bookmark_file: Option<&str>,
        auto_load_recent: bool,
        system_executor: Box<dyn SystemCommandExecutor>,
    ) -> Self {
        let book_manager = match book_directory {
            Some(dir) => BookManager::new_with_directory(dir),
            None => BookManager::new(),
        };

        let navigation_panel = NavigationPanel::new(&book_manager);
        let text_reader = MarkdownTextReader::new();
        let bookmarks = Bookmarks::load_or_ephemeral(bookmark_file);

        let image_storage = Arc::new(ImageStorage::new_in_project_temp().unwrap_or_else(|e| {
            error!("Failed to initialize image storage: {e}. Using fallback.");
            ImageStorage::new(std::env::temp_dir().join("bookrat_images"))
                .expect("Failed to create fallback image storage")
        }));

        let book_images = BookImages::new(image_storage.clone());

        let terminal_size = if let Ok((width, height)) = crossterm::terminal::size() {
            debug!("Initial terminal size: {width}x{height}");
            Rect::new(0, 0, width, height)
        } else {
            Rect::new(0, 0, 80, 24)
        };

        let mut app = Self {
            book_manager,
            navigation_panel,
            text_reader,
            bookmarks,
            book_images,
            current_book: None,
            focused_panel: FocusedPanel::Main(MainPanel::NavigationList),
            system_command_executor: system_executor,
            last_bookmark_save: std::time::Instant::now(),
            mouse_tracker: MouseTracker::new(),
            key_sequence: KeySeq::new(),
            reading_history: None,
            image_popup: None,
            terminal_size,
            profiler: Arc::new(Mutex::new(None)),
            book_stat: BookStat::new(),
            jump_list: JumpList::new(20),
            book_search: None,
        };

        if auto_load_recent
            && let Some((recent_path, _)) = app.bookmarks.get_most_recent()
            && app.book_manager.contains_book(&recent_path)
        {
            if let Err(e) = app.open_book_for_reading_by_path(&recent_path) {
                error!("Failed to auto-load most recent book: {e}");
            }
        }

        app
    }

    fn toggle_profiling(&self) {
        let mut profiler_lock = self.profiler.lock().unwrap();

        if profiler_lock.is_none() {
            debug!("Profiling started");
            *profiler_lock = Some(pprof::ProfilerGuard::new(1000).unwrap());
        } else {
            debug!("Profiling stopped and saved");

            if let Some(guard) = profiler_lock.take() {
                if let Ok(report) = guard.report().build() {
                    let file = std::fs::File::create("flamegraph.svg").unwrap();
                    report.flamegraph(file).unwrap();
                } else {
                    debug!("Could not build profile report");
                }
            }
        }
    }

    // =============================================================================
    // HIGH-LEVEL APPLICATION ACTIONS
    // =============================================================================
    // These methods encapsulate complete user actions and maintain consistent state

    /// Open a book for reading - the proper way to load and start reading a book
    pub fn open_book_for_reading(&mut self, book_index: usize) -> Result<()> {
        if let Some(book_info) = self.book_manager.get_book_info(book_index) {
            let path = book_info.path.clone();

            self.save_bookmark_with_throttle(true);
            self.load_epub(&path, false)?;

            self.navigation_panel.current_book_index = Some(book_index);
            self.focused_panel = FocusedPanel::Main(MainPanel::Content);

            Ok(())
        } else {
            anyhow::bail!("Invalid book index: {}", book_index)
        }
    }

    pub fn open_book_for_reading_by_path(&mut self, path: &str) -> Result<()> {
        let book_index = self
            .book_manager
            .books
            .iter()
            .position(|book| book.path == path)
            .ok_or_else(|| anyhow::anyhow!("Book not found in manager: {}", path))?;

        self.open_book_for_reading(book_index)
    }

    /// Navigate to a specific chapter - ensures all state is properly updated
    pub fn navigate_to_chapter(&mut self, chapter_index: usize) -> Result<()> {
        if let Some(doc) = &mut self.current_book {
            if doc.epub.set_current_page(chapter_index) {
                self.text_reader.clear_active_anchor();
                self.update_content();
                self.update_toc_state();
                self.save_bookmark_with_throttle(true); //save new location as a bookmark

                Ok(())
            } else {
                anyhow::bail!(
                    "Failed to navigate to chapter {}. Chapter is out of the range",
                    chapter_index
                )
            }
        } else {
            anyhow::bail!("No EPUB document loaded")
        }
    }

    /// Navigate to next or previous chapter - maintains all state consistency
    pub fn navigate_chapter_relative(&mut self, direction: ChapterDirection) -> Result<()> {
        if let Some(book) = &mut self.current_book {
            if (direction == ChapterDirection::Next && book.epub.go_next())
                || (direction == ChapterDirection::Previous && book.epub.go_prev())
            {
                self.update_content();
                self.update_toc_state();
                self.save_bookmark_with_throttle(true);
                Ok(())
            } else {
                anyhow::bail!("Already at the end/beginning of the book")
            }
        } else {
            anyhow::bail!("No document loaded")
        }
    }

    pub fn switch_to_book_list_mode(&mut self) {
        self.navigation_panel.switch_to_book_mode();
        self.focused_panel = FocusedPanel::Main(MainPanel::NavigationList);
    }

    // =============================================================================
    // LOW-LEVEL INTERNAL METHODS
    // =============================================================================
    // These methods should only be called by high-level actions above

    pub fn load_epub(&mut self, path: &str, ignore_bookmarks: bool) -> Result<()> {
        let mut doc = self.book_manager.load_epub(path).map_err(|e| {
            error!("Failed to load EPUB document: {e}");
            anyhow::anyhow!("Failed to load EPUB: {}", e)
        })?;

        info!(
            "Successfully loaded EPUB document {}, total_chapter: {}, current position: {}",
            path,
            doc.get_num_pages(),
            doc.get_current_page()
        );

        let path_buf = std::path::PathBuf::from(path);
        if let Err(e) = self.book_images.load_book(&path_buf) {
            error!("Failed to load book in BookImages: {e}");
        }

        self.initialize_search_engine(&mut doc);

        match BookComments::new(&path_buf) {
            Ok(comments) => {
                let comments_arc = Arc::new(Mutex::new(comments));
                self.text_reader.set_book_comments(comments_arc);
            }
            Err(e) => {
                warn!("Failed to initialize book comments: {e}");
            }
        }

        // Variables to store position to restore after content is loaded
        let mut node_to_restore = None;

        if !ignore_bookmarks && let Some(bookmark) = self.bookmarks.get_bookmark(path) {
            let chapter_to_restore = Self::find_chapter_index_by_href(&doc, &bookmark.chapter_href);

            if let Some(chapter_index) = chapter_to_restore {
                if !doc.set_current_page(chapter_index) {
                    // Fallback: ensure we're within bounds
                    let safe_chapter = chapter_index.min(doc.get_num_pages().saturating_sub(1));
                    if !doc.set_current_page(safe_chapter) {
                        error!("Failed to restore bookmark, staying at chapter 0");
                    }
                }

                if let Some(node_idx) = bookmark.node_index {
                    node_to_restore = Some(node_idx);
                }
            } else {
                warn!("Could not find chapter for href: {}", bookmark.chapter_href);
            }
        } else if doc.get_num_pages() > 1 {
            if doc.go_next() {
                if doc.get_current_str().is_none() {
                    error!(
                        "WARNING: No content at new position {} after go_next()",
                        doc.get_current_page()
                    );
                }
            } else {
                error!("Failed to move to next chapter with go_next()");
                error!(
                    "Current position: {}, Total chapters: {}",
                    doc.get_current_page(),
                    doc.get_num_pages()
                );

                // Try alternative: set_current_page
                info!("Attempting fallback: set_current_page(1)");
                if doc.set_current_page(1) {
                    info!("Fallback successful: moved to chapter 1 using set_current_page");
                } else {
                    error!("Fallback also failed - unable to navigate in this EPUB");
                    // Don't fail completely - stay at chapter 0
                    info!("Staying at chapter 0 as fallback");
                }
            }
        }

        let current_book = EpubBook::new(path.to_string(), doc);
        self.switch_to_toc_mode(&current_book);

        self.current_book = Some(current_book);
        self.update_content();

        if let Some(node_idx) = node_to_restore {
            self.text_reader.restore_to_node_index(node_idx);
        }
        Ok(())
    }

    /// Get the href/path for a chapter at a specific index using the EPUB spine
    fn get_chapter_href(
        doc: &EpubDoc<BufReader<std::fs::File>>,
        chapter_index: usize,
    ) -> Option<String> {
        if chapter_index < doc.spine.len() {
            let spine_item = &doc.spine[chapter_index];
            if let Some((path, _)) = doc.resources.get(&spine_item.idref) {
                return Some(path.to_string_lossy().to_string());
            }
        }
        None
    }

    /// Find chapter index by href/path
    fn find_chapter_index_by_href(
        doc: &EpubDoc<BufReader<std::fs::File>>,
        target_href: &str,
    ) -> Option<usize> {
        for (index, spine_item) in doc.spine.iter().enumerate() {
            if let Some((path, _)) = doc.resources.get(&spine_item.idref) {
                let path_str = path.to_string_lossy();
                if path_str == target_href
                    || path_str.contains(target_href)
                    || target_href.contains(&*path_str)
                {
                    return Some(index);
                }
            }
        }
        None
    }

    fn switch_to_toc_mode(&mut self, book: &EpubBook) {
        let toc_items = TocParser::parse_toc_structure(&book.epub);
        let active_section = self.text_reader.get_active_section(book.current_chapter());
        let current_chapter_href = Self::get_chapter_href(&book.epub, book.current_chapter());

        let book_info = CurrentBookInfo {
            path: book.file.clone(),
            toc_items,
            current_chapter: book.current_chapter(),
            current_chapter_href,
            active_section,
        };

        self.navigation_panel.switch_to_toc_mode(book_info);
    }

    fn update_toc_state(&mut self) {
        let nav_area = self.get_navigation_panel_area();
        let toc_height = nav_area.height as usize;

        if let Some(book) = &self.current_book {
            let current_chapter_href = Self::get_chapter_href(&book.epub, book.current_chapter());
            let current_chapter = book.current_chapter();
            let active_selection = self.text_reader.get_active_section(book.current_chapter());

            debug!("active_selection: {:?}", active_selection);
            self.navigation_panel
                .table_of_contents
                .update_navigation_info(
                    current_chapter,
                    current_chapter_href,
                    active_selection.clone(),
                );

            self.navigation_panel
                .table_of_contents
                .update_active_section(&active_selection, toc_height); // todo: double update is dumb
        }
    }

    pub fn save_bookmark(&mut self) {
        self.save_bookmark_with_throttle(false);
    }

    pub fn save_bookmark_with_throttle(&mut self, force: bool) {
        if let Some(book) = &self.current_book {
            let chapter_href = Self::get_chapter_href(&book.epub, book.current_chapter())
                .unwrap_or_else(|| format!("chapter_{}", book.current_chapter()));

            self.bookmarks.update_bookmark(
                &book.file,
                chapter_href,
                Some(self.text_reader.get_current_node_index()),
                Some(book.current_chapter()),
                Some(book.total_chapters()),
            );

            // Only save to disk if enough time has passed or if forced
            let now = std::time::Instant::now();
            if force
                || now.duration_since(self.last_bookmark_save)
                    > std::time::Duration::from_millis(500)
            {
                if let Err(e) = self.bookmarks.save() {
                    error!("Failed to save bookmark: {e}");
                }
                self.last_bookmark_save = now;
            }
        }
    }

    fn update_content(&mut self) {
        if let Some(book) = &mut self.current_book {
            let (content, title) = match book.epub.get_current_str() {
                Some((raw_html, _mime)) => {
                    let title = TextGenerator::extract_chapter_title(&raw_html);
                    (raw_html, title)
                }
                None => {
                    error!("Failed to get raw HTML");
                    ("Error reading chapter content.".to_string(), None)
                }
            };

            if let Some(chapter_file) = Self::get_chapter_href(&book.epub, book.current_chapter()) {
                self.text_reader
                    .set_current_chapter_file(Some(chapter_file));
            } else {
                self.text_reader.set_current_chapter_file(None);
            }

            self.text_reader.set_content_from_string(&content, title);
            self.text_reader.preload_image_dimensions(&self.book_images);
        } else {
            error!("No EPUB document loaded");
            self.text_reader.clear_content();
        }
    }

    pub fn scroll_down(&mut self) {
        self.text_reader.scroll_down();
        self.save_bookmark();
        self.update_toc_state(); // This will update active section
    }

    pub fn scroll_up(&mut self) {
        self.text_reader.scroll_up();
        self.save_bookmark();
        self.update_toc_state(); // This will update active section
    }

    pub fn scroll_half_screen_down(&mut self, screen_height: usize) {
        self.text_reader.scroll_half_screen_down(screen_height);
        self.save_bookmark();
        self.update_toc_state(); // This will update active section
    }

    fn scroll_half_screen_up(&mut self, screen_height: usize) {
        self.text_reader.scroll_half_screen_up(screen_height);
        self.save_bookmark();
        self.update_toc_state(); // This will update active section
    }

    /// Handle a mouse event with optional batching for scroll events
    /// When event_source is provided, scroll events will be batched for smoother scrolling
    ///
    /// event_source = None is only for testing to simulate scroll signals
    pub fn handle_and_drain_mouse_events(
        &mut self,
        initial_mouse_event: MouseEvent,
        event_source: Option<&mut dyn crate::event_source::EventSource>,
    ) {
        use std::time::Duration;

        let is_scroll_event = matches!(
            initial_mouse_event.kind,
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
        );

        if !is_scroll_event {
            self.handle_non_scroll_mouse_event(initial_mouse_event);
            return;
        }

        // for testing: event_source is None -> don't need to drain events
        if event_source.is_none() {
            match initial_mouse_event.kind {
                MouseEventKind::ScrollDown => self.apply_scroll(1, initial_mouse_event.column),
                MouseEventKind::ScrollUp => self.apply_scroll(-1, initial_mouse_event.column),
                _ => unreachable!(),
            }
            return;
        }

        // Batching logic for scroll events
        let event_source = event_source.unwrap();
        let mut scroll_down_count = 0;
        let mut scroll_up_count = 0;

        let initial_column = initial_mouse_event.column;

        // Count the initial event
        match initial_mouse_event.kind {
            MouseEventKind::ScrollDown => {
                scroll_down_count += 1;
            }
            MouseEventKind::ScrollUp => {
                scroll_up_count += 1;
            }
            _ => unreachable!(), // We already checked this is a scroll event
        }

        // Drain additional mouse scroll events that are queued up
        let drain_timeout = Duration::from_millis(0); // Non-blocking poll
        let max_drain_iterations = 50; // Safety limit to prevent infinite loops
        let mut drain_count = 0;
        let batch_start_time = std::time::Instant::now();

        while drain_count < max_drain_iterations
            && event_source.poll(drain_timeout).unwrap_or(false)
        {
            drain_count += 1;

            // Timeout circuit breaker - prevent infinite loops or excessive processing
            if batch_start_time.elapsed() > std::time::Duration::from_millis(100) {
                break;
            }

            if drain_count > 20 {
                // Safety check
                warn!(
                    "Warning: draining many events ({drain_count}), may indicate event accumulation issue"
                );
            }

            match event_source.read() {
                Ok(Event::Mouse(mouse_event)) => match mouse_event.kind {
                    MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                        //ignore
                        break;
                    }
                    MouseEventKind::ScrollDown => scroll_down_count += 1,
                    MouseEventKind::ScrollUp => scroll_up_count += 1,
                    _ => {
                        self.handle_non_scroll_mouse_event(mouse_event);
                        break;
                    }
                },
                Ok(_) => {
                    // Non-mouse event, stop draining.
                    // TODO: this event will be losts. in practice this doesn't happen
                    break;
                }
                Err(e) => {
                    warn!("Error reading event during batching: {e:?}");
                    break;
                }
            }
        }

        let net_scroll = scroll_down_count - scroll_up_count;

        self.apply_scroll(net_scroll, initial_column);
    }

    /// Handle non-scroll mouse events (clicks, drags, etc.)
    fn handle_non_scroll_mouse_event(&mut self, mouse_event: MouseEvent) {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if image popup is shown first - close it on any click
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ImagePopup)
                ) {
                    let click_x = mouse_event.column;
                    let click_y = mouse_event.row;
                    if let Some(ref popup) = self.image_popup {
                        if popup.is_outside_popup_area(click_x, click_y) {
                            self.image_popup = None;
                            self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                        }
                    }
                    return; // Block all other interactions
                }

                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ReadingHistory)
                ) {
                    let click_type = self
                        .mouse_tracker
                        .detect_click_type(mouse_event.column, mouse_event.row);

                    if let Some(ref mut history) = self.reading_history {
                        match click_type {
                            ClickType::Single | ClickType::Triple => {
                                history.handle_mouse_click(mouse_event.column, mouse_event.row);
                            }
                            ClickType::Double => {
                                if history.handle_mouse_click(mouse_event.column, mouse_event.row) {
                                    if let Some(path) = history.selected_path() {
                                        let ptmp = path.to_string();
                                        let _ = self.open_book_for_reading_by_path(&ptmp);
                                        self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                                        self.reading_history = None;
                                    }
                                }
                            }
                        }
                    }
                    return;
                }

                let nav_panel_width = self.nav_panel_width();
                if mouse_event.column < nav_panel_width {
                    self.focused_panel = FocusedPanel::Main(MainPanel::NavigationList);
                    self.text_reader.clear_selection();

                    let nav_area = self.get_navigation_panel_area();
                    let click_type = self
                        .mouse_tracker
                        .detect_click_type(mouse_event.column, mouse_event.row);

                    match click_type {
                        ClickType::Single | ClickType::Triple => {
                            self.navigation_panel.handle_mouse_click(
                                mouse_event.column,
                                mouse_event.row,
                                nav_area,
                            );
                        }
                        ClickType::Double => {
                            if self.navigation_panel.handle_mouse_click(
                                mouse_event.column,
                                mouse_event.row,
                                nav_area,
                            ) {
                                self.handle_navigation_panel_enter();
                            }
                        }
                    }
                } else {
                    // Click in content area (right 70% of screen)
                    if !self.is_main_panel(MainPanel::Content) {
                        self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                        // Clear manual navigation flag when switching to content
                        self.navigation_panel
                            .table_of_contents
                            .clear_manual_navigation();
                    }

                    let click_type = self
                        .mouse_tracker
                        .detect_click_type(mouse_event.column, mouse_event.row);

                    match click_type {
                        ClickType::Single => {
                            // Check if click is on a link first
                            if let Some(image_src) = self
                                .text_reader
                                .check_image_click(mouse_event.column, mouse_event.row)
                            {
                                self.handle_image_click(&image_src, self.terminal_size);
                            } else {
                                self.text_reader
                                    .handle_mouse_down(mouse_event.column, mouse_event.row);
                            }
                        }
                        ClickType::Double => {
                            self.text_reader
                                .handle_double_click(mouse_event.column, mouse_event.row);
                        }
                        ClickType::Triple => {
                            self.text_reader
                                .handle_triple_click(mouse_event.column, mouse_event.row);
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ImagePopup)
                ) {
                    return;
                }

                let nav_panel_width = self.nav_panel_width();
                if mouse_event.column >= nav_panel_width {
                    if let Some(url) = self
                        .text_reader
                        .handle_mouse_up(mouse_event.column, mouse_event.row)
                    {
                        if let Err(e) = self.handle_link_click(&LinkInfo::from_url(url)) {
                            error!("Failed to handle link click: {e}");
                        }
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Block if image popup is shown
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ImagePopup)
                ) {
                    return;
                }

                let nav_panel_width = self.nav_panel_width();
                if mouse_event.column >= nav_panel_width {
                    let old_scroll_offset = self.text_reader.get_scroll_offset();
                    self.text_reader
                        .handle_mouse_drag(mouse_event.column, mouse_event.row);
                    if self.text_reader.get_scroll_offset() != old_scroll_offset {
                        self.save_bookmark();
                    }
                }
            }
            _ => {
                //do nothing
            }
        }
    }

    fn handle_link_click(&mut self, link_info: &LinkInfo) -> std::io::Result<bool> {
        if link_info.link_type != crate::markdown::LinkType::External
            && let Some(book) = &self.current_book
        {
            let current_location = JumpLocation {
                epub_path: book.file.clone(),
                chapter_index: book.current_chapter(),
                node_index: self.text_reader.get_current_node_index(),
            };
            self.jump_list.push(current_location);
        }

        match &link_info.link_type {
            crate::markdown::LinkType::External => {
                if let Err(e) = open::that(&link_info.url) {
                    error!("Failed to open external link: {e}");
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
            crate::markdown::LinkType::InternalAnchor => {
                if let Some(anchor_id) = &link_info.target_anchor {
                    self.scroll_to_anchor(anchor_id)
                } else {
                    Ok(false)
                }
            }
            crate::markdown::LinkType::InternalChapter => {
                if let Some(chapter_file) = &link_info.target_chapter {
                    if let Some(current_chapter_file) = self.text_reader.get_current_chapter_file()
                    {
                        let current_filename = std::path::Path::new(current_chapter_file)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or(current_chapter_file);
                        let target_filename = std::path::Path::new(chapter_file)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or(chapter_file);

                        if current_filename == target_filename {
                            if let Some(anchor_id) = &link_info.target_anchor {
                                self.scroll_to_anchor(anchor_id)
                            } else {
                                Ok(true)
                            }
                        } else {
                            self.navigate_to_chapter_by_file(
                                chapter_file,
                                link_info.target_anchor.as_ref(),
                            )
                        }
                    } else {
                        self.navigate_to_chapter_by_file(
                            chapter_file,
                            link_info.target_anchor.as_ref(),
                        )
                    }
                } else {
                    Ok(false)
                }
            }
        }
    }

    fn scroll_to_anchor(&mut self, anchor_id: &str) -> std::io::Result<bool> {
        if let Some(target_line) = self.text_reader.get_anchor_position(anchor_id) {
            self.text_reader.scroll_to_line(target_line);
            self.text_reader
                .highlight_line_temporarily(target_line, Duration::from_secs(2));
            Ok(true)
        } else {
            warn!("Anchor '{anchor_id}' not found in current chapter");
            Ok(false)
        }
    }

    fn navigate_to_chapter_by_file(
        &mut self,
        chapter_file: &str,
        anchor_id: Option<&String>,
    ) -> std::io::Result<bool> {
        if let Some(chapter_index) = self.find_chapter_by_filename(chapter_file) {
            if self.navigate_to_chapter(chapter_index).is_err() {
                return Ok(false);
            }

            if let Some(anchor) = anchor_id {
                self.text_reader.store_pending_anchor_scroll(anchor.clone());
            }

            Ok(true)
        } else {
            warn!("Chapter file '{chapter_file}' not found in TOC");
            Ok(false)
        }
    }

    /// todo: this is mislocated and feature envy including find_chapter_recursive
    /// Find chapter index by filename
    fn find_chapter_by_filename(&self, chapter_file: &str) -> Option<usize> {
        // Get the current book's TOC items
        if let Some(current_book_info) = &self
            .navigation_panel
            .table_of_contents
            .get_current_book_info()
        {
            self.find_chapter_recursive(&current_book_info.toc_items, chapter_file)
        } else {
            None
        }
    }

    /// Recursively search for a chapter by filename in TOC items
    fn find_chapter_recursive(&self, items: &[TocItem], filename: &str) -> Option<usize> {
        for item in items {
            match item {
                TocItem::Chapter { href, .. } => {
                    let href_without_anchor = href.split('#').next().unwrap_or(href);

                    if href_without_anchor == filename
                        || href_without_anchor.ends_with(&format!("/{filename}"))
                        || (filename.contains('/') && href_without_anchor.ends_with(filename))
                    {
                        return self.find_spine_index_by_href(href);
                    }
                }
                TocItem::Section { href, children, .. } => {
                    if let Some(section_href) = href {
                        let href_without_anchor =
                            section_href.split('#').next().unwrap_or(section_href);

                        if href_without_anchor == filename
                            || href_without_anchor.ends_with(&format!("/{filename}"))
                            || (filename.contains('/') && href_without_anchor.ends_with(filename))
                        {
                            return self.find_spine_index_by_href(section_href);
                        }
                    }
                    if let Some(found) = self.find_chapter_recursive(children, filename) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }

    /// Find the spine index for a given href
    fn find_spine_index_by_href(&self, href: &str) -> Option<usize> {
        fn normalize_href(href: &str) -> String {
            let normalized = href
                .trim_start_matches("../")
                .trim_start_matches("./")
                .trim_start_matches("OEBPS/");

            // Remove fragment identifiers (e.g., "#ch1", "#tit") for matching
            if let Some(fragment_pos) = normalized.find('#') {
                normalized[..fragment_pos].to_string()
            } else {
                normalized.to_string()
            }
        }

        let book = self.current_book.as_ref()?;

        let normalized_href = normalize_href(href);

        for (index, spine_item) in book.epub.spine.iter().enumerate() {
            if let Some((path, _)) = book.epub.resources.get(&spine_item.idref) {
                let path_str = path.to_string_lossy();
                let normalized_path = normalize_href(&path_str);

                if normalized_path == normalized_href
                    || normalized_path.ends_with(&normalized_href)
                    || normalized_href.ends_with(&normalized_path)
                {
                    return Some(index);
                }
            }
        }

        None
    }

    fn handle_image_click(&mut self, image_src: &str, terminal_size: Rect) {
        let picker = match self.text_reader.get_image_picker() {
            Some(picker) => picker,
            None => {
                // image picker not available
                return;
            }
        };

        let original_image = if let Some(image) = self.text_reader.get_loaded_image(image_src) {
            image
        } else if let Some(image) = self.book_images.get_image(image_src) {
            Arc::new(image)
        } else {
            debug!("Image not loaded and could not be loaded: {image_src}");
            return;
        };

        let font_size = picker.font_size();
        let (img_width, img_height) = original_image.dimensions();

        // Calculate 2x scaled dimensions in pixels
        let scaled_width = img_width * 2;
        let scaled_height = img_height * 2;

        // Calculate max dimensions that fit on screen (in pixels)
        let max_width_pixels = terminal_size.width.saturating_sub(6) as u32 * font_size.0 as u32;
        let max_height_pixels = terminal_size.height.saturating_sub(6) as u32 * font_size.1 as u32;

        // Determine final dimensions maintaining aspect ratio
        let (final_width, final_height) =
            if scaled_width <= max_width_pixels && scaled_height <= max_height_pixels {
                // 2x scale fits
                (scaled_width, scaled_height)
            } else {
                // Scale to fit screen
                let width_scale = max_width_pixels as f32 / img_width as f32;
                let height_scale = max_height_pixels as f32 / img_height as f32;
                let scale = width_scale.min(height_scale);

                (
                    (img_width as f32 * scale) as u32,
                    (img_height as f32 * scale) as u32,
                )
            };

        // Pre-scale the image using fast_image_resize for better performance
        let prescaled_image = if final_width != img_width || final_height != img_height {
            match self
                .book_images
                .resize_image_to(&original_image, final_width, final_height)
            {
                Ok(resized) => Arc::new(resized),
                Err(e) => {
                    warn!("Failed to pre-scale image with fast_image_resize: {e}, using original");
                    original_image
                }
            }
        } else {
            original_image
        };

        let popup = ImagePopup::new(prescaled_image, picker, image_src.to_string());
        self.image_popup = Some(popup);
        self.focused_panel = FocusedPanel::Popup(PopupWindow::ImagePopup);
    }

    /// Apply scroll events (positive for down, negative for up)
    fn apply_scroll(&mut self, scroll_amount: i32, column: u16) {
        if scroll_amount == 0 {
            return;
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::BookSearch)
        ) {
            if let Some(ref mut book_search) = self.book_search {
                let search_height = self.terminal_size.height;
                if scroll_amount > 0 {
                    for _ in 0..scroll_amount.min(10) {
                        book_search.scroll_down(search_height);
                    }
                } else {
                    for _ in 0..(-scroll_amount).min(10) {
                        book_search.scroll_up(search_height);
                    }
                }
            }
            return;
        }

        // Block scrolling for other popups
        if self.has_active_popup() {
            return;
        }

        let is_nav_panel = column < self.nav_panel_width();

        if is_nav_panel {
            let nav_panel_height = self.terminal_size.height.saturating_sub(2);
            if scroll_amount > 0 {
                for _ in 0..scroll_amount.min(10) {
                    self.navigation_panel.scroll_down(nav_panel_height);
                }
            } else {
                for _ in 0..(-scroll_amount).min(10) {
                    self.navigation_panel.scroll_up(nav_panel_height);
                }
            }
        } else if scroll_amount > 0 {
            for _ in 0..scroll_amount.min(10) {
                self.scroll_down();
            }
        } else {
            for _ in 0..(-scroll_amount).min(10) {
                self.scroll_up();
            }
        }
    }

    pub fn open_with_system_viewer(&self) {
        if let Some(book) = &self.current_book {
            match self
                .system_command_executor
                .open_file_at_chapter(&book.file, book.current_chapter())
            {
                Ok(_) => info!(
                    "Successfully opened EPUB with system viewer at chapter {}",
                    book.current_chapter()
                ),
                Err(e) => error!("Failed to open EPUB with system viewer: {e}"),
            }
        } else {
            error!("No EPUB file currently loaded");
        }
    }

    pub fn get_scroll_offset(&self) -> usize {
        self.text_reader.get_scroll_offset()
    }

    fn jump_to_location(&mut self, location: JumpLocation) -> Result<()> {
        if self.current_book.as_ref().map(|x| &x.file) != Some(&location.epub_path) {
            self.load_epub(&location.epub_path, true)?;
        }

        if self.current_book.as_ref().map(|x| x.current_chapter()) != Some(location.chapter_index) {
            self.navigate_to_chapter(location.chapter_index)?;
        }

        self.text_reader.restore_to_node_index(location.node_index);

        self.save_bookmark();

        Ok(())
    }

    /// Handle Ctrl+O - jump back in history
    fn jump_back(&mut self) {
        if let Some(location) = self.jump_list.jump_back() {
            if let Err(e) = self.jump_to_location(location) {
                error!("Failed to jump back: {e}");
            }
        }
    }

    /// Handle Ctrl+I - jump forward in history
    fn jump_forward(&mut self) {
        if let Some(location) = self.jump_list.jump_forward() {
            if let Err(e) = self.jump_to_location(location) {
                error!("Failed to jump forward: {e}");
            }
        }
    }

    /// Calculate the navigation panel width based on stored terminal width
    fn nav_panel_width(&self) -> u16 {
        // 30% of terminal width, minimum 20 columns
        ((self.terminal_size.width * 30) / 100).max(20)
    }

    /// Get the navigation panel area based on current terminal size
    fn get_navigation_panel_area(&self) -> Rect {
        use ratatui::layout::{Constraint, Direction, Layout};
        // Calculate the same layout as in render
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(self.terminal_size);
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunks[0]);
        main_chunks[0]
    }

    /// Handle Enter key press in navigation panel
    fn handle_navigation_panel_enter(&mut self) {
        use crate::navigation_panel::SelectedActionOwned;
        match self.navigation_panel.get_selected_action() {
            SelectedActionOwned::BookIndex(index) => {
                if let Err(e) = self.open_book_for_reading(index) {
                    error!("Failed to open book at index {index}: {e}");
                }
            }
            SelectedActionOwned::BackToBooks => {
                self.navigation_panel.switch_to_book_mode();
            }
            SelectedActionOwned::TocItem(toc_item) => {
                match toc_item {
                    TocItem::Chapter { href, anchor, .. } => {
                        // Find the spine index for this href
                        if let Some(spine_index) = self.find_spine_index_by_href(&href) {
                            let _ = self.navigate_to_chapter(spine_index);
                            // Handle anchor if present
                            if let Some(anchor_id) = anchor {
                                self.text_reader
                                    .store_pending_anchor_scroll(anchor_id.clone());
                                self.text_reader.set_active_anchor(Some(anchor_id));
                            } else {
                                self.text_reader.set_active_anchor(None);
                            }

                            self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                            self.navigation_panel
                                .table_of_contents
                                .clear_manual_navigation();
                            self.update_toc_state();
                        } else {
                            error!("Could not find spine index for href: {href}");
                        }
                    }
                    TocItem::Section { href, anchor, .. } => {
                        if let Some(section_href) = href {
                            // This section has content - navigate to it
                            if let Some(spine_index) = self.find_spine_index_by_href(&section_href)
                            {
                                let _ = self.navigate_to_chapter(spine_index);
                                // Handle anchor if present
                                if let Some(anchor_id) = anchor {
                                    self.text_reader
                                        .store_pending_anchor_scroll(anchor_id.clone());
                                    // Set this anchor as the active one
                                    self.text_reader.set_active_anchor(Some(anchor_id));
                                } else {
                                    self.text_reader.set_active_anchor(None);
                                }
                                self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                                // Update the cache to reflect the correct active section
                                self.update_toc_state();
                            } else {
                                error!(
                                    "Could not find spine index for section href: {section_href}"
                                );
                            }
                        } else {
                            // No href - just toggle expansion
                            self.navigation_panel
                                .table_of_contents
                                .toggle_selected_expansion();
                        }
                    }
                }
            }
            SelectedActionOwned::None => {
                // Nothing selected
            }
        }
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame, fps_counter: &FPSCounter) {
        let auto_scroll_updated = self.text_reader.update_auto_scroll();
        if auto_scroll_updated {
            self.save_bookmark();
        }

        self.terminal_size = f.area();

        let background_block = Block::default().style(Style::default().bg(OCEANIC_NEXT.base_00));
        f.render_widget(background_block, f.area());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(f.area());

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunks[0]);

        self.navigation_panel.render(
            f,
            main_chunks[0],
            self.is_main_panel(MainPanel::NavigationList),
            &OCEANIC_NEXT,
            &self.book_manager,
        );

        if let Some(ref book) = self.current_book {
            self.text_reader.render(
                f,
                main_chunks[1],
                book.current_chapter(),
                book.total_chapters(),
                &OCEANIC_NEXT,
                self.is_main_panel(MainPanel::Content),
            );
        } else {
            self.render_default_content(f, main_chunks[1], "Select a file to view its content");
        }

        self.render_help_bar(f, chunks[1], fps_counter);

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::ReadingHistory)
        ) {
            // First render a dimming overlay
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10)) // Very dark but not black
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            if let Some(ref mut history) = self.reading_history {
                history.render(f, f.area());
            }
        }

        if let Some(ref mut image_popup) = self.image_popup {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10)) // todo: this is not from pallette
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            image_popup.render(f, f.area());
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::BookSearch)
        ) {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10))
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            if let Some(ref mut book_search) = self.book_search {
                book_search.render(f, f.area(), &OCEANIC_NEXT);
            }
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::BookStats)
        ) {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10))
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            self.book_stat.render(f, f.area());
        }
    }

    fn render_default_content(&self, f: &mut ratatui::Frame, area: Rect, content: &str) {
        // Use focus-aware colors instead of hardcoded false
        let (text_color, border_color, _bg_color) =
            OCEANIC_NEXT.get_panel_colors(self.is_main_panel(MainPanel::Content));

        let content_border = Block::default()
            .borders(Borders::ALL)
            .title("Content")
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(OCEANIC_NEXT.base_00));

        let paragraph = Paragraph::new(content)
            .block(content_border)
            .style(Style::default().fg(text_color).bg(OCEANIC_NEXT.base_00));

        f.render_widget(paragraph, area);
    }

    fn render_help_bar(&self, f: &mut ratatui::Frame, area: Rect, fps_counter: &FPSCounter) {
        let (_, _, border_color, _, _) = OCEANIC_NEXT.get_interface_colors(false);

        let help_content = if self.is_in_search_mode() {
            let search_state = if self.navigation_panel.is_searching() {
                self.navigation_panel.get_search_state()
            } else {
                self.text_reader.get_search_state()
            };
            match search_state.mode {
                SearchMode::InputMode => {
                    let query = &search_state.query;
                    let match_info = if search_state.matches.is_empty() && !query.is_empty() {
                        "No matches"
                    } else if !search_state.matches.is_empty() {
                        &format!("{} matches", search_state.matches.len())
                    } else {
                        ""
                    };
                    format!("/ {query}█  {match_info}  ESC: Cancel | Enter: Search")
                }
                SearchMode::NavigationMode => {
                    let query = &search_state.query;
                    let match_info = search_state.get_match_info();
                    format!("/{query}  {match_info}  n/N: Navigate | ESC: Exit")
                }
                _ => "Search mode active".to_string(),
            }
        } else if self.text_reader.has_text_selection() {
            "a: Add comment | c/Ctrl+C: Copy to clipboard | ESC: Clear selection".to_string()
        } else {
            let help_text = match self.focused_panel {
                FocusedPanel::Main(MainPanel::NavigationList) => {
                    "j/k: Navigate | Enter: Select | Space+h: History | H/L: Fold/Unfold All | Tab: Switch | q: Quit"
                }
                FocusedPanel::Main(MainPanel::Content) => {
                    "j/k: Scroll | h/l: Chapter | Ctrl+d/u: Half-screen | Space+h: History | Tab: Switch | Space+o: Open | q: Quit"
                }
                FocusedPanel::Popup(PopupWindow::ReadingHistory) => {
                    "j/k: Navigate | Enter: Open | ESC: Close"
                }
                FocusedPanel::Popup(PopupWindow::BookStats) => "j/k/Ctrl+d/u: Scroll | ESC: Close",
                FocusedPanel::Popup(PopupWindow::ImagePopup) => "ESC/Any key: Close",
                FocusedPanel::Popup(PopupWindow::BookSearch) => {
                    "Space+f: Reopen | Space+F: New Search"
                }
            };
            help_text.to_string()
        };

        let help = Paragraph::new(format!(
            "{} | FPS: {}",
            help_content, fps_counter.current_fps
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(OCEANIC_NEXT.base_00)),
        )
        .style(
            Style::default()
                .fg(OCEANIC_NEXT.base_03)
                .bg(OCEANIC_NEXT.base_00),
        );

        f.render_widget(help, area);
    }

    /// Handle a key sequence and return true if it was handled
    fn handle_key_sequence(&mut self, key_char: char) -> bool {
        let sequence: String = self.key_sequence.handle_key(key_char);

        match sequence.as_str() {
            "gg" => {
                // Handle 'gg' motion - go to top
                self.text_reader.handle_gg();
                self.save_bookmark();
                self.key_sequence.clear();
                true
            }
            " s" => {
                // Handle Space->s to show raw HTML source
                if self.is_main_panel(MainPanel::Content) && self.current_book.is_some() {
                    // Get raw HTML content for current chapter
                    if let Some(ref mut book) = self.current_book {
                        if let Some((raw_html, _)) = book.epub.get_current_str() {
                            self.text_reader.set_raw_html(raw_html);
                            self.text_reader.toggle_raw_html();
                        }
                    }
                }
                self.key_sequence.clear();
                true
            }
            " f" => {
                // Handle Space->f to open book search (reuse existing search)
                if self.current_book.is_some() {
                    self.open_book_search(false); // Don't clear input
                }
                self.key_sequence.clear();
                true
            }
            " F" => {
                // Handle Space->F to open book search (clear input)
                if self.current_book.is_some() {
                    self.open_book_search(true); // Clear input for new search
                }
                self.key_sequence.clear();
                true
            }
            " d" => {
                if self.current_book.is_some() {
                    if let Some(ref mut book) = self.current_book {
                        let terminal_size = (self.terminal_size.width, self.terminal_size.height);
                        if let Err(e) = self
                            .book_stat
                            .calculate_stats(&mut book.epub, terminal_size)
                        {
                            error!("Failed to calculate book statistics: {e}");
                        } else {
                            self.book_stat.show();
                            self.focused_panel = FocusedPanel::Popup(PopupWindow::BookStats);
                        }
                    }
                }
                self.key_sequence.clear();
                true
            }
            " z" => {
                // Handle Space->z to copy raw_text_lines for debugging
                if self.is_main_panel(MainPanel::Content) {
                    if let Err(e) = self.text_reader.copy_raw_text_lines_to_clipboard() {
                        debug!("Copy raw_text_lines failed: {e}");
                    } else {
                        debug!("Successfully copied raw_text_lines to clipboard for debugging");
                    }
                }
                self.key_sequence.clear();
                true
            }
            " c" => {
                // Handle Space->c to copy entire chapter content
                if self.is_main_panel(MainPanel::Content) {
                    if let Err(e) = self.text_reader.copy_chapter_to_clipboard() {
                        debug!("Copy chapter failed: {e}");
                    } else {
                        debug!("Successfully copied chapter content to clipboard");
                    }
                }
                self.key_sequence.clear();
                true
            }
            " o" => {
                // Handle Space->o to open current EPUB with system viewer
                self.open_with_system_viewer();
                self.key_sequence.clear();
                true
            }
            " h" => {
                // Handle Space->h to toggle reading history
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ReadingHistory)
                ) {
                    // Close history
                    self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                    self.reading_history = None;
                } else {
                    // Open history
                    self.reading_history = Some(ReadingHistory::new(&self.bookmarks));
                    self.focused_panel = FocusedPanel::Popup(PopupWindow::ReadingHistory);
                }
                self.key_sequence.clear();
                true
            }
            _ if sequence.len() >= 2 => {
                // Unknown sequence of 2+ chars, reset
                self.key_sequence.clear();
                false
            }
            _ => {
                // Still building the sequence
                false
            }
        }
    }

    /// Handle a single key event with optional screen height for half-screen scrolling
    pub fn handle_key_event_with_screen_height(
        &mut self,
        key: crossterm::event::KeyEvent,
        screen_height: Option<usize>,
    ) -> Option<AppAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // If comment input is active, route all input to the text area
        if self.text_reader.is_comment_input_active() {
            if let Some(input) = map_keys_to_input(key) {
                if self.text_reader.handle_comment_input(input) {
                    return None;
                }
            }
        }

        // If image popup is shown, close it on any key press
        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::ImagePopup)
        ) {
            self.image_popup = None;
            self.focused_panel = FocusedPanel::Main(MainPanel::Content);
            return None;
        }

        // If book search popup is shown, handle keys for it
        if self.focused_panel == FocusedPanel::Popup(PopupWindow::BookSearch) {
            let action = if let Some(ref mut book_search) = self.book_search {
                book_search.handle_key_event(key)
            } else {
                None
            };

            // Handle the action outside of the borrow
            if let Some(action) = action {
                match action {
                    BookSearchAction::JumpToChapter {
                        chapter_index,
                        line_number,
                    } => {
                        self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                        if let Err(e) = self.navigate_to_chapter(chapter_index) {
                            error!("Failed to navigate to chapter {chapter_index}: {e}");
                        } else {
                            self.text_reader.scroll_to_line(line_number);
                        }
                    }
                    BookSearchAction::Close => {
                        self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                    }
                }
            }
            return None;
        }

        // If book stat popup is shown, handle keys for it
        if self.focused_panel == FocusedPanel::Popup(PopupWindow::BookStats) {
            match self.book_stat.handle_key(key, &mut self.key_sequence) {
                Some(BookStatAction::Close) => {
                    self.book_stat.hide();
                    self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                }
                Some(BookStatAction::JumpToChapter { chapter_index }) => {
                    self.book_stat.hide();
                    self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                    if let Err(e) = self.navigate_to_chapter(chapter_index) {
                        error!("Failed to navigate to chapter {chapter_index}: {e}");
                    }
                }
                None => {}
            }
            return None;
        }

        // If reading history popup is shown, handle keys for it
        if self.focused_panel == FocusedPanel::Popup(PopupWindow::ReadingHistory) {
            let action = if let Some(ref mut history) = self.reading_history {
                history.handle_key(key, &mut self.key_sequence)
            } else {
                None
            };

            if let Some(action) = action {
                use crate::reading_history::ReadingHistoryAction;
                match action {
                    ReadingHistoryAction::Close => {
                        self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                        self.reading_history = None;
                    }
                    ReadingHistoryAction::OpenBook { path } => {
                        if let Some(book_index) = self.book_manager.find_book_index_by_path(&path) {
                            self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                            self.reading_history = None;
                            let _ = self.open_book_for_reading(book_index);
                        }
                    }
                }
            }
            return None;
        }

        if self.is_search_input_mode() {
            match key.code {
                KeyCode::Char(c) => self.handle_search_input(c),
                KeyCode::Backspace => self.handle_search_backspace(),
                KeyCode::Esc => self.cancel_current_search(),

                KeyCode::Enter => {
                    // Handle Enter in search mode
                    if self.navigation_panel.is_searching() {
                        self.navigation_panel.confirm_search();
                    } else if self.text_reader.is_searching() {
                        self.text_reader.confirm_search();
                    }
                }
                _ => {}
            }
            return None;
        }

        // If navigation panel (file list) has focus, handle keys for it
        if self.is_main_panel(MainPanel::NavigationList) && !self.is_search_input_mode() {
            let action = self
                .navigation_panel
                .handle_key(key, &mut self.key_sequence);
            let mut bypass = false;
            if let Some(action) = action {
                use crate::navigation_panel::NavigationPanelAction;
                match action {
                    NavigationPanelAction::Bypass => {
                        bypass = true;
                    }
                    NavigationPanelAction::SelectBook { book_index } => {
                        let _ = self.open_book_for_reading(book_index);
                    }
                    NavigationPanelAction::SwitchToBookList => {
                        self.switch_to_book_list_mode();
                    }
                    NavigationPanelAction::NavigateToChapter { href, anchor } => {
                        if let Some(chapter_index) = self.find_spine_index_by_href(&href) {
                            let _ = self.navigate_to_chapter(chapter_index);
                            if let Some(anchor_id) = anchor {
                                self.text_reader.store_pending_anchor_scroll(anchor_id);
                            }
                            self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                        }
                    }
                    NavigationPanelAction::ToggleSection => {
                        self.navigation_panel
                            .table_of_contents
                            .toggle_selected_expansion();
                    }
                }
            }

            if key.code == KeyCode::Char('q') {
                self.save_bookmark_with_throttle(true);
                return Some(AppAction::Quit);
            }

            if !bypass {
                return None;
            }
        }

        match key.code {
            KeyCode::Char('/') => {
                if self.is_main_panel(MainPanel::Content) {
                    self.text_reader.start_search();
                }
            }
            KeyCode::Char('n') if self.is_in_search_mode() => {
                if self.navigation_panel.is_searching() {
                    let search_state = self.navigation_panel.get_search_state();
                    if search_state.mode == SearchMode::InputMode {
                        self.handle_search_input('n');
                    }
                } else if self.text_reader.is_searching() {
                    let search_state = self.text_reader.get_search_state();
                    if search_state.mode == SearchMode::NavigationMode {
                        self.text_reader.next_match();
                    } else {
                        self.handle_search_input('n');
                    }
                }
            }
            KeyCode::Char('N') if self.is_in_search_mode() => {
                if self.navigation_panel.is_searching() {
                    let search_state = self.navigation_panel.get_search_state();
                    if search_state.mode == SearchMode::InputMode {
                        self.handle_search_input('N');
                    }
                } else if self.text_reader.is_searching() {
                    let search_state = self.text_reader.get_search_state();
                    if search_state.mode == SearchMode::NavigationMode {
                        self.text_reader.previous_match();
                    } else {
                        self.handle_search_input('N');
                    }
                }
            }
            KeyCode::Char('f') => if self.handle_key_sequence('f') {},
            KeyCode::Char('F') => if self.handle_key_sequence('F') {},
            KeyCode::Char('s') => if self.handle_key_sequence('s') {},
            KeyCode::Char(' ') => if !self.handle_key_sequence(' ') {},
            KeyCode::Char('g') => if !self.handle_key_sequence('g') {},

            KeyCode::Char('d') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.handle_key_sequence('d') {
                } else if !self.text_reader.is_comment_input_active() {
                    match self.text_reader.delete_comment_at_cursor() {
                        Ok(true) => {
                            info!("Comment deleted successfully");
                        }
                        Ok(false) => {
                            // Cursor not on a comment, ignore
                        }
                        Err(e) => {
                            error!("Failed to delete comment: {e}");
                        }
                    }
                }
            }
            KeyCode::Char('j') => {
                self.scroll_down();
            }
            KeyCode::Char('k') => {
                self.scroll_up();
            }
            KeyCode::Char('h') => {
                if !self.handle_key_sequence('h') {
                    let _ = self.navigate_chapter_relative(ChapterDirection::Previous);
                }
            }
            KeyCode::Char('l') => {
                let _ = self.navigate_chapter_relative(ChapterDirection::Next);
            }
            KeyCode::Char('i') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.jump_forward();
            }
            KeyCode::Char('o') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.jump_back();
                } else if !self.handle_key_sequence('o') {
                }
            }
            KeyCode::Char('p') => {
                self.toggle_profiling();
            }
            KeyCode::Tab => {
                if !self.has_active_popup() {
                    self.focused_panel = match self.focused_panel {
                        FocusedPanel::Main(MainPanel::NavigationList) => {
                            self.navigation_panel
                                .table_of_contents
                                .clear_manual_navigation();
                            FocusedPanel::Main(MainPanel::Content)
                        }
                        FocusedPanel::Main(MainPanel::Content) => {
                            FocusedPanel::Main(MainPanel::NavigationList)
                        }
                        FocusedPanel::Popup(_) => self.focused_panel, // No tab switching in popups
                    };
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(visible_height) = screen_height {
                    self.scroll_half_screen_down(visible_height);
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(visible_height) = screen_height {
                    self.scroll_half_screen_up(visible_height);
                }
            }

            KeyCode::Char('G') => {
                if self.current_book.is_some() {
                    self.text_reader.handle_upper_g();
                }
            }
            KeyCode::Char('a') => {
                if self.text_reader.has_text_selection() && self.text_reader.start_comment_input() {
                    debug!("Started comment input mode");
                }
            }
            KeyCode::Char('c') => {
                if !self.handle_key_sequence('c') {
                    if let Err(e) = self.text_reader.copy_selection_to_clipboard() {
                        error!("Copy failed: {e}");
                    }
                }
            }
            KeyCode::Char('q') => {
                self.save_bookmark_with_throttle(true);
                return Some(AppAction::Quit);
            }
            KeyCode::Esc => {
                if self.text_reader.has_text_selection() {
                    self.text_reader.clear_selection();
                } else if self.is_in_search_mode() {
                    self.cancel_current_search();
                }
            }
            _ => {}
        }
        None
    }

    pub fn handle_resize(&mut self) {
        // text reader needs to update image picker and line wraps
        self.text_reader.handle_terminal_resize();
    }

    //todo this does extra parsing of a book. damn claude is dumb
    fn initialize_search_engine(&mut self, doc: &mut EpubDoc<BufReader<std::fs::File>>) {
        fn extract_text_from_markdown_doc(doc: &crate::markdown::Document) -> String {
            let mut lines = Vec::new();
            for node in &doc.blocks {
                extract_text_from_block(&node.block, &mut lines);
            }
            lines.join("\n")
        }

        fn extract_text_from_block(block: &crate::markdown::Block, lines: &mut Vec<String>) {
            use crate::markdown::Block;

            match block {
                Block::Paragraph { content } | Block::Heading { content, .. } => {
                    let plain_text = extract_text_from_text(content);
                    if !plain_text.trim().is_empty() {
                        lines.push(plain_text);
                    }
                }
                Block::List { items, .. } => {
                    for item in items {
                        // ListItem content is Vec<Node>, so process each node
                        for node in &item.content {
                            extract_text_from_block(&node.block, lines);
                        }
                    }
                }
                Block::Quote { content } => {
                    for node in content {
                        extract_text_from_block(&node.block, lines);
                    }
                }
                Block::CodeBlock { content, .. } => {
                    lines.push(content.clone());
                }
                Block::Table { rows, header, .. } => {
                    if let Some(header_row) = header {
                        let row_text: Vec<String> = header_row
                            .cells
                            .iter()
                            .map(|cell| extract_text_from_text(&cell.content))
                            .collect();
                        if !row_text.is_empty() {
                            lines.push(row_text.join(" "));
                        }
                    }
                    for row in rows {
                        let row_text: Vec<String> = row
                            .cells
                            .iter()
                            .map(|cell| extract_text_from_text(&cell.content))
                            .collect();
                        if !row_text.is_empty() {
                            lines.push(row_text.join(" "));
                        }
                    }
                }
                Block::DefinitionList { items } => {
                    for item in items {
                        lines.push(extract_text_from_text(&item.term));
                        // Process each definition (Vec<Vec<Node>>)
                        for definition in &item.definitions {
                            for node in definition {
                                extract_text_from_block(&node.block, lines);
                            }
                        }
                    }
                }
                Block::EpubBlock { content, .. } => {
                    for node in content {
                        extract_text_from_block(&node.block, lines);
                    }
                }
                _ => {}
            }
        }

        fn extract_text_from_text(text: &crate::markdown::Text) -> String {
            let mut result = String::new();

            for part in text.iter() {
                match part {
                    crate::markdown::TextOrInline::Text(text_node) => {
                        result.push_str(&text_node.content);
                    }
                    crate::markdown::TextOrInline::Inline(inline) => match inline {
                        crate::markdown::Inline::Link { text, .. } => {
                            result.push_str(&extract_text_from_text(text));
                        }
                        crate::markdown::Inline::Image { alt_text, .. } => {
                            result.push_str(alt_text);
                        }
                        crate::markdown::Inline::LineBreak => {
                            result.push(' ');
                        }
                        _ => {}
                    },
                }
            }

            result
        }

        let mut search_engine = SearchEngine::new();
        let mut chapters = Vec::new();
        use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
        let mut converter = HtmlToMarkdownConverter::new();

        // Process all chapters to extract readable text
        for chapter_index in 0..doc.get_num_pages() {
            if doc.set_current_page(chapter_index) {
                if let Some((raw_html, _mime)) = doc.get_current_str() {
                    let title = TextGenerator::extract_chapter_title(&raw_html)
                        .unwrap_or_else(|| format!("Chapter {}", chapter_index + 1));

                    let markdown_doc = converter.convert(&raw_html);

                    let clean_text = extract_text_from_markdown_doc(&markdown_doc);
                    chapters.push((chapter_index, title, clean_text));
                }
            }
        }

        search_engine.process_chapters(chapters);

        self.book_search = Some(BookSearch::new(search_engine));
    }

    fn open_book_search(&mut self, clear_input: bool) {
        if let Some(ref mut book_search) = self.book_search {
            book_search.open(clear_input);
            self.focused_panel = FocusedPanel::Popup(PopupWindow::BookSearch);
        } else {
            error!(
                "Cannot open book search - search engine not initialized. This should never happen"
            );
        }
    }
}

pub struct FPSCounter {
    last_measure: Instant,
    ticks: u16,
    current_fps: u16,
}

impl Default for FPSCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl FPSCounter {
    pub fn new() -> FPSCounter {
        FPSCounter {
            last_measure: Instant::now(),
            ticks: 0,
            current_fps: 0,
        }
    }

    fn tick(&mut self) {
        self.ticks = self.ticks.saturating_add(1);
        let elapsed = self.last_measure.elapsed();
        if elapsed > Duration::from_secs(1) {
            self.current_fps = self.ticks;
            self.last_measure = Instant::now();
            self.ticks = 0;
        }
    }
}

pub fn run_app_with_event_source<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    event_source: &mut dyn EventSource,
) -> Result<()> {
    let tick_rate = Duration::from_millis(50); // Faster tick rate for smoother animation
    let mut last_tick = std::time::Instant::now();
    let mut fps_counter = FPSCounter::new();
    let mut first_render = true; // Ensure we always render at least once on startup
    loop {
        let mut events_processed = 0;
        let mut should_quit = false;
        fps_counter.tick();
        while event_source.poll(Duration::from_millis(0))? && events_processed < 50 {
            let event = event_source.read()?;
            events_processed += 1;

            match event {
                Event::Mouse(mouse_event) => {
                    match mouse_event.kind {
                        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                            // Completely ignore horizontal scroll events to prevent flooding
                        }
                        _ => {
                            app.handle_and_drain_mouse_events(mouse_event, Some(event_source));
                        }
                    }
                }
                Event::Key(key) => {
                    let visible_height = terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
                    if app.handle_key_event_with_screen_height(key, Some(visible_height))
                        == Some(AppAction::Quit)
                    {
                        should_quit = true;
                    }
                }
                Event::Resize(_cols, _rows) => {
                    app.handle_resize();
                }
                _ => {}
            }

            if should_quit {
                break;
            }
        }

        let mut needs_redraw = events_processed > 0;

        if first_render {
            needs_redraw = true;
            first_render = false;
        }

        if last_tick.elapsed() >= tick_rate {
            let highlight_changed = app.text_reader.update_highlight(); // Update highlight state
            let images_loaded = app.text_reader.check_for_loaded_images();
            if images_loaded {
                needs_redraw = true;
                debug!("Images loaded, forcing redraw");
            }
            if highlight_changed {
                needs_redraw = true;
                debug!("Highlight expired, forcing redraw");
            }
            last_tick = std::time::Instant::now();
        }

        if needs_redraw {
            let draw_start = std::time::Instant::now();
            terminal.draw(|f| app.draw(f, &fps_counter))?;
            let draw_duration = draw_start.elapsed();

            // Log if drawing/flushing took longer than 10ms
            if draw_duration.as_millis() > 10 {
                debug!("Terminal draw/flush took {}ms", draw_duration.as_millis());
            }
        }

        // If no events were processed, wait a bit to avoid busy-waiting
        if events_processed == 0 {
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            let _ = event_source.poll(timeout);
        }

        if should_quit {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {}
