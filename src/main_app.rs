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
use crate::navigation_panel::{CurrentBookInfo, NavigationPanel, TableOfContents};
use crate::notification::NotificationManager;
use crate::parsing::text_generator::TextGenerator;
use crate::parsing::toc_parser::TocParser;
use crate::reading_history::ReadingHistory;
use crate::search::{SearchMode, SearchablePanel};
use crate::search_engine::{SearchEngine, SearchLine};
use crate::settings;
use crate::system_command::{RealSystemCommandExecutor, SystemCommandExecutor};
use crate::table_of_contents::TocItem;
use crate::theme::{current_theme, current_theme_name};
use crate::types::LinkInfo;
use crate::widget::help_popup::{HelpPopup, HelpPopupAction};
use crate::widget::theme_selector::{ThemeSelector, ThemeSelectorAction};
use image::GenericImageView;
use log::warn;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChapterDirection {
    Next,
    Previous,
}

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use epub::doc::EpubDoc;
use log::{debug, error, info};
use ratatui::{
    Terminal,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
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
        self.epub.get_num_chapters()
    }

    fn current_chapter(&self) -> usize {
        self.epub.get_current_chapter()
    }
}

/// URL-decode percent-encoded characters in a string (e.g., %27 -> ')
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Try to read two hex digits
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            // If parsing failed, just keep the original %XX sequence
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }

    result
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
    previous_main_panel: MainPanel,
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
    help_popup: Option<HelpPopup>,
    comments_viewer: Option<crate::widget::comments_viewer::CommentsViewer>,
    theme_selector: Option<ThemeSelector>,
    notifications: NotificationManager,
    help_bar_area: Rect,
    zen_mode: bool,
    comments_dir: Option<PathBuf>,
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
    Help,
    CommentsViewer,
    ThemeSelector,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::new_with_config(None, Some("bookmarks.json"), true, None)
    }

    /// Helper method to check if focus is on a main panel (not a popup)
    fn is_main_panel(&self, panel: MainPanel) -> bool {
        match self.focused_panel {
            FocusedPanel::Main(p) => p == panel,
            FocusedPanel::Popup(_) => false,
        }
    }

    /// Set focus to a main panel and track it for popup dismissal
    fn set_main_panel_focus(&mut self, panel: MainPanel) {
        self.previous_main_panel = panel;
        self.focused_panel = FocusedPanel::Main(panel);
    }

    /// Close current popup and return focus to previous main panel
    fn close_popup_to_previous(&mut self) {
        self.focused_panel = FocusedPanel::Main(self.previous_main_panel);
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

    /// Show an informational notification to the user
    pub fn show_info(&mut self, message: impl Into<String>) {
        self.notifications.show_info(message);
    }

    /// Show a warning notification to the user
    pub fn show_warning(&mut self, message: impl Into<String>) {
        self.notifications.show_warning(message);
    }

    /// Show an error notification to the user
    pub fn show_error(&mut self, message: impl Into<String>) {
        self.notifications.show_error(message);
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_with_mock_system_executor(
        book_directory: Option<&str>,
        bookmark_file: Option<&str>,
        auto_load_recent: bool,
        system_executor: crate::system_command::MockSystemCommandExecutor,
        comments_dir: Option<&Path>,
    ) -> Self {
        Self::new_with_config_and_executor(
            book_directory,
            bookmark_file,
            auto_load_recent,
            Box::new(system_executor),
            comments_dir,
        )
    }

    pub fn new_with_config(
        book_directory: Option<&str>,
        bookmark_file: Option<&str>,
        auto_load_recent: bool,
        comments_dir: Option<&Path>,
    ) -> Self {
        Self::new_with_config_and_executor(
            book_directory,
            bookmark_file,
            auto_load_recent,
            Box::new(RealSystemCommandExecutor),
            comments_dir,
        )
    }

    fn new_with_config_and_executor(
        book_directory: Option<&str>,
        bookmark_file: Option<&str>,
        auto_load_recent: bool,
        system_executor: Box<dyn SystemCommandExecutor>,
        comments_dir: Option<&Path>,
    ) -> Self {
        let book_manager = match book_directory {
            Some(dir) => BookManager::new_with_directory(dir),
            None => BookManager::new(),
        };

        let navigation_panel = NavigationPanel::new(&book_manager);
        let mut text_reader = MarkdownTextReader::new();
        text_reader.set_margin(settings::get_margin());
        let bookmarks = Bookmarks::load_or_ephemeral(bookmark_file);

        let image_storage = Arc::new(ImageStorage::new_in_project_temp().unwrap_or_else(|e| {
            error!("Failed to initialize image storage: {e}. Using fallback.");
            ImageStorage::new(std::env::temp_dir().join("bookokrat_images"))
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
            previous_main_panel: MainPanel::NavigationList,
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
            help_popup: None,
            comments_viewer: None,
            theme_selector: None,
            notifications: NotificationManager::new(),
            help_bar_area: Rect::default(),
            zen_mode: false,
            comments_dir: comments_dir.map(|p| p.to_path_buf()),
        };

        if auto_load_recent
            && let Some((recent_path, _)) = app.bookmarks.get_most_recent()
            && app.book_manager.contains_book(&recent_path)
        {
            if let Err(e) = app.open_book_for_reading_by_path(&recent_path) {
                error!("Failed to auto-load most recent book: {e}");
                app.show_error(format!("Failed to auto-load recent book: {e}"));
            }
        } else if auto_load_recent && app.bookmarks.get_most_recent().is_none() {
            // No bookmarks exist - show help popup for first-time users
            // Set previous panel to NavigationList so ESC returns there
            app.previous_main_panel = MainPanel::NavigationList;
            app.help_popup = Some(HelpPopup::new());
            app.focused_panel = FocusedPanel::Popup(PopupWindow::Help);
        }

        app
    }

    fn is_profiling(&self) -> bool {
        self.profiler.lock().unwrap().is_some()
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
            if doc.epub.set_current_chapter(chapter_index) {
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

    pub fn navigate_to_chapter_by_href(&mut self, href: &str) -> Result<()> {
        if let Some(ref mut book) = self.current_book {
            let chapter_path = std::path::PathBuf::from(href);
            if let Some(chapter_idx) = book.epub.resource_uri_to_chapter(&chapter_path) {
                self.navigate_to_chapter(chapter_idx)
            } else {
                anyhow::bail!("Failed to find chapter with href: {}", href)
            }
        } else {
            anyhow::bail!("No EPUB document loaded")
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
            self.show_error(format!("Failed to load EPUB: {e}"));
            anyhow::anyhow!("Failed to load EPUB: {}", e)
        })?;

        info!(
            "Successfully loaded EPUB document {}, total_chapter: {}, current position: {}",
            path,
            doc.get_num_chapters(),
            doc.get_current_chapter()
        );

        let path_buf = std::path::PathBuf::from(path);
        if let Err(e) = self.book_images.load_book(&path_buf) {
            error!("Failed to load book in BookImages: {e}");
        }

        self.initialize_search_engine(&mut doc);

        match BookComments::new(&path_buf, self.comments_dir.as_deref()) {
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
                if !doc.set_current_chapter(chapter_index) {
                    // Fallback: ensure we're within bounds
                    let safe_chapter = chapter_index.min(doc.get_num_chapters().saturating_sub(1));
                    if !doc.set_current_chapter(safe_chapter) {
                        error!("Failed to restore bookmark, staying at chapter 0");
                    }
                }

                if let Some(node_idx) = bookmark.node_index {
                    node_to_restore = Some(node_idx);
                }
            } else {
                warn!("Could not find chapter for href: {}", bookmark.chapter_href);
            }
        } else if doc.get_num_chapters() > 1 {
            if doc.go_next() {
                if doc.get_current_str().is_none() {
                    error!(
                        "WARNING: No content at new position {} after go_next()",
                        doc.get_current_chapter()
                    );
                }
            } else {
                error!("Failed to move to next chapter with go_next()");
                error!(
                    "Current position: {}, Total chapters: {}",
                    doc.get_current_chapter(),
                    doc.get_num_chapters()
                );

                // Try alternative: set_current_chapter
                info!("Attempting fallback: set_current_chapter(1)");
                if doc.set_current_chapter(1) {
                    info!("Fallback successful: moved to chapter 1 using set_current_chapter");
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
            if let Some(resource) = doc.resources.get(&spine_item.idref) {
                return Some(resource.path.to_string_lossy().to_string());
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
            if let Some(resource) = doc.resources.get(&spine_item.idref) {
                let path_str = resource.path.to_string_lossy();
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
        let current_chapter_href = Self::get_chapter_href(&book.epub, book.current_chapter());
        let available_anchors =
            TableOfContents::anchors_for_items(&toc_items, current_chapter_href.as_deref());
        let active_section = self.text_reader.get_active_section(
            book.current_chapter(),
            current_chapter_href.as_deref(),
            &available_anchors,
        );

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
            let available_anchors = self
                .navigation_panel
                .table_of_contents
                .anchors_for_chapter(current_chapter_href.as_deref());
            let active_selection = self.text_reader.get_active_section(
                book.current_chapter(),
                current_chapter_href.as_deref(),
                &available_anchors,
            );

            self.navigation_panel
                .table_of_contents
                .update_navigation_info(
                    current_chapter,
                    current_chapter_href.clone(),
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
                if self.handle_help_bar_click(mouse_event.column, mouse_event.row) {
                    return;
                }

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
                            self.close_popup_to_previous();
                        }
                    }
                    return; // Block all other interactions
                }

                // Handle help popup mouse clicks
                if matches!(self.focused_panel, FocusedPanel::Popup(PopupWindow::Help)) {
                    let click_x = mouse_event.column;
                    let click_y = mouse_event.row;

                    if let Some(ref help_popup) = self.help_popup {
                        // Check if click is outside popup area - close it
                        if help_popup.is_outside_popup_area(click_x, click_y) {
                            self.help_popup = None;
                            self.close_popup_to_previous();
                        }
                    }
                    return; // Block all other interactions
                }

                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ReadingHistory)
                ) {
                    let click_x = mouse_event.column;
                    let click_y = mouse_event.row;

                    if let Some(ref mut history) = self.reading_history {
                        // Check if click is outside popup area - close it
                        if history.is_outside_popup_area(click_x, click_y) {
                            self.reading_history = None;
                            self.close_popup_to_previous();
                            return;
                        }

                        let click_type = self
                            .mouse_tracker
                            .detect_click_type(mouse_event.column, mouse_event.row);

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

                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::BookStats)
                ) {
                    let click_x = mouse_event.column;
                    let click_y = mouse_event.row;

                    // Check if click is outside popup area - close it
                    if self.book_stat.is_outside_popup_area(click_x, click_y) {
                        self.book_stat.hide();
                        self.close_popup_to_previous();
                        return;
                    }

                    let click_type = self
                        .mouse_tracker
                        .detect_click_type(mouse_event.column, mouse_event.row);

                    match click_type {
                        ClickType::Single | ClickType::Triple => {
                            self.book_stat
                                .handle_mouse_click(mouse_event.column, mouse_event.row);
                        }
                        ClickType::Double => {
                            if self
                                .book_stat
                                .handle_mouse_click(mouse_event.column, mouse_event.row)
                            {
                                if let Some(chapter_index) =
                                    self.book_stat.get_selected_chapter_index()
                                {
                                    self.book_stat.hide();
                                    self.set_main_panel_focus(MainPanel::Content);
                                    if let Err(e) = self.navigate_to_chapter(chapter_index) {
                                        error!(
                                            "Failed to navigate to chapter {chapter_index}: {e}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    return;
                }

                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::CommentsViewer)
                ) {
                    let click_x = mouse_event.column;
                    let click_y = mouse_event.row;

                    if let Some(ref mut viewer) = self.comments_viewer {
                        // Check if click is outside popup area - close it
                        if viewer.is_outside_popup_area(click_x, click_y) {
                            viewer.save_position();
                            self.comments_viewer = None;
                            self.close_popup_to_previous();
                            return;
                        }

                        let click_type = self
                            .mouse_tracker
                            .detect_click_type(mouse_event.column, mouse_event.row);

                        match click_type {
                            ClickType::Single | ClickType::Triple => {
                                viewer.handle_mouse_click(mouse_event.column, mouse_event.row);
                            }
                            ClickType::Double => {
                                if viewer.handle_mouse_click(mouse_event.column, mouse_event.row) {
                                    if let Some(entry) = viewer.selected_comment() {
                                        let chapter_href = entry.chapter_href.clone();
                                        let node_index = entry.primary_comment().node_index();
                                        viewer.save_position();
                                        self.comments_viewer = None;
                                        self.close_popup_to_previous();
                                        self.set_main_panel_focus(MainPanel::Content);

                                        // Ensure the reader restores to the correct node after navigation
                                        self.text_reader.restore_to_node_index(node_index);

                                        if let Err(e) =
                                            self.navigate_to_chapter_by_href(&chapter_href)
                                        {
                                            error!(
                                                "Failed to navigate to chapter {chapter_href}: {e}"
                                            );
                                            self.show_error(format!(
                                                "Failed to navigate to comment: {e}"
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return;
                }

                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ThemeSelector)
                ) {
                    let click_x = mouse_event.column;
                    let click_y = mouse_event.row;

                    if let Some(ref mut selector) = self.theme_selector {
                        // Check if click is outside popup area - close it
                        if selector.is_outside_popup_area(click_x, click_y) {
                            self.theme_selector = None;
                            self.close_popup_to_previous();
                            return;
                        }

                        let click_type = self
                            .mouse_tracker
                            .detect_click_type(mouse_event.column, mouse_event.row);

                        match click_type {
                            ClickType::Single | ClickType::Triple => {
                                selector.handle_mouse_click(mouse_event.column, mouse_event.row);
                            }
                            ClickType::Double => {
                                if selector.handle_mouse_click(mouse_event.column, mouse_event.row)
                                {
                                    // Apply theme on double-click
                                    if let Some(action) = selector.handle_key(
                                        crossterm::event::KeyEvent::new(
                                            crossterm::event::KeyCode::Enter,
                                            crossterm::event::KeyModifiers::NONE,
                                        ),
                                        &mut self.key_sequence,
                                    ) {
                                        match action {
                                            ThemeSelectorAction::ThemeChanged => {
                                                self.text_reader.invalidate_render_cache();
                                                self.show_info(format!(
                                                    "Theme: {}",
                                                    current_theme_name()
                                                ));
                                                self.theme_selector = None;
                                                self.close_popup_to_previous();
                                            }
                                            ThemeSelectorAction::Close => {
                                                self.theme_selector = None;
                                                self.close_popup_to_previous();
                                            }
                                        }
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
                // Block mouse up events for all popups
                if self.has_active_popup() {
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
                // Block drag events for all popups
                if self.has_active_popup() {
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
            let without_fragment = if let Some(fragment_pos) = normalized.find('#') {
                &normalized[..fragment_pos]
            } else {
                normalized
            };

            // URL-decode percent-encoded characters (e.g., %27 -> ')
            percent_decode(without_fragment)
        }

        let book = self.current_book.as_ref()?;

        let normalized_href = normalize_href(href);

        for (index, spine_item) in book.epub.spine.iter().enumerate() {
            if let Some(resource) = book.epub.resources.get(&spine_item.idref) {
                let path_str = resource.path.to_string_lossy();
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

        // Save current main panel before opening image popup
        if let FocusedPanel::Main(panel) = self.focused_panel {
            self.previous_main_panel = panel;
        }

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

        if matches!(self.focused_panel, FocusedPanel::Popup(PopupWindow::Help)) {
            if let Some(ref mut help_popup) = self.help_popup {
                if scroll_amount > 0 {
                    for _ in 0..scroll_amount.min(10) {
                        help_popup.scroll_down();
                    }
                } else {
                    for _ in 0..(-scroll_amount).min(10) {
                        help_popup.scroll_up();
                    }
                }
            }
            return;
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::BookStats)
        ) {
            if scroll_amount > 0 {
                for _ in 0..scroll_amount.min(10) {
                    self.book_stat.handle_j();
                }
            } else {
                for _ in 0..(-scroll_amount).min(10) {
                    self.book_stat.handle_k();
                }
            }
            return;
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::ReadingHistory)
        ) {
            if let Some(ref mut history) = self.reading_history {
                if scroll_amount > 0 {
                    for _ in 0..scroll_amount.min(10) {
                        history.handle_j();
                    }
                } else {
                    for _ in 0..(-scroll_amount).min(10) {
                        history.handle_k();
                    }
                }
            }
            return;
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::CommentsViewer)
        ) {
            if let Some(ref mut viewer) = self.comments_viewer {
                if !viewer.handle_mouse_scroll(column, scroll_amount) {
                    if scroll_amount > 0 {
                        for _ in 0..scroll_amount.min(10) {
                            viewer.handle_j();
                        }
                    } else {
                        for _ in 0..(-scroll_amount).min(10) {
                            viewer.handle_k();
                        }
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

    pub fn open_with_system_viewer(&mut self) {
        if let Some(book) = &self.current_book {
            match self
                .system_command_executor
                .open_file_at_chapter(&book.file, book.current_chapter())
            {
                Ok(_) => {
                    info!(
                        "Successfully opened EPUB with system viewer at chapter {}",
                        book.current_chapter()
                    );
                    self.show_info("Opened in external viewer");
                }
                Err(e) => {
                    error!("Failed to open EPUB with system viewer: {e}");
                    self.show_error(format!("Failed to open in external viewer: {e}"));
                }
            }
        } else {
            error!("No EPUB file currently loaded");
            self.show_error("No EPUB file currently loaded");
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
                self.show_error(format!("Failed to jump back: {e}"));
            }
        }
    }

    /// Handle Ctrl+I - jump forward in history
    fn jump_forward(&mut self) {
        if let Some(location) = self.jump_list.jump_forward() {
            if let Err(e) = self.jump_to_location(location) {
                error!("Failed to jump forward: {e}");
                self.show_error(format!("Failed to jump forward: {e}"));
            }
        }
    }

    /// Calculate the navigation panel width based on stored terminal width
    fn nav_panel_width(&self) -> u16 {
        if self.zen_mode {
            0
        } else {
            // 30% of terminal width, minimum 20 columns
            ((self.terminal_size.width * 30) / 100).max(20)
        }
    }

    /// Get the navigation panel area based on current terminal size
    fn get_navigation_panel_area(&self) -> Rect {
        if self.zen_mode {
            return Rect::default(); // No navigation panel in zen mode
        }
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
                    self.show_error(format!("Failed to open book: {e}"));
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
                            self.show_error("Chapter not found in book");
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
                                self.show_error("Section not found in book");
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

        let background_block = Block::default().style(Style::default().bg(current_theme().base_00));
        f.render_widget(background_block, f.area());

        if self.zen_mode {
            // Zen mode: full screen content, no navigation panel or help bar
            if let Some(ref book) = self.current_book {
                self.text_reader.render(
                    f,
                    f.area(),
                    book.current_chapter(),
                    book.total_chapters(),
                    current_theme(),
                    true, // always focused in zen mode
                );
            } else {
                self.render_default_content(f, f.area(), "Select a file to view its content");
            }
            // Don't set help_bar_area in zen mode - it's hidden
        } else {
            // Normal mode: existing layout
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
                current_theme(),
                &self.book_manager,
            );

            if let Some(ref book) = self.current_book {
                self.text_reader.render(
                    f,
                    main_chunks[1],
                    book.current_chapter(),
                    book.total_chapters(),
                    current_theme(),
                    self.is_main_panel(MainPanel::Content),
                );
            } else {
                self.render_default_content(f, main_chunks[1], "Select a file to view its content");
            }

            self.render_help_bar(f, chunks[1], fps_counter);
            self.help_bar_area = chunks[1];
        }

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
                book_search.render(f, f.area(), current_theme());
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

        if matches!(self.focused_panel, FocusedPanel::Popup(PopupWindow::Help)) {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10))
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            if let Some(ref mut help_popup) = self.help_popup {
                help_popup.render(f, f.area());
            }
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::CommentsViewer)
        ) {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10))
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            if let Some(ref mut comments_viewer) = self.comments_viewer {
                comments_viewer.render(f, f.area());
            }
        }

        if matches!(
            self.focused_panel,
            FocusedPanel::Popup(PopupWindow::ThemeSelector)
        ) {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10))
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            if let Some(ref mut theme_selector) = self.theme_selector {
                theme_selector.render(f, f.area());
            }
        }
    }

    fn render_default_content(&self, f: &mut ratatui::Frame, area: Rect, content: &str) {
        // Use focus-aware colors instead of hardcoded false
        let (text_color, border_color, _bg_color) =
            current_theme().get_panel_colors(self.is_main_panel(MainPanel::Content));

        let content_border = Block::default()
            .borders(Borders::ALL)
            .title("Content")
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(current_theme().base_00));

        let paragraph = Paragraph::new(content)
            .block(content_border)
            .style(Style::default().fg(text_color).bg(current_theme().base_00));

        f.render_widget(paragraph, area);
    }

    fn handle_help_bar_click(&mut self, click_x: u16, click_y: u16) -> bool {
        let area = self.help_bar_area;
        if click_y < area.y || click_y >= area.y + area.height {
            return false;
        }
        if click_x < area.x || click_x >= area.x + area.width {
            return false;
        }

        let inner_x = click_x.saturating_sub(area.x + 1);
        let inner_y = click_y.saturating_sub(area.y + 1);

        if inner_y != 0 {
            return false;
        }

        let width = area.width.saturating_sub(2);
        let comments_full = "[Space+a: Comments] ";
        let history_full = "[Space+h: History] ";
        let stats_full = "[Space+d: Stats] ";
        let theme_full = "[Space+t: Theme] ";
        let help_full = "[?: Help]";

        let total_len = (comments_full.len()
            + history_full.len()
            + stats_full.len()
            + theme_full.len()
            + help_full.len()) as u16;
        let section_start = width.saturating_sub(total_len);

        let comments_start = section_start + "[".len() as u16;
        let comments_end = comments_start + "Space+a: Comments".len() as u16;
        let history_start = section_start + comments_full.len() as u16 + "[".len() as u16;
        let history_end = history_start + "Space+h: History".len() as u16;
        let stats_start = section_start
            + comments_full.len() as u16
            + history_full.len() as u16
            + "[".len() as u16;
        let stats_end = stats_start + "Space+d: Stats".len() as u16;
        let theme_start = section_start
            + comments_full.len() as u16
            + history_full.len() as u16
            + stats_full.len() as u16
            + "[".len() as u16;
        let theme_end = theme_start + "Space+t: Theme".len() as u16;
        let help_start = section_start
            + comments_full.len() as u16
            + history_full.len() as u16
            + stats_full.len() as u16
            + theme_full.len() as u16
            + "[".len() as u16;
        let help_end = help_start + "?: Help".len() as u16;

        if inner_x >= comments_start && inner_x < comments_end {
            if self.current_book.is_some() {
                if let FocusedPanel::Main(panel) = self.focused_panel {
                    self.previous_main_panel = panel;
                }
                if let Some(ref mut book) = self.current_book {
                    let toc_items = self.navigation_panel.get_toc_items();
                    let current_chapter_href =
                        Self::get_chapter_href(&book.epub, book.current_chapter());
                    let mut viewer = crate::widget::comments_viewer::CommentsViewer::new(
                        self.text_reader.get_comments(),
                        &mut book.epub,
                        &toc_items,
                        current_chapter_href,
                    );
                    viewer.restore_position();
                    self.comments_viewer = Some(viewer);
                    self.focused_panel = FocusedPanel::Popup(PopupWindow::CommentsViewer);
                }
            }
            return true;
        }

        if inner_x >= history_start && inner_x < history_end {
            if let FocusedPanel::Main(panel) = self.focused_panel {
                self.previous_main_panel = panel;
            }
            self.reading_history = Some(ReadingHistory::new(&self.bookmarks));
            self.focused_panel = FocusedPanel::Popup(PopupWindow::ReadingHistory);
            return true;
        }

        if inner_x >= stats_start && inner_x < stats_end {
            if let Some(ref mut book) = self.current_book {
                let terminal_size = (self.terminal_size.width, self.terminal_size.height);
                if let Err(e) = self
                    .book_stat
                    .calculate_stats(&mut book.epub, terminal_size)
                {
                    error!("Failed to calculate book statistics: {e}");
                    self.show_error(format!("Failed to calculate statistics: {e}"));
                } else {
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    self.book_stat.show();
                    self.focused_panel = FocusedPanel::Popup(PopupWindow::BookStats);
                }
            }
            return true;
        }

        if inner_x >= theme_start && inner_x < theme_end {
            if let FocusedPanel::Main(panel) = self.focused_panel {
                self.previous_main_panel = panel;
            }
            self.theme_selector = Some(ThemeSelector::new());
            self.focused_panel = FocusedPanel::Popup(PopupWindow::ThemeSelector);
            return true;
        }

        if inner_x >= help_start && inner_x < help_end {
            if let FocusedPanel::Main(panel) = self.focused_panel {
                self.previous_main_panel = panel;
            }
            self.help_popup = Some(HelpPopup::new());
            self.focused_panel = FocusedPanel::Popup(PopupWindow::Help);
            return true;
        }

        false
    }

    fn render_help_bar(&self, f: &mut ratatui::Frame, area: Rect, fps_counter: &FPSCounter) {
        use crate::notification::NotificationLevel;
        let (_, _, border_color, _, _) = current_theme().get_interface_colors(false);

        let help_content = if let Some(notification) = self.notifications.get_current() {
            let level_str = match notification.level {
                NotificationLevel::Info => "INFO",
                NotificationLevel::Warning => "WARNING",
                NotificationLevel::Error => "ERROR",
            };
            format!("[{}] {} | ESC: Dismiss", level_str, notification.message)
        } else if self.is_in_search_mode() {
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
                    "j/k: Navigate | Enter: Select | h/l: Fold/Unfold | H/L: Fold/Unfold All | Tab: Switch | q: Quit"
                }
                FocusedPanel::Main(MainPanel::Content) => {
                    "j/k: Scroll | h/l: Chapter | Ctrl+d/u: Half-screen | Tab: Switch | Space+o: Open | q: Quit"
                }
                FocusedPanel::Popup(PopupWindow::ReadingHistory) => {
                    "j/k/Scroll: Navigate | Enter/DblClick: Open | ESC: Close"
                }
                FocusedPanel::Popup(PopupWindow::BookStats) => {
                    "j/k/Ctrl+d/u/Scroll: Scroll | Enter/DblClick: Jump | ESC: Close"
                }
                FocusedPanel::Popup(PopupWindow::ImagePopup) => "ESC/Any key: Close",
                FocusedPanel::Popup(PopupWindow::BookSearch) => {
                    "Space+f: Reopen | Space+F: New Search"
                }
                FocusedPanel::Popup(PopupWindow::Help) => {
                    "j/k/Ctrl+d/u: Scroll | gg/G: Top/Bottom | ESC/?: Close"
                }
                FocusedPanel::Popup(PopupWindow::CommentsViewer) => {
                    "j/k/Ctrl+d/u: Scroll | /: Search | Enter/DblClick: Jump | ESC: Close"
                }
                FocusedPanel::Popup(PopupWindow::ThemeSelector) => {
                    "j/k: Navigate | Enter: Apply | ESC: Close"
                }
            };
            help_text.to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(current_theme().base_00));

        let inner_area = block.inner(area);
        f.render_widget(block, area);

        let left_content = if self.is_profiling() {
            format!("{} | FPS: {}", help_content, fps_counter.current_fps)
        } else {
            help_content
        };
        let left_para = Paragraph::new(left_content).style(
            Style::default()
                .fg(current_theme().base_03)
                .bg(current_theme().base_00),
        );
        f.render_widget(left_para, inner_area);

        let text_color = current_theme().base_03;
        let right_content = Line::from(vec![
            Span::raw("["),
            Span::styled(
                "Space+a: Comments",
                Style::default()
                    .fg(text_color)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw("] "),
            Span::raw("["),
            Span::styled(
                "Space+h: History",
                Style::default()
                    .fg(text_color)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw("] "),
            Span::raw("["),
            Span::styled(
                "Space+d: Stats",
                Style::default()
                    .fg(text_color)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw("] "),
            Span::raw("["),
            Span::styled(
                "Space+t: Theme",
                Style::default()
                    .fg(text_color)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw("] "),
            Span::raw("["),
            Span::styled(
                "?: Help",
                Style::default()
                    .fg(text_color)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw("]"),
        ]);

        let right_para = Paragraph::new(right_content)
            .alignment(Alignment::Right)
            .style(Style::default().bg(current_theme().base_00));
        f.render_widget(right_para, inner_area);
    }

    fn toggle_zen_mode(&mut self) {
        // Save current content position before toggling zen mode
        let current_node = self.text_reader.get_current_node_index();
        self.zen_mode = !self.zen_mode;
        // Restore position after width change causes re-render
        self.text_reader.restore_to_node_index(current_node);
        // When entering zen mode while on NavigationList, switch to Content
        if self.zen_mode
            && self.is_main_panel(MainPanel::NavigationList)
            && self.current_book.is_some()
        {
            self.set_main_panel_focus(MainPanel::Content);
        }
    }

    pub fn set_zen_mode(&mut self, enabled: bool) {
        if self.zen_mode != enabled {
            self.toggle_zen_mode();
        }
    }

    /// Check if a key is a global hotkey that should work regardless of focus
    /// Returns true if the key was handled as a global hotkey
    fn handle_global_hotkeys(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('?') => {
                // Save current main panel before opening help
                if let FocusedPanel::Main(panel) = self.focused_panel {
                    self.previous_main_panel = panel;
                }
                self.help_popup = Some(HelpPopup::new());
                self.focused_panel = FocusedPanel::Popup(PopupWindow::Help);
                true
            }
            KeyCode::Char(' ') => {
                self.key_sequence.handle_key(' ');
                true
            }
            KeyCode::Char(c) if self.key_sequence.current_sequence() == " " => {
                // Handle space + key combinations (global across all panels)
                self.handle_key_sequence(c)
            }
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_zen_mode();
                true
            }
            _ => false,
        }
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
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    self.open_book_search(false); // Don't clear input
                }
                self.key_sequence.clear();
                true
            }
            " F" => {
                // Handle Space->F to open book search (clear input)
                if self.current_book.is_some() {
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
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
                            self.show_error(format!("Failed to calculate statistics: {e}"));
                        } else {
                            if let FocusedPanel::Main(panel) = self.focused_panel {
                                self.previous_main_panel = panel;
                            }
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
                // Handle Space->o to open current EPUB with system viewer (global)
                self.open_with_system_viewer();
                self.key_sequence.clear();
                true
            }
            " h" => {
                // Handle Space->h to toggle reading history (global)
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ReadingHistory)
                ) {
                    // Close history - return to previous panel
                    self.close_popup_to_previous();
                    self.reading_history = None;
                } else {
                    // Open history - save current main panel
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    self.reading_history = Some(ReadingHistory::new(&self.bookmarks));
                    self.focused_panel = FocusedPanel::Popup(PopupWindow::ReadingHistory);
                }
                self.key_sequence.clear();
                true
            }
            " a" => {
                // Handle Space->a to toggle comments viewer (global)
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::CommentsViewer)
                ) {
                    // Close comments viewer - return to previous panel
                    if let Some(ref mut viewer) = self.comments_viewer {
                        viewer.save_position();
                    }
                    self.close_popup_to_previous();
                    self.comments_viewer = None;
                } else if self.current_book.is_some() {
                    // Open comments viewer - save current main panel
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    if let Some(ref mut book) = self.current_book {
                        let toc_items = self.navigation_panel.get_toc_items();
                        let current_chapter_href =
                            Self::get_chapter_href(&book.epub, book.current_chapter());
                        let mut viewer = crate::widget::comments_viewer::CommentsViewer::new(
                            self.text_reader.get_comments(),
                            &mut book.epub,
                            &toc_items,
                            current_chapter_href,
                        );
                        viewer.restore_position();
                        self.comments_viewer = Some(viewer);
                        self.focused_panel = FocusedPanel::Popup(PopupWindow::CommentsViewer);
                    }
                }
                self.key_sequence.clear();
                true
            }
            " t" => {
                // Handle Space->t to toggle theme selector (global)
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::ThemeSelector)
                ) {
                    // Close theme selector - return to previous panel
                    self.close_popup_to_previous();
                    self.theme_selector = None;
                } else {
                    // Open theme selector - save current main panel
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    self.theme_selector = Some(ThemeSelector::new());
                    self.focused_panel = FocusedPanel::Popup(PopupWindow::ThemeSelector);
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

    fn handle_pending_find_motion(&mut self, key: &crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        if self.text_reader.has_pending_motion() {
            if let KeyCode::Char(ch) = key.code {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.execute_pending_find(ch);
                }
            } else {
                self.text_reader.clear_pending_motion();
                self.text_reader.clear_count();
            }
            return true;
        }

        false
    }

    fn handle_normal_mode_count_prefix(&mut self, key: &crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        if let KeyCode::Char(ch) = key.code {
            if ch.is_ascii_digit() && (ch != '0' || self.text_reader.has_pending_count()) {
                return self.text_reader.append_count_digit(ch);
            }
        }

        false
    }

    fn handle_common_normal_mode_motions(
        &mut self,
        key: &crossterm::event::KeyEvent,
        screen_height: Option<usize>,
    ) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_left();
                }
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_down();
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_up();
                }
                true
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_right();
                }
                true
            }
            KeyCode::Char('w') => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_word_forward();
                }
                true
            }
            KeyCode::Char('W') => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_big_word_forward();
                }
                true
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_word_end();
                }
                true
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_word_backward();
                }
                true
            }
            KeyCode::Char('0') => {
                self.text_reader.clear_count();
                self.text_reader.normal_mode_line_start();
                true
            }
            KeyCode::Char('^') => {
                self.text_reader.clear_count();
                self.text_reader.normal_mode_first_non_whitespace();
                true
            }
            KeyCode::Char('$') => {
                self.text_reader.clear_count();
                self.text_reader.normal_mode_line_end();
                true
            }
            KeyCode::Char('{') => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_paragraph_up();
                }
                true
            }
            KeyCode::Char('}') => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.normal_mode_paragraph_down();
                }
                true
            }
            KeyCode::Char('g') => {
                let seq = self.key_sequence.handle_key('g');
                if seq == "gg" {
                    self.text_reader.clear_count();
                    self.text_reader.normal_mode_document_top();
                    self.key_sequence.clear();
                }
                true
            }
            KeyCode::Char('G') => {
                self.text_reader.clear_count();
                self.text_reader.normal_mode_document_bottom();
                true
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text_reader.clear_count();
                if let Some(h) = screen_height {
                    self.text_reader.normal_mode_half_page_down(h);
                }
                true
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text_reader.clear_count();
                if let Some(h) = screen_height {
                    self.text_reader.normal_mode_half_page_up(h);
                }
                true
            }
            KeyCode::Char('f') => {
                self.text_reader.set_pending_find_forward();
                true
            }
            KeyCode::Char('F') => {
                self.text_reader.set_pending_find_backward();
                true
            }
            KeyCode::Char('t') => {
                self.text_reader.set_pending_till_forward();
                true
            }
            KeyCode::Char('T') => {
                self.text_reader.set_pending_till_backward();
                true
            }
            KeyCode::Char(';') => {
                let count = self.text_reader.take_count();
                for _ in 0..count {
                    self.text_reader.repeat_last_find();
                }
                true
            }
            _ => false,
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
            self.close_popup_to_previous();
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
                        node_index,
                        line_number: _,
                        query,
                    } => {
                        self.set_main_panel_focus(MainPanel::Content);
                        if let Err(e) = self.navigate_to_chapter(chapter_index) {
                            error!("Failed to navigate to chapter {chapter_index}: {e}");
                            self.show_error(format!("Failed to navigate to chapter: {e}"));
                        } else {
                            self.text_reader.restore_to_node_index(node_index);
                            self.text_reader
                                .queue_global_search_activation(query, node_index);
                        }
                    }
                    BookSearchAction::Close => {
                        self.close_popup_to_previous();
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
                    self.close_popup_to_previous();
                }
                Some(BookStatAction::JumpToChapter { chapter_index }) => {
                    self.book_stat.hide();
                    self.set_main_panel_focus(MainPanel::Content);
                    if let Err(e) = self.navigate_to_chapter(chapter_index) {
                        error!("Failed to navigate to chapter {chapter_index}: {e}");
                        self.show_error(format!("Failed to navigate to chapter: {e}"));
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
                        self.close_popup_to_previous();
                        self.reading_history = None;
                    }
                    ReadingHistoryAction::OpenBook { path } => {
                        if let Some(book_index) = self.book_manager.find_book_index_by_path(&path) {
                            self.set_main_panel_focus(MainPanel::Content);
                            self.reading_history = None;
                            let _ = self.open_book_for_reading(book_index);
                        }
                    }
                }
            }
            return None;
        }

        // If help popup is shown, handle keys for it
        if self.focused_panel == FocusedPanel::Popup(PopupWindow::Help) {
            let action = if let Some(ref mut help) = self.help_popup {
                help.handle_key(key, &mut self.key_sequence)
            } else {
                None
            };

            if let Some(HelpPopupAction::Close) = action {
                self.close_popup_to_previous();
                self.help_popup = None;
            }
            return None;
        }

        // If comments viewer popup is shown, handle keys for it
        if self.focused_panel == FocusedPanel::Popup(PopupWindow::CommentsViewer) {
            let action = if let Some(ref mut viewer) = self.comments_viewer {
                viewer.handle_key(key, &mut self.key_sequence)
            } else {
                None
            };

            if let Some(action) = action {
                use crate::widget::comments_viewer::CommentsViewerAction;
                match action {
                    CommentsViewerAction::Close => {
                        if let Some(ref mut viewer) = self.comments_viewer {
                            viewer.save_position();
                        }
                        self.close_popup_to_previous();
                        self.comments_viewer = None;
                    }
                    CommentsViewerAction::JumpToComment {
                        chapter_href,
                        target,
                    } => {
                        if let Some(ref mut viewer) = self.comments_viewer {
                            viewer.save_position();
                        }
                        self.close_popup_to_previous();
                        self.set_main_panel_focus(MainPanel::Content);

                        // Set pending node restore before navigating
                        self.text_reader.restore_to_node_index(target.node_index());

                        if let Err(e) = self.navigate_to_chapter_by_href(&chapter_href) {
                            error!("Failed to navigate to chapter {chapter_href}: {e}");
                            self.show_error(format!("Failed to navigate to comment: {e}"));
                        }
                    }
                    CommentsViewerAction::DeleteSelectedComment => {
                        if let Some(entry) = self
                            .comments_viewer
                            .as_ref()
                            .and_then(|v| v.selected_comment().cloned())
                        {
                            let mut delete_success = false;
                            let comments = self.text_reader.get_comments();
                            match comments.lock() {
                                Ok(mut guard) => {
                                    for comment in &entry.comments {
                                        if let Err(e) = guard.delete_comment_by_id(&comment.id) {
                                            error!("Failed to delete comment: {e}");
                                            self.show_error(format!(
                                                "Failed to delete comment: {e}"
                                            ));
                                            delete_success = false;
                                            break;
                                        }
                                        delete_success = true;
                                    }
                                }
                                Err(_) => {
                                    error!("Failed to lock comments for deletion");
                                    self.show_error("Failed to delete comment: lock error");
                                }
                            }

                            if delete_success {
                                for comment in &entry.comments {
                                    self.text_reader.delete_comment_by_id(&comment.id);
                                }
                                if let Some(ref mut viewer) = self.comments_viewer {
                                    viewer.remove_selected_comment();
                                }
                                let msg = if entry.comments.len() > 1 {
                                    "Comments deleted"
                                } else {
                                    "Comment deleted"
                                };
                                self.show_info(msg);
                            }
                        }
                    }
                }
            }
            return None;
        }

        // If theme selector popup is shown, handle keys for it
        if self.focused_panel == FocusedPanel::Popup(PopupWindow::ThemeSelector) {
            let action = if let Some(ref mut selector) = self.theme_selector {
                selector.handle_key(key, &mut self.key_sequence)
            } else {
                None
            };

            if let Some(action) = action {
                match action {
                    ThemeSelectorAction::Close => {
                        self.close_popup_to_previous();
                        self.theme_selector = None;
                    }
                    ThemeSelectorAction::ThemeChanged => {
                        self.text_reader.invalidate_render_cache();
                        self.show_info(format!("Theme: {}", current_theme_name()));
                        self.close_popup_to_previous();
                        self.theme_selector = None;
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
            // Check for global hotkeys first
            if self.handle_global_hotkeys(key) {
                return None;
            }

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
                        if let Err(e) = self.open_book_for_reading(book_index) {
                            self.show_error(format!("Failed to open book: {e}"));
                        }
                    }
                    NavigationPanelAction::SwitchToBookList => {
                        self.switch_to_book_list_mode();
                    }
                    NavigationPanelAction::NavigateToChapter { href, anchor } => {
                        if let Some(chapter_index) = self.find_spine_index_by_href(&href) {
                            let _ = self.navigate_to_chapter(chapter_index);
                            let nav_area = self.get_navigation_panel_area();
                            let toc_height = nav_area.height as usize;
                            let anchor_ref = anchor.as_deref();
                            self.navigation_panel
                                .table_of_contents
                                .set_active_from_hint(&href, anchor_ref, Some(toc_height));

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

        // Handle vim normal mode keys when active
        if self.is_main_panel(MainPanel::Content) && self.text_reader.is_normal_mode_active() {
            // Clear expired yank highlight
            self.text_reader.clear_expired_yank_highlight();

            // Check for pending f/F motion first
            if self.handle_pending_find_motion(&key) {
                return None;
            }

            // Check for pending yank
            if self.text_reader.has_pending_yank() {
                use crate::markdown_text_reader::{PendingCharMotion, PendingYank};
                let pending = self.text_reader.get_pending_yank();

                match pending {
                    PendingYank::WaitingForMotion => {
                        if let KeyCode::Char(ch) = key.code {
                            // Check for sub-state transitions first (don't consume count)
                            match ch {
                                'g' => {
                                    self.text_reader.set_pending_yank(PendingYank::WaitingForG);
                                    return None;
                                }
                                'i' => {
                                    self.text_reader
                                        .set_pending_yank(PendingYank::WaitingForInnerObject);
                                    return None;
                                }
                                'a' => {
                                    self.text_reader
                                        .set_pending_yank(PendingYank::WaitingForAroundObject);
                                    return None;
                                }
                                'f' => {
                                    self.text_reader.set_pending_yank(
                                        PendingYank::WaitingForFindChar(
                                            PendingCharMotion::FindForward,
                                        ),
                                    );
                                    return None;
                                }
                                'F' => {
                                    self.text_reader.set_pending_yank(
                                        PendingYank::WaitingForFindChar(
                                            PendingCharMotion::FindBackward,
                                        ),
                                    );
                                    return None;
                                }
                                't' => {
                                    self.text_reader.set_pending_yank(
                                        PendingYank::WaitingForFindChar(
                                            PendingCharMotion::TillForward,
                                        ),
                                    );
                                    return None;
                                }
                                'T' => {
                                    self.text_reader.set_pending_yank(
                                        PendingYank::WaitingForFindChar(
                                            PendingCharMotion::TillBackward,
                                        ),
                                    );
                                    return None;
                                }
                                _ => {}
                            }
                            // Now consume count for actual yank operations
                            let count = self.text_reader.take_count();
                            let yanked = match ch {
                                'y' => self.text_reader.yank_line(count),
                                'w' => self.text_reader.yank_word_forward(count),
                                'W' => self.text_reader.yank_big_word_forward(count),
                                'e' => self.text_reader.yank_word_end(count),
                                'b' => self.text_reader.yank_word_backward(count),
                                '$' => self.text_reader.yank_to_line_end(),
                                '0' => self.text_reader.yank_to_line_start(),
                                '^' => self.text_reader.yank_to_first_non_whitespace(),
                                '{' => self.text_reader.yank_paragraph_up(count),
                                '}' => self.text_reader.yank_paragraph_down(count),
                                'G' => self.text_reader.yank_to_document_bottom(),
                                _ => {
                                    self.text_reader.clear_pending_yank();
                                    self.text_reader.clear_count();
                                    None
                                }
                            };
                            if let Some(text) = yanked {
                                let _ = self.text_reader.copy_to_clipboard(text);
                            }
                        } else {
                            self.text_reader.clear_pending_yank();
                            self.text_reader.clear_count();
                        }
                        return None;
                    }
                    PendingYank::WaitingForG => {
                        self.text_reader.clear_count();
                        if let KeyCode::Char('g') = key.code {
                            if let Some(text) = self.text_reader.yank_to_document_top() {
                                let _ = self.text_reader.copy_to_clipboard(text);
                            }
                        }
                        self.text_reader.clear_pending_yank();
                        return None;
                    }
                    PendingYank::WaitingForInnerObject => {
                        if let KeyCode::Char(ch) = key.code {
                            let count = self.text_reader.take_count();
                            let yanked = match ch {
                                'w' => self.text_reader.yank_inner_word(),
                                'W' => self.text_reader.yank_inner_big_word(),
                                'p' => self.text_reader.yank_inner_paragraph(count),
                                '"' => self.text_reader.yank_inner_quotes('"'),
                                '\'' => self.text_reader.yank_inner_quotes('\''),
                                '`' => self.text_reader.yank_inner_quotes('`'),
                                '(' | ')' => self.text_reader.yank_inner_brackets('(', ')'),
                                '[' | ']' => self.text_reader.yank_inner_brackets('[', ']'),
                                '{' | '}' => self.text_reader.yank_inner_brackets('{', '}'),
                                '<' | '>' => self.text_reader.yank_inner_brackets('<', '>'),
                                _ => None,
                            };
                            if let Some(text) = yanked {
                                let _ = self.text_reader.copy_to_clipboard(text);
                            }
                        }
                        self.text_reader.clear_pending_yank();
                        return None;
                    }
                    PendingYank::WaitingForAroundObject => {
                        if let KeyCode::Char(ch) = key.code {
                            let count = self.text_reader.take_count();
                            let yanked = match ch {
                                'w' => self.text_reader.yank_a_word(),
                                'W' => self.text_reader.yank_a_big_word(),
                                'p' => self.text_reader.yank_a_paragraph(count),
                                '"' => self.text_reader.yank_around_quotes('"'),
                                '\'' => self.text_reader.yank_around_quotes('\''),
                                '`' => self.text_reader.yank_around_quotes('`'),
                                '(' | ')' => self.text_reader.yank_around_brackets('(', ')'),
                                '[' | ']' => self.text_reader.yank_around_brackets('[', ']'),
                                '{' | '}' => self.text_reader.yank_around_brackets('{', '}'),
                                '<' | '>' => self.text_reader.yank_around_brackets('<', '>'),
                                _ => None,
                            };
                            if let Some(text) = yanked {
                                let _ = self.text_reader.copy_to_clipboard(text);
                            }
                        }
                        self.text_reader.clear_pending_yank();
                        return None;
                    }
                    PendingYank::WaitingForFindChar(motion) => {
                        if let KeyCode::Char(ch) = key.code {
                            let count = self.text_reader.take_count();
                            if let Some(text) = self
                                .text_reader
                                .yank_find_char_with_count(motion, ch, count)
                            {
                                let _ = self.text_reader.copy_to_clipboard(text);
                            }
                        } else {
                            self.text_reader.clear_count();
                        }
                        self.text_reader.clear_pending_yank();
                        return None;
                    }
                    PendingYank::None => {}
                }
            }

            // Handle visual mode keys when active
            if self.text_reader.is_visual_mode_active() {
                use crate::markdown_text_reader::VisualMode;
                let visual_mode = self.text_reader.get_visual_mode();

                match key.code {
                    KeyCode::Char('y') => {
                        if let Some(text) = self.text_reader.yank_visual_selection() {
                            let _ = self.text_reader.copy_to_clipboard(text);
                        }
                        self.text_reader.clear_count();
                        return None;
                    }
                    KeyCode::Char('a') => {
                        // Add annotation on visual selection
                        if self.text_reader.start_comment_input() {
                            debug!("Started comment input mode from visual selection");
                        }
                        self.text_reader.clear_count();
                        return None;
                    }
                    KeyCode::Char('d') => {
                        // Delete annotation on visual selection
                        match self.text_reader.delete_comment_at_cursor() {
                            Ok(true) => {
                                info!("Comment deleted successfully");
                                self.show_info("Comment deleted");
                            }
                            Ok(false) => {
                                // Selection not on a comment, ignore
                            }
                            Err(e) => {
                                error!("Failed to delete comment: {e}");
                                self.show_error(format!("Failed to delete comment: {e}"));
                            }
                        }
                        self.text_reader.clear_count();
                        return None;
                    }
                    KeyCode::Esc => {
                        self.text_reader.exit_visual_mode();
                        self.text_reader.clear_count();
                        return None;
                    }
                    KeyCode::Char('v') if visual_mode == VisualMode::CharacterWise => {
                        self.text_reader.exit_visual_mode();
                        self.text_reader.clear_count();
                        return None;
                    }
                    KeyCode::Char('V') if visual_mode == VisualMode::LineWise => {
                        self.text_reader.exit_visual_mode();
                        self.text_reader.clear_count();
                        return None;
                    }
                    KeyCode::Char('v') if visual_mode == VisualMode::LineWise => {
                        self.text_reader
                            .enter_visual_mode(VisualMode::CharacterWise);
                        self.text_reader.clear_count();
                        return None;
                    }
                    KeyCode::Char('V') if visual_mode == VisualMode::CharacterWise => {
                        self.text_reader.enter_visual_mode(VisualMode::LineWise);
                        self.text_reader.clear_count();
                        return None;
                    }
                    _ => {
                        if self.handle_common_normal_mode_motions(&key, screen_height) {
                            return None;
                        }
                        if self.handle_normal_mode_count_prefix(&key) {
                            return None;
                        }
                    }
                }
                return None;
            }

            // Handle digit input for count prefix (1-9, or 0 if count already started)
            if self.handle_normal_mode_count_prefix(&key) {
                return None;
            }

            if self.handle_common_normal_mode_motions(&key, screen_height) {
                return None;
            }

            match key.code {
                KeyCode::Char('v') => {
                    use crate::markdown_text_reader::VisualMode;
                    self.text_reader
                        .enter_visual_mode(VisualMode::CharacterWise);
                    self.text_reader.clear_count();
                    return None;
                }
                KeyCode::Char('V') => {
                    use crate::markdown_text_reader::VisualMode;
                    self.text_reader.enter_visual_mode(VisualMode::LineWise);
                    self.text_reader.clear_count();
                    return None;
                }
                KeyCode::Char('y') => {
                    self.text_reader.start_yank();
                    return None;
                }
                KeyCode::Char('n') => {
                    // If in search navigation mode, 'n' goes to next match
                    // Otherwise, toggle normal mode off
                    if self.text_reader.is_searching() {
                        let search_state = self.text_reader.get_search_state();
                        if search_state.mode == SearchMode::NavigationMode {
                            self.text_reader.next_match();
                            return None;
                        }
                    }
                    self.text_reader.clear_count();
                    self.text_reader.toggle_normal_mode();
                    return None;
                }
                KeyCode::Esc => {
                    self.text_reader.clear_count();
                    self.text_reader.toggle_normal_mode();
                    return None;
                }
                _ => {}
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
            KeyCode::Char('n') if !self.is_in_search_mode() => {
                if self.is_main_panel(MainPanel::Content) {
                    self.text_reader.toggle_normal_mode();
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
                            self.show_info("Comment deleted");
                        }
                        Ok(false) => {
                            // Cursor not on a comment, ignore
                        }
                        Err(e) => {
                            error!("Failed to delete comment: {e}");
                            self.show_error(format!("Failed to delete comment: {e}"));
                        }
                    }
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_up();
            }
            KeyCode::Char('h') => {
                if !self.handle_key_sequence('h') {
                    let _ = self.navigate_chapter_relative(ChapterDirection::Previous);
                }
            }
            KeyCode::Left => {
                let _ = self.navigate_chapter_relative(ChapterDirection::Previous);
            }
            KeyCode::Char('l') | KeyCode::Right => {
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
                    match self.focused_panel {
                        FocusedPanel::Main(MainPanel::NavigationList) => {
                            self.navigation_panel
                                .table_of_contents
                                .clear_manual_navigation();
                            self.set_main_panel_focus(MainPanel::Content);
                        }
                        FocusedPanel::Main(MainPanel::Content) => {
                            self.set_main_panel_focus(MainPanel::NavigationList);
                        }
                        FocusedPanel::Popup(_) => {} // No tab switching in popups
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
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_zen_mode();
            }

            KeyCode::Char('G') => {
                if self.current_book.is_some() {
                    self.text_reader.handle_upper_g();
                }
            }
            KeyCode::Char('a') => {
                if !self.handle_key_sequence('a')
                    && (self.text_reader.has_text_selection()
                        || self.text_reader.is_visual_mode_active())
                    && self.text_reader.start_comment_input()
                {
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
            KeyCode::Char('t') => {
                self.handle_key_sequence('t');
            }
            KeyCode::Char('?') => {
                self.help_popup = Some(HelpPopup::new());
                self.focused_panel = FocusedPanel::Popup(PopupWindow::Help);
            }
            KeyCode::Char('q') => {
                self.save_bookmark_with_throttle(true);
                return Some(AppAction::Quit);
            }
            KeyCode::Esc => {
                if self.notifications.has_notification() {
                    self.notifications.dismiss();
                } else if self.text_reader.has_text_selection() {
                    self.text_reader.clear_selection();
                } else if self.is_in_search_mode() {
                    self.cancel_current_search();
                }
            }
            KeyCode::Char('=') | KeyCode::Char('+') => {
                let current_node = self.text_reader.get_current_node_index();
                self.text_reader.increase_margin();
                self.text_reader.restore_to_node_index(current_node);
                settings::set_margin(self.text_reader.get_margin());
            }
            KeyCode::Char('-') => {
                let current_node = self.text_reader.get_current_node_index();
                self.text_reader.decrease_margin();
                self.text_reader.restore_to_node_index(current_node);
                settings::set_margin(self.text_reader.get_margin());
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
        fn extract_text_from_markdown_doc(doc: &crate::markdown::Document) -> Vec<SearchLine> {
            let mut lines = Vec::new();
            for (node_index, node) in doc.blocks.iter().enumerate() {
                extract_text_from_block(&node.block, node_index, &mut lines);
            }
            lines
        }

        fn extract_text_from_block(
            block: &crate::markdown::Block,
            node_index: usize,
            lines: &mut Vec<SearchLine>,
        ) {
            use crate::markdown::Block;

            match block {
                Block::Paragraph { content } | Block::Heading { content, .. } => {
                    let plain_text = extract_text_from_text(content);
                    if !plain_text.trim().is_empty() {
                        lines.push(SearchLine {
                            text: plain_text,
                            node_index,
                        });
                    }
                }
                Block::List { items, .. } => {
                    for item in items {
                        // ListItem content is Vec<Node>, so process each node
                        for node in &item.content {
                            extract_text_from_block(&node.block, node_index, lines);
                        }
                    }
                }
                Block::Quote { content } => {
                    for node in content {
                        extract_text_from_block(&node.block, node_index, lines);
                    }
                }
                Block::CodeBlock { content, .. } => {
                    lines.push(SearchLine {
                        text: content.clone(),
                        node_index,
                    });
                }
                Block::Table { rows, header, .. } => {
                    if let Some(header_row) = header {
                        let row_text: Vec<String> = header_row
                            .cells
                            .iter()
                            .map(|cell| {
                                extract_text_from_cell_content(&cell.content, node_index, lines)
                            })
                            .collect();
                        if !row_text.is_empty() {
                            lines.push(SearchLine {
                                text: row_text.join(" "),
                                node_index,
                            });
                        }
                    }
                    for row in rows {
                        let row_text: Vec<String> = row
                            .cells
                            .iter()
                            .map(|cell| {
                                extract_text_from_cell_content(&cell.content, node_index, lines)
                            })
                            .collect();
                        if !row_text.is_empty() {
                            lines.push(SearchLine {
                                text: row_text.join(" "),
                                node_index,
                            });
                        }
                    }
                }
                Block::DefinitionList { items } => {
                    for item in items {
                        lines.push(SearchLine {
                            text: extract_text_from_text(&item.term),
                            node_index,
                        });
                        // Process each definition (Vec<Vec<Node>>)
                        for definition in &item.definitions {
                            for node in definition {
                                extract_text_from_block(&node.block, node_index, lines);
                            }
                        }
                    }
                }
                Block::EpubBlock { content, .. } => {
                    for node in content {
                        extract_text_from_block(&node.block, node_index, lines);
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

        fn extract_text_from_cell_content(
            content: &crate::markdown::TableCellContent,
            node_index: usize,
            lines: &mut Vec<SearchLine>,
        ) -> String {
            match content {
                crate::markdown::TableCellContent::Simple(text) => extract_text_from_text(text),
                crate::markdown::TableCellContent::Rich(nodes) => {
                    let mut result = String::new();
                    for node in nodes {
                        extract_text_from_block(&node.block, node_index, lines);
                        // Also collect text inline
                        result.push_str(&extract_node_text(node));
                    }
                    result
                }
            }
        }

        fn extract_node_text(node: &crate::markdown::Node) -> String {
            use crate::markdown::Block;
            match &node.block {
                Block::Paragraph { content } => extract_text_from_text(content),
                Block::Heading { content, .. } => extract_text_from_text(content),
                Block::CodeBlock { content, .. } => content.clone(),
                Block::Quote { content } => content
                    .iter()
                    .map(extract_node_text)
                    .collect::<Vec<_>>()
                    .join(" "),
                Block::List { items, .. } => items
                    .iter()
                    .flat_map(|item| item.content.iter().map(extract_node_text))
                    .collect::<Vec<_>>()
                    .join(" "),
                Block::Table { header, rows, .. } => {
                    let mut text = String::new();
                    if let Some(h) = header {
                        text.push_str(
                            &h.cells
                                .iter()
                                .map(|c| match &c.content {
                                    crate::markdown::TableCellContent::Simple(t) => {
                                        extract_text_from_text(t)
                                    }
                                    crate::markdown::TableCellContent::Rich(n) => n
                                        .iter()
                                        .map(extract_node_text)
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                })
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                    }
                    for row in rows {
                        text.push_str(
                            &row.cells
                                .iter()
                                .map(|c| match &c.content {
                                    crate::markdown::TableCellContent::Simple(t) => {
                                        extract_text_from_text(t)
                                    }
                                    crate::markdown::TableCellContent::Rich(n) => n
                                        .iter()
                                        .map(extract_node_text)
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                })
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                    }
                    text
                }
                _ => String::new(),
            }
        }

        let mut search_engine = SearchEngine::new();
        let mut chapters = Vec::new();
        use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
        let mut converter = HtmlToMarkdownConverter::new();

        // Process all chapters to extract readable text
        for chapter_index in 0..doc.get_num_chapters() {
            if doc.set_current_chapter(chapter_index) {
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

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn testing_current_chapter_file(&self) -> Option<String> {
        self.text_reader.get_current_chapter_file().clone()
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn testing_rendered_lines(&self) -> &[crate::markdown_text_reader::RenderedLine] {
        self.text_reader.testing_rendered_lines()
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn testing_comment_target_for_selection(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Option<crate::comments::CommentTarget> {
        self.text_reader
            .testing_comment_target_for_selection(start_line, start_col, end_line, end_col)
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn testing_add_comment(&mut self, comment: crate::comments::Comment) {
        let comments_arc = self.text_reader.get_comments();
        if let Ok(mut guard) = comments_arc.lock() {
            let _ = guard.add_comment(comment.clone());
        }
        self.text_reader.rebuild_chapter_comments();
        self.text_reader.invalidate_render_cache();
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn testing_last_copied_text(&self) -> Option<String> {
        self.text_reader.get_last_copied_text()
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
            let notification_expired = app.notifications.update();
            if images_loaded {
                needs_redraw = true;
                debug!("Images loaded, forcing redraw");
            }
            if highlight_changed {
                needs_redraw = true;
                debug!("Highlight expired, forcing redraw");
            }
            if notification_expired {
                needs_redraw = true;
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
