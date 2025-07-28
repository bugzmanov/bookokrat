use crate::book_list::BookList;
use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::event_source::EventSource;
use crate::text_generator::TextGenerator;
use crate::text_reader::TextReader;
use crate::theme::OCEANIC_NEXT;

use std::{
    io::BufReader,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use epub::doc::EpubDoc;
use log::{debug, error, info};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

/// Trait for executing system commands (mockable for testing)
pub trait SystemCommandExecutor {
    fn open_file(&self, path: &str) -> Result<(), String>;
    fn open_file_at_chapter(&self, path: &str, chapter: usize) -> Result<(), String>;

    fn as_any(&self) -> &dyn std::any::Any;
}

/// Real system command executor
pub struct RealSystemCommandExecutor;

impl SystemCommandExecutor for RealSystemCommandExecutor {
    fn open_file(&self, path: &str) -> Result<(), String> {
        self.open_file_at_chapter(path, 0) // Default to opening without specific chapter
    }

    fn open_file_at_chapter(&self, path: &str, chapter: usize) -> Result<(), String> {
        use std::path::PathBuf;

        // Convert to absolute path
        let absolute_path = if std::path::Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?
                .join(path)
        };

        // Check if file exists
        if !absolute_path.exists() {
            return Err(format!("File does not exist: {}", absolute_path.display()));
        }

        let absolute_path_str = absolute_path.to_string_lossy();

        // Try to open with chapter-aware EPUB readers first, then fall back to default
        let result = if cfg!(target_os = "macos") {
            self.open_with_macos_epub_reader(absolute_path_str.as_ref(), chapter)
                .or_else(|_| Command::new("open").arg(absolute_path_str.as_ref()).spawn())
        } else if cfg!(target_os = "windows") {
            self.open_with_windows_epub_reader(absolute_path_str.as_ref(), chapter)
                .or_else(|_| {
                    Command::new("cmd")
                        .args(["/C", "start", "", absolute_path_str.as_ref()])
                        .spawn()
                })
        } else {
            // Linux and other Unix-like systems
            self.open_with_linux_epub_reader(absolute_path_str.as_ref(), chapter)
                .or_else(|_| {
                    Command::new("xdg-open")
                        .arg(absolute_path_str.as_ref())
                        .spawn()
                })
        };

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "Failed to open file '{}': {}",
                absolute_path.display(),
                e
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl RealSystemCommandExecutor {
    /// Try to open EPUB with macOS-specific readers at the given chapter
    fn open_with_macos_epub_reader(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        // Try ClearView first
        if let Ok(child) = self.try_clearview(path, chapter) {
            return Ok(child);
        }

        // Try Calibre ebook-viewer
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }

        // Try Skim (PDF/EPUB viewer)
        if let Ok(child) = self.try_skim(path, chapter) {
            return Ok(child);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No compatible EPUB reader found",
        ))
    }

    /// Try to open EPUB with Windows-specific readers at the given chapter
    fn open_with_windows_epub_reader(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        // Try Calibre ebook-viewer first
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No compatible EPUB reader found",
        ))
    }

    /// Try to open EPUB with Linux-specific readers at the given chapter
    fn open_with_linux_epub_reader(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        // Try Calibre ebook-viewer first
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }

        // Try FBReader
        if let Ok(child) = self.try_fbreader(path, chapter) {
            return Ok(child);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No compatible EPUB reader found",
        ))
    }

    /// Try to open with ClearView (macOS)
    fn try_clearview(
        &self,
        path: &str,
        _chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        // ClearView is a GUI-only application without CLI chapter navigation support
        // Just open the file normally - user will need to navigate manually
        Command::new("open").args(["-a", "ClearView", path]).spawn()
    }

    /// Try to open with Calibre ebook-viewer (cross-platform)
    fn try_calibre_viewer(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        // Calibre ebook-viewer supports --goto option with TOC navigation
        // We'll try to navigate to the chapter using TOC pattern matching
        if chapter > 0 {
            // Try different chapter naming patterns that are common in EPUBs
            let chapter_patterns = [
                format!("toc:Chapter {}", chapter + 1), // Chapter 1, Chapter 2, etc.
                format!("toc:Ch {}", chapter + 1),      // Ch 1, Ch 2, etc.
                format!("toc:{}", chapter + 1),         // Just the number
                format!("toc:Chapter{}", chapter + 1),  // Chapter1, Chapter2, etc.
            ];

            // Try each pattern
            for pattern in &chapter_patterns {
                if let Ok(child) = Command::new("ebook-viewer")
                    .arg(format!("--goto={}", pattern))
                    .arg(path)
                    .spawn()
                {
                    return Ok(child);
                }
            }
        }

        // Fallback: just open the file normally
        Command::new("ebook-viewer").arg(path).spawn()
    }

    /// Try to open with Skim (macOS)
    fn try_skim(&self, path: &str, _chapter: usize) -> Result<std::process::Child, std::io::Error> {
        // Skim doesn't support command-line chapter navigation
        Command::new("open").args(["-a", "Skim", path]).spawn()
    }

    /// Try to open with FBReader (Linux)
    fn try_fbreader(
        &self,
        path: &str,
        _chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        // FBReader doesn't support command-line chapter navigation
        Command::new("fbreader").arg(path).spawn()
    }
}

/// Mock system command executor for testing
pub struct MockSystemCommandExecutor {
    pub executed_commands: std::cell::RefCell<Vec<String>>,
    pub should_fail: bool,
}

impl MockSystemCommandExecutor {
    pub fn new() -> Self {
        Self {
            executed_commands: std::cell::RefCell::new(Vec::new()),
            should_fail: false,
        }
    }

    pub fn new_with_failure() -> Self {
        Self {
            executed_commands: std::cell::RefCell::new(Vec::new()),
            should_fail: true,
        }
    }

    pub fn get_executed_commands(&self) -> Vec<String> {
        self.executed_commands.borrow().clone()
    }
}

impl SystemCommandExecutor for MockSystemCommandExecutor {
    fn open_file(&self, path: &str) -> Result<(), String> {
        self.executed_commands.borrow_mut().push(path.to_string());
        if self.should_fail {
            Err("Mock failure".to_string())
        } else {
            Ok(())
        }
    }

    fn open_file_at_chapter(&self, path: &str, chapter: usize) -> Result<(), String> {
        self.executed_commands
            .borrow_mut()
            .push(format!("{}@chapter{}", path, chapter));
        if self.should_fail {
            Err("Mock failure".to_string())
        } else {
            Ok(())
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct App {
    pub book_manager: BookManager,
    pub book_list: BookList,
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

        let book_list = BookList::new(&book_manager);
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
            book_list,
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
        };

        // Auto-load the most recently read book if available
        if auto_load_recent {
            if let Some((recent_path, _)) = app.bookmarks.get_most_recent() {
                // Check if the most recent book still exists in the managed books
                if app.book_manager.contains_book(&recent_path) {
                    info!("Auto-loading most recent book: {}", recent_path);
                    app.load_epub(&recent_path);
                }
            }
        }

        app
    }

    pub fn load_epub(&mut self, path: &str) {
        match self.book_manager.load_epub(path) {
            Ok(mut doc) => {
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
                            error!("Failed to move to next chapter");
                        }
                    }
                }

                self.current_epub = Some(doc);
                self.current_file = Some(path.to_string());
                self.update_content();
            }
            Err(e) => {
                error!("Failed to load EPUB: {}", e);
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

    pub fn next_chapter(&mut self) {
        if let Some(doc) = &mut self.current_epub {
            if self.current_chapter < self.total_chapters - 1 {
                if doc.go_next().is_ok() {
                    self.current_chapter += 1;
                    info!("Moving to next chapter: {}", self.current_chapter + 1);
                    self.update_content();
                    self.text_reader.reset_scroll();
                    self.save_bookmark_with_throttle(true);
                } else {
                    error!("Failed to move to next chapter");
                }
            } else {
                info!("Already at last chapter");
            }
        }
    }

    pub fn prev_chapter(&mut self) {
        if let Some(doc) = &mut self.current_epub {
            if self.current_chapter > 0 {
                if doc.go_prev().is_ok() {
                    self.current_chapter -= 1;
                    info!("Moving to previous chapter: {}", self.current_chapter + 1);
                    self.update_content();
                    self.text_reader.reset_scroll();
                    self.save_bookmark_with_throttle(true);
                } else {
                    error!("Failed to move to previous chapter");
                }
            } else {
                info!("Already at first chapter");
            }
        }
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

    pub fn handle_mouse_event(&mut self, mouse_event: MouseEvent) {
        let start_time = std::time::Instant::now();
        debug!(
            "handle_mouse_event called with: {:?} at ({}, {})",
            mouse_event.kind, mouse_event.column, mouse_event.row
        );

        // Extra validation for horizontal scrolls to prevent crossterm overflow bug
        if matches!(
            mouse_event.kind,
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
        ) {
            if !self.is_valid_mouse_coordinates(mouse_event.column, mouse_event.row) {
                debug!(
                    "Dropping horizontal scroll event with invalid coordinates: ({}, {})",
                    mouse_event.column, mouse_event.row
                );
                return;
            }
        }

        match mouse_event.kind {
            MouseEventKind::ScrollDown => {
                // Allow scrolling in both file list and content
                if mouse_event.column < 30 {
                    self.book_list.move_selection_down(&self.book_manager);
                } else {
                    self.scroll_down();
                }
            }
            MouseEventKind::ScrollUp => {
                // Allow scrolling in both file list and content
                if mouse_event.column < 30 {
                    self.book_list.move_selection_up();
                } else {
                    self.scroll_up();
                }
            }
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                // Horizontal scrolling is not supported in this app, but we handle it
                // explicitly to avoid issues with event processing
                debug!(
                    "Horizontal scroll event ignored: {:?} at ({}, {})",
                    mouse_event.kind, mouse_event.column, mouse_event.row
                );
                // Explicitly return after handling to ensure no further processing
                return;
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Handle panel switching based on click location
                if mouse_event.column < 30 {
                    // Click in book list area (left 30% of screen)
                    if self.focused_panel != FocusedPanel::FileList {
                        self.focused_panel = FocusedPanel::FileList;
                    }
                } else {
                    // Click in content area (right 70% of screen)
                    if self.focused_panel != FocusedPanel::Content {
                        self.focused_panel = FocusedPanel::Content;
                    }

                    // Get the content area for coordinate conversion
                    let content_area = self.get_content_area_rect();

                    // Handle click detection (double-click, triple-click)
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
                // Handle mouse up in content area only (right side)
                if mouse_event.column >= 30 {
                    let content_area = self.get_content_area_rect();
                    self.text_reader.handle_mouse_up(
                        mouse_event.column,
                        mouse_event.row,
                        content_area,
                    );
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Handle mouse drag in content area only (right side)
                if mouse_event.column >= 30 {
                    let content_area = self.get_content_area_rect();
                    let old_scroll_offset = self.text_reader.scroll_offset;
                    self.text_reader.handle_mouse_drag(
                        mouse_event.column,
                        mouse_event.row,
                        content_area,
                    );
                    // Save bookmark if auto-scroll occurred
                    if self.text_reader.scroll_offset != old_scroll_offset {
                        self.save_bookmark();
                    }
                }
            }
            _ => {
                // Handle other mouse events like clicks, moves, etc.
                debug!("Unhandled mouse event: {:?}", mouse_event.kind);
            }
        }

        let elapsed = start_time.elapsed();
        if elapsed > std::time::Duration::from_millis(10) {
            debug!(
                "handle_mouse_event took {}ms for event {:?}",
                elapsed.as_millis(),
                mouse_event.kind
            );
        }
    }

    pub fn handle_mouse_event_with_batching(
        &mut self,
        initial_mouse_event: MouseEvent,
        event_source: &mut dyn crate::event_source::EventSource,
    ) {
        use std::time::Duration;

        debug!("Processing mouse event: {:?}", initial_mouse_event.kind);

        let mut scroll_down_count = 0;
        let mut scroll_up_count = 0;

        // Count the initial event
        match initial_mouse_event.kind {
            MouseEventKind::ScrollDown => {
                scroll_down_count += 1;
                debug!("Starting vertical scroll batching with ScrollDown");
            }
            MouseEventKind::ScrollUp => {
                scroll_up_count += 1;
                debug!("Starting vertical scroll batching with ScrollUp");
            }
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                // Horizontal scroll events are not batched, handle individually and return
                debug!(
                    "Handling horizontal scroll event individually: {:?} at ({}, {})",
                    initial_mouse_event.kind, initial_mouse_event.column, initial_mouse_event.row
                );

                // Validate coordinates to prevent crossterm overflow bug
                if self
                    .is_valid_mouse_coordinates(initial_mouse_event.column, initial_mouse_event.row)
                {
                    self.handle_mouse_event(initial_mouse_event);
                } else {
                    debug!(
                        "Skipping horizontal scroll event with invalid coordinates: ({}, {})",
                        initial_mouse_event.column, initial_mouse_event.row
                    );
                }
                return;
            }
            _ => {
                // Other mouse events are handled normally
                debug!(
                    "Handling non-scroll mouse event: {:?}",
                    initial_mouse_event.kind
                );
                self.handle_mouse_event(initial_mouse_event);
                return;
            }
        }

        // Drain additional mouse scroll events that are queued up
        // Use a very short timeout to avoid blocking but catch rapid events
        let drain_timeout = Duration::from_millis(0); // Non-blocking poll
        let max_drain_iterations = 50; // Safety limit to prevent infinite loops
        let mut drain_count = 0;
        let start_time = std::time::Instant::now();

        while drain_count < max_drain_iterations
            && event_source.poll(drain_timeout).unwrap_or(false)
        {
            drain_count += 1;

            // Timeout circuit breaker - prevent infinite loops or excessive processing
            if start_time.elapsed() > std::time::Duration::from_millis(100) {
                debug!(
                    "Batching timeout reached ({}ms), breaking out of event drain loop",
                    start_time.elapsed().as_millis()
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
                Ok(Event::Mouse(mouse_event)) => {
                    match mouse_event.kind {
                        MouseEventKind::ScrollDown => scroll_down_count += 1,
                        MouseEventKind::ScrollUp => scroll_up_count += 1,
                        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                            // Horizontal scroll events during batching - handle individually and stop batching
                            debug!("Horizontal scroll during vertical batch, handling individually: {:?} at ({}, {})",
                                   mouse_event.kind, mouse_event.column, mouse_event.row);

                            // Validate coordinates to prevent crossterm overflow bug
                            if self.is_valid_mouse_coordinates(mouse_event.column, mouse_event.row)
                            {
                                self.handle_mouse_event(mouse_event);
                                debug!(
                                    "Completed horizontal scroll handling, breaking out of batch"
                                );
                            } else {
                                debug!("Skipping horizontal scroll event with invalid coordinates during batching: ({}, {})",
                                       mouse_event.column, mouse_event.row);
                            }
                            break;
                        }
                        _ => {
                            // Other mouse events, handle normally and stop batching
                            self.handle_mouse_event(mouse_event);
                            break;
                        }
                    }
                }
                Ok(_) => {
                    // Non-mouse event, stop draining (we'll process it in the next iteration)
                    break;
                }
                Err(e) => {
                    // Error reading event, log and stop draining
                    debug!("Error reading event during batching: {:?}", e);
                    break;
                }
            }
        }

        // Process the net scroll effect
        let net_scroll = scroll_down_count as i32 - scroll_up_count as i32;

        debug!(
            "Batched mouse events: {} down, {} up, net: {}, drained: {} events",
            scroll_down_count, scroll_up_count, net_scroll, drain_count
        );

        if net_scroll > 0 {
            // Net scroll down - content area only
            for _ in 0..net_scroll.min(10) {
                self.scroll_down();
            }
        } else if net_scroll < 0 {
            // Net scroll up - content area only
            for _ in 0..(-net_scroll).min(10) {
                self.scroll_up();
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
        self.book_list.render(
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
                            app.handle_mouse_event_with_batching(mouse_event, event_source);
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
                        KeyCode::Char('j') => {
                            // Navigate based on focused panel
                            if app.focused_panel == FocusedPanel::FileList {
                                app.book_list.move_selection_down(&app.book_manager);
                            } else {
                                app.scroll_down();
                            }
                        }
                        KeyCode::Char('k') => {
                            // Navigate based on focused panel
                            if app.focused_panel == FocusedPanel::FileList {
                                app.book_list.move_selection_up();
                            } else {
                                app.scroll_up();
                            }
                        }
                        KeyCode::Char('h') => {
                            // Always allow chapter navigation
                            app.prev_chapter();
                        }
                        KeyCode::Char('l') => {
                            // Always allow chapter navigation
                            app.next_chapter();
                        }
                        KeyCode::Char('o') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Ctrl+O: Open current EPUB with system viewer
                                app.open_with_system_viewer();
                            }
                        }
                        KeyCode::Enter => {
                            // Select book from file list (works from any panel)
                            if let Some(book_info) =
                                app.book_manager.get_book_info(app.book_list.selected)
                            {
                                let path = book_info.path.clone();
                                // Save bookmark for current file before switching
                                app.save_bookmark_with_throttle(true);
                                app.load_epub(&path);
                                // Switch focus to content after loading
                                app.focused_panel = FocusedPanel::Content;
                            }
                        }
                        KeyCode::Tab => {
                            // Switch focus between panels
                            app.focused_panel = match app.focused_panel {
                                FocusedPanel::FileList => FocusedPanel::Content,
                                FocusedPanel::Content => FocusedPanel::FileList,
                            };
                        }
                        KeyCode::Char('d') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Get the visible height for half-screen calculation
                                let visible_height =
                                    terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
                                app.scroll_half_screen_down(visible_height);
                            }
                        }
                        KeyCode::Char('u') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Get the visible height for half-screen calculation
                                let visible_height =
                                    terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
                                app.scroll_half_screen_up(visible_height);
                            }
                        }
                        KeyCode::Char('c') => {
                            // Handle 'c' with any modifiers (c, Ctrl+C, Cmd+C, etc.) for copy functionality
                            debug!("Copy key 'c' pressed with modifiers: {:?}", key.modifiers);
                            if let Err(e) = app.text_reader.copy_selection_to_clipboard() {
                                debug!("Copy failed: {}", e);
                            }
                        }
                        KeyCode::Esc => {
                            if app.text_reader.has_text_selection() {
                                // Clear text selection when ESC is pressed
                                app.text_reader.clear_selection();
                                debug!("Text selection cleared via ESC key");
                            }
                        }
                        _ => {}
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
