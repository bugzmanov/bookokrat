use crate::book_manager::BookManager;
use crate::book_stat::BookStat;
use crate::bookmark::Bookmarks;
use crate::event_source::EventSource;
use crate::images::book_images::BookImages;
use crate::images::image_popup::ImagePopup;
use crate::images::image_storage::ImageStorage;
use crate::jump_list::{JumpList, JumpLocation};
use crate::markdown_text_reader::MarkdownTextReader;
use crate::navigation_panel::{CurrentBookInfo, NavigationMode, NavigationPanel};
use crate::parsing::text_generator::TextGenerator;
use crate::reading_history::ReadingHistory;
use crate::system_command::{
    MockSystemCommandExecutor, RealSystemCommandExecutor, SystemCommandExecutor,
};
use crate::table_of_contents::{SelectedTocItem, TocItem};
use crate::text_reader_trait::{LinkInfo, TextReaderTrait};
use crate::theme::OCEANIC_NEXT;
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
use crossterm::event::{Event, KeyCode, MouseButton, MouseEvent, MouseEventKind};
use epub::doc::EpubDoc;
use log::{debug, error, info};
use ratatui::{
    Terminal,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

pub struct App {
    pub book_manager: BookManager,
    pub navigation_panel: NavigationPanel,
    text_generator: TextGenerator,
    text_reader: MarkdownTextReader,
    bookmarks: Bookmarks,
    image_storage: Arc<ImageStorage>,
    book_images: BookImages,
    current_epub: Option<EpubDoc<BufReader<std::fs::File>>>,
    current_chapter: usize,
    total_chapters: usize,
    current_file: Option<String>,
    current_chapter_title: Option<String>,
    pub focused_panel: FocusedPanel,
    pub system_command_executor: Box<dyn SystemCommandExecutor>,
    last_bookmark_save: std::time::Instant,
    // Click tracking for double/triple-click detection
    last_click_time: Option<Instant>,
    last_click_position: Option<(u16, u16)>,
    click_count: u32,
    // Cached chapter information to avoid re-parsing on every render
    cached_current_book_info: Option<CurrentBookInfo>,
    // Key sequence tracking for multi-key commands
    key_sequence: Vec<char>,
    last_key_time: Option<Instant>,
    // Store terminal dimensions for calculating panel boundaries
    terminal_width: u16,
    terminal_height: u16,
    // Reading history
    reading_history: Option<ReadingHistory>,
    show_reading_history: bool,
    // Image popup
    image_popup: Option<ImagePopup>,
    image_popup_area: Option<Rect>,
    last_terminal_size: Rect,
    profiler: Arc<Mutex<Option<pprof::ProfilerGuard<'static>>>>,
    // Book statistics popup
    book_stat: BookStat,
    show_book_stat: bool,
    // Jump list for navigation history (Ctrl+O/Ctrl+I)
    jump_list: JumpList,
}

pub trait VimNavMotions {
    fn handle_h(&mut self);
    fn handle_j(&mut self);
    fn handle_k(&mut self);
    fn handle_l(&mut self);
    fn handle_ctrl_d(&mut self);
    fn handle_ctrl_u(&mut self);
    fn handle_gg(&mut self);
    fn handle_G(&mut self);
}

#[derive(PartialEq, Debug)]
pub enum FocusedPanel {
    FileList,
    Content,
}

#[derive(PartialEq, Debug)]
enum ClickType {
    Single,
    Double,
    Triple,
}

impl App {
    pub fn new() -> Self {
        Self::new_with_config(None, Some("bookmarks.json"), true)
    }

    pub fn new_with_mock_system_executor(
        book_directory: Option<&str>,
        bookmark_file: Option<&str>,
        auto_load_recent: bool,
        system_executor: MockSystemCommandExecutor,
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
        let text_generator = crate::parsing::text_generator::TextGenerator::new();

        let text_reader = MarkdownTextReader::new();

        let bookmarks = Bookmarks::load_or_ephemeral(bookmark_file);

        // Initialize image storage in project temp directory
        let image_storage = Arc::new(ImageStorage::new_in_project_temp().unwrap_or_else(|e| {
            error!("Failed to initialize image storage: {}. Using fallback.", e);
            // Create a fallback image storage in system temp directory
            ImageStorage::new(std::env::temp_dir().join("bookrat_images"))
                .expect("Failed to create fallback image storage")
        }));

        // Initialize book images abstraction
        let book_images = BookImages::new(image_storage.clone());

        // let guard = pprof::ProfilerGuardBuilder::default()
        //     .frequency(1000)
        //     .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        //     .build()
        //     .unwrap();
        let mut app = Self {
            book_manager,
            navigation_panel,
            text_generator,
            text_reader,
            bookmarks,
            image_storage,
            book_images,
            current_epub: None,
            current_chapter: 0,
            total_chapters: 0,
            current_file: None,
            current_chapter_title: None,
            focused_panel: FocusedPanel::FileList,
            system_command_executor: system_executor,
            last_bookmark_save: std::time::Instant::now(),
            // Initialize click tracking
            last_click_time: None,
            last_click_position: None,
            click_count: 0,
            cached_current_book_info: None,
            key_sequence: Vec::new(),
            last_key_time: None,
            terminal_width: 80,  // Default width, will be updated on render
            terminal_height: 24, // Default height, will be updated on render
            reading_history: None,
            show_reading_history: false,
            image_popup: None,
            image_popup_area: None,
            last_terminal_size: Rect::new(0, 0, 80, 24),
            profiler: Arc::new(Mutex::new(None)),
            book_stat: BookStat::new(),
            show_book_stat: false,
            jump_list: JumpList::new(20),
        };

        // Get actual terminal size on startup
        if let Ok((width, height)) = crossterm::terminal::size() {
            app.terminal_width = width;
            app.terminal_height = height;
            debug!("Initial terminal size: {}x{}", width, height);
        }

        // Auto-load the most recently read book if available
        if auto_load_recent {
            if let Some((recent_path, _)) = app.bookmarks.get_most_recent() {
                // Check if the most recent book still exists in the managed books
                if app.book_manager.contains_book(&recent_path) {
                    info!("Auto-loading most recent book: {}", recent_path);

                    // Find the book index before loading
                    let book_index = app
                        .book_manager
                        .books
                        .iter()
                        .position(|book| book.path == recent_path);

                    if let Some(idx) = book_index {
                        // Use the high-level action method to ensure consistent state
                        if let Err(e) = app.open_book_for_reading(idx) {
                            error!("Failed to auto-load most recent book: {}", e);
                        }
                    }
                }
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

            // Save bookmark for current file before switching
            self.save_bookmark_with_throttle(true);

            // Load the EPUB document
            self.load_epub_internal(&path)?;

            // Update UI state to reflect the opened book
            self.navigation_panel.current_book_index = Some(book_index);
            if let Some(book_info) = self.cached_current_book_info.clone() {
                self.navigation_panel
                    .switch_to_toc_mode(book_index, book_info);
            }

            // Switch focus to content after loading
            self.focused_panel = FocusedPanel::Content;

            info!("Successfully opened book for reading: {}", path);
            Ok(())
        } else {
            anyhow::bail!("Invalid book index: {}", book_index)
        }
    }

    /// Navigate to a specific chapter - ensures all state is properly updated
    pub fn navigate_to_chapter(&mut self, chapter_index: usize) -> Result<()> {
        if let Some(doc) = &mut self.current_epub {
            if chapter_index < self.total_chapters {
                if doc.set_current_page(chapter_index) {
                    self.current_chapter = chapter_index;

                    self.text_reader.clear_active_anchor();
                    self.update_content();
                    self.update_current_chapter_in_cache();
                    self.save_bookmark_with_throttle(true);

                    Ok(())
                } else {
                    anyhow::bail!("Failed to navigate to chapter {}", chapter_index)
                }
            } else {
                anyhow::bail!(
                    "Chapter index {} out of range (max: {})",
                    chapter_index,
                    self.total_chapters - 1
                )
            }
        } else {
            anyhow::bail!("No EPUB document loaded")
        }
    }

    /// Navigate to next or previous chapter - maintains all state consistency
    pub fn navigate_chapter_relative(&mut self, direction: ChapterDirection) -> Result<()> {
        if let Some(doc) = &mut self.current_epub {
            match direction {
                ChapterDirection::Next => {
                    if self.current_chapter < self.total_chapters - 1 {
                        if doc.go_next() {
                            self.current_chapter += 1;
                            info!("Moving to next chapter: {}", self.current_chapter + 1);
                            self.update_content();
                            self.update_current_chapter_in_cache();
                            self.save_bookmark_with_throttle(true);
                            Ok(())
                        } else {
                            anyhow::bail!("Failed to move to next chapter")
                        }
                    } else {
                        info!("Already at last chapter");
                        Ok(())
                    }
                }
                ChapterDirection::Previous => {
                    if self.current_chapter > 0 {
                        if doc.go_prev() {
                            self.current_chapter -= 1;
                            info!("Moving to previous chapter: {}", self.current_chapter + 1);
                            self.update_content();
                            self.update_current_chapter_in_cache();
                            self.save_bookmark_with_throttle(true);
                            Ok(())
                        } else {
                            anyhow::bail!("Failed to move to previous chapter")
                        }
                    } else {
                        info!("Already at first chapter");
                        Ok(())
                    }
                }
            }
        } else {
            anyhow::bail!("No document loaded")
        }
    }

    /// Switch back to book list mode - ensures clean state transition
    pub fn switch_to_book_list_mode(&mut self) {
        self.navigation_panel.switch_to_book_mode();
        self.focused_panel = FocusedPanel::FileList;
        info!("Switched to book list mode");
    }

    /// Open a book for reading by path - for testing and compatibility
    /// This method finds the book by path and uses the high-level action
    pub fn open_book_for_reading_by_path(&mut self, path: &str) -> Result<()> {
        // Find the book index by path
        let book_index = self
            .book_manager
            .books
            .iter()
            .position(|book| book.path == path)
            .ok_or_else(|| anyhow::anyhow!("Book not found in manager: {}", path))?;

        self.open_book_for_reading(book_index)
    }

    /// Legacy method for backward compatibility with tests
    /// This maintains the old behavior while using the new high-level action internally
    #[deprecated(note = "Use open_book_for_reading_by_path instead")]
    pub fn load_epub(&mut self, path: &str) {
        // For backward compatibility, ignore errors to match old behavior
        let _ = self.open_book_for_reading_by_path(path);
    }

    // =============================================================================
    // LOW-LEVEL INTERNAL METHODS
    // =============================================================================
    // These methods should only be called by high-level actions above

    fn load_epub_internal_without_bookmark(&mut self, path: &str) -> Result<()> {
        let doc = self
            .book_manager
            .load_epub(path)
            .map_err(|e| anyhow::anyhow!("Failed to load EPUB: {}", e))?;

        info!("Successfully loaded EPUB document (no bookmark restoration)");
        self.total_chapters = doc.get_num_pages();
        info!("Total chapters: {}", self.total_chapters);

        // Extract images from the EPUB to the temp directory
        let path_buf = std::path::PathBuf::from(path);
        if let Err(e) = self.image_storage.extract_images(&path_buf) {
            error!("Failed to extract images from EPUB: {}", e);
            // Continue loading even if image extraction fails
        } else {
            info!("Successfully extracted images from EPUB");
        }

        // Load the book in BookImages abstraction
        if let Err(e) = self.book_images.load_book(&path_buf) {
            error!("Failed to load book in BookImages: {}", e);
            // Continue loading even if BookImages fails
        }

        // Don't restore bookmarks - just set to chapter 0
        self.current_chapter = 0;

        self.current_epub = Some(doc);
        self.current_file = Some(path.to_string());
        self.update_content();
        self.refresh_chapter_cache();
        Ok(())
    }

    fn load_epub_internal(&mut self, path: &str) -> Result<()> {
        let mut doc = self
            .book_manager
            .load_epub(path)
            .map_err(|e| anyhow::anyhow!("Failed to load EPUB: {}", e))?;

        info!("Successfully loaded EPUB document");
        self.total_chapters = doc.get_num_pages();
        info!("Total chapters: {}", self.total_chapters);

        let path_buf = std::path::PathBuf::from(path);
        if let Err(e) = self.image_storage.extract_images(&path_buf) {
            error!("Failed to extract images from EPUB: {}", e);
        }

        // Load the book in BookImages abstraction
        if let Err(e) = self.book_images.load_book(&path_buf) {
            error!("Failed to load book in BookImages: {}", e);
            // Continue loading even if BookImages fails
        }

        // Try to load bookmark
        if let Some(bookmark) = self.bookmarks.get_bookmark(path) {
            // Skip metadata page if needed
            if bookmark.chapter > 0 {
                for _ in 0..bookmark.chapter {
                    if !doc.go_next() {
                        error!("Failed to navigate to bookmarked chapter");
                        break;
                    }
                }
                self.current_chapter = bookmark.chapter;
                self.text_reader
                    .restore_scroll_position(bookmark.scroll_offset);
            }
        } else {
            // Skip the first chapter if it's just metadata
            if self.total_chapters > 1 {
                if doc.go_next() {
                    self.current_chapter = 1;
                    info!("Skipped metadata page, moved to chapter 2");
                } else {
                    return Err(anyhow::anyhow!("Failed to move to next chapter"));
                }
            }
        }

        self.current_epub = Some(doc);
        self.current_file = Some(path.to_string());
        self.update_content();
        self.refresh_chapter_cache();
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

    fn refresh_chapter_cache(&mut self) {
        if let (Some(current_file), Some(epub)) = (&self.current_file, &mut self.current_epub) {
            let toc_items = self.text_generator.parse_toc_structure(epub);

            let active_section = self.text_reader.get_active_section(self.current_chapter);

            let current_chapter_href = Self::get_chapter_href(epub, self.current_chapter);

            let book_info = CurrentBookInfo {
                path: current_file.clone(),
                toc_items,
                current_chapter: self.current_chapter,
                current_chapter_href,
                active_section,
            };
            self.cached_current_book_info = Some(book_info.clone());

            // Update the table of contents with the new book info
            if self.navigation_panel.mode == NavigationMode::TableOfContents {
                self.navigation_panel
                    .table_of_contents
                    .set_current_book_info(book_info);
            }
        } else {
            self.cached_current_book_info = None;
        }
    }

    fn update_current_chapter_in_cache(&mut self) {
        // Get the navigation area first to avoid borrow issues
        let nav_area = if self.navigation_panel.mode == NavigationMode::TableOfContents {
            Some(self.get_navigation_panel_area())
        } else {
            None
        };

        if let Some(ref mut cached_info) = self.cached_current_book_info {
            cached_info.current_chapter = self.current_chapter;

            // Update active section
            cached_info.active_section = self.text_reader.get_active_section(self.current_chapter);

            // Update the table of contents with the updated book info
            if self.navigation_panel.mode == NavigationMode::TableOfContents {
                self.navigation_panel
                    .table_of_contents
                    .set_current_book_info(cached_info.clone());

                // Update the active section and ensure it's visible
                if let Some(area) = nav_area {
                    let toc_height = area.height as usize;
                    self.navigation_panel
                        .table_of_contents
                        .update_active_section(&cached_info.active_section, toc_height);
                }
            }
        }
    }

    pub fn save_bookmark(&mut self) {
        self.save_bookmark_with_throttle(false);
    }

    pub fn save_bookmark_with_throttle(&mut self, force: bool) {
        if let Some(path) = &self.current_file {
            self.bookmarks.update_bookmark(
                path,
                self.current_chapter,
                self.text_reader.get_scroll_offset(),
                self.total_chapters,
            );

            // Only save to disk if enough time has passed or if forced
            let now = std::time::Instant::now();
            if force
                || now.duration_since(self.last_bookmark_save)
                    > std::time::Duration::from_millis(500)
            {
                if let Err(e) = self.bookmarks.save() {
                    error!("Failed to save bookmark: {}", e);
                }
                self.last_bookmark_save = now;
            }
        }
    }

    fn update_highlight(&mut self) -> bool {
        // Update highlight state in text reader
        self.text_reader.update_highlight()
    }

    fn update_content(&mut self) {
        if let Some(doc) = &mut self.current_epub {
            let (content, title) = match doc.get_current_str() {
                Some((raw_html, _mime)) => {
                    let title = self.text_generator.extract_chapter_title(&raw_html);
                    (raw_html, title)
                }
                None => {
                    error!("Failed to get raw HTML");
                    ("Error reading chapter content.".to_string(), None)
                }
            };

            self.current_chapter_title = title.clone();

            if let Some(chapter_file) = Self::get_chapter_href(doc, self.current_chapter) {
                self.text_reader
                    .set_current_chapter_file(Some(chapter_file));
            } else {
                self.text_reader.set_current_chapter_file(None);
            }

            self.text_reader.content_updated(content.len());

            if self.current_file.is_some() {
                self.text_reader.set_content_from_string(&content, title);
                self.text_reader.preload_image_dimensions(&self.book_images);
            }
        } else {
            error!("No EPUB document loaded");
            self.text_reader.content_updated(0);
        }
    }

    /// Toggle expansion of a section by its title
    /// Find a TOC item by title and return a mutable reference
    fn find_toc_item_mut<'a>(toc_items: &'a mut [TocItem], title: &str) -> Option<&'a mut TocItem> {
        for item in toc_items {
            if item.title() == title {
                return Some(item);
            }
            if let TocItem::Section { children, .. } = item {
                if let Some(found) = Self::find_toc_item_mut(children, title) {
                    return Some(found);
                }
            }
        }
        None
    }

    pub fn scroll_down(&mut self) {
        self.text_reader.scroll_down();
        self.save_bookmark();
        self.update_current_chapter_in_cache(); // This will update active section
    }

    pub fn scroll_up(&mut self) {
        self.text_reader.scroll_up();
        self.save_bookmark();
        self.update_current_chapter_in_cache(); // This will update active section
    }

    pub fn scroll_half_screen_down(&mut self, screen_height: usize) {
        self.text_reader.scroll_half_screen_down(screen_height);
        self.save_bookmark();
        self.update_current_chapter_in_cache(); // This will update active section
    }

    fn scroll_half_screen_up(&mut self, screen_height: usize) {
        self.text_reader.scroll_half_screen_up(screen_height);
        self.save_bookmark();
        self.update_current_chapter_in_cache(); // This will update active section
    }

    /// Handle a mouse event with optional batching for scroll events
    /// When event_source is provided, scroll events will be batched for smoother scrolling
    pub fn handle_mouse_event(
        &mut self,
        initial_mouse_event: MouseEvent,
        event_source: Option<&mut dyn crate::event_source::EventSource>,
    ) {
        use std::time::Duration;

        let start_time = std::time::Instant::now();

        // Extra validation for horizontal scrolls to prevent crossterm overflow bug
        if matches!(
            initial_mouse_event.kind,
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
        ) {
            if !self.is_valid_mouse_coordinates(initial_mouse_event.column, initial_mouse_event.row)
            {
                return;
            }
        }

        let is_scroll_event = matches!(
            initial_mouse_event.kind,
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
        );

        if !is_scroll_event {
            self.handle_non_scroll_mouse_event(initial_mouse_event);
            return;
        }

        // Handle scroll events - with or without batching
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

        // Store the initial mouse position to determine which area to scroll
        let initial_column = initial_mouse_event.column;
        let initial_row = initial_mouse_event.row;

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
                debug!(
                    "Batching timeout reached ({}ms), breaking out of event drain loop",
                    batch_start_time.elapsed().as_millis()
                );
                break;
            }

            // Safety check - if we're draining too many events, something might be wrong
            if drain_count > 20 {
                debug!(
                    "Warning: draining many events ({}), may indicate event accumulation issue",
                    drain_count
                );
            }

            match event_source.read() {
                Ok(Event::Mouse(mouse_event)) => match mouse_event.kind {
                    MouseEventKind::ScrollDown => scroll_down_count += 1,
                    MouseEventKind::ScrollUp => scroll_up_count += 1,
                    MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                        //ignore
                        break;
                    }
                    _ => {
                        self.handle_non_scroll_mouse_event(mouse_event);
                        break;
                    }
                },
                Ok(_) => {
                    // Non-mouse event, stop draining
                    break;
                }
                Err(e) => {
                    debug!("Error reading event during batching: {:?}", e);
                    break;
                }
            }
        }

        let net_scroll = scroll_down_count as i32 - scroll_up_count as i32;

        debug!(
            "Batched mouse events: {} down, {} up, net: {}, drained: {} events at position ({}, {})",
            scroll_down_count,
            scroll_up_count,
            net_scroll,
            drain_count,
            initial_column,
            initial_row
        );

        self.apply_scroll(net_scroll, initial_column);

        let elapsed = start_time.elapsed();
        if elapsed > std::time::Duration::from_millis(10) {
            debug!(
                "handle_mouse_event took {}ms for batched scroll",
                elapsed.as_millis()
            );
        }
    }

    /// Handle non-scroll mouse events (clicks, drags, etc.)
    fn handle_non_scroll_mouse_event(&mut self, mouse_event: MouseEvent) {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if image popup is shown first - close it on any click
                if self.image_popup.is_some() {
                    // Check if click is outside the popup area
                    if let Some(popup_area) = self.image_popup_area {
                        let click_x = mouse_event.column;
                        let click_y = mouse_event.row;

                        // If click is outside popup area, close the popup
                        if click_x < popup_area.x
                            || click_x >= popup_area.x + popup_area.width
                            || click_y < popup_area.y
                            || click_y >= popup_area.y + popup_area.height
                        {
                            self.image_popup = None;
                            self.image_popup_area = None;
                            debug!("Image popup closed via mouse click outside");
                        }
                    }
                    return; // Block all other interactions
                }

                // Check if reading history is shown next
                if self.show_reading_history {
                    let click_type = self.detect_click_type(mouse_event.column, mouse_event.row);

                    if let Some(ref mut history) = self.reading_history {
                        match click_type {
                            ClickType::Single => {
                                history.handle_mouse_click(mouse_event.column, mouse_event.row);
                            }
                            ClickType::Double => {
                                if history.handle_mouse_click(mouse_event.column, mouse_event.row) {
                                    // Double-click acts as Enter - open the selected book
                                    if let Some(path) = history.selected_path() {
                                        if let Some(book_index) =
                                            self.book_manager.find_book_index_by_path(path)
                                        {
                                            // Close history and open the book
                                            self.show_reading_history = false;
                                            self.reading_history = None;
                                            let _ = self.open_book_for_reading(book_index);
                                        }
                                    }
                                }
                            }
                            ClickType::Triple => {
                                history.handle_mouse_click(mouse_event.column, mouse_event.row);
                            }
                        }
                    }
                    return; // Don't process other clicks when history is shown
                }

                let nav_panel_width = self.calculate_navigation_panel_width();
                if mouse_event.column < nav_panel_width {
                    self.focused_panel = FocusedPanel::FileList;
                    self.text_reader.clear_selection();

                    let nav_area = self.get_navigation_panel_area();
                    let click_type = self.detect_click_type(mouse_event.column, mouse_event.row);

                    match click_type {
                        ClickType::Single => {
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
                        ClickType::Triple => {
                            self.navigation_panel.handle_mouse_click(
                                mouse_event.column,
                                mouse_event.row,
                                nav_area,
                            );
                        }
                    }
                } else {
                    // Click in content area (right 70% of screen)
                    if self.focused_panel != FocusedPanel::Content {
                        self.focused_panel = FocusedPanel::Content;
                        // Clear manual navigation flag when switching to content
                        self.navigation_panel
                            .table_of_contents
                            .clear_manual_navigation();
                    }

                    let content_area = self.get_content_area_rect();
                    let click_type = self.detect_click_type(mouse_event.column, mouse_event.row);

                    match click_type {
                        ClickType::Single => {
                            // Check if click is on a link first
                            if let Some(image_src) = self.text_reader.check_image_click(
                                mouse_event.column,
                                mouse_event.row,
                                content_area,
                            ) {
                                self.handle_image_click(&image_src, self.last_terminal_size);
                            } else {
                                self.text_reader.handle_mouse_down(
                                    mouse_event.column,
                                    mouse_event.row,
                                    content_area,
                                );
                            }
                        }
                        ClickType::Double => {
                            self.text_reader.handle_double_click(
                                mouse_event.column,
                                mouse_event.row,
                                content_area,
                            );
                        }
                        ClickType::Triple => {
                            self.text_reader.handle_triple_click(
                                mouse_event.column,
                                mouse_event.row,
                                content_area,
                            );
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // Block if image popup is shown
                if self.image_popup.is_some() {
                    return;
                }

                let nav_panel_width = self.calculate_navigation_panel_width();
                if mouse_event.column >= nav_panel_width {
                    let content_area = self.get_content_area_rect();
                    if let Some(url) = self.text_reader.handle_mouse_up(
                        mouse_event.column,
                        mouse_event.row,
                        content_area,
                    ) {
                        // Directly handle the URL-based link without trying to find LinkInfo again
                        if url.starts_with("http://") || url.starts_with("https://") {
                            self.open_external_link(&url);
                        } else {
                            // Handle internal link by classifying the URL directly
                            let (link_type, target_chapter, target_anchor) =
                                crate::markdown::classify_link_href(&url);
                            debug!(
                                "Internal link clicked: url='{}', link_type={:?}, target_chapter={:?}, target_anchor={:?}",
                                url, link_type, target_chapter, target_anchor
                            );

                            // Create a temporary LinkInfo for the link handling system
                            let temp_link_info = LinkInfo {
                                text: "".to_string(), // We don't need the text for navigation
                                url: url.clone(),
                                line: 0, // Not needed for navigation
                                start_col: 0,
                                end_col: 0,
                                link_type: Some(link_type),
                                target_chapter,
                                target_anchor,
                            };

                            if let Err(e) = self.handle_link_click(&temp_link_info) {
                                error!("Failed to handle link click: {}", e);
                            }
                        }
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Block if image popup is shown
                if self.image_popup.is_some() {
                    return;
                }

                let nav_panel_width = self.calculate_navigation_panel_width();
                if mouse_event.column >= nav_panel_width {
                    let content_area = self.get_content_area_rect();
                    let old_scroll_offset = self.text_reader.get_scroll_offset();
                    self.text_reader.handle_mouse_drag(
                        mouse_event.column,
                        mouse_event.row,
                        content_area,
                    );
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

    /// Handle image click by creating or showing the image popup
    fn handle_link_click(&mut self, link_info: &LinkInfo) -> std::io::Result<bool> {
        // Save current location to jump list before navigating
        if let Some(current_file) = &self.current_file {
            let current_location = JumpLocation {
                epub_path: current_file.clone(),
                chapter_index: self.current_chapter,
                scroll_position: self.text_reader.get_scroll_offset(),
                anchor: None,
            };
            self.jump_list.push(current_location);
        }

        match &link_info.link_type {
            Some(crate::markdown::LinkType::External) => self.handle_external_link(&link_info.url),
            Some(crate::markdown::LinkType::InternalAnchor) => {
                if let Some(anchor_id) = &link_info.target_anchor {
                    self.scroll_to_anchor(anchor_id)
                } else {
                    Ok(false)
                }
            }
            Some(crate::markdown::LinkType::InternalChapter) => {
                if let Some(chapter_file) = &link_info.target_chapter {
                    // Check if we're already in the target chapter
                    if let Some(current_chapter_file) = self.text_reader.get_current_chapter_file()
                    {
                        // Normalize paths by extracting just the filename for comparison
                        let current_filename = std::path::Path::new(current_chapter_file)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or(current_chapter_file);
                        let target_filename = std::path::Path::new(chapter_file)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or(chapter_file);

                        debug!(
                            "Comparing current chapter '{}' (filename: '{}') with target chapter '{}' (filename: '{}')",
                            current_chapter_file, current_filename, chapter_file, target_filename
                        );
                        if current_filename == target_filename {
                            // Same chapter - just scroll to anchor if provided
                            if let Some(anchor_id) = &link_info.target_anchor {
                                debug!(
                                    "Same-chapter link: scrolling to anchor '{}' in current chapter",
                                    anchor_id
                                );
                                self.scroll_to_anchor(anchor_id)
                            } else {
                                // No anchor - nothing to do (already in the right chapter)
                                Ok(true)
                            }
                        } else {
                            // Different chapter - navigate to it
                            self.navigate_to_chapter_by_file(
                                chapter_file,
                                link_info.target_anchor.as_ref(),
                            )
                        }
                    } else {
                        // No current chapter file info - try to navigate anyway
                        debug!(
                            "No current chapter file set - trying to navigate to '{}'",
                            chapter_file
                        );
                        self.navigate_to_chapter_by_file(
                            chapter_file,
                            link_info.target_anchor.as_ref(),
                        )
                    }
                } else {
                    Ok(false)
                }
            }
            None => {
                // Legacy link handling - fallback for old LinkInfo without classification
                self.handle_legacy_link_click(&link_info.url)
            }
        }
    }

    fn handle_external_link(&mut self, url: &str) -> std::io::Result<bool> {
        debug!("Opening external link: {}", url);
        if let Err(e) = open::that(url) {
            error!("Failed to open external link: {}", e);
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn scroll_to_anchor(&mut self, anchor_id: &str) -> std::io::Result<bool> {
        debug!("Searching for anchor: '{}'", anchor_id);
        if let Some(target_line) = self.text_reader.get_anchor_position(anchor_id) {
            self.text_reader.scroll_to_line(target_line);
            self.text_reader
                .highlight_line_temporarily(target_line, Duration::from_secs(2));
            debug!("Scrolled to anchor '{}' at line {}", anchor_id, target_line);
            Ok(true)
        } else {
            debug!("Anchor '{}' not found in current chapter", anchor_id);
            Ok(false)
        }
    }

    fn navigate_to_chapter_by_file(
        &mut self,
        chapter_file: &str,
        anchor_id: Option<&String>,
    ) -> std::io::Result<bool> {
        if let Some(chapter_index) = self.find_chapter_by_filename(chapter_file) {
            if let Err(_) = self.navigate_to_chapter(chapter_index) {
                return Ok(false);
            }

            // Store pending anchor scroll for after content loads
            if let Some(anchor) = anchor_id {
                self.text_reader
                    .handle_pending_anchor_scroll(Some(anchor.clone()));
            }

            Ok(true)
        } else {
            debug!("Chapter file '{}' not found in TOC", chapter_file);
            Ok(false)
        }
    }

    fn handle_legacy_link_click(&mut self, url: &str) -> std::io::Result<bool> {
        debug!("Handling legacy link click for: {}", url);

        // Check if it's an internal link (within the EPUB)
        if !url.starts_with("http://") && !url.starts_with("https://") {
            // Internal link - try to navigate to the chapter
            if let Some(ref mut doc) = self.current_epub {
                // Try to find the chapter with this href
                let normalized_url = url.trim_start_matches('#');

                // Search through spine for matching chapter
                for i in 0..doc.get_num_pages() {
                    if let Some(href) = Self::get_chapter_href(doc, i) {
                        if href.contains(normalized_url) || normalized_url.contains(&href) {
                            // Navigate to this chapter using existing navigation method
                            if let Err(e) = self.navigate_to_chapter(i) {
                                error!("Failed to navigate to chapter {}: {}", url, e);
                                return Ok(false);
                            }
                            return Ok(true);
                        }
                    }
                }

                // If not found in spine, might be a fragment/anchor in current chapter
                info!("Internal link not found in spine: {}", url);
            }
            Ok(false)
        } else {
            // External link - open in browser
            self.handle_external_link(url)
        }
    }

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
                        || href_without_anchor.ends_with(&format!("/{}", filename))
                        || (filename.contains('/') && href_without_anchor.ends_with(filename))
                    {
                        // Found the matching TOC item, now find its spine index
                        return self.find_spine_index_by_href(href);
                    }
                }
                TocItem::Section { href, children, .. } => {
                    // Check if this section matches
                    if let Some(section_href) = href {
                        // Strip any anchor from the href for comparison
                        let href_without_anchor =
                            section_href.split('#').next().unwrap_or(section_href);

                        if href_without_anchor == filename
                            || href_without_anchor.ends_with(&format!("/{}", filename))
                            || (filename.contains('/') && href_without_anchor.ends_with(filename))
                        {
                            // Found the matching section, find its spine index
                            return self.find_spine_index_by_href(section_href);
                        }
                    }
                    // Recursively search children
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
        let doc = self.current_epub.as_ref()?;

        // Normalize the href for comparison
        let normalized_href = self.text_generator.normalize_href(href);

        // Search through the spine
        for (index, spine_item) in doc.spine.iter().enumerate() {
            if let Some((path, _)) = doc.resources.get(&spine_item.idref) {
                let path_str = path.to_string_lossy();
                let normalized_path = self.text_generator.normalize_href(&path_str);

                // Check if the paths match
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

    fn open_external_link(&mut self, url: &str) {
        debug!("Opening external link: {}", url);

        // Use the open crate to open the URL in the default browser
        if let Err(e) = open::that(url) {
            error!("Failed to open URL {}: {}", url, e);
        }
    }

    fn handle_image_click(&mut self, image_src: &str, terminal_size: Rect) {
        debug!("Handling image click for: {}", image_src);

        // Get the image picker - required for creating protocols
        let picker = match self.text_reader.get_image_picker() {
            Some(picker) => picker,
            None => {
                debug!("No image picker available for popup");
                return;
            }
        };

        // Get the original image
        let original_image = if let Some(image) = self.text_reader.get_loaded_image(image_src) {
            debug!("Using already loaded image for popup: {}", image_src);
            image
        } else if let Some(image) = self.book_images.get_image(image_src) {
            debug!("Loading image directly for popup: {}", image_src);
            Arc::new(image)
        } else {
            debug!("Image not loaded and could not be loaded: {}", image_src);
            return;
        };

        // Calculate the desired size for the popup (2x scale or max screen)
        // terminal_size is already passed as parameter
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
            let resize_start = std::time::Instant::now();
            match self
                .book_images
                .resize_image_to(&original_image, final_width, final_height)
            {
                Ok(resized) => {
                    let resize_duration = resize_start.elapsed();
                    debug!(
                        "Pre-scaled image from {}x{} to {}x{} using fast_image_resize in {}ms",
                        img_width,
                        img_height,
                        final_width,
                        final_height,
                        resize_duration.as_millis()
                    );
                    Arc::new(resized)
                }
                Err(e) => {
                    warn!(
                        "Failed to pre-scale image with fast_image_resize: {}, using original",
                        e
                    );
                    original_image
                }
            }
        } else {
            original_image
        };

        let popup = ImagePopup::new(prescaled_image, picker, image_src.to_string());
        self.image_popup = Some(popup);
        self.image_popup_area = None; // Will be set on render
    }

    /// Apply scroll events (positive for down, negative for up)
    fn apply_scroll(&mut self, scroll_amount: i32, column: u16) {
        if self.image_popup.is_some() {
            return;
        }
        if scroll_amount == 0 {
            return;
        }

        let nav_panel_width = self.calculate_navigation_panel_width();
        let is_nav_panel = column < nav_panel_width;

        if is_nav_panel {
            debug!("Applying scroll to navigation panel");
            if scroll_amount > 0 {
                for _ in 0..scroll_amount.min(10) {
                    self.navigation_panel.move_selection_down();
                }
            } else {
                for _ in 0..(-scroll_amount).min(10) {
                    self.navigation_panel.move_selection_up();
                }
            }
        } else {
            debug!("Applying scroll to content area");
            if scroll_amount > 0 {
                for _ in 0..scroll_amount.min(10) {
                    self.scroll_down();
                }
            } else {
                for _ in 0..(-scroll_amount).min(10) {
                    self.scroll_up();
                }
            }
        }
    }

    /// Validate mouse coordinates to prevent crossterm overflow bug
    pub fn is_valid_mouse_coordinates(&self, column: u16, row: u16) -> bool {
        // Crossterm overflow bug occurs when coordinates are at edge values
        // The bug happens when column or row is 0, which can cause underflow
        // in crossterm's internal parsing logic
        if column == 0 || row == 0 {
            debug!(
                "Invalid mouse coordinates detected: column={}, row={}",
                column, row
            );
            return false;
        }

        // Also check for suspiciously high values that might indicate corruption
        if column > 10000 || row > 10000 {
            debug!(
                "Suspiciously large mouse coordinates detected: column={}, row={}",
                column, row
            );
            return false;
        }

        true
    }

    /// Calculate the content area rectangle for coordinate conversion
    fn get_content_area_rect(&self) -> Rect {
        // Use the stored content area from the last render
        if let Some(area) = self.text_reader.get_last_content_area() {
            area
        } else {
            // Fallback to a reasonable default
            Rect {
                x: 40,
                y: 1,
                width: 80,
                height: 20,
            }
        }
    }

    pub fn open_with_system_viewer(&self) {
        if let Some(path) = &self.current_file {
            info!(
                "Opening EPUB with system viewer: {} at chapter {}",
                path, self.current_chapter
            );

            match self
                .system_command_executor
                .open_file_at_chapter(path, self.current_chapter)
            {
                Ok(_) => info!(
                    "Successfully opened EPUB with system viewer at chapter {}",
                    self.current_chapter
                ),
                Err(e) => error!("Failed to open EPUB with system viewer: {}", e),
            }
        } else {
            error!("No EPUB file currently loaded");
        }
    }

    pub fn get_scroll_offset(&self) -> usize {
        self.text_reader.get_scroll_offset()
    }

    /// Jump to a specific location from the jump list
    fn jump_to_location(&mut self, location: JumpLocation) -> Result<()> {
        // Check if we need to open a different book
        if self.current_file.as_ref() != Some(&location.epub_path) {
            // Open the book WITHOUT restoring bookmarks (we'll set our own position)
            self.load_epub_internal_without_bookmark(&location.epub_path)?;
        }

        // Navigate to the chapter if needed
        if self.current_chapter != location.chapter_index {
            // Navigate without triggering bookmark save
            self.navigate_to_chapter(location.chapter_index)?;
        }

        // Force restore scroll position after any content updates
        // This needs to happen AFTER navigate_to_chapter which resets scroll
        self.text_reader
            .restore_scroll_position(location.scroll_position);

        // If there's an anchor, scroll to it
        if let Some(ref anchor) = location.anchor {
            let _ = self.scroll_to_anchor(anchor);
        }

        // Save bookmark at the jumped-to location
        self.save_bookmark();

        Ok(())
    }

    /// Handle Ctrl+O - jump back in history
    fn jump_back(&mut self) {
        if let Some(location) = self.jump_list.jump_back() {
            if let Err(e) = self.jump_to_location(location) {
                error!("Failed to jump back: {}", e);
            }
        }
    }

    /// Handle Ctrl+I - jump forward in history
    fn jump_forward(&mut self) {
        if let Some(location) = self.jump_list.jump_forward() {
            if let Err(e) = self.jump_to_location(location) {
                error!("Failed to jump forward: {}", e);
            }
        }
    }

    /// Calculate the navigation panel width based on stored terminal width
    fn calculate_navigation_panel_width(&self) -> u16 {
        // 30% of terminal width, minimum 20 columns
        ((self.terminal_width * 30) / 100).max(20)
    }

    /// Get the navigation panel area based on current terminal size
    fn get_navigation_panel_area(&self) -> Rect {
        use ratatui::layout::{Constraint, Direction, Layout};
        // Calculate the same layout as in render
        let full_area = Rect::new(0, 0, self.terminal_width, self.terminal_height);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(full_area);
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
                // Open the selected book
                if let Err(e) = self.open_book_for_reading(index) {
                    error!("Failed to open book at index {}: {}", index, e);
                }
            }
            SelectedActionOwned::BackToBooks => {
                // Switch back to book selection mode
                self.navigation_panel.switch_to_book_mode();
            }
            SelectedActionOwned::TocItem(toc_item) => {
                // Check if this is a section or a chapter
                match toc_item {
                    TocItem::Chapter { href, anchor, .. } => {
                        // Find the spine index for this href
                        if let Some(spine_index) = self.find_spine_index_by_href(&href) {
                            let _ = self.navigate_to_chapter(spine_index);
                            // Handle anchor if present
                            if let Some(anchor_id) = anchor {
                                self.text_reader
                                    .handle_pending_anchor_scroll(Some(anchor_id.clone()));
                                self.text_reader.set_active_anchor(Some(anchor_id));
                            } else {
                                self.text_reader.set_active_anchor(None);
                            }

                            self.focused_panel = FocusedPanel::Content;
                            // Clear manual navigation flag when jumping to content
                            self.navigation_panel
                                .table_of_contents
                                .clear_manual_navigation();
                            // Update the cache to reflect the correct active section
                            self.update_current_chapter_in_cache();
                        } else {
                            error!("Could not find spine index for href: {}", href);
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
                                        .handle_pending_anchor_scroll(Some(anchor_id.clone()));
                                    // Set this anchor as the active one
                                    self.text_reader.set_active_anchor(Some(anchor_id));
                                } else {
                                    self.text_reader.set_active_anchor(None);
                                }
                                self.focused_panel = FocusedPanel::Content;
                                // Update the cache to reflect the correct active section
                                self.update_current_chapter_in_cache();
                            } else {
                                error!(
                                    "Could not find spine index for section href: {}",
                                    section_href
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

    fn detect_click_type(&mut self, column: u16, row: u16) -> ClickType {
        const DOUBLE_CLICK_TIME_MS: u64 = 500; // Maximum time between clicks for double-click
        const CLICK_DISTANCE_THRESHOLD: u16 = 3; // Maximum distance between clicks

        let now = Instant::now();
        let position = (column, row);

        let is_within_time = if let Some(last_time) = self.last_click_time {
            now.duration_since(last_time).as_millis() <= DOUBLE_CLICK_TIME_MS as u128
        } else {
            false
        };

        let is_within_distance = if let Some(last_pos) = self.last_click_position {
            let distance_x = if column > last_pos.0 {
                column - last_pos.0
            } else {
                last_pos.0 - column
            };
            let distance_y = if row > last_pos.1 {
                row - last_pos.1
            } else {
                last_pos.1 - row
            };
            distance_x <= CLICK_DISTANCE_THRESHOLD && distance_y <= CLICK_DISTANCE_THRESHOLD
        } else {
            false
        };

        if is_within_time && is_within_distance {
            self.click_count += 1;
        } else {
            self.click_count = 1;
        }

        self.last_click_time = Some(now);
        self.last_click_position = Some(position);

        match self.click_count {
            2 => ClickType::Double,
            3 => ClickType::Triple,
            _ => ClickType::Single,
        }
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame, fps_counter: &FPSCounter) {
        // Update auto-scroll state for continuous scrolling during text selection
        let auto_scroll_updated = self.text_reader.update_auto_scroll();
        if auto_scroll_updated {
            // Save bookmark when auto-scrolling changes position
            self.save_bookmark();
        }

        // Update terminal dimensions for mouse event calculations
        self.terminal_width = f.area().width;
        self.terminal_height = f.area().height;
        self.last_terminal_size = f.area();

        // Clear the entire frame with the dark background first
        let background_block = Block::default().style(Style::default().bg(OCEANIC_NEXT.base_00));
        f.render_widget(background_block, f.area());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(f.area());

        // Fixed layout: 30% file list, 70% content
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunks[0]);

        // Delegate rendering to components
        self.navigation_panel.render(
            f,
            main_chunks[0],
            self.focused_panel == FocusedPanel::FileList,
            &OCEANIC_NEXT,
            &self.bookmarks,
            &self.book_manager,
        );

        // Render text content or default message
        if self.current_epub.is_some() {
            // Update wrapped lines based on current area dimensions
            // self.text_reader
            //     .update_wrapped_lines_if_needed(content, main_chunks[1]);

            self.text_reader.render(
                f,
                main_chunks[1],
                &self.current_chapter_title,
                self.current_chapter,
                self.total_chapters,
                &OCEANIC_NEXT,
                self.focused_panel == FocusedPanel::Content,
            );
        } else {
            // Render default content area when no EPUB is loaded
            self.render_default_content(f, main_chunks[1], "Select a file to view its content");
        }

        // Draw help bar
        self.render_help_bar(f, chunks[1], fps_counter);

        // Render reading history popup if active
        if self.show_reading_history {
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

        // Render image popup if active
        if let Some(ref mut image_popup) = self.image_popup {
            let dim_block = Block::default().style(
                Style::default()
                    .bg(Color::Rgb(10, 10, 10)) // todo: this is not from pallette
                    .add_modifier(Modifier::DIM),
            );
            f.render_widget(dim_block, f.area());

            let popup_area = image_popup.render(f, f.area());
            self.image_popup_area = Some(popup_area);
        } else {
            self.image_popup_area = None;
        }

        // Render book statistics popup if active
        if self.show_book_stat {
            // First render a dimming overlay
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
            OCEANIC_NEXT.get_panel_colors(self.focused_panel == FocusedPanel::Content);

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

        let help_text = if self.text_reader.has_text_selection() {
            "c/Ctrl+C: Copy to clipboard | ESC: Clear selection"
        } else {
            match self.focused_panel {
                FocusedPanel::FileList => {
                    "j/k: Navigate | Enter: Select | H: History | Tab: Switch | q: Quit"
                }
                FocusedPanel::Content => {
                    "j/k: Scroll | h/l: Chapter | Ctrl+d/u: Half-screen | H: History | Tab: Switch | Space+o: Open | q: Quit"
                }
            }
        };

        let help = Paragraph::new(format!("{} | FPS: {}", help_text, fps_counter.current_fps))
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

    /// Check if the key sequence has timed out
    fn should_reset_key_sequence(&self) -> bool {
        const KEY_SEQUENCE_TIMEOUT_MS: u64 = 1000; // 1 second timeout

        if let Some(last_time) = self.last_key_time {
            Instant::now().duration_since(last_time).as_millis() > KEY_SEQUENCE_TIMEOUT_MS as u128
        } else {
            false
        }
    }

    /// Handle a key sequence and return true if it was handled
    fn handle_key_sequence(&mut self, key_char: char) -> bool {
        // Reset sequence if timed out
        if self.should_reset_key_sequence() {
            self.key_sequence.clear();
        }

        // Add the new key to the sequence
        self.key_sequence.push(key_char);
        self.last_key_time = Some(Instant::now());

        // Check for known sequences
        let sequence: String = self.key_sequence.iter().collect();

        match sequence.as_str() {
            "gg" => {
                // Handle 'gg' motion - go to top
                if self.show_reading_history {
                    // Use VimNavMotions for reading history
                    if let Some(ref mut history) = self.reading_history {
                        history.handle_gg();
                    }
                } else if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.handle_gg();
                } else {
                    // For content, reset scroll to top
                    self.text_reader.restore_scroll_position(0);
                    self.save_bookmark();
                }
                self.key_sequence.clear();
                true
            }
            " s" => {
                // Handle Space->s to show raw HTML source
                if self.focused_panel == FocusedPanel::Content && self.current_epub.is_some() {
                    // Get raw HTML content for current chapter
                    if let Some(ref mut epub) = self.current_epub {
                        if let Some((raw_html, _)) = epub.get_current_str() {
                            self.text_reader.set_raw_html(raw_html);
                            self.text_reader.toggle_raw_html();
                        }
                    }
                }
                self.key_sequence.clear();
                true
            }
            " d" => {
                // Handle Space->d to show book statistics (document stats)
                if self.focused_panel == FocusedPanel::Content && self.current_epub.is_some() {
                    // Calculate and show book statistics
                    if let Some(ref mut epub) = self.current_epub {
                        let terminal_size = (self.terminal_width, self.terminal_height);
                        if let Err(e) = self.book_stat.calculate_stats(epub, terminal_size) {
                            error!("Failed to calculate book statistics: {}", e);
                        } else {
                            self.book_stat.show();
                            self.show_book_stat = true;
                        }
                    }
                }
                self.key_sequence.clear();
                true
            }
            " v" => {
                // Handle Space->v to show raw HTML (alternative way)
                if self.focused_panel == FocusedPanel::Content && self.current_epub.is_some() {
                    // Get raw HTML content for current chapter
                    if let Some(ref mut epub) = self.current_epub {
                        if let Some((raw_html, _)) = epub.get_current_str() {
                            self.text_reader.set_raw_html(raw_html);
                            self.text_reader.toggle_raw_html();
                        }
                    }
                }
                self.key_sequence.clear();
                true
            }
            " c" => {
                // Handle Space->c to copy entire chapter content
                if self.focused_panel == FocusedPanel::Content {
                    if let Err(e) = self.text_reader.copy_chapter_to_clipboard() {
                        debug!("Copy chapter failed: {}", e);
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

    /// Handle a single key event - useful for testing
    pub fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) {
        self.handle_key_event_with_screen_height(key, None);
    }

    /// Handle a single key event with optional screen height for half-screen scrolling
    pub fn handle_key_event_with_screen_height(
        &mut self,
        key: crossterm::event::KeyEvent,
        screen_height: Option<usize>,
    ) {
        use crossterm::event::{KeyCode, KeyModifiers};

        // If image popup is shown, close it on any key press
        if self.image_popup.is_some() {
            self.image_popup = None;
            self.image_popup_area = None;
            return;
        }

        // If book stat popup is shown, handle keys for it
        if self.show_book_stat {
            if self.book_stat.handle_key(key) {
                if !self.book_stat.is_visible() {
                    self.show_book_stat = false;
                }
                return;
            }
            // Handle vim navigation keys for book stat
            match key.code {
                KeyCode::Char('j') => {
                    self.book_stat.handle_j();
                    return;
                }
                KeyCode::Char('k') => {
                    self.book_stat.handle_k();
                    return;
                }
                KeyCode::Char('g') => {
                    // Check if this completes a 'gg' sequence
                    if !self.handle_key_sequence('g') {
                        // 'g' by itself doesn't do anything, waiting for next key
                    } else if self.key_sequence.len() == 2 && self.key_sequence == vec!['g', 'g'] {
                        // Handle 'gg' - go to top
                        self.book_stat.handle_gg();
                        self.key_sequence.clear();
                    }
                    return;
                }
                KeyCode::Char('G') => {
                    // Go to bottom
                    self.book_stat.handle_G();
                    return;
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Scroll half page down
                    self.book_stat.handle_ctrl_d();
                    return;
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Scroll half page up
                    self.book_stat.handle_ctrl_u();
                    return;
                }
                KeyCode::Esc => {
                    self.book_stat.hide();
                    self.show_book_stat = false;
                    return;
                }
                KeyCode::Enter => {
                    // Jump to selected chapter
                    if let Some(chapter_index) = self.book_stat.get_selected_chapter_index() {
                        // Hide the statistics popup
                        self.book_stat.hide();
                        self.show_book_stat = false;

                        // Navigate to the selected chapter
                        if let Err(e) = self.navigate_to_chapter(chapter_index) {
                            error!("Failed to navigate to chapter {}: {}", chapter_index, e);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        // For non-character keys or keys with modifiers (except shift), clear any pending sequence
        match &key.code {
            KeyCode::Char(_)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                // Character keys without Ctrl/Alt can be part of sequences
            }
            _ => {
                // Any other key clears the sequence
                self.key_sequence.clear();
            }
        }

        match key.code {
            KeyCode::Char('j') => {
                if self.show_reading_history {
                    // Use VimNavMotions for reading history
                    if let Some(ref mut history) = self.reading_history {
                        history.handle_j();
                    }
                } else if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.move_selection_down();
                } else {
                    self.scroll_down();
                }
            }
            KeyCode::Char('k') => {
                if self.show_reading_history {
                    // Use VimNavMotions for reading history
                    if let Some(ref mut history) = self.reading_history {
                        history.handle_k();
                    }
                } else if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.move_selection_up();
                } else {
                    self.scroll_up();
                }
            }
            KeyCode::Char('h') => {
                if self.show_reading_history {
                    // Use VimNavMotions for reading history (could close history)
                    if let Some(ref mut history) = self.reading_history {
                        history.handle_h();
                    }
                } else if self.focused_panel == FocusedPanel::FileList {
                    // Use VimNavMotions for navigation panel
                    self.navigation_panel.handle_h();
                } else {
                    // Allow chapter navigation in content view
                    let _ = self.navigate_chapter_relative(ChapterDirection::Previous);
                }
            }
            KeyCode::Char('H') => {
                // Toggle reading history
                if self.show_reading_history {
                    // Close history
                    self.show_reading_history = false;
                    self.reading_history = None;
                } else {
                    // Open history
                    self.reading_history = Some(ReadingHistory::new(&self.bookmarks));
                    self.show_reading_history = true;
                }
            }
            KeyCode::Char('i') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+I: Jump forward in history (vim-style)
                    self.jump_forward();
                }
                // 'i' by itself doesn't do anything
            }
            KeyCode::Char('l') => {
                if self.show_reading_history {
                    // Use VimNavMotions for reading history (could select/enter)
                    if let Some(ref mut history) = self.reading_history {
                        history.handle_l();
                    }
                } else if self.focused_panel == FocusedPanel::FileList {
                    // Use VimNavMotions for navigation panel (future: could expand/enter)
                    self.navigation_panel.handle_l();
                } else {
                    // Allow chapter navigation in content view
                    let _ = self.navigate_chapter_relative(ChapterDirection::Next);
                }
            }
            KeyCode::Char('o') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+O: Jump back in history (vim-style)
                    self.jump_back();
                } else if !self.handle_key_sequence('o') {
                    // 'o' is part of Space+o sequence or does nothing by itself
                }
            }
            KeyCode::Char('p') => {
                self.toggle_profiling();
            }
            KeyCode::Enter => {
                if self.show_reading_history {
                    // Handle selection from reading history
                    if let Some(ref history) = self.reading_history {
                        if let Some(path) = history.selected_path() {
                            // Find the book index by path
                            if let Some(book_index) =
                                self.book_manager.find_book_index_by_path(path)
                            {
                                // Close history and open the book
                                self.show_reading_history = false;
                                self.reading_history = None;
                                let _ = self.open_book_for_reading(book_index);
                            }
                        }
                    }
                } else {
                    // Handle selection based on what's currently selected
                    match self.navigation_panel.mode {
                        NavigationMode::TableOfContents => {
                            // Handle TOC selection
                            match self.navigation_panel.table_of_contents.get_selected_item() {
                                Some(SelectedTocItem::BackToBooks) => {
                                    // Switch back to book list mode
                                    self.switch_to_book_list_mode();
                                }
                                Some(SelectedTocItem::TocItem(toc_item)) => {
                                    // Check if this is a section or a chapter
                                    match toc_item {
                                        TocItem::Chapter { href, anchor, .. } => {
                                            if let Some(chapter_index) =
                                                self.find_spine_index_by_href(href)
                                            {
                                                let anchor_id = anchor.clone();
                                                // Navigate to the chapter
                                                let _ = self.navigate_to_chapter(chapter_index);
                                                // If there's an anchor, scroll to it after navigation
                                                if let Some(anchor_id) = anchor_id {
                                                    self.text_reader.handle_pending_anchor_scroll(
                                                        Some(anchor_id),
                                                    );
                                                }
                                                self.focused_panel = FocusedPanel::Content;
                                            }
                                        }
                                        TocItem::Section { href, anchor, .. } => {
                                            if let Some(href_str) = href {
                                                if let Some(chapter_index) =
                                                    self.find_spine_index_by_href(href_str)
                                                {
                                                    let anchor_id = anchor.clone();
                                                    // This section has content - navigate to it
                                                    let _ = self.navigate_to_chapter(chapter_index);
                                                    // If there's an anchor, scroll to it after navigation
                                                    if let Some(anchor_id) = anchor_id {
                                                        self.text_reader
                                                            .handle_pending_anchor_scroll(Some(
                                                                anchor_id,
                                                            ));
                                                    }
                                                    self.focused_panel = FocusedPanel::Content;
                                                }
                                            } else {
                                                // This section has no content - just toggle expansion
                                                self.navigation_panel
                                                    .table_of_contents
                                                    .toggle_selected_expansion();
                                            }
                                        }
                                    }
                                }
                                None => {}
                            }
                        }
                        NavigationMode::BookSelection => {
                            // Select book from file list using high-level action
                            let book_index = self.navigation_panel.get_selected_book_index();
                            let _ = self.open_book_for_reading(book_index);
                        }
                    }
                }
            }
            KeyCode::Char(' ') => {
                // Check if this might be part of a key sequence (space-s for raw HTML)
                if self.focused_panel == FocusedPanel::Content && !self.handle_key_sequence(' ') {
                    // Space by itself in content view doesn't do anything, it's waiting for the next key
                } else if self.focused_panel == FocusedPanel::FileList
                    && self.navigation_panel.mode == NavigationMode::TableOfContents
                {
                    // Toggle section expansion when focused on file list and in TOC mode
                    // Get the currently selected TOC item and toggle its expansion if it's a section
                    if let Some(ref cached_info) = self.cached_current_book_info {
                        if let Some(SelectedTocItem::TocItem(toc_item)) =
                            self.navigation_panel.table_of_contents.get_selected_item()
                        {
                            // Clone the toc_items to avoid borrow issues
                            let mut updated_toc_items = cached_info.toc_items.clone();
                            if let Some(item) =
                                Self::find_toc_item_mut(&mut updated_toc_items, toc_item.title())
                            {
                                item.toggle_expansion();
                                // Update the cached info with the modified toc_items
                                if let Some(ref mut cached_info) = self.cached_current_book_info {
                                    cached_info.toc_items = updated_toc_items;
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Tab => {
                // Switch focus between panels
                self.focused_panel = match self.focused_panel {
                    FocusedPanel::FileList => {
                        // Clear manual navigation flag when leaving TOC
                        self.navigation_panel
                            .table_of_contents
                            .clear_manual_navigation();
                        FocusedPanel::Content
                    }
                    FocusedPanel::Content => FocusedPanel::FileList,
                };
            }
            KeyCode::Char('d') => {
                // First check if this is part of a key sequence (e.g., Space+d)
                if self.handle_key_sequence('d') {
                    // Key was handled as part of a sequence
                } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if self.show_reading_history {
                        // Use VimNavMotions for reading history
                        if let Some(ref mut history) = self.reading_history {
                            history.handle_ctrl_d();
                        }
                    } else if self.focused_panel == FocusedPanel::FileList {
                        // Use VimNavMotions for navigation panel
                        self.navigation_panel.handle_ctrl_d();
                    } else if let Some(visible_height) = screen_height {
                        self.scroll_half_screen_down(visible_height);
                    }
                }
            }
            KeyCode::Char('u') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if self.show_reading_history {
                        // Use VimNavMotions for reading history
                        if let Some(ref mut history) = self.reading_history {
                            history.handle_ctrl_u();
                        }
                    } else if self.focused_panel == FocusedPanel::FileList {
                        // Use VimNavMotions for navigation panel
                        self.navigation_panel.handle_ctrl_u();
                    } else if let Some(visible_height) = screen_height {
                        self.scroll_half_screen_up(visible_height);
                    }
                }
            }
            KeyCode::Char('s') => {
                // Check if this completes a key sequence (space-s for raw HTML)
                if !self.handle_key_sequence('s') {
                    // 's' by itself doesn't do anything if not part of a sequence
                }
            }
            KeyCode::Char('g') => {
                // Check if this completes a key sequence
                if !self.handle_key_sequence('g') {
                    // 'g' by itself doesn't do anything, it's waiting for the next key
                }
            }
            KeyCode::Char('G') => {
                if self.show_reading_history {
                    if let Some(ref mut history) = self.reading_history {
                        history.handle_G();
                    }
                } else if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.handle_G();
                } else if self.current_epub.is_some() {
                    self.text_reader.handle_G();
                }
            }
            KeyCode::Char('c') => {
                // Check if this completes a key sequence (space-c for copy chapter)
                if !self.handle_key_sequence('c') {
                    // 'c' by itself copies selected text
                    if let Err(e) = self.text_reader.copy_selection_to_clipboard() {
                        debug!("Copy failed: {}", e);
                    }
                }
            }
            KeyCode::Esc => {
                if self.show_reading_history {
                    // Close reading history
                    self.show_reading_history = false;
                    self.reading_history = None;
                } else if self.text_reader.has_text_selection() {
                    // Clear text selection when ESC is pressed
                    self.text_reader.clear_selection();
                    debug!("Text selection cleared via ESC key");
                }
            }
            _ => {}
        }
    }

    pub fn handle_resize(&mut self) {
        debug!("Terminal resize detected");
        // Tell the text reader to update its image picker with new font size
        self.text_reader.handle_terminal_resize();
    }
}

pub struct FPSCounter {
    last_measure: Instant,
    ticks: u16,
    current_fps: u16,
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
        // Process all available events first before drawing
        let mut events_processed = 0;
        let mut should_quit = false;
        fps_counter.tick();
        // Drain all available events without blocking
        while event_source.poll(Duration::from_millis(0))? && events_processed < 50 {
            let event = event_source.read()?;
            events_processed += 1;

            match event {
                Event::Mouse(mouse_event) => {
                    // Handle horizontal scroll events immediately without batching
                    match mouse_event.kind {
                        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                            // Completely ignore horizontal scroll events to prevent flooding
                        }
                        _ => {
                            // Handle other mouse events with potential batching for rapid scrolling
                            app.handle_mouse_event(mouse_event, Some(event_source));
                        }
                    }
                }
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Char('q') => {
                            // Save bookmark before quitting
                            app.save_bookmark_with_throttle(true);
                            should_quit = true;
                        }
                        _ => {
                            // Calculate screen height for half-screen scrolling commands
                            let visible_height =
                                terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
                            // Handle all keys through the common handler
                            app.handle_key_event_with_screen_height(key, Some(visible_height));
                        }
                    }
                }
                Event::Resize(_cols, _rows) => {
                    // Terminal has been resized - need to update font size detection
                    app.handle_resize();
                }
                _ => {}
            }

            if should_quit {
                break;
            }
        }

        // Handle timing and check for loaded images
        let mut needs_redraw = events_processed > 0;

        // Ensure we always render at least once on startup
        if first_render {
            needs_redraw = true;
            first_render = false;
        }

        if last_tick.elapsed() >= tick_rate {
            let highlight_changed = app.update_highlight(); // Update highlight state
            // Check for loaded images from background thread
            let images_loaded = app.text_reader.check_for_loaded_images();
            if images_loaded {
                needs_redraw = true;
                debug!("Images loaded, forcing redraw");
            }
            if highlight_changed {
                needs_redraw = true;
                debug!("Highlight expired, forcing redraw");
            }
            // Only redraw when something actually changed - no more forced redraws
            last_tick = std::time::Instant::now();
        }

        // Draw if needed
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
