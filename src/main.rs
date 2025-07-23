mod bookmark;
mod text_generator;
mod book_list;
mod text_reader;
mod theme;
mod book_manager;

use std::{
    fs::File,
    io::{stdout, BufReader},
    time::Duration,
};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use epub::doc::EpubDoc;
use log::{error, info};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use simplelog::{Config, LevelFilter, WriteLogger};
use crate::text_generator::TextGenerator;
use crate::book_list::BookList;
use crate::text_reader::TextReader;
use crate::theme::OCEANIC_NEXT;
use crate::bookmark::Bookmarks;
use crate::book_manager::BookManager;


struct App {
    book_manager: BookManager,
    book_list: BookList,
    text_generator: TextGenerator,
    text_reader: TextReader,
    bookmarks: Bookmarks,
    current_content: Option<String>,
    current_epub: Option<EpubDoc<BufReader<std::fs::File>>>,
    current_chapter: usize,
    total_chapters: usize,
    mode: Mode,
    current_file: Option<String>,
    current_chapter_title: Option<String>,
    // Animation state
    animation_progress: f32,  // 0.0 = file list mode, 1.0 = reading mode
    target_progress: f32,     // Target animation value
    is_animating: bool,
}

#[derive(PartialEq)]
enum Mode {
    FileList,
    Content,
}


impl App {
    fn new() -> Self {
        let book_manager = BookManager::new();
        let book_list = BookList::new(&book_manager);
        let text_generator = TextGenerator::new();
        let text_reader = TextReader::new();
        
        let bookmarks = Bookmarks::load().unwrap_or_else(|e| {
            error!("Failed to load bookmarks: {}", e);
            Bookmarks::new()
        });
        
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
            animation_progress: 0.0,  // Start in file list mode
            target_progress: 0.0,
            is_animating: false,
        };

        // Auto-load the most recently read book if available
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

        app
    }

    fn load_epub(&mut self, path: &str) {
        match self.book_manager.load_epub(path) {
            Ok(mut doc) => {
                info!("Successfully loaded EPUB document");
                self.total_chapters = doc.get_num_pages();
                info!("Total chapters: {}", self.total_chapters);
                
                // Try to load bookmark
                if let Some(bookmark) = self.bookmarks.get_bookmark(path) {
                    info!("Found bookmark: chapter {}, offset {}", bookmark.chapter, bookmark.scroll_offset);
                    // Skip metadata page if needed
                    if bookmark.chapter > 0 {
                        for _ in 0..bookmark.chapter {
                            if doc.go_next().is_err() {
                                error!("Failed to navigate to bookmarked chapter");
                                break;
                            }
                        }
                        self.current_chapter = bookmark.chapter;
                        self.text_reader.restore_scroll_position(bookmark.scroll_offset);
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

    fn save_bookmark(&mut self) {
        if let Some(path) = &self.current_file {
            self.bookmarks.update_bookmark(path, self.current_chapter, self.text_reader.scroll_offset);
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
                    self.text_reader.set_content_length(self.current_content.as_ref().unwrap().len());
                }
                Err(e) => {
                    error!("Failed to process chapter: {}", e);
                    self.current_content = Some("Error reading chapter content.".to_string());
                    self.text_reader.set_content_length(0);
                }
            }
        } else {
            error!("No EPUB document loaded");
            self.current_content = Some("No EPUB document loaded.".to_string());
            self.text_reader.set_content_length(0);
        }
    }

    fn next_chapter(&mut self) {
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

    fn prev_chapter(&mut self) {
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

    fn scroll_down(&mut self) {
        if let Some(content) = &self.current_content {
            self.text_reader.scroll_down(content);
            self.save_bookmark();
        }
    }

    fn scroll_up(&mut self) {
        if let Some(content) = &self.current_content {
            self.text_reader.scroll_up(content);
            self.save_bookmark();
        }
    }

    fn scroll_half_screen_down(&mut self, screen_height: usize) {
        if let Some(content) = &self.current_content {
            self.text_reader.scroll_half_screen_down(content, screen_height);
            self.save_bookmark();
        }
    }

    fn scroll_half_screen_up(&mut self, screen_height: usize) {
        if let Some(content) = &self.current_content {
            self.text_reader.scroll_half_screen_up(content, screen_height);
            self.save_bookmark();
        }
    }

    fn draw(&mut self, f: &mut ratatui::Frame) {
        // Clear the entire frame with the dark background first
        let background_block = Block::default().style(Style::default().bg(OCEANIC_NEXT.base_00));
        f.render_widget(background_block, f.size());
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
            ])
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
            &self.book_manager
        );

        // Render text content or default message
        if let Some(content) = &self.current_content {
            if self.current_epub.is_some() {
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
            Mode::Content => "j/k: Scroll | Ctrl+d/u: Half-screen | h/l: Chapter | Tab: Switch | q: Quit",
        };
        
        let help = Paragraph::new(help_text)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(OCEANIC_NEXT.base_00)))
            .style(Style::default().fg(OCEANIC_NEXT.base_03).bg(OCEANIC_NEXT.base_00));
            
        f.render_widget(help, area);
    }
}

fn main() -> Result<()> {
    // Initialize logging
    WriteLogger::init(
        LevelFilter::Debug,
        Config::default(),
        File::create("bookrat.log")?,
    )?;

    info!("Starting BookRat EPUB reader");

    // Terminal initialization
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        error!("Application error: {:?}", err);
        println!("{err:?}");
    }

    info!("Shutting down BookRat");
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(50); // Faster tick rate for smoother animation
    let mut last_tick = std::time::Instant::now();

    loop {
        terminal.draw(|f| app.draw(f))?;
        let timeout = if app.is_animating {
            Duration::from_millis(16) // ~60 FPS when animating
        } else {
            tick_rate.checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0))
        };
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        // Save bookmark before quitting
                        app.save_bookmark();
                        return Ok(());
                    },
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
                    KeyCode::Enter => {
                        if app.mode == Mode::FileList {
                            if let Some(book_info) = app.book_manager.get_book_info(app.book_list.selected) {
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
                                if let Some(index) = app.book_manager.find_book_by_path(current_file) {
                                    app.book_list.set_selection_to_index(index);
                                }
                            }
                            app.mode = Mode::FileList;
                            app.start_animation(0.0); // Contract to file list mode
                        };
                    }
                    KeyCode::Char('d') => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) && app.mode == Mode::Content {
                            // Get the visible height for half-screen calculation
                            let visible_height = terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
                            app.scroll_half_screen_down(visible_height);
                        }
                    }
                    KeyCode::Char('u') => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) && app.mode == Mode::Content {
                            // Get the visible height for half-screen calculation
                            let visible_height = terminal.size().unwrap().height.saturating_sub(5) as usize; // Account for borders and help bar
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
