use crate::book_list::BookList;
use crate::book_manager::BookManager;
use crate::bookmark::Bookmarks;
use crate::event_source::EventSource;
use crate::text_generator::TextGenerator;
use crate::text_reader::TextReader;
use crate::theme::OCEANIC_NEXT;

use std::{io::BufReader, process::Command, time::Duration};

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use epub::doc::EpubDoc;
use log::{error, info};
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
                .or_else(|_| Command::new("xdg-open").arg(absolute_path_str.as_ref()).spawn())
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
    fn open_with_macos_epub_reader(&self, path: &str, chapter: usize) -> Result<std::process::Child, std::io::Error> {
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
        
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No compatible EPUB reader found"))
    }
    
    /// Try to open EPUB with Windows-specific readers at the given chapter
    fn open_with_windows_epub_reader(&self, path: &str, chapter: usize) -> Result<std::process::Child, std::io::Error> {
        // Try Calibre ebook-viewer first
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }
        
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No compatible EPUB reader found"))
    }
    
    /// Try to open EPUB with Linux-specific readers at the given chapter
    fn open_with_linux_epub_reader(&self, path: &str, chapter: usize) -> Result<std::process::Child, std::io::Error> {
        // Try Calibre ebook-viewer first
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }
        
        // Try FBReader
        if let Ok(child) = self.try_fbreader(path, chapter) {
            return Ok(child);
        }
        
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No compatible EPUB reader found"))
    }
    
    /// Try to open with ClearView (macOS)
    fn try_clearview(&self, path: &str, _chapter: usize) -> Result<std::process::Child, std::io::Error> {
        // ClearView is a GUI-only application without CLI chapter navigation support
        // Just open the file normally - user will need to navigate manually
        Command::new("open")
            .args(["-a", "ClearView", path])
            .spawn()
    }
    
    /// Try to open with Calibre ebook-viewer (cross-platform)
    fn try_calibre_viewer(&self, path: &str, chapter: usize) -> Result<std::process::Child, std::io::Error> {
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
                    .spawn() {
                    return Ok(child);
                }
            }
        }
        
        // Fallback: just open the file normally
        Command::new("ebook-viewer")
            .arg(path)
            .spawn()
    }
    
    /// Try to open with Skim (macOS)
    fn try_skim(&self, path: &str, _chapter: usize) -> Result<std::process::Child, std::io::Error> {
        // Skim doesn't support command-line chapter navigation
        Command::new("open")
            .args(["-a", "Skim", path])
            .spawn()
    }
    
    /// Try to open with FBReader (Linux)
    fn try_fbreader(&self, path: &str, _chapter: usize) -> Result<std::process::Child, std::io::Error> {
        // FBReader doesn't support command-line chapter navigation
        Command::new("fbreader")
            .arg(path)
            .spawn()
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
        self.executed_commands.borrow_mut().push(format!("{}@chapter{}", path, chapter));
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
    pub mode: Mode,
    current_file: Option<String>,
    current_chapter_title: Option<String>,
    // Animation state
    pub animation_progress: f32, // 0.0 = file list mode, 1.0 = reading mode
    target_progress: f32,        // Target animation value
    is_animating: bool,
    pub system_command_executor: Box<dyn SystemCommandExecutor>,
}

#[derive(PartialEq, Debug)]
pub enum Mode {
    FileList,
    Content,
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
            None => Bookmarks::load().unwrap_or_else(|e| {
                error!("Failed to load bookmarks: {}", e);
                Bookmarks::new()
            }),
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
            mode: Mode::FileList,
            current_file: None,
            current_chapter_title: None,
            // Initialize animation state
            animation_progress: 0.0, // Start in file list mode
            target_progress: 0.0,
            is_animating: false,
            system_command_executor: system_executor,
        };

        // Auto-load the most recently read book if available
        if auto_load_recent {
            if let Some((recent_path, _)) = app.bookmarks.get_most_recent() {
                // Check if the most recent book still exists in the managed books
                if app.book_manager.contains_book(&recent_path) {
                    info!("Auto-loading most recent book: {}", recent_path);
                    app.load_epub(&recent_path);
                    app.mode = Mode::Content;
                    app.target_progress = 1.0;
                    app.animation_progress = 1.0; // Skip animation on startup
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
                self.mode = Mode::Content;
            }
            Err(e) => {
                error!("Failed to load EPUB: {}", e);
            }
        }
    }

    pub fn save_bookmark(&mut self) {
        if let Some(path) = &self.current_file {
            self.bookmarks.update_bookmark(
                path,
                self.current_chapter,
                self.text_reader.scroll_offset,
            );
            if let Err(e) = self.bookmarks.save() {
                error!("Failed to save bookmark: {}", e);
            }
        }
    }

    fn update_animation(&mut self) {
        if self.is_animating {
            let animation_speed = 0.15; // Animation speed (higher = faster)
            let diff = self.target_progress - self.animation_progress;

            if diff.abs() < 0.01 {
                // Animation complete
                self.animation_progress = self.target_progress;
                self.is_animating = false;
            } else {
                // Continue animation
                self.animation_progress += diff * animation_speed;
            }
        }

        // Update highlight state in text reader
        self.text_reader.update_highlight();
    }

    fn start_animation(&mut self, target: f32) {
        self.target_progress = target;
        self.is_animating = true;
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
                    self.save_bookmark();
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
                    self.save_bookmark();
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

    pub fn open_with_system_viewer(&self) {
        if let Some(path) = &self.current_file {
            info!("Opening EPUB with system viewer: {} at chapter {}", path, self.current_chapter);

            match self.system_command_executor.open_file_at_chapter(path, self.current_chapter) {
                Ok(_) => info!("Successfully opened EPUB with system viewer at chapter {}", self.current_chapter),
                Err(e) => error!("Failed to open EPUB with system viewer: {}", e),
            }
        } else {
            error!("No EPUB file currently loaded");
        }
    }

    #[cfg(test)]
    pub fn get_scroll_offset(&self) -> usize {
        self.text_reader.scroll_offset
    }

    fn scroll_half_screen_up(&mut self, screen_height: usize) {
        if let Some(content) = &self.current_content {
            self.text_reader
                .scroll_half_screen_up(content, screen_height);
            self.save_bookmark();
        }
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame) {
        // Clear the entire frame with the dark background first
        let background_block = Block::default().style(Style::default().bg(OCEANIC_NEXT.base_00));
        f.render_widget(background_block, f.size());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(f.size());

        // Calculate animated layout percentages
        // animation_progress: 0.0 = (30%, 70%), 1.0 = (10%, 90%)
        let file_list_percentage = 30 - (20.0 * self.animation_progress) as u16;
        let content_percentage = 70 + (20.0 * self.animation_progress) as u16;

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(file_list_percentage),
                Constraint::Percentage(content_percentage),
            ])
            .split(chunks[0]);

        // Delegate rendering to components
        self.book_list.render(
            f,
            main_chunks[0],
            self.mode == Mode::FileList,
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
                    self.mode == Mode::Content,
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
        let (_, text_color, border_color, _, _) = OCEANIC_NEXT.get_interface_colors(false);

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

        let help_text = match self.mode {
            Mode::FileList => "j/k: Navigate | Enter: Select | Tab: Switch View | q: Quit",
            Mode::Content => "j/k: Scroll | Ctrl+d/u: Half-screen | h/l: Chapter | Ctrl+O: Open | Tab: Switch | q: Quit",
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
        terminal.draw(|f| app.draw(f))?;
        let timeout = if app.is_animating {
            Duration::from_millis(16) // ~60 FPS when animating
        } else {
            tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0))
        };
        if event_source.poll(timeout)? {
            if let Event::Key(key) = event_source.read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        // Save bookmark before quitting
                        app.save_bookmark();
                        return Ok(());
                    }
                    KeyCode::Char('j') => {
                        if app.mode == Mode::FileList {
                            app.book_list.move_selection_down(&app.book_manager);
                        } else {
                            app.scroll_down();
                        }
                    }
                    KeyCode::Char('k') => {
                        if app.mode == Mode::FileList {
                            app.book_list.move_selection_up();
                        } else {
                            app.scroll_up();
                        }
                    }
                    KeyCode::Char('h') => {
                        if app.mode == Mode::Content {
                            app.prev_chapter();
                        }
                    }
                    KeyCode::Char('l') => {
                        if app.mode == Mode::Content {
                            app.next_chapter();
                        }
                    }
                    KeyCode::Char('o') => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            // Ctrl+O: Open current EPUB with system viewer
                            if app.mode == Mode::Content {
                                app.open_with_system_viewer();
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if app.mode == Mode::FileList {
                            if let Some(book_info) =
                                app.book_manager.get_book_info(app.book_list.selected)
                            {
                                let path = book_info.path.clone();
                                // Save bookmark for current file before switching
                                app.save_bookmark();
                                app.load_epub(&path);
                            }
                        }
                    }
                    KeyCode::Tab => {
                        if app.mode == Mode::FileList {
                            app.mode = Mode::Content;
                            app.start_animation(1.0); // Expand to reading mode
                        } else {
                            // When switching back to file list, restore selection to current file
                            if let Some(current_file) = &app.current_file {
                                if let Some(index) =
                                    app.book_manager.find_book_by_path(current_file)
                                {
                                    app.book_list.set_selection_to_index(index);
                                }
                            }
                            app.mode = Mode::FileList;
                            app.start_animation(0.0); // Contract to file list mode
                        };
                    }
                    KeyCode::Char('d') => {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && app.mode == Mode::Content
                        {
                            // Get the visible height for half-screen calculation
                            let visible_height =
                                terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
                            app.scroll_half_screen_down(visible_height);
                        }
                    }
                    KeyCode::Char('u') => {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && app.mode == Mode::Content
                        {
                            // Get the visible height for half-screen calculation
                            let visible_height =
                                terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
                            app.scroll_half_screen_up(visible_height);
                        }
                    }
                    _ => {}
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            app.update_animation(); // Update animation state
            last_tick = std::time::Instant::now();
        }
    }
}
