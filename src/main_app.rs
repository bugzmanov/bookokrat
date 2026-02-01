use crate::book_manager::{BookFormat, BookManager};
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
use image::GenericImageView;
use log::warn;

// Settings popup (used for themes in all modes)
use crate::widget::settings_popup::{SettingsAction, SettingsPopup, SettingsTab};

// PDF support (feature-gated)
#[cfg(feature = "pdf")]
use crate::pdf::{
    CellSize, RenderService, DEFAULT_CACHE_SIZE, DEFAULT_CACHE_SIZE_KITTY,
    DEFAULT_PREFETCH_RADIUS, DEFAULT_WORKERS,
};
#[cfg(feature = "pdf")]
use crate::widget::pdf_reader::{InputAction, InputOutcome, PdfDisplayPlan, PdfReaderState};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChapterDirection {
    Next,
    Previous,
}

use std::io::{BufReader, stdout};
use std::path::{Path, PathBuf};
#[cfg(feature = "pdf")]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::EndSynchronizedUpdate;
use epub::doc::EpubDoc;
use log::{debug, error, info};
use pprof::ProfilerGuard;
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

#[cfg(feature = "pdf")]
struct PdfEventResult {
    handled: bool,
    action: Option<AppAction>,
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
    profiler: Arc<Mutex<Option<ProfilerGuard<'static>>>>,
    book_stat: BookStat,
    jump_list: JumpList,
    book_search: Option<BookSearch>,
    help_popup: Option<HelpPopup>,
    comments_viewer: Option<crate::widget::comments_viewer::CommentsViewer>,
    settings_popup: Option<SettingsPopup>,
    notifications: NotificationManager,
    help_bar_area: Rect,
    zen_mode: bool,
    test_mode: bool,
    comments_dir: Option<PathBuf>,
    // PDF support (feature-gated)
    #[cfg(feature = "pdf")]
    pdf_service: Option<RenderService>,
    #[cfg(feature = "pdf")]
    pdf_reader: Option<PdfReaderState>,
    #[cfg(feature = "pdf")]
    pdf_font_size: CellSize,
    #[cfg(feature = "pdf")]
    pdf_picker: Option<crate::vendored::ratatui_image::picker::Picker>,
    #[cfg(feature = "pdf")]
    pdf_conversion_tx: Option<flume::Sender<crate::pdf::ConversionCommand>>,
    #[cfg(feature = "pdf")]
    pdf_conversion_rx:
        Option<flume::Receiver<Result<crate::pdf::RenderedFrame, crate::pdf::WorkerFault>>>,
    #[cfg(feature = "pdf")]
    pdf_pending_display: Option<PdfDisplayPlan>,
    #[cfg(feature = "pdf")]
    pdf_kitty_shm_support: Option<bool>,
    #[cfg(feature = "pdf")]
    pdf_kitty_delete_range_support: Option<bool>,
    /// For non-Kitty protocols: track which page we're waiting for to avoid
    /// unnecessary redraws while the page is being converted.
    #[cfg(feature = "pdf")]
    pdf_waiting_for_page: Option<usize>,
    /// For non-Kitty protocols: suppress redraw while waiting for viewport update.
    /// Set when scroll/viewport changes, cleared when frame arrives.
    #[cfg(feature = "pdf")]
    pdf_waiting_for_viewport: bool,
    /// Path to the currently opened PDF document (for search indexing)
    #[cfg(feature = "pdf")]
    pdf_document_path: Option<PathBuf>,
}

#[cfg(feature = "pdf")]
static PDF_KITTY_SHM_SUPPORT: OnceLock<Option<bool>> = OnceLock::new();
#[cfg(feature = "pdf")]
static PDF_KITTY_DELETE_RANGE_SUPPORT: OnceLock<Option<bool>> = OnceLock::new();

#[cfg(feature = "pdf")]
pub fn set_kitty_shm_support_override(support: Option<bool>) {
    let _ = PDF_KITTY_SHM_SUPPORT.set(support);
}

#[cfg(feature = "pdf")]
pub fn set_kitty_delete_range_support_override(support: Option<bool>) {
    let _ = PDF_KITTY_DELETE_RANGE_SUPPORT.set(support);
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
    Settings,
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
        let panel = if self.zen_mode {
            MainPanel::Content
        } else {
            self.previous_main_panel
        };
        self.focused_panel = FocusedPanel::Main(panel);
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
            settings_popup: None,
            notifications: NotificationManager::new(),
            help_bar_area: Rect::default(),
            zen_mode: false,
            test_mode: false,
            comments_dir: comments_dir.map(|p| p.to_path_buf()),
            #[cfg(feature = "pdf")]
            pdf_service: None,
            #[cfg(feature = "pdf")]
            pdf_reader: None,
            #[cfg(feature = "pdf")]
            pdf_font_size: CellSize::new(8, 16), // Default, updated on PDF load
            #[cfg(feature = "pdf")]
            pdf_picker: None,
            #[cfg(feature = "pdf")]
            pdf_conversion_tx: None,
            #[cfg(feature = "pdf")]
            pdf_conversion_rx: None,
            #[cfg(feature = "pdf")]
            pdf_pending_display: None,
            #[cfg(feature = "pdf")]
            pdf_kitty_shm_support: PDF_KITTY_SHM_SUPPORT.get().copied().unwrap_or(None),
            #[cfg(feature = "pdf")]
            pdf_kitty_delete_range_support: PDF_KITTY_DELETE_RANGE_SUPPORT
                .get()
                .copied()
                .unwrap_or(None),
            #[cfg(feature = "pdf")]
            pdf_waiting_for_page: None,
            #[cfg(feature = "pdf")]
            pdf_waiting_for_viewport: false,
            #[cfg(feature = "pdf")]
            pdf_document_path: None,
        };

        // Fix incompatible PDF settings (e.g., Scroll mode in non-Kitty terminal)
        crate::settings::fix_incompatible_pdf_settings();

        let is_first_time_user = app.bookmarks.get_most_recent().is_none();

        if auto_load_recent
            && let Some((recent_path, _)) = app.bookmarks.get_most_recent()
            && app.book_manager.contains_book(&recent_path)
        {
            if let Err(e) = app.open_book_for_reading_by_path(&recent_path) {
                error!("Failed to auto-load most recent book: {e}");
                app.show_error(format!("Failed to auto-load recent book: {e}"));
            }
        } else if auto_load_recent && is_first_time_user {
            // No bookmarks exist - show help popup for first-time users
            // Set previous panel to NavigationList so ESC returns there
            app.previous_main_panel = MainPanel::NavigationList;
            app.help_popup = Some(HelpPopup::new());
            app.focused_panel = FocusedPanel::Popup(PopupWindow::Help);
        }

        // Show PDF settings popup for upgrading users who haven't configured PDF settings yet
        // (but only if terminal supports graphics and not first-time user)
        #[cfg(feature = "pdf")]
        if !is_first_time_user
            && !crate::settings::is_pdf_settings_configured()
            && crate::terminal::detect_terminal().supports_graphics
        {
            app.previous_main_panel = MainPanel::NavigationList;
            app.settings_popup = Some(SettingsPopup::new_with_tab(SettingsTab::PdfSupport));
            app.focused_panel = FocusedPanel::Popup(PopupWindow::Settings);
            // Mark as configured so we don't show again
            crate::settings::set_pdf_settings_configured(true);
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

    /// Check if we're currently in PDF reading mode
    #[cfg(feature = "pdf")]
    pub fn is_pdf_mode(&self) -> bool {
        self.pdf_reader.is_some()
    }

    #[cfg(not(feature = "pdf"))]
    pub fn is_pdf_mode(&self) -> bool {
        false
    }

    /// Clear PDF graphics from terminal when switching away from PDF mode
    #[cfg(feature = "pdf")]
    fn clear_pdf_graphics(is_kitty: bool) {
        use std::io::Write;
        if is_kitty {
            // Send Kitty graphics protocol command to delete all images
            // Format: ESC _G a=d,d=A,q=2 ESC \
            // a=d: action=delete, d=A: delete all from memory, q=2: quiet mode
            let delete_cmd = b"\x1b_Ga=d,d=A,q=2\x1b\\";
            let _ = stdout().write_all(delete_cmd);
            let _ = stdout().flush();
        }
        // For iTerm2/Sixel, images are inline and will be overwritten by new content
    }

    /// Open a book for reading by index - delegates to path-based opening
    pub fn open_book_for_reading(&mut self, book_index: usize) -> Result<()> {
        if let Some(book_info) = self.book_manager.get_book_info(book_index) {
            let path = book_info.path.clone();
            self.open_book_for_reading_by_path(&path)
        } else {
            anyhow::bail!("Invalid book index: {}", book_index)
        }
    }

    pub fn open_book_for_reading_by_path(&mut self, path: &str) -> Result<()> {
        let book_info = self
            .book_manager
            .get_book_by_path(path)
            .ok_or_else(|| anyhow::anyhow!("Book not found in manager: {}", path))?;

        let format = book_info.format;
        let path_owned = path.to_string();

        self.save_bookmark_with_throttle(true);

        match format {
            #[cfg(feature = "pdf")]
            BookFormat::Pdf => {
                self.load_pdf(&path_owned, self.test_mode)?;
            }
            BookFormat::Epub | BookFormat::Html => {
                self.load_epub(&path_owned, self.test_mode)?;
            }
        }

        self.navigation_panel.current_book_path = Some(path_owned);
        self.focused_panel = FocusedPanel::Main(MainPanel::Content);

        Ok(())
    }

    /// Navigate to a specific chapter - ensures all state is properly updated
    /// If `skip_jump_list` is true, don't save to jump list (used during Ctrl+O/I navigation)
    pub fn navigate_to_chapter_inner(
        &mut self,
        chapter_index: usize,
        skip_jump_list: bool,
    ) -> Result<()> {
        // Save current location to jump list if changing chapters (unless skipping)
        if !skip_jump_list
            && self
                .current_book
                .as_ref()
                .is_some_and(|b| b.current_chapter() != chapter_index)
        {
            self.save_to_jump_list();
        }

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

    /// Navigate to a specific chapter - convenience wrapper that saves to jump list
    pub fn navigate_to_chapter(&mut self, chapter_index: usize) -> Result<()> {
        self.navigate_to_chapter_inner(chapter_index, false)
    }

    /// Navigate to next or previous chapter - maintains all state consistency
    pub fn navigate_chapter_relative(&mut self, direction: ChapterDirection) -> Result<()> {
        // Save current location to jump list before navigating
        self.save_to_jump_list();

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
        #[cfg(feature = "pdf")]
        {
            // Clear any PDF graphics from terminal before switching to EPUB
            if let Some(ref pdf_reader) = self.pdf_reader {
                Self::clear_pdf_graphics(pdf_reader.is_kitty);
            }
            self.pdf_service = None;
            self.pdf_reader = None;
            self.pdf_picker = None;
            self.pdf_conversion_tx = None;
            self.pdf_conversion_rx = None;
            self.pdf_pending_display = None;
            self.pdf_document_path = None;
        }

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

        // Clear jump list when opening a new book (jump list is per-book)
        self.jump_list.clear();

        let path_buf = std::path::PathBuf::from(path);
        if let Err(e) = self.book_images.load_book(&path_buf) {
            error!("Failed to load book in BookImages: {e}");
        }

        self.initialize_search_engine(&mut doc);

        // In test mode (ignore_bookmarks=true), use empty comments to avoid loading persistent state
        let comments = if ignore_bookmarks {
            BookComments::new_empty()
        } else {
            match BookComments::new(&path_buf, self.comments_dir.as_deref()) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to initialize book comments: {e}");
                    BookComments::new_empty()
                }
            }
        };
        let comments_arc = Arc::new(Mutex::new(comments));
        self.text_reader.set_book_comments(comments_arc);

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

    /// Load a PDF document
    #[cfg(feature = "pdf")]
    pub fn load_pdf(&mut self, path: &str, ignore_bookmarks: bool) -> Result<()> {
        info!("Loading PDF document: {path}");

        // Close any existing EPUB
        self.current_book = None;
        // Clear any existing book search (will be re-initialized on demand for new PDF)
        self.book_search = None;

        // Get terminal font size for rendering
        let cell_size = self.get_terminal_font_size();

        // Get theme colors for rendering (MuPDF format: 0xRRGGBB)
        let palette = crate::theme::current_theme();
        let (black, white) = Self::palette_to_mupdf_colors(palette);

        // Detect terminal protocol and capabilities
        let mut picker = crate::vendored::ratatui_image::picker::Picker::from_query_stdio().ok();
        let caps = match picker.as_mut() {
            Some(picker) => crate::terminal::detect_terminal_with_picker(picker),
            None => crate::terminal::detect_terminal(),
        };

        if let Some(reason) = caps.pdf.blocked_reason.as_ref() {
            warn!("{reason}");
            self.notifications.show_warning(reason.clone());
            return Ok(());
        }

        let is_kitty = matches!(
            caps.protocol,
            Some(crate::terminal::GraphicsProtocol::Kitty)
        );
        let use_kitty = is_kitty;

        // Create render service (use a smaller cache for Kitty to cap memory)
        let cache_size = if is_kitty {
            DEFAULT_CACHE_SIZE_KITTY
        } else {
            DEFAULT_CACHE_SIZE
        };
        let doc_path = std::path::PathBuf::from(path);
        let service = RenderService::with_config(
            doc_path.clone(),
            cell_size,
            black,
            white,
            DEFAULT_WORKERS,
            cache_size,
            DEFAULT_PREFETCH_RADIUS,
        );

        // Get document info
        let doc_info = service.document_info();
        let page_count = doc_info.as_ref().map_or(0, |info| info.page_count);
        let doc_title = doc_info.as_ref().and_then(|info| info.title.clone());
        let toc_entries = doc_info
            .as_ref()
            .map_or_else(Vec::new, |info| info.toc.clone());

        info!(
            "PDF loaded: {} pages, title: {:?}",
            page_count,
            doc_title.as_deref().unwrap_or("(none)")
        );
        // is_iterm = actual iTerm terminal (for feature restrictions)
        let is_iterm = caps.kind == crate::terminal::TerminalKind::ITerm;
        let supports_comments = caps.pdf.supports_comments;

        // Get initial page and zoom from bookmark if available (unless ignored)
        let bookmark = if ignore_bookmarks {
            None
        } else {
            self.bookmarks.get_bookmark(path)
        };
        let mut initial_page = bookmark
            .and_then(|b| {
                b.pdf_page
                    .or(b.chapter_index)
                    .or_else(|| b.chapter_href.parse::<usize>().ok())
            })
            .unwrap_or(0);
        if page_count > 0 && initial_page >= page_count {
            initial_page = page_count - 1;
        }
        let bookmark_zoom = bookmark.and_then(|b| b.pdf_zoom);

        // Initialize PDF comments for terminals with image protocol support (Kitty, iTerm2).
        // Comments are always loaded so underlines are visible even in ToC mode.
        // comments_enabled controls sidebar UI and interactions (zen mode only).
        // In test mode (ignore_bookmarks=true), use empty comments to avoid loading persistent state.
        let (comments_enabled, book_comments) = if supports_comments {
            let comments = if ignore_bookmarks {
                crate::comments::BookComments::new_empty()
            } else {
                match crate::comments::BookComments::new(
                    std::path::Path::new(path),
                    self.comments_dir.as_deref(),
                ) {
                    Ok(c) => c,
                    Err(e) => {
                        log::warn!("Failed to initialize PDF comments: {e}");
                        crate::comments::BookComments::new_empty()
                    }
                }
            };
            (
                self.zen_mode, // UI interactions only in zen mode
                Some(std::sync::Arc::new(std::sync::Mutex::new(comments))),
            )
        } else {
            (false, None)
        };

        // Create PDF reader state with persisted settings
        // Prefer per-book zoom from bookmark, fall back to global setting
        let pdf_scale = bookmark_zoom.unwrap_or_else(crate::settings::get_pdf_scale);
        let pdf_pan_shift = crate::settings::get_pdf_pan_shift();
        let mut pdf_reader = PdfReaderState::new(
            path.to_string(),
            is_kitty,
            is_iterm,
            initial_page,
            pdf_scale,
            pdf_pan_shift,
            0, // global_scroll_offset
            palette.clone(),
            crate::theme::current_theme_index(),
            comments_enabled,
            supports_comments,
            book_comments,
            path.to_string(),
        );
        if let Some(supported) = self.pdf_kitty_delete_range_support {
            pdf_reader.kitty_delete_range_supported = supported;
        }

        pdf_reader.set_doc_title(doc_title);
        pdf_reader.toc_entries = toc_entries;
        let initial_comment_rects = pdf_reader.initial_comment_rects();
        if page_count > 0 {
            let mut rendered = Vec::with_capacity(page_count);
            for _ in 0..page_count {
                rendered.push(crate::widget::pdf_reader::RenderedInfo::default());
            }
            pdf_reader.rendered = rendered;
            pdf_reader.page_numbers.set_targets(page_count);

            // Feed pre-collected page number samples for content-page mode
            if let Some(info) = doc_info {
                for &(page_num, printed) in &info.page_number_samples {
                    pdf_reader.page_numbers.observe_sample(page_num, printed);
                }
            }
        }

        let mut conversion_tx = None;
        let mut conversion_rx = None;
        let cached_shm_support = self.pdf_kitty_shm_support;
        let disable_shm = std::env::var("BOOKOKRAT_DISABLE_KITTY_SHM").is_ok();
        let mut kitty_shm_support = cached_shm_support.unwrap_or(!disable_shm);
        let mut pdf_picker = picker;

        if let Some(picker) = pdf_picker.take() {
            let (cmd_tx, cmd_rx) = flume::unbounded();
            let (render_tx, render_rx) = flume::unbounded();
            const PRERENDER_PAGES: usize = 20;

            if use_kitty {
                self.pdf_kitty_shm_support = Some(kitty_shm_support);
            } else {
                kitty_shm_support = false;
            }

            if let Err(e) = std::thread::Builder::new()
                .name("pdf-converter".to_string())
                .spawn(move || {
                    let _ = crate::pdf::run_conversion_loop(
                        render_tx,
                        cmd_rx,
                        picker,
                        PRERENDER_PAGES,
                        kitty_shm_support,
                    );
                })
            {
                log::error!("Failed to spawn PDF converter thread: {e}");
            }

            conversion_tx = Some(cmd_tx);
            conversion_rx = Some(render_rx);
        }

        self.pdf_service = Some(service);
        self.pdf_reader = Some(pdf_reader);
        self.pdf_document_path = Some(doc_path);
        self.pdf_font_size = cell_size;
        self.pdf_picker = pdf_picker;
        self.pdf_conversion_tx = conversion_tx;
        self.pdf_conversion_rx = conversion_rx;

        // Sync initial page to service so first render requests the correct page.
        // Use set_current_page_no_render to avoid triggering a render with zero area.
        if let Some(ref mut service) = self.pdf_service {
            service.set_current_page_no_render(initial_page);
        }

        // Defer the initial render until the layout is known to avoid 0-sized pages.
        if let Some(cmd_tx) = self.pdf_conversion_tx.as_ref() {
            let _ = cmd_tx.send(crate::pdf::ConversionCommand::SetPageCount(page_count));
            let _ = cmd_tx.send(crate::pdf::ConversionCommand::NavigateTo(initial_page));
            if !initial_comment_rects.is_empty() {
                let _ = cmd_tx.send(crate::pdf::ConversionCommand::UpdateComments(
                    initial_comment_rects,
                ));
            }
        }

        // Switch navigation panel to PDF TOC mode
        self.switch_to_pdf_toc_mode();
        self.save_bookmark_with_throttle(true);

        Ok(())
    }

    /// Get terminal font size in pixels
    #[cfg(feature = "pdf")]
    fn get_terminal_font_size(&self) -> CellSize {
        // Default cell size if we can't detect
        let default = CellSize::new(8, 16);

        // Try to get from picker
        if let Ok(picker) = crate::vendored::ratatui_image::picker::Picker::from_query_stdio() {
            let (width, height) = picker.font_size();
            CellSize::new(width, height)
        } else {
            default
        }
    }

    /// Convert palette colors to MuPDF format (0xRRGGBB as i32)
    #[cfg(feature = "pdf")]
    fn palette_to_mupdf_colors(palette: &crate::theme::Base16Palette) -> (i32, i32) {
        fn color_to_i32(color: Color) -> i32 {
            match color {
                Color::Rgb(r, g, b) => ((r as i32) << 16) | ((g as i32) << 8) | (b as i32),
                _ => 0x000000, // Default to black for non-RGB colors
            }
        }

        let black = color_to_i32(palette.base_00); // Background
        let white = color_to_i32(palette.base_05); // Foreground
        (black, white)
    }

    #[cfg(feature = "pdf")]
    fn render_pdf_in_area(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let Some(mut pdf_reader) = self.pdf_reader.take() else {
            return;
        };

        let (text_color, border_color, _bg_color) =
            current_theme().get_panel_colors(self.is_main_panel(MainPanel::Content));
        let toc_height = self.get_navigation_panel_area().height as usize;

        pdf_reader.render_in_area(
            f,
            area,
            self.pdf_font_size.as_tuple(),
            text_color,
            border_color,
            current_theme().base_00,
            self.pdf_service.as_mut(),
            self.pdf_conversion_tx.as_ref(),
            &mut self.pdf_pending_display,
            &mut self.bookmarks,
            &mut self.last_bookmark_save,
            &mut self.navigation_panel.table_of_contents,
            toc_height,
        );

        self.pdf_reader = Some(pdf_reader);
    }

    /// Switch navigation panel to PDF TOC mode
    #[cfg(feature = "pdf")]
    fn switch_to_pdf_toc_mode(&mut self) {
        let Some(ref pdf_reader) = self.pdf_reader else {
            return;
        };
        pdf_reader.switch_to_toc_mode(&mut self.navigation_panel);
    }

    #[cfg(feature = "pdf")]
    fn execute_pdf_display_plan(&mut self) {
        let Some(plan) = self.pdf_pending_display.take() else {
            return;
        };

        let has_popup = self.has_active_popup();

        let Some(pdf_reader) = self.pdf_reader.as_mut() else {
            return;
        };

        crate::widget::pdf_reader::execute_display_plan(
            plan,
            pdf_reader,
            has_popup,
            self.pdf_conversion_tx.as_ref(),
        );
    }

    #[cfg(feature = "pdf")]
    fn update_non_kitty_viewport(&mut self) {
        let Some(pdf_reader) = self.pdf_reader.as_mut() else {
            return;
        };
        crate::widget::pdf_reader::update_non_kitty_viewport(
            pdf_reader,
            self.pdf_conversion_tx.as_ref(),
        );
    }

    /// Handle Kitty graphics protocol eviction responses.
    /// When Kitty evicts an image from its cache and we try to display it,
    /// it returns an ENOENT error. This method processes those errors and
    /// marks the affected pages for re-render.
    #[cfg(feature = "pdf")]
    fn handle_kitty_eviction_responses(&mut self, event_source: &mut dyn EventSource) {
        use crate::pdf::ConversionCommand;

        let responses = event_source.take_kitty_responses();
        if responses.is_empty() {
            return;
        }

        let Some(pdf_reader) = self.pdf_reader.as_mut() else {
            return;
        };

        let mut evicted_pages = Vec::new();

        for response in responses {
            if response.is_evicted() || response.is_error() {
                if let Some(image_id) = response.image_id {
                    // Image IDs are based on page numbers (page_image_id = page + 1)
                    let page = image_id.saturating_sub(1) as usize;
                    log::debug!(
                        "Kitty response error for image {} (page {}): {}",
                        image_id,
                        page,
                        response.message
                    );

                    // Clear the Uploaded state for this page so it gets re-converted
                    if let Some(info) = pdf_reader.rendered.get_mut(page) {
                        if let Some(crate::pdf::ConvertedImage::Kitty { ref img, .. }) = info.img {
                            if img.is_uploaded() {
                                log::debug!("Clearing evicted page {page} for re-render");
                                info.img = None;
                                evicted_pages.push(page);
                            }
                        }
                    }
                }
            } else {
                log::info!(
                    "Kitty response (non-eviction): image_id={:?} message={}",
                    response.image_id,
                    response.message
                );
            }
        }

        // Notify converter about failed pages
        if !evicted_pages.is_empty() {
            if let Some(tx) = self.pdf_conversion_tx.as_ref() {
                let _ = tx.send(ConversionCommand::DisplayFailed(evicted_pages.clone()));
            }
            if let (Some(service), Some(tx)) =
                (self.pdf_service.as_ref(), self.pdf_conversion_tx.as_ref())
            {
                for &page in &evicted_pages {
                    if let Some(cached) = service.get_cached_page(page) {
                        let _ = tx.send(ConversionCommand::EnqueuePage(Arc::clone(&cached)));
                    }
                }
            }
            // Force a redraw to trigger re-render
            if let Some(reader) = self.pdf_reader.as_mut() {
                reader.force_redraw();
            }
        }
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

    /// Extract book title from file path (without extension)
    fn extract_book_title(file_path: &str) -> String {
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("book")
            .to_string()
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
        // Handle PDF bookmarks
        #[cfg(feature = "pdf")]
        if let Some(ref pdf_reader) = self.pdf_reader {
            pdf_reader.save_bookmark_with_throttle(
                &mut self.bookmarks,
                &mut self.last_bookmark_save,
                force,
            );
            return;
        }

        // Handle EPUB bookmarks
        if let Some(book) = &self.current_book {
            let chapter_href = Self::get_chapter_href(&book.epub, book.current_chapter())
                .unwrap_or_else(|| format!("chapter_{}", book.current_chapter()));

            self.bookmarks.update_bookmark(
                &book.file,
                chapter_href,
                Some(self.text_reader.get_current_node_index()),
                Some(book.current_chapter()),
                Some(book.total_chapters()),
                None,
                None,
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

    #[cfg(feature = "pdf")]
    fn should_route_pdf_mouse_to_ui(&self, mouse_event: &MouseEvent) -> bool {
        crate::widget::pdf_reader::should_route_mouse_to_ui(
            mouse_event,
            self.has_active_popup(),
            self.zen_mode,
            self.nav_panel_width(),
            self.help_bar_area,
        )
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
                                        let node_index =
                                            entry.primary_comment().node_index().unwrap_or(0);
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
                                if let Some(action) = self.navigation_panel.get_enter_action() {
                                    self.handle_navigation_panel_action(action);
                                }
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
                            // Check if click is on an image
                            if let Some(image_src) = self
                                .text_reader
                                .check_image_click(mouse_event.column, mouse_event.row)
                            {
                                let is_ctrl_held =
                                    mouse_event.modifiers.contains(KeyModifiers::CONTROL);

                                if is_ctrl_held {
                                    // Ctrl+Click always opens image popup
                                    self.handle_image_click(&image_src, self.terminal_size);
                                } else if let Some(link_info) =
                                    self.text_reader.check_link_at_screen_position(
                                        mouse_event.column,
                                        mouse_event.row,
                                    )
                                {
                                    // Click on linked image navigates to link
                                    if let Err(e) = self.handle_link_click(&link_info) {
                                        error!("Failed to handle link click: {e}");
                                    }
                                } else {
                                    // Click on non-linked image opens popup
                                    self.handle_image_click(&image_src, self.terminal_size);
                                }
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
            let current_location = JumpLocation::epub(
                book.file.clone(),
                book.current_chapter(),
                self.text_reader.get_current_node_index(),
            );
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
                // Save to jump list for same-chapter anchor navigation
                self.save_to_jump_list();
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
                            // Same chapter - save to jump list for anchor navigation
                            self.save_to_jump_list();
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
        // First try TOC lookup
        if let Some(current_book_info) = &self
            .navigation_panel
            .table_of_contents
            .get_current_book_info()
        {
            if let Some(index) =
                self.find_chapter_recursive(&current_book_info.toc_items, chapter_file)
            {
                return Some(index);
            }
        }

        // Fall back to direct spine lookup if not found in TOC
        self.find_spine_index_by_href(chapter_file)
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
        match location {
            JumpLocation::Epub {
                path,
                chapter,
                node,
            } => {
                if self.current_book.as_ref().map(|x| &x.file) != Some(&path) {
                    self.load_epub(&path, true)?;
                }

                if self.current_book.as_ref().map(|x| x.current_chapter()) != Some(chapter) {
                    self.navigate_to_chapter(chapter)?;
                }

                self.text_reader.restore_to_node_index(node);

                self.save_bookmark();
            }
            #[cfg(feature = "pdf")]
            JumpLocation::Pdf {
                path,
                page,
                scroll_offset,
            } => {
                // PDF jump handling will be implemented when PDF reader is added
                log::debug!(
                    "PDF jump to {path} page {page} offset {scroll_offset} - not yet implemented"
                );
            }
        }

        Ok(())
    }

    /// Save current location to jump list before navigating away
    fn save_to_jump_list(&mut self) {
        if let Some(book) = &self.current_book {
            let current_location = JumpLocation::epub(
                book.file.clone(),
                book.current_chapter(),
                self.text_reader.get_current_node_index(),
            );
            self.jump_list.push(current_location);
        }
    }

    /// Handle Ctrl+O - jump back in history
    fn jump_back(&mut self) {
        let current_location = self.current_book.as_ref().map(|book| {
            JumpLocation::epub(
                book.file.clone(),
                book.current_chapter(),
                self.text_reader.get_current_node_index(),
            )
        });

        if let Some(location) = self.jump_list.jump_back(current_location) {
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

    /// Handle a navigation panel action (used by both keyboard Enter and mouse double-click)
    /// Returns true if the action was a Bypass (caller should continue processing)
    fn handle_navigation_panel_action(
        &mut self,
        action: crate::navigation_panel::NavigationPanelAction,
    ) -> bool {
        use crate::navigation_panel::NavigationPanelAction;
        match action {
            NavigationPanelAction::Bypass => true,
            NavigationPanelAction::SelectBook { book_path } => {
                if let Err(e) = self.open_book_for_reading_by_path(&book_path) {
                    error!("Failed to open book at path {book_path}: {e}");
                    self.show_error(format!("Failed to open book: {e}"));
                }
                false
            }
            NavigationPanelAction::SwitchToBookList => {
                self.switch_to_book_list_mode();
                false
            }
            NavigationPanelAction::NavigateToChapter { href, anchor } => {
                // Check if this is a PDF navigation
                #[cfg(feature = "pdf")]
                {
                    if href.starts_with("pdf:page:") {
                        if let Some(page_str) = href.strip_prefix("pdf:page:") {
                            if let Ok(page) = page_str.parse::<usize>() {
                                self.navigate_pdf_to_page(page);
                                self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                            }
                        }
                        return false;
                    } else if href.starts_with("pdf:printed:") {
                        if let Some(printed_str) = href.strip_prefix("pdf:printed:") {
                            if let Ok(printed) = printed_str.parse::<usize>() {
                                if let Some(ref pdf_reader) = self.pdf_reader {
                                    let n_pages = pdf_reader.rendered.len();
                                    let page_idx = pdf_reader
                                        .page_numbers
                                        .map_printed_to_pdf(printed, n_pages)
                                        .or_else(|| {
                                            printed.checked_sub(1).filter(|&p| p < n_pages)
                                        });
                                    if let Some(page_idx) = page_idx {
                                        self.navigate_pdf_to_page(page_idx);
                                        self.focused_panel = FocusedPanel::Main(MainPanel::Content);
                                    }
                                }
                            }
                        }
                        return false;
                    } else if href.starts_with("pdf:external:") {
                        if let Some(url) = href.strip_prefix("pdf:external:") {
                            if let Err(e) = open::that(url) {
                                error!("Failed to open external link: {e}");
                            }
                        }
                        return false;
                    }
                }

                // Find the spine index for this href (EPUB navigation)
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
                } else {
                    error!("Could not find spine index for href: {href}");
                    self.show_error("Chapter not found in book");
                }
                false
            }
            NavigationPanelAction::ToggleSection => {
                self.navigation_panel
                    .table_of_contents
                    .toggle_selected_expansion();
                false
            }
        }
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame, fps_counter: &FPSCounter) {
        let draw_closure_start = std::time::Instant::now();
        #[cfg(feature = "pdf")]
        let mut pdf_area = None;
        let auto_scroll_updated = self.text_reader.update_auto_scroll();
        if auto_scroll_updated {
            self.save_bookmark();
        }

        self.terminal_size = f.area();

        let background_block = Block::default().style(Style::default().bg(current_theme().base_00));
        f.render_widget(background_block, f.area());

        if self.zen_mode {
            // Zen mode: full screen content, no navigation panel or help bar
            #[cfg(feature = "pdf")]
            if self.pdf_reader.is_some() {
                self.render_pdf_in_area(f, f.area());
                pdf_area = Some(f.area());
            } else if let Some(ref book) = self.current_book {
                self.text_reader.render(
                    f,
                    f.area(),
                    book.current_chapter(),
                    book.total_chapters(),
                    current_theme(),
                    true, // always focused in zen mode
                    true, // zen mode
                );
            } else {
                self.render_default_content(f, f.area(), "Select a file to view its content");
            }
            #[cfg(not(feature = "pdf"))]
            if let Some(ref book) = self.current_book {
                self.text_reader.render(
                    f,
                    f.area(),
                    book.current_chapter(),
                    book.total_chapters(),
                    current_theme(),
                    true, // always focused in zen mode
                    true, // zen_mode: show search hints on border
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

            #[cfg(feature = "pdf")]
            if self.pdf_reader.is_some() {
                self.render_pdf_in_area(f, main_chunks[1]);
                pdf_area = Some(main_chunks[1]);
            } else if let Some(ref book) = self.current_book {
                self.text_reader.render(
                    f,
                    main_chunks[1],
                    book.current_chapter(),
                    book.total_chapters(),
                    current_theme(),
                    self.is_main_panel(MainPanel::Content),
                    false, // not zen mode
                );
            } else {
                self.render_default_content(f, main_chunks[1], "Select a file to view its content");
            }
            #[cfg(not(feature = "pdf"))]
            if let Some(ref book) = self.current_book {
                self.text_reader.render(
                    f,
                    main_chunks[1],
                    book.current_chapter(),
                    book.total_chapters(),
                    current_theme(),
                    self.is_main_panel(MainPanel::Content),
                    false, // not zen_mode: search hints in help bar
                );
            } else {
                self.render_default_content(f, main_chunks[1], "Select a file to view its content");
            }

            self.render_help_bar(f, chunks[1], fps_counter);
            self.help_bar_area = chunks[1];
        }

        #[cfg(feature = "pdf")]
        if self.has_active_popup()
            && self.pdf_reader.is_some()
            && let Some(area) = pdf_area
        {
            // Clear image skip flags so the dim overlay can draw over PDFs.
            f.render_widget(crate::widget::pdf_reader::TextRegion, area);
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
            FocusedPanel::Popup(PopupWindow::Settings)
        ) {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10))
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            if let Some(ref mut settings_popup) = self.settings_popup {
                settings_popup.render(f, f.area());
            }
        }
        let draw_closure_elapsed = draw_closure_start.elapsed();
        if draw_closure_elapsed.as_millis() > 5 {
            log::debug!(
                "draw() render closure took {}ms",
                draw_closure_elapsed.as_millis()
            );
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
            if self.is_pdf_mode() {
                #[cfg(feature = "pdf")]
                {
                    self.open_comments_viewer_for_pdf();
                }
            } else if self.current_book.is_some() {
                if let FocusedPanel::Main(panel) = self.focused_panel {
                    self.previous_main_panel = panel;
                }
                if let Some(ref mut book) = self.current_book {
                    let toc_items = self.navigation_panel.get_toc_items();
                    let current_chapter_href =
                        Self::get_chapter_href(&book.epub, book.current_chapter());
                    let book_title = Self::extract_book_title(&book.file);
                    let mut viewer = crate::widget::comments_viewer::CommentsViewer::new(
                        self.text_reader.get_comments(),
                        &mut book.epub,
                        &toc_items,
                        current_chapter_href,
                        book_title,
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
            let terminal_size = (self.terminal_size.width, self.terminal_size.height);
            let mut opened = false;

            if let Some(ref mut book) = self.current_book {
                if let Err(e) = self
                    .book_stat
                    .calculate_stats(&mut book.epub, terminal_size)
                {
                    error!("Failed to calculate book statistics: {e}");
                    self.show_error(format!("Failed to calculate statistics: {e}"));
                } else {
                    opened = true;
                }
            } else if self.is_pdf_mode() {
                #[cfg(feature = "pdf")]
                if let Some(ref pdf_reader) = self.pdf_reader {
                    if let Err(e) = self.book_stat.calculate_pdf_stats(
                        &pdf_reader.toc_entries,
                        pdf_reader.rendered.len(),
                        &pdf_reader.page_numbers,
                        terminal_size,
                    ) {
                        error!("Failed to calculate PDF statistics: {e}");
                        self.show_error(format!("Failed to calculate statistics: {e}"));
                    } else {
                        opened = true;
                    }
                }
            }

            if opened {
                if let FocusedPanel::Main(panel) = self.focused_panel {
                    self.previous_main_panel = panel;
                }
                self.book_stat.show();
                self.focused_panel = FocusedPanel::Popup(PopupWindow::BookStats);
            }
            return true;
        }

        if inner_x >= theme_start && inner_x < theme_end {
            if let FocusedPanel::Main(panel) = self.focused_panel {
                self.previous_main_panel = panel;
            }
            self.settings_popup = Some(SettingsPopup::new_with_tab(SettingsTab::Themes));
            self.focused_panel = FocusedPanel::Popup(PopupWindow::Settings);
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
                FocusedPanel::Popup(PopupWindow::Settings) => {
                    "Tab/h/l: Tabs | j/k: Navigate | Enter: Apply | ESC: Close"
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

        // For PDF in Kitty mode, treat zen toggle as reopening at the same page
        // This prevents glitches when the viewport size changes dramatically
        #[cfg(feature = "pdf")]
        if let Some(ref mut pdf_reader) = self.pdf_reader {
            let nav_width = ((self.terminal_size.width * 30) / 100).max(20);
            pdf_reader.handle_zen_mode_toggle(
                self.zen_mode,
                self.terminal_size.width,
                nav_width,
                self.comments_dir.as_deref(),
                self.test_mode,
                self.pdf_conversion_tx.as_ref(),
                self.pdf_service.as_mut(),
            );
        }
    }

    pub fn set_zen_mode(&mut self, enabled: bool) {
        if self.zen_mode != enabled {
            self.toggle_zen_mode();
        }
    }

    pub fn set_test_mode(&mut self, enabled: bool) {
        self.test_mode = enabled;
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
            #[cfg(feature = "pdf")]
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_settings_popup();
                true
            }
            _ => false,
        }
    }

    #[cfg(feature = "pdf")]
    fn open_settings_popup(&mut self) {
        if let FocusedPanel::Main(panel) = self.focused_panel {
            self.previous_main_panel = panel;
        }
        self.settings_popup = Some(SettingsPopup::new());
        self.focused_panel = FocusedPanel::Popup(PopupWindow::Settings);
    }

    #[cfg(feature = "pdf")]
    fn pdf_text_input_active(&self) -> bool {
        self.pdf_reader
            .as_ref()
            .is_some_and(|reader| reader.is_text_input_active())
    }

    #[cfg(not(feature = "pdf"))]
    fn pdf_text_input_active(&self) -> bool {
        false
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
            "ss" => {
                // Raw HTML source toggle (for EPUB/HTML)
                if self.is_main_panel(MainPanel::Content) && self.current_book.is_some() {
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
            " s" => {
                #[cfg(feature = "pdf")]
                {
                    // Settings popup (only with PDF feature)
                    self.open_settings_popup();
                    self.key_sequence.clear();
                    true
                }
                #[cfg(not(feature = "pdf"))]
                {
                    false
                }
            }
            " f" => {
                // Handle Space->f to open book search (reuse existing search)
                // Works for both EPUB and PDF
                let has_document = self.current_book.is_some() || self.is_pdf_mode();
                if has_document {
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
                // Works for both EPUB and PDF
                let has_document = self.current_book.is_some() || self.is_pdf_mode();
                if has_document {
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    self.open_book_search(true); // Clear input for new search
                }
                self.key_sequence.clear();
                true
            }
            " d" => {
                let terminal_size = (self.terminal_size.width, self.terminal_size.height);
                let mut opened = false;

                if let Some(ref mut book) = self.current_book {
                    if let Err(e) = self
                        .book_stat
                        .calculate_stats(&mut book.epub, terminal_size)
                    {
                        error!("Failed to calculate book statistics: {e}");
                        self.show_error(format!("Failed to calculate statistics: {e}"));
                    } else {
                        opened = true;
                    }
                } else if self.is_pdf_mode() {
                    #[cfg(feature = "pdf")]
                    if let Some(ref pdf_reader) = self.pdf_reader {
                        if let Err(e) = self.book_stat.calculate_pdf_stats(
                            &pdf_reader.toc_entries,
                            pdf_reader.rendered.len(),
                            &pdf_reader.page_numbers,
                            terminal_size,
                        ) {
                            error!("Failed to calculate PDF statistics: {e}");
                            self.show_error(format!("Failed to calculate statistics: {e}"));
                        } else {
                            opened = true;
                        }
                    }
                }

                if opened {
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    self.book_stat.show();
                    self.focused_panel = FocusedPanel::Popup(PopupWindow::BookStats);
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
                if self.is_pdf_mode() {
                    #[cfg(feature = "pdf")]
                    {
                        let page = self.pdf_reader.as_ref().map(|reader| reader.page);
                        if let (Some(page), Some(service)) = (page, self.pdf_service.as_mut()) {
                            service.extract_text(vec![crate::pdf::PageSelectionBounds {
                                page,
                                start_x: 0.0,
                                end_x: f32::MAX,
                                min_y: 0.0,
                                max_y: f32::MAX,
                            }]);
                            self.notifications.info("Extracting current page text...");
                        }
                    }
                } else if self.is_main_panel(MainPanel::Content) {
                    // Handle Space->c to copy entire chapter content
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
                } else if self.is_pdf_mode() {
                    #[cfg(feature = "pdf")]
                    {
                        self.open_comments_viewer_for_pdf();
                    }
                } else if self.current_book.is_some() {
                    // Open comments viewer - save current main panel
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    if let Some(ref mut book) = self.current_book {
                        let toc_items = self.navigation_panel.get_toc_items();
                        let current_chapter_href =
                            Self::get_chapter_href(&book.epub, book.current_chapter());
                        let book_title = Self::extract_book_title(&book.file);
                        let mut viewer = crate::widget::comments_viewer::CommentsViewer::new(
                            self.text_reader.get_comments(),
                            &mut book.epub,
                            &toc_items,
                            current_chapter_href,
                            book_title,
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
                // Handle Space->t to toggle theme selector (opens settings on Themes tab)
                if matches!(
                    self.focused_panel,
                    FocusedPanel::Popup(PopupWindow::Settings)
                ) {
                    // Close settings - return to previous panel
                    self.close_popup_to_previous();
                    self.settings_popup = None;
                } else {
                    // Open settings on Themes tab - save current main panel
                    if let FocusedPanel::Main(panel) = self.focused_panel {
                        self.previous_main_panel = panel;
                    }
                    self.settings_popup = Some(SettingsPopup::new_with_tab(SettingsTab::Themes));
                    self.focused_panel = FocusedPanel::Popup(PopupWindow::Settings);
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

        let _ = self.text_reader.dismiss_error_hud();

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
                    #[cfg(feature = "pdf")]
                    BookSearchAction::JumpToPdfPage {
                        page_index,
                        line_index,
                        line_y_bounds,
                        query,
                    } => {
                        self.set_main_panel_focus(MainPanel::Content);
                        self.jump_to_pdf_search_result(
                            page_index,
                            line_index,
                            line_y_bounds,
                            &query,
                        );
                    }
                    #[cfg(not(feature = "pdf"))]
                    BookSearchAction::JumpToPdfPage { .. } => {
                        // PDF not supported
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
                    if self.is_pdf_mode() {
                        #[cfg(feature = "pdf")]
                        self.navigate_pdf_to_page(chapter_index);
                    } else if let Err(e) = self.navigate_to_chapter(chapter_index) {
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

                        // Handle PDF comments (page-based)
                        #[cfg(feature = "pdf")]
                        if let Some(page) = target.page() {
                            if let Some(pdf_reader) = self.pdf_reader.as_mut() {
                                let action = pdf_reader.jump_to_page_action(page);
                                if let InputAction::JumpingToPage { page, .. } = action {
                                    if let Some(service) = self.pdf_service.as_mut() {
                                        service.apply_command(crate::pdf::Command::GoToPage(page));
                                    }
                                    if let Some(tx) = &self.pdf_conversion_tx {
                                        let _ = tx
                                            .send(crate::pdf::ConversionCommand::NavigateTo(page));
                                    }
                                }
                            }
                            self.comments_viewer = None;
                            return None;
                        }

                        // Set pending node restore before navigating (EPUB text comments only)
                        if let Some(node_index) = target.node_index() {
                            self.text_reader.restore_to_node_index(node_index);
                        }

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
                            let is_pdf_comment = entry.primary_comment().is_pdf();
                            let mut delete_success = false;
                            let mut error_msg: Option<String> = None;

                            #[cfg(feature = "pdf")]
                            if is_pdf_comment {
                                let book_comments = self
                                    .pdf_reader
                                    .as_ref()
                                    .and_then(|r| r.book_comments.clone());
                                if let Some(book_comments) = book_comments {
                                    match book_comments.lock() {
                                        Ok(mut guard) => {
                                            for comment in &entry.comments {
                                                if let Err(e) =
                                                    guard.delete_comment_by_id(&comment.id)
                                                {
                                                    error!("Failed to delete comment: {e}");
                                                    error_msg = Some(format!(
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
                                            error_msg = Some(
                                                "Failed to delete comment: lock error".to_string(),
                                            );
                                        }
                                    }
                                }
                                if delete_success {
                                    if let Some(pdf_reader) = self.pdf_reader.as_mut() {
                                        pdf_reader.refresh_comment_rects();
                                    }
                                }
                            }

                            #[cfg(not(feature = "pdf"))]
                            let _ = is_pdf_comment;

                            if !is_pdf_comment {
                                let comments = self.text_reader.get_comments();
                                match comments.lock() {
                                    Ok(mut guard) => {
                                        for comment in &entry.comments {
                                            if let Err(e) = guard.delete_comment_by_id(&comment.id)
                                            {
                                                error!("Failed to delete comment: {e}");
                                                error_msg =
                                                    Some(format!("Failed to delete comment: {e}"));
                                                delete_success = false;
                                                break;
                                            }
                                            delete_success = true;
                                        }
                                    }
                                    Err(_) => {
                                        error!("Failed to lock comments for deletion");
                                        error_msg = Some(
                                            "Failed to delete comment: lock error".to_string(),
                                        );
                                    }
                                }

                                if delete_success {
                                    for comment in &entry.comments {
                                        self.text_reader.delete_comment_by_id(&comment.id);
                                    }
                                }
                            }

                            if let Some(msg) = error_msg {
                                self.show_error(msg);
                            } else if delete_success {
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
                    CommentsViewerAction::ExportComments { filename } => {
                        if let Some(ref viewer) = self.comments_viewer {
                            let exporter = viewer.create_exporter();
                            let content = exporter.generate_markdown();
                            match std::fs::write(&filename, &content) {
                                Ok(_) => {
                                    self.show_info(format!("Exported to {filename}"));
                                }
                                Err(e) => {
                                    error!("Failed to export comments to {filename}: {e}");
                                    self.show_error(format!("Failed to export: {e}"));
                                }
                            }
                        }
                    }
                }
            }
            return None;
        }

        // If settings popup is shown, handle keys for it
        if self.focused_panel == FocusedPanel::Popup(PopupWindow::Settings) {
            let action = if let Some(ref mut popup) = self.settings_popup {
                popup.handle_key(key, &mut self.key_sequence)
            } else {
                None
            };

            if let Some(action) = action {
                match action {
                    SettingsAction::Close => {
                        self.close_popup_to_previous();
                        self.settings_popup = None;
                    }
                    SettingsAction::SettingsChanged => {
                        // Invalidate render cache for theme changes
                        self.text_reader.invalidate_render_cache();
                        self.apply_theme_to_pdf_reader();
                        self.show_info(format!("Theme: {}", current_theme_name()));

                        // Refresh book list in case PDF support was toggled
                        self.book_manager.refresh();
                        self.navigation_panel
                            .book_list
                            .set_books(self.book_manager.get_books());

                        #[cfg(feature = "pdf")]
                        {
                            // Close any open PDF if PDF support was disabled
                            if !crate::settings::is_pdf_enabled() && self.is_pdf_mode() {
                                if let Some(ref pdf_reader) = self.pdf_reader {
                                    Self::clear_pdf_graphics(pdf_reader.is_kitty);
                                }
                                self.pdf_service = None;
                                self.pdf_reader = None;
                                self.pdf_picker = None;
                                self.pdf_conversion_tx = None;
                                self.pdf_conversion_rx = None;
                                self.pdf_pending_display = None;
                                self.pdf_document_path = None;
                                // Clear current book path since the PDF is no longer available
                                self.navigation_panel.current_book_path = None;
                                // Switch back to book list
                                self.navigation_panel.switch_to_book_mode();
                                self.focused_panel = FocusedPanel::Main(MainPanel::NavigationList);
                                self.close_popup_to_previous();
                                self.settings_popup = None;
                            }
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
            // Check for global hotkeys first
            if self.handle_global_hotkeys(key) {
                return None;
            }

            let action = self
                .navigation_panel
                .handle_key(key, &mut self.key_sequence);
            let bypass = action
                .map(|a| self.handle_navigation_panel_action(a))
                .unwrap_or(false);

            if key.code == KeyCode::Char('q') {
                self.save_bookmark_with_throttle(true);
                return Some(AppAction::Quit);
            }

            // Handle ESC in navigation panel mode - dismiss notifications or exit search
            if key.code == KeyCode::Esc {
                if self.notifications.has_notification() {
                    self.notifications.dismiss();
                } else if self.is_in_search_mode() {
                    self.cancel_current_search();
                }
                return None;
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
                        // Clear search first if active (pressing Esc again will exit visual mode)
                        if self.text_reader.is_searching() {
                            self.cancel_current_search();
                            return None;
                        }
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
                KeyCode::Enter => {
                    self.text_reader.clear_count();
                    if let Some(link_info) = self.text_reader.get_link_at_cursor() {
                        if let Err(e) = self.handle_link_click(&link_info) {
                            error!("Failed to handle link click: {e}");
                        }
                    }
                    return None;
                }
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
                    // Clear search first if active (pressing Esc again will exit normal mode)
                    if self.text_reader.is_searching() {
                        self.cancel_current_search();
                        return None;
                    }
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
                if !self.has_active_popup() && !self.zen_mode {
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
            KeyCode::Char('-') => {
                let current_node = self.text_reader.get_current_node_index();
                self.text_reader.increase_margin();
                self.text_reader.restore_to_node_index(current_node);
                settings::set_margin(self.text_reader.get_margin());
            }
            KeyCode::Char('=') | KeyCode::Char('+') => {
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
                            y_bounds: None,
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
                        y_bounds: None,
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
                                y_bounds: None,
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
                                y_bounds: None,
                            });
                        }
                    }
                }
                Block::DefinitionList { items } => {
                    for item in items {
                        lines.push(SearchLine {
                            text: extract_text_from_text(&item.term),
                            node_index,
                            y_bounds: None,
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

    /// Initialize search engine for PDF documents
    /// This extracts text from all PDF pages and indexes them for search
    #[cfg(feature = "pdf")]
    fn initialize_pdf_search_engine(&mut self) {
        use mupdf::text_page::TextBlockType;
        use mupdf::{Document, TextPageFlags};

        let Some(ref doc_path) = self.pdf_document_path else {
            error!("Cannot initialize PDF search - no document path");
            return;
        };

        info!("Initializing PDF search engine for {doc_path:?}");

        let doc = match Document::open(doc_path.to_string_lossy().as_ref()) {
            Ok(d) => d,
            Err(e) => {
                error!("Failed to open PDF document for search: {e}");
                return;
            }
        };

        let page_count = doc.page_count().unwrap_or(0) as usize;
        let mut pages = Vec::with_capacity(page_count);

        for page_num in 0..page_count {
            let Ok(page) = doc.load_page(page_num as i32) else {
                continue;
            };

            let Ok(text_page) = page.to_text_page(TextPageFlags::empty()) else {
                continue;
            };

            let mut lines = Vec::new();
            let mut line_idx = 0;

            for block in text_page.blocks() {
                if block.r#type() != TextBlockType::Text {
                    continue;
                }

                for line in block.lines() {
                    let bbox = line.bounds();
                    let text: String = line.chars().filter_map(|ch| ch.char()).collect();

                    if !text.trim().is_empty() {
                        lines.push(SearchLine {
                            text,
                            node_index: line_idx,
                            y_bounds: Some((bbox.y0, bbox.y1)),
                        });
                    }
                    line_idx += 1;
                }
            }

            pages.push((page_num, lines));
        }

        info!("PDF search indexed {} pages", pages.len());

        let mut search_engine = SearchEngine::new();
        search_engine.process_pdf_pages(pages);

        self.book_search = Some(BookSearch::new(search_engine));
    }

    fn open_book_search(&mut self, clear_input: bool) {
        // For PDF, lazily initialize the search engine when first requested
        #[cfg(feature = "pdf")]
        if self.pdf_reader.is_some() && self.book_search.is_none() {
            self.initialize_pdf_search_engine();
        }

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

    /// Poll PDF render service for completed renders and update state
    /// Returns true if any renders were processed
    #[cfg(feature = "pdf")]
    pub fn poll_pdf_renders(&mut self) -> bool {
        let Some(service) = self.pdf_service.as_mut() else {
            return false;
        };

        let responses = service.poll_responses();
        let Some(pdf_reader) = self.pdf_reader.as_mut() else {
            return false;
        };

        let result = crate::widget::pdf_reader::apply_render_responses(
            pdf_reader,
            responses,
            self.pdf_conversion_tx.as_ref(),
            self.pdf_conversion_rx.as_ref(),
            self.pdf_picker.as_ref(),
            &mut self.notifications,
        );

        // Clear waiting flags when a frame arrives
        if let Some(frame_page) = result.converted_frame_page {
            if self.pdf_waiting_for_page == Some(frame_page) {
                log::trace!("Clearing pdf_waiting_for_page: page {frame_page} frame arrived");
                self.pdf_waiting_for_page = None;
            }
            // Clear viewport waiting - any frame arrival means converter is responding
            if self.pdf_waiting_for_viewport {
                log::trace!("Clearing pdf_waiting_for_viewport: frame arrived");
                self.pdf_waiting_for_viewport = false;
            }
        }

        result.updated
    }

    #[cfg(not(feature = "pdf"))]
    pub fn poll_pdf_renders(&mut self) -> bool {
        false
    }

    /// Navigate PDF to a specific page (from TOC navigation)
    #[cfg(feature = "pdf")]
    fn navigate_pdf_to_page(&mut self, page: usize) {
        let toc_height = self.get_navigation_panel_area().height as usize;
        let Some(mut pdf_reader) = self.pdf_reader.take() else {
            return;
        };
        crate::widget::pdf_reader::navigate_pdf_to_page(
            &mut pdf_reader,
            page,
            self.pdf_service.as_mut(),
            self.pdf_conversion_tx.as_ref(),
            &mut self.navigation_panel.table_of_contents,
            toc_height,
            &mut self.bookmarks,
            &mut self.last_bookmark_save,
        );
        // For non-Kitty protocols: wait for the page to be converted before redrawing
        if !pdf_reader.is_kitty {
            self.pdf_waiting_for_page = Some(page);
            log::trace!("Set pdf_waiting_for_page to {page} for TOC navigation");
        }
        self.pdf_reader = Some(pdf_reader);
    }

    /// Jump to a PDF search result, navigating to the page and selecting the matched text
    #[cfg(feature = "pdf")]
    fn jump_to_pdf_search_result(
        &mut self,
        page_index: usize,
        _line_index: usize,
        _line_y_bounds: (f32, f32),
        query: &str,
    ) {
        let toc_height = self.get_navigation_panel_area().height as usize;
        let Some(mut pdf_reader) = self.pdf_reader.take() else {
            return;
        };

        // Use the jump_to_search_result method which handles navigation + selection
        let action = pdf_reader.jump_to_search_result(page_index, query);

        // Process the resulting action using apply_input_action
        let _outcome = pdf_reader.apply_input_action(
            action,
            self.pdf_service.as_mut(),
            self.pdf_conversion_tx.as_ref(),
            &mut self.notifications,
            &mut self.bookmarks,
            &mut self.last_bookmark_save,
            &mut self.navigation_panel.table_of_contents,
            toc_height,
            &self.profiler,
        );

        // For non-Kitty protocols: wait for the page to be converted before redrawing
        if !pdf_reader.is_kitty {
            self.pdf_waiting_for_page = Some(page_index);
            log::trace!("Set pdf_waiting_for_page to {page_index} for search result");
        }
        self.pdf_reader = Some(pdf_reader);
    }

    /// Handle input event in PDF mode
    /// Returns Some(AppAction::Quit) if the app should quit
    #[cfg(feature = "pdf")]
    fn handle_pdf_event(&mut self, event: &crossterm::event::Event) -> PdfEventResult {
        let toc_height = self.get_navigation_panel_area().height as usize;
        let Some(mut pdf_reader) = self.pdf_reader.take() else {
            return PdfEventResult {
                handled: false,
                action: None,
            };
        };
        let response = pdf_reader.handle_event(event);
        if !response.handled {
            self.pdf_reader = Some(pdf_reader);
            return PdfEventResult {
                handled: false,
                action: None,
            };
        }

        // Track current page before action to detect navigation
        let page_before = pdf_reader.page;
        let is_kitty = pdf_reader.is_kitty;

        // For non-Kitty protocols: set viewport waiting flag before applying action
        // Don't redraw until the converter responds
        let is_viewport_change = matches!(response.action, Some(InputAction::ViewportChanged(_)));
        if !is_kitty && is_viewport_change {
            self.pdf_waiting_for_viewport = true;
            log::trace!("Set pdf_waiting_for_viewport=true for scroll event");
        }

        let outcome = if let Some(action) = response.action {
            pdf_reader.apply_input_action(
                action,
                self.pdf_service.as_mut(),
                self.pdf_conversion_tx.as_ref(),
                &mut self.notifications,
                &mut self.bookmarks,
                &mut self.last_bookmark_save,
                &mut self.navigation_panel.table_of_contents,
                toc_height,
                &self.profiler,
            )
        } else {
            InputOutcome::None
        };

        // For non-Kitty protocols: if page changed, wait for the new page to be ready
        let page_after = pdf_reader.page;
        if !is_kitty && page_after != page_before {
            self.pdf_waiting_for_page = Some(page_after);
            log::trace!("Set pdf_waiting_for_page to {page_after} (was {page_before})");
        }

        self.pdf_reader = Some(pdf_reader);

        let action = match outcome {
            InputOutcome::Quit => Some(AppAction::Quit),
            InputOutcome::None => None,
        };

        PdfEventResult {
            handled: true,
            action,
        }
    }

    /// Open comments viewer from PDF mode
    #[cfg(feature = "pdf")]
    fn open_comments_viewer_for_pdf(&mut self) {
        let Some(pdf_reader) = self.pdf_reader.as_ref() else {
            return;
        };
        let Some(book_comments) = pdf_reader.book_comments.clone() else {
            return;
        };

        if let FocusedPanel::Main(panel) = self.focused_panel {
            self.previous_main_panel = panel;
        }

        let doc_id = &pdf_reader.comments_doc_id;
        let toc_entries = &pdf_reader.toc_entries;
        let current_page = pdf_reader.page;
        let book_title = pdf_reader
            .doc_title
            .clone()
            .unwrap_or_else(|| pdf_reader.name.clone());
        let page_count = self
            .pdf_service
            .as_ref()
            .and_then(|s| s.document_info())
            .map(|info| info.page_count)
            .unwrap_or(0);

        let mut viewer = crate::widget::comments_viewer::CommentsViewer::new_for_pdf(
            book_comments,
            doc_id,
            toc_entries,
            page_count,
            current_page,
            book_title,
        );
        viewer.restore_position();
        self.comments_viewer = Some(viewer);
        self.focused_panel = FocusedPanel::Popup(PopupWindow::CommentsViewer);
    }

    /// Apply current global theme to PDF reader
    #[cfg(feature = "pdf")]
    fn apply_theme_to_pdf_reader(&mut self) {
        let palette = current_theme();
        let theme_index = crate::theme::current_theme_index();

        if let Some(pdf_reader) = self.pdf_reader.as_mut() {
            crate::widget::pdf_reader::apply_theme_to_pdf_reader(
                pdf_reader,
                palette,
                theme_index,
                self.pdf_service.as_mut(),
                self.pdf_conversion_tx.as_ref(),
            );
        }
    }

    #[cfg(not(feature = "pdf"))]
    fn apply_theme_to_pdf_reader(&mut self) {
        // No-op when PDF feature is disabled
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
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
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

            // Route events to PDF handler when in PDF mode AND (focused on PDF content OR popup is active)
            #[cfg(feature = "pdf")]
            if app.is_pdf_mode()
                && (app.is_main_panel(MainPanel::Content) || app.has_active_popup())
            {
                use crossterm::event::KeyCode;
                match &event {
                    Event::Key(key) => {
                        // When a popup is active, route key events through the standard handler
                        // so popups can be closed with ESC and other keys work correctly
                        if app.has_active_popup() {
                            let visible_height =
                                terminal.size().unwrap().height.saturating_sub(5) as usize;
                            if app.handle_key_event_with_screen_height(*key, Some(visible_height))
                                == Some(AppAction::Quit)
                            {
                                should_quit = true;
                            }
                            continue;
                        }

                        let result = app.handle_pdf_event(&event);
                        if result.action == Some(AppAction::Quit) {
                            should_quit = true;
                        }
                        if result.handled {
                            continue;
                        }

                        if !app.pdf_text_input_active() && app.handle_global_hotkeys(*key) {
                            continue;
                        }
                        if key.code == KeyCode::Char('/') && !app.pdf_text_input_active() {
                            if let FocusedPanel::Main(panel) = app.focused_panel {
                                app.previous_main_panel = panel;
                            }
                            app.open_book_search(false);
                            continue;
                        }
                        if key.code == KeyCode::Tab && !app.has_active_popup() && !app.zen_mode {
                            app.set_main_panel_focus(MainPanel::NavigationList);
                        }
                    }
                    Event::Resize(_, _) => {
                        app.handle_resize();
                        if let Some(pdf_reader) = app.pdf_reader.as_mut() {
                            pdf_reader.force_redraw();
                        }
                    }
                    Event::Mouse(mouse_event) => {
                        if app.should_route_pdf_mouse_to_ui(mouse_event) {
                            match mouse_event.kind {
                                MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {}
                                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                                    app.handle_and_drain_mouse_events(
                                        *mouse_event,
                                        Some(event_source),
                                    );
                                }
                                _ => {
                                    app.handle_non_scroll_mouse_event(*mouse_event);
                                }
                            }
                        } else {
                            let result = app.handle_pdf_event(&event);
                            if result.action == Some(AppAction::Quit) {
                                should_quit = true;
                            }
                        }
                    }
                    _ => {
                        let result = app.handle_pdf_event(&event);
                        if result.action == Some(AppAction::Quit) {
                            should_quit = true;
                        }
                    }
                }
            } else {
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
                        let visible_height =
                            terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
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
            }
            #[cfg(not(feature = "pdf"))]
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
            let epub_hud_expired = app.text_reader.update_hud_message();
            let images_loaded = app.text_reader.check_for_loaded_images();
            let notification_expired = app.notifications.update();
            #[cfg(feature = "pdf")]
            let pdf_hud_expired = app
                .pdf_reader
                .as_mut()
                .is_some_and(|reader| reader.update_hud_message());
            let pdf_renders_ready = app.poll_pdf_renders();
            if images_loaded {
                needs_redraw = true;
                debug!("Images loaded, forcing redraw");
            }
            if highlight_changed {
                needs_redraw = true;
                debug!("Highlight expired, forcing redraw");
            }
            if epub_hud_expired {
                needs_redraw = true;
            }
            if notification_expired {
                needs_redraw = true;
            }
            #[cfg(feature = "pdf")]
            if pdf_hud_expired {
                needs_redraw = true;
            }
            if pdf_renders_ready {
                needs_redraw = true;
            }
            last_tick = std::time::Instant::now();
        }

        // For non-Kitty PDF: suppress redraw while waiting for page/viewport to be converted.
        // This prevents flicker and wasted CPU drawing incomplete state.
        #[cfg(feature = "pdf")]
        if app.pdf_waiting_for_page.is_some() || app.pdf_waiting_for_viewport {
            let hud_active = app
                .pdf_reader
                .as_ref()
                .and_then(|reader| reader.hud_message.as_ref())
                .is_some();
            if !hud_active {
                needs_redraw = false;
            }
        }

        if needs_redraw {
            terminal.draw(|f| app.draw(f, &fps_counter))?;
            #[cfg(feature = "pdf")]
            {
                app.execute_pdf_display_plan();
                app.update_non_kitty_viewport();
                app.handle_kitty_eviction_responses(event_source);
            }
            let _ = execute!(stdout(), EndSynchronizedUpdate);
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
