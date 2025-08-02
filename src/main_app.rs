use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::event_source::EventSource;
use crate::navigation_panel::{CurrentBookInfo, NavigationMode, NavigationPanel};
use crate::system_command::{
    MockSystemCommandExecutor, RealSystemCommandExecutor, SystemCommandExecutor,
};
use crate::table_of_contents::{SelectedTocItem, TocItem};
use crate::text_generator::TextGenerator;
use crate::text_reader::TextReader;
use crate::theme::OCEANIC_NEXT;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChapterDirection {
    Next,
    Previous,
}

use std::{
    io::BufReader,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::event::{Event, KeyCode, MouseButton, MouseEvent, MouseEventKind};
use epub::doc::EpubDoc;
use log::{debug, error, info};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

pub struct App {
    pub book_manager: BookManager,
    pub navigation_panel: NavigationPanel,
    text_generator: TextGenerator,
    text_reader: TextReader,
    bookmarks: Bookmarks,
    current_content: Option<String>,
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
        Self::new_with_config(None, None, true)
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
        let text_generator = TextGenerator::new();
        let text_reader = TextReader::new();

        let bookmarks = match bookmark_file {
            Some(file) => Bookmarks::load_from_file(file).unwrap_or_else(|e| {
                error!("Failed to load bookmarks from {}: {}", file, e);
                Bookmarks::new()
            }),
            None => Bookmarks::new(),
        };

        let mut app = Self {
            book_manager,
            navigation_panel,
            text_generator,
            text_reader,
            bookmarks,
            current_content: None,
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
                if doc.set_current_page(chapter_index).is_ok() {
                    self.current_chapter = chapter_index;
                    info!("Navigating to chapter: {}", self.current_chapter + 1);
                    self.update_content();
                    self.text_reader.reset_scroll();
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
                        if doc.go_next().is_ok() {
                            self.current_chapter += 1;
                            info!("Moving to next chapter: {}", self.current_chapter + 1);
                            self.update_content();
                            self.text_reader.reset_scroll();
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
                        if doc.go_prev().is_ok() {
                            self.current_chapter -= 1;
                            info!("Moving to previous chapter: {}", self.current_chapter + 1);
                            self.update_content();
                            self.text_reader.reset_scroll();
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
            anyhow::bail!("No EPUB document loaded")
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

    fn load_epub_internal(&mut self, path: &str) -> Result<()> {
        let mut doc = self
            .book_manager
            .load_epub(path)
            .map_err(|e| anyhow::anyhow!("Failed to load EPUB: {}", e))?;

        info!("Successfully loaded EPUB document");
        self.total_chapters = doc.get_num_pages();
        info!("Total chapters: {}", self.total_chapters);

        // Try to load bookmark
        if let Some(bookmark) = self.bookmarks.get_bookmark(path) {
            info!(
                "Found bookmark: chapter {}, offset {}",
                bookmark.chapter, bookmark.scroll_offset
            );
            // Skip metadata page if needed
            if bookmark.chapter > 0 {
                for _ in 0..bookmark.chapter {
                    if doc.go_next().is_err() {
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
                if doc.go_next().is_ok() {
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
            let spine_id = &doc.spine[chapter_index];
            if let Some((path, _)) = doc.resources.get(spine_id) {
                return Some(path.to_string_lossy().to_string());
            }
        }
        None
    }

    /// Map chapter indices from the chapter_map to TocItem entries
    fn map_chapter_indices(
        toc_items: &mut Vec<TocItem>,
        chapter_map: &std::collections::HashMap<String, usize>,
        text_generator: &TextGenerator,
    ) {
        for item in toc_items.iter_mut() {
            match item {
                TocItem::Chapter { href, index, .. } => {
                    let normalized_href = text_generator.normalize_href(href);
                    if let Some(&chapter_index) = chapter_map.get(&normalized_href) {
                        *index = chapter_index;
                    }
                }
                TocItem::Section {
                    href,
                    index,
                    children,
                    ..
                } => {
                    if let Some(href_str) = href {
                        let normalized_href = text_generator.normalize_href(href_str);
                        if let Some(&chapter_index) = chapter_map.get(&normalized_href) {
                            *index = Some(chapter_index);
                        }
                    }
                    // Recursively map children
                    Self::map_chapter_indices(children, chapter_map, text_generator);
                }
            }
        }
    }

    /// Convert TOC entries to section structure

    fn refresh_chapter_cache(&mut self) {
        debug!("refresh_chapter_cache called");
        if let (Some(ref current_file), Some(ref mut epub)) =
            (&self.current_file, &mut self.current_epub)
        {
            debug!("refresh_chapter_cache: processing book '{}'", current_file);

            // Parse TOC structure to create hierarchical sections
            let mut toc_items = self.text_generator.parse_toc_structure(epub);

            // Debug: Log the parsed TOC structure
            debug!("Parsed {} TOC items", toc_items.len());
            for (i, item) in toc_items.iter().enumerate() {
                match item {
                    TocItem::Chapter { title, href, .. } => {
                        debug!("TOC Item {}: Chapter '{}' -> '{}'", i, title, href);
                    }
                    TocItem::Section {
                        title,
                        href,
                        children,
                        ..
                    } => {
                        let href_str = href.as_ref().map(|h| h.as_str()).unwrap_or("None");
                        debug!(
                            "TOC Item {}: Section '{}' -> '{}' (children: {})",
                            i,
                            title,
                            href_str,
                            children.len()
                        );
                        for (j, child) in children.iter().enumerate() {
                            match child {
                                TocItem::Chapter { title, href, .. } => {
                                    debug!("  Child {}: Chapter '{}' -> '{}'", j, title, href);
                                }
                                TocItem::Section { title, .. } => {
                                    debug!("  Child {}: Section '{}'", j, title);
                                }
                            }
                        }
                    }
                }
            }

            // Store current position to restore later
            let original_chapter = epub.get_current_page();

            // First, get chapter information for all chapters
            let mut chapter_map = std::collections::HashMap::new();
            debug!("Building chapter map for {} chapters", self.total_chapters);
            for i in 0..self.total_chapters {
                if epub.set_current_page(i).is_ok() {
                    if let Ok(_content) = epub.get_current_str() {
                        if let Some(chapter_href) = Self::get_chapter_href(epub, i) {
                            // Normalize href for matching
                            let normalized_href = self.text_generator.normalize_href(&chapter_href);
                            debug!(
                                "Chapter {}: '{}' -> normalized: '{}'",
                                i, chapter_href, normalized_href
                            );
                            chapter_map.insert(normalized_href, i);
                        }
                    }
                }
            }

            debug!("Chapter map contains {} entries", chapter_map.keys().len());

            // Restore original position
            let _ = epub.set_current_page(original_chapter);

            // Map chapter indices to the TOC items
            Self::map_chapter_indices(&mut toc_items, &chapter_map, &self.text_generator);

            let book_info = CurrentBookInfo {
                path: current_file.clone(),
                toc_items,
                current_chapter: self.current_chapter,
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

    pub fn get_current_book_info(&self) -> Option<&CurrentBookInfo> {
        self.cached_current_book_info.as_ref()
    }

    fn update_current_chapter_in_cache(&mut self) {
        if let Some(ref mut cached_info) = self.cached_current_book_info {
            cached_info.current_chapter = self.current_chapter;

            // Update the table of contents with the updated book info
            if self.navigation_panel.mode == NavigationMode::TableOfContents {
                self.navigation_panel
                    .table_of_contents
                    .set_current_book_info(cached_info.clone());
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
                self.text_reader.scroll_offset,
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

    fn update_highlight(&mut self) {
        // Update highlight state in text reader
        self.text_reader.update_highlight();
    }

    fn update_content(&mut self) {
        if let Some(doc) = &mut self.current_epub {
            match self.text_generator.process_chapter_content(doc) {
                Ok((content, title)) => {
                    self.current_chapter_title = title;
                    self.current_content = Some(content);
                    self.text_reader
                        .set_content_length(self.current_content.as_ref().unwrap().len());
                    // Reset wrapped lines count - it will be calculated on next render
                    self.text_reader.total_wrapped_lines = 0;
                    self.text_reader.visible_height = 0;
                }
                Err(e) => {
                    error!("Failed to process chapter: {}", e);
                    self.current_content = Some("Error reading chapter content.".to_string());
                    self.text_reader.set_content_length(0);
                    self.text_reader.total_wrapped_lines = 0;
                    self.text_reader.visible_height = 0;
                }
            }
        } else {
            error!("No EPUB document loaded");
            self.current_content = Some("No EPUB document loaded.".to_string());
            self.text_reader.set_content_length(0);
            self.text_reader.total_wrapped_lines = 0;
            self.text_reader.visible_height = 0;
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
        if let Some(content) = &self.current_content {
            self.text_reader.scroll_down(content);
            self.save_bookmark();
        }
    }

    pub fn scroll_up(&mut self) {
        if let Some(content) = &self.current_content {
            self.text_reader.scroll_up(content);
            self.save_bookmark();
        }
    }

    pub fn scroll_half_screen_down(&mut self, screen_height: usize) {
        if let Some(content) = &self.current_content {
            self.text_reader
                .scroll_half_screen_down(content, screen_height);
            self.save_bookmark();
        }
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
        debug!(
            "handle_mouse_event called with: {:?} at ({}, {})",
            initial_mouse_event.kind, initial_mouse_event.column, initial_mouse_event.row
        );

        // Extra validation for horizontal scrolls to prevent crossterm overflow bug
        if matches!(
            initial_mouse_event.kind,
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
        ) {
            if !self.is_valid_mouse_coordinates(initial_mouse_event.column, initial_mouse_event.row)
            {
                debug!(
                    "Dropping horizontal scroll event with invalid coordinates: ({}, {})",
                    initial_mouse_event.column, initial_mouse_event.row
                );
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
                debug!(
                    "Starting vertical scroll batching with ScrollDown at ({}, {})",
                    initial_column, initial_row
                );
            }
            MouseEventKind::ScrollUp => {
                scroll_up_count += 1;
                debug!(
                    "Starting vertical scroll batching with ScrollUp at ({}, {})",
                    initial_column, initial_row
                );
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
            scroll_down_count, scroll_up_count, net_scroll, drain_count, initial_column, initial_row
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
                    }

                    let content_area = self.get_content_area_rect();
                    let click_type = self.detect_click_type(mouse_event.column, mouse_event.row);

                    match click_type {
                        ClickType::Single => {
                            self.text_reader.handle_mouse_down(
                                mouse_event.column,
                                mouse_event.row,
                                content_area,
                            );
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
                let nav_panel_width = self.calculate_navigation_panel_width();
                if mouse_event.column >= nav_panel_width {
                    let content_area = self.get_content_area_rect();
                    self.text_reader.handle_mouse_up(
                        mouse_event.column,
                        mouse_event.row,
                        content_area,
                    );
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let nav_panel_width = self.calculate_navigation_panel_width();
                if mouse_event.column >= nav_panel_width {
                    let content_area = self.get_content_area_rect();
                    let old_scroll_offset = self.text_reader.scroll_offset;
                    self.text_reader.handle_mouse_drag(
                        mouse_event.column,
                        mouse_event.row,
                        content_area,
                    );
                    if self.text_reader.scroll_offset != old_scroll_offset {
                        self.save_bookmark();
                    }
                }
            }
            _ => {
                debug!("Unhandled mouse event: {:?}", mouse_event.kind);
            }
        }
    }

    /// Apply scroll events (positive for down, negative for up)
    fn apply_scroll(&mut self, scroll_amount: i32, column: u16) {
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
        if let Some(area) = self.text_reader.last_content_area {
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
        self.text_reader.scroll_offset
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
                    TocItem::Chapter { index, .. } => {
                        let _ = self.navigate_to_chapter(index);
                        self.focused_panel = FocusedPanel::Content;
                    }
                    TocItem::Section { index, .. } => {
                        if let Some(chapter_index) = index {
                            let _ = self.navigate_to_chapter(chapter_index);
                            self.focused_panel = FocusedPanel::Content;
                        } else {
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

    fn scroll_half_screen_up(&mut self, screen_height: usize) {
        if let Some(content) = &self.current_content {
            self.text_reader
                .scroll_half_screen_up(content, screen_height);
            self.save_bookmark();
        }
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame) {
        // Update auto-scroll state for continuous scrolling during text selection
        let auto_scroll_updated = self.text_reader.update_auto_scroll();
        if auto_scroll_updated {
            // Save bookmark when auto-scrolling changes position
            self.save_bookmark();
        }

        // Update terminal dimensions for mouse event calculations
        self.terminal_width = f.size().width;
        self.terminal_height = f.size().height;

        // Clear the entire frame with the dark background first
        let background_block = Block::default().style(Style::default().bg(OCEANIC_NEXT.base_00));
        f.render_widget(background_block, f.size());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(f.size());

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
        if let Some(content) = &self.current_content {
            if self.current_epub.is_some() {
                // Update wrapped lines based on current area dimensions
                self.text_reader
                    .update_wrapped_lines_if_needed(content, main_chunks[1]);

                self.text_reader.render(
                    f,
                    main_chunks[1],
                    content,
                    &self.current_chapter_title,
                    self.current_chapter,
                    self.total_chapters,
                    &OCEANIC_NEXT,
                    self.focused_panel == FocusedPanel::Content,
                );
            } else {
                // Render default content area when no EPUB is loaded
                self.render_default_content(f, main_chunks[1], content);
            }
        } else {
            self.render_default_content(f, main_chunks[1], "Select a file to view its content");
        }

        // Draw help bar
        self.render_help_bar(f, chunks[1]);
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

    fn render_help_bar(&self, f: &mut ratatui::Frame, area: Rect) {
        let (_, _, border_color, _, _) = OCEANIC_NEXT.get_interface_colors(false);

        let help_text = if self.text_reader.has_text_selection() {
            "c/Ctrl+C: Copy to clipboard | ESC: Clear selection"
        } else {
            match self.focused_panel {
                FocusedPanel::FileList => "j/k: Navigate files | Enter: Select | Tab: Switch to content | q: Quit",
                FocusedPanel::Content => "j/k: Scroll | h/l: Chapter | Ctrl+d/u: Half-screen | Tab: Switch to files | Ctrl+O: Open | q: Quit",
            }
        };

        let help = Paragraph::new(help_text)
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
                if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.handle_gg();
                } else {
                    // For content, reset scroll to top
                    self.text_reader.scroll_offset = 0;
                    self.save_bookmark();
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
                // Navigate based on focused panel
                if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.move_selection_down();
                } else {
                    self.scroll_down();
                }
            }
            KeyCode::Char('k') => {
                // Navigate based on focused panel
                if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.move_selection_up();
                } else {
                    self.scroll_up();
                }
            }
            KeyCode::Char('h') => {
                if self.focused_panel == FocusedPanel::FileList {
                    // Use VimNavMotions for navigation panel
                    self.navigation_panel.handle_h();
                } else {
                    // Allow chapter navigation in content view
                    let _ = self.navigate_chapter_relative(ChapterDirection::Previous);
                }
            }
            KeyCode::Char('l') => {
                if self.focused_panel == FocusedPanel::FileList {
                    // Use VimNavMotions for navigation panel (future: could expand/enter)
                    self.navigation_panel.handle_l();
                } else {
                    // Allow chapter navigation in content view
                    let _ = self.navigate_chapter_relative(ChapterDirection::Next);
                }
            }
            KeyCode::Char('o') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+O: Open current EPUB with system viewer
                    self.open_with_system_viewer();
                }
            }
            KeyCode::Enter => {
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
                                    TocItem::Chapter { index, .. } => {
                                        // Navigate to the chapter
                                        let _ = self.navigate_to_chapter(*index);
                                        self.focused_panel = FocusedPanel::Content;
                                    }
                                    TocItem::Section { index, .. } => {
                                        if let Some(chapter_index) = index {
                                            // This section has content - navigate to it
                                            let _ = self.navigate_to_chapter(*chapter_index);
                                            self.focused_panel = FocusedPanel::Content;
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
            KeyCode::Char(' ') => {
                // Toggle section expansion when focused on file list and in TOC mode
                if self.focused_panel == FocusedPanel::FileList
                    && self.navigation_panel.mode == NavigationMode::TableOfContents
                {
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
                    FocusedPanel::FileList => FocusedPanel::Content,
                    FocusedPanel::Content => FocusedPanel::FileList,
                };
            }
            KeyCode::Char('d') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if self.focused_panel == FocusedPanel::FileList {
                        // Use VimNavMotions for navigation panel
                        self.navigation_panel.handle_ctrl_d();
                    } else if let Some(visible_height) = screen_height {
                        self.scroll_half_screen_down(visible_height);
                    }
                }
            }
            KeyCode::Char('u') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if self.focused_panel == FocusedPanel::FileList {
                        // Use VimNavMotions for navigation panel
                        self.navigation_panel.handle_ctrl_u();
                    } else if let Some(visible_height) = screen_height {
                        self.scroll_half_screen_up(visible_height);
                    }
                }
            }
            KeyCode::Char('g') => {
                // Check if this completes a key sequence
                if !self.handle_key_sequence('g') {
                    // 'g' by itself doesn't do anything, it's waiting for the next key
                }
            }
            KeyCode::Char('G') => {
                // Handle 'G' motion - go to bottom
                if self.focused_panel == FocusedPanel::FileList {
                    self.navigation_panel.handle_G();
                } else if let Some(_content) = &self.current_content {
                    // For content, scroll to bottom
                    // Calculate the maximum scroll offset
                    if self.text_reader.total_wrapped_lines > self.text_reader.visible_height {
                        self.text_reader.scroll_offset =
                            self.text_reader.total_wrapped_lines - self.text_reader.visible_height;
                        self.save_bookmark();
                    }
                }
            }
            KeyCode::Char('c') => {
                // Handle copy
                if let Err(e) = self.text_reader.copy_selection_to_clipboard() {
                    debug!("Copy failed: {}", e);
                }
            }
            KeyCode::Esc => {
                if self.text_reader.has_text_selection() {
                    // Clear text selection when ESC is pressed
                    self.text_reader.clear_selection();
                    debug!("Text selection cleared via ESC key");
                }
            }
            _ => {}
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

    loop {
        // Process all available events first before drawing
        let mut events_processed = 0;
        let mut should_quit = false;

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
                _ => {}
            }

            if should_quit {
                break;
            }
        }

        // Draw if we processed events or on tick
        if events_processed > 0 || last_tick.elapsed() >= tick_rate {
            terminal.draw(|f| app.draw(f))?;
        }

        // Handle timing
        if last_tick.elapsed() >= tick_rate {
            app.update_highlight(); // Update highlight state
            last_tick = std::time::Instant::now();
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
