mod bookmark;
mod text_generator;
mod book_list;
mod text_renderer;
mod theme;

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
use log::{debug, error, info};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use simplelog::{Config, LevelFilter, WriteLogger};
use crate::text_generator::TextGenerator;
use crate::book_list::BookList;
use crate::text_renderer::TextRenderer;
use crate::theme::OCEANIC_NEXT;


struct App {
    book_list: BookList,
    text_generator: TextGenerator,
    text_renderer: TextRenderer,
    current_content: Option<String>,
    current_epub: Option<EpubDoc<BufReader<std::fs::File>>>,
    current_chapter: usize,
    total_chapters: usize,
    scroll_offset: usize,
    mode: Mode,
    current_file: Option<String>,
    content_length: usize,
    last_scroll_time: std::time::Instant,
    scroll_speed: usize,
    current_chapter_title: Option<String>,
    // Animation state
    animation_progress: f32,  // 0.0 = file list mode, 1.0 = reading mode
    target_progress: f32,     // Target animation value
    is_animating: bool,
    // Highlight state for navigation aid
    highlight_visual_line: Option<usize>,  // Visual line number to highlight (0-based)
    highlight_end_time: std::time::Instant,  // When to stop highlighting
}

#[derive(PartialEq)]
enum Mode {
    FileList,
    Content,
}


impl App {
    fn new() -> Self {
        let book_list = BookList::new();
        let text_generator = TextGenerator::new();
        let text_renderer = TextRenderer::new();
        
        let mut app = Self {
            book_list,
            text_generator,
            text_renderer,
            current_content: None,
            current_epub: None,
            current_chapter: 0,
            total_chapters: 0,
            scroll_offset: 0,
            mode: Mode::FileList,
            current_file: None,
            content_length: 0,
            last_scroll_time: std::time::Instant::now(),
            scroll_speed: 1,
            current_chapter_title: None,
            // Initialize animation state
            animation_progress: 0.0,  // Start in file list mode
            target_progress: 0.0,
            is_animating: false,
            // Initialize highlight state
            highlight_visual_line: None,
            highlight_end_time: std::time::Instant::now(),
        };

        // Auto-load the most recently read book if available
        if let Some(recent_path) = app.book_list.get_most_recent_book() {
            info!("Auto-loading most recent book: {}", recent_path);
            app.load_epub(&recent_path);
            app.mode = Mode::Content;
            app.target_progress = 1.0;
            app.animation_progress = 1.0; // Skip animation on startup
        }

        app
    }

    fn load_epub(&mut self, path: &str) {
        info!("Attempting to load EPUB: {}", path);
        if let Ok(mut doc) = EpubDoc::new(path) {
            info!("Successfully created EPUB document");
            self.total_chapters = doc.get_num_pages();
            info!("Total chapters: {}", self.total_chapters);
            
            // Try to load bookmark
            if let Some(bookmark) = self.book_list.bookmarks.get_bookmark(path) {
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
                    self.scroll_offset = bookmark.scroll_offset;
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
        } else {
            error!("Failed to load EPUB: {}", path);
        }
    }

    fn save_bookmark(&mut self) {
        if let Some(path) = &self.current_file {
            self.book_list.bookmarks.update_bookmark(path, self.current_chapter, self.scroll_offset);
            if let Err(e) = self.book_list.bookmarks.save() {
                error!("Failed to save bookmark: {}", e);
            }
        }
    }

    fn calculate_reading_time(&self, text: &str) -> (u32, u32) {
        self.text_renderer.calculate_reading_time(text)
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
        
        // Clear expired highlight
        if let Some(_) = self.highlight_visual_line {
            if std::time::Instant::now() >= self.highlight_end_time {
                self.highlight_visual_line = None;
            }
        }
    }

    fn start_animation(&mut self, target: f32) {
        self.target_progress = target;
        self.is_animating = true;
    }


    fn parse_styled_text(&self, text: &str) -> Text<'static> {
        self.text_renderer.parse_styled_text(text, &self.current_chapter_title, &OCEANIC_NEXT)
    }


    fn update_content(&mut self) {
        if let Some(doc) = &mut self.current_epub {
            match self.text_generator.process_chapter_content(doc) {
                Ok((content, title)) => {
                    self.current_chapter_title = title;
                    self.current_content = Some(content);
                    self.content_length = self.current_content.as_ref().unwrap().len();
                }
                Err(e) => {
                    error!("Failed to process chapter: {}", e);
                    self.current_content = Some("Error reading chapter content.".to_string());
                    self.content_length = 0;
                }
            }
        } else {
            error!("No EPUB document loaded");
            self.current_content = Some("No EPUB document loaded.".to_string());
            self.content_length = 0;
        }
    }

    fn next_chapter(&mut self) {
        if let Some(doc) = &mut self.current_epub {
            if self.current_chapter < self.total_chapters - 1 {
                if doc.go_next().is_ok() {
                    self.current_chapter += 1;
                    info!("Moving to next chapter: {}", self.current_chapter + 1);
                    self.update_content();
                    self.scroll_offset = 0;
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
                    self.scroll_offset = 0;
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
            // Check if we're scrolling continuously
            let now = std::time::Instant::now();
            if now.duration_since(self.last_scroll_time) < std::time::Duration::from_millis(100) {
                // Increase scroll speed up to a maximum
                self.scroll_speed = (self.scroll_speed + 1).min(10);
            } else {
                // Reset scroll speed if there was a pause
                self.scroll_speed = 1;
            }
            self.last_scroll_time = now;

            // Apply scroll with current speed
            self.scroll_offset = self.scroll_offset.saturating_add(self.scroll_speed);
            let total_lines = content.lines().count();
            debug!("Scrolling down to offset: {}/{} (speed: {})", self.scroll_offset, total_lines, self.scroll_speed);
            self.save_bookmark();
        }
    }

    fn scroll_up(&mut self) {
        if let Some(content) = &self.current_content {
            // Check if we're scrolling continuously
            let now = std::time::Instant::now();
            if now.duration_since(self.last_scroll_time) < std::time::Duration::from_millis(100) {
                // Increase scroll speed up to a maximum
                self.scroll_speed = (self.scroll_speed + 1).min(10);
            } else {
                // Reset scroll speed if there was a pause
                self.scroll_speed = 1;
            }
            self.last_scroll_time = now;

            // Apply scroll with current speed
            self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);
            let total_lines = content.lines().count();
            debug!("Scrolling up to offset: {}/{} (speed: {})", self.scroll_offset, total_lines, self.scroll_speed);
            self.save_bookmark();
        }
    }

    fn scroll_half_screen_down(&mut self, screen_height: usize) {
        if let Some(content) = &self.current_content {
            let half_screen = (screen_height / 2).max(1);
            self.scroll_offset = self.scroll_offset.saturating_add(half_screen);
            
            // Simply highlight the middle line of the current window
            let middle_line = screen_height / 2;
            
            let total_lines = content.lines().count();
            debug!("Half-screen down to offset: {}/{}, highlighting middle line at screen position: {}", 
                   self.scroll_offset, total_lines, middle_line);
            
            // Set up highlighting for 2 seconds
            self.highlight_visual_line = Some(middle_line);
            self.highlight_end_time = std::time::Instant::now() + std::time::Duration::from_secs(1);
            
            self.save_bookmark();
        }
    }

    fn scroll_half_screen_up(&mut self, screen_height: usize) {
        if let Some(content) = &self.current_content {
            let half_screen = (screen_height / 2).max(1);
            self.scroll_offset = self.scroll_offset.saturating_sub(half_screen);
            
            // Simply highlight the middle line of the current window
            let middle_line = screen_height / 2;
            
            let total_lines = content.lines().count();
            debug!("Half-screen up to offset: {}/{}, highlighting middle line at screen position: {}", 
                   self.scroll_offset, total_lines, middle_line);
            
            // Set up highlighting for 2 seconds
            self.highlight_visual_line = Some(middle_line);
            self.highlight_end_time = std::time::Instant::now() + std::time::Duration::from_secs(1);
            
            self.save_bookmark();
        }
    }

    fn draw(&mut self, f: &mut ratatui::Frame) {
        // Clear the entire frame with the dark background first
        let background_block = Block::default().style(Style::default().bg(OCEANIC_NEXT.base_00));
        f.render_widget(background_block, f.size());
        
        // Define colors using Oceanic Next palette
        let (interface_color, text_color, border_color, highlight_bg, highlight_fg) = 
            OCEANIC_NEXT.get_interface_colors(self.mode == Mode::Content);
        
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

        // Draw file list
        let items: Vec<ListItem> = self
            .book_list
            .epub_files
            .iter()
            .map(|file| {
                let bookmark = self.book_list.bookmarks.get_bookmark(file);
                let last_read = bookmark
                    .map(|b| b.last_read.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Never".to_string());
                
                let display_name = BookList::get_display_name(file);
                
                let content = Line::from(vec![
                    Span::styled(
                        display_name,
                        Style::default().fg(interface_color),
                    ),
                    Span::styled(
                        format!(" ({})", last_read),
                        Style::default().fg(OCEANIC_NEXT.base_03),
                    ),
                ]);
                ListItem::new(content)
            })
            .collect();

        let files = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Books")
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(OCEANIC_NEXT.base_00)))
            .highlight_style(Style::default().bg(highlight_bg).fg(highlight_fg))
            .style(Style::default().bg(OCEANIC_NEXT.base_00));

        f.render_stateful_widget(files, main_chunks[0], &mut self.book_list.list_state.clone());

        // Draw content with margins
        let content_area = main_chunks[1];
        
        let content = self
            .current_content
            .as_deref()
            .unwrap_or("Select a file to view its content");
        
        let title = if self.current_epub.is_some() {
            let chapter_progress = if self.content_length > 0 {
                // Get the visible area width and height (accounting for margins and borders)
                // We subtract 10 for left/right margins (5 each) and 2 for borders
                let visible_width = content_area.width.saturating_sub(12) as usize;
                // We subtract 2 for borders and 1 for top margin
                let visible_height = content_area.height.saturating_sub(3) as usize;
                
                let content = self.current_content.as_ref().unwrap();
                self.text_renderer.calculate_progress(self.scroll_offset, content, visible_width, visible_height)
            } else {
                0
            };
            // Calculate reading time for remaining content in chapter
            let remaining_content = if self.scroll_offset < self.current_content.as_ref().unwrap().lines().count() {
                let lines: Vec<&str> = self.current_content.as_ref().unwrap().lines().collect();
                let remaining_lines = &lines[self.scroll_offset.min(lines.len())..];
                remaining_lines.join("\n")
            } else {
                String::new()
            };
            
            let (hours, minutes) = self.calculate_reading_time(&remaining_content);
            let time_text = if hours > 0 {
                format!(" • {}h {}m left", hours, minutes)
            } else if minutes > 0 {
                format!(" • {}m left", minutes)
            } else {
                " • Done".to_string()
            };
            
            format!("Chapter {}/{} {}%{}", self.current_chapter + 1, self.total_chapters, chapter_progress, time_text)
        } else {
            "Content".to_string()
        };

        // Draw the border with title around the entire content area
        let content_border = Block::default().borders(Borders::ALL).title(title)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(OCEANIC_NEXT.base_00));
        
        // Get the inner area (inside the borders) before rendering the block
        let inner_area = content_border.inner(content_area);
        
        // Now render the border
        f.render_widget(content_border, content_area);
        
        // Create vertical margins first (top margin)
        let vertical_margined_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // Top margin
                Constraint::Min(0),     // Content area
            ])
            .split(inner_area);
        
        // Create horizontal margins within the vertically margined area
        let margined_content_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(5),  // Left margin (increased from 4)
                Constraint::Min(0),     // Content area
                Constraint::Length(5),  // Right margin (increased from 4)
            ])
            .split(vertical_margined_area[1]);
        
        // Render the actual content in the margined area
        let styled_content = if self.current_content.is_some() {
            self.parse_styled_text(content)
        } else {
            Text::raw(content)
        };
        
        let content_paragraph = Paragraph::new(styled_content)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll_offset as u16, 0))
            .style(Style::default().fg(text_color).bg(OCEANIC_NEXT.base_00));
            
        f.render_widget(content_paragraph, margined_content_area[1]);

        // Draw highlight overlay if active
        if let Some(highlight_line) = self.highlight_visual_line {
            if std::time::Instant::now() < self.highlight_end_time {
                let content_area = margined_content_area[1];
                // Only highlight if the line is within the visible area
                if highlight_line < content_area.height as usize {
                    let highlight_area = ratatui::layout::Rect {
                        x: content_area.x,
                        y: content_area.y + highlight_line as u16,
                        width: content_area.width,
                        height: 1,
                    };
                    let highlight_block = Block::default()
                        .style(Style::default().bg(Color::Yellow));
                    f.render_widget(highlight_block, highlight_area);
                }
            }
        }

        // Draw help bar
        let help_text = match self.mode {
            Mode::FileList => "j/k: Navigate | Enter: Select | Tab: Switch View | q: Quit",
            Mode::Content => "j/k: Scroll | Ctrl+d/u: Half-screen | h/l: Chapter | Tab: Switch | q: Quit",
        };
        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(OCEANIC_NEXT.base_00)))
            .style(Style::default().fg(OCEANIC_NEXT.base_03).bg(OCEANIC_NEXT.base_00));
        f.render_widget(help, chunks[1]);
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
                            app.book_list.move_selection_down();
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
                            if let Some(path) = app.book_list.get_selected_file() {
                                let path_owned = path.to_string();
                                // Save bookmark for current file before switching
                                app.save_bookmark();
                                app.load_epub(&path_owned);
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
                                app.book_list.set_selection_to_file(current_file);
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
