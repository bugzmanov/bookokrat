mod bookmark;

use std::{
    fs::File,
    io::{stdout, BufReader},
    time::Duration,
    path::Path,
};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use epub::doc::EpubDoc;
use log::{debug, error, info, warn};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use simplelog::{Config, LevelFilter, WriteLogger};
use regex;

use crate::bookmark::Bookmarks;

struct App {
    epub_files: Vec<String>,
    selected: usize,
    current_content: Option<String>,
    list_state: ListState,
    current_epub: Option<EpubDoc<BufReader<std::fs::File>>>,
    current_chapter: usize,
    total_chapters: usize,
    scroll_offset: usize,
    mode: Mode,
    bookmarks: Bookmarks,
    current_file: Option<String>,
    content_length: usize,
    last_scroll_time: std::time::Instant,
    scroll_speed: usize,
    p_tag_re: regex::Regex,
    h_open_re: regex::Regex,
    h_close_re: regex::Regex,
    remaining_tags_re: regex::Regex,
    multi_space_re: regex::Regex,
    multi_newline_re: regex::Regex,
    leading_space_re: regex::Regex,
    line_leading_space_re: regex::Regex,
    words_per_minute: f32,
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
        let p_tag_re = regex::Regex::new(r"<p[^>]*>")
            .expect("Failed to compile paragraph tag regex");
        let h_open_re = regex::Regex::new(r"<h[1-6][^>]*>")
            .expect("Failed to compile header open tag regex");
        let h_close_re = regex::Regex::new(r"</h[1-6]>")
            .expect("Failed to compile header close tag regex");
        let remaining_tags_re = regex::Regex::new(r"<[^>]*>")
            .expect("Failed to compile remaining tags regex");
        let multi_space_re = regex::Regex::new(r" +")
            .expect("Failed to compile multi space regex");
        let multi_newline_re = regex::Regex::new(r"\n{3,}")
            .expect("Failed to compile multi newline regex");
        let leading_space_re = regex::Regex::new(r"^ +")
            .expect("Failed to compile leading space regex");
        let line_leading_space_re = regex::Regex::new(r"\n +")
            .expect("Failed to compile line leading space regex");

        let epub_files: Vec<String> = std::fs::read_dir(".")
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension()?.to_str()? == "epub" {
                    // Store full path for internal use
                    Some(path.to_str()?.to_string())
                } else {
                    None
                }
            })
            .collect();

        let mut list_state = ListState::default();
        // Select first book if available
        let has_files = !epub_files.is_empty();
        if has_files {
            list_state.select(Some(0));
        }

        let bookmarks = Bookmarks::load().unwrap_or_else(|e| {
            error!("Failed to load bookmarks: {}", e);
            Bookmarks::new()
        });

        Self {
            epub_files: epub_files.clone(),
            selected: if has_files { 0 } else { 0 },
            current_content: None,
            list_state,
            current_epub: None,
            current_chapter: 0,
            total_chapters: 0,
            scroll_offset: 0,
            mode: Mode::FileList,
            bookmarks,
            current_file: None,
            content_length: 0,
            last_scroll_time: std::time::Instant::now(),
            scroll_speed: 1,
            p_tag_re,
            h_open_re,
            h_close_re,
            remaining_tags_re,
            multi_space_re,
            multi_newline_re,
            leading_space_re,
            line_leading_space_re,
            words_per_minute: 250.0,  // Average reading speed
            current_chapter_title: None,
            // Initialize animation state
            animation_progress: 0.0,  // Start in file list mode
            target_progress: 0.0,
            is_animating: false,
        }
    }

    fn load_epub(&mut self, path: &str) {
        info!("Attempting to load EPUB: {}", path);
        if let Ok(mut doc) = EpubDoc::new(path) {
            info!("Successfully created EPUB document");
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
            self.bookmarks.update_bookmark(path, self.current_chapter, self.scroll_offset);
            if let Err(e) = self.bookmarks.save() {
                error!("Failed to save bookmark: {}", e);
            }
        }
    }

    fn calculate_reading_time(&self, text: &str) -> (u32, u32) {
        let word_count = text.split_whitespace().count() as f32;
        let total_minutes = word_count / self.words_per_minute;
        let hours = (total_minutes / 60.0) as u32;
        let minutes = (total_minutes % 60.0) as u32;
        (hours, minutes)
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
    }

    fn start_animation(&mut self, target: f32) {
        self.target_progress = target;
        self.is_animating = true;
    }

    fn extract_chapter_title(&self, html_content: &str) -> Option<String> {
        // Try to extract title from h1, h2, or title tags
        let title_patterns = [
            regex::Regex::new(r"<h[12][^>]*>([^<]+)</h[12]>").ok()?,
            regex::Regex::new(r"<title[^>]*>([^<]+)</title>").ok()?,
        ];
        
        for pattern in &title_patterns {
            if let Some(captures) = pattern.captures(html_content) {
                if let Some(title_match) = captures.get(1) {
                    let title = title_match.as_str().trim();
                    if !title.is_empty() && title.len() < 100 {
                        return Some(title.to_string());
                    }
                }
            }
        }
        None
    }

    fn format_text_with_spacing(&self, text: &str) -> String {
        let mut formatted = String::new();
        let paragraphs: Vec<&str> = text.split("\n\n").collect();
        
        for (i, paragraph) in paragraphs.iter().enumerate() {
            if paragraph.trim().is_empty() {
                continue;
            }
            
            // Add indentation to the first line of each paragraph (except headers)
            let trimmed = paragraph.trim();
            // Detect headers: short lines (< 60 chars) with mostly uppercase or numbers
            let is_header = trimmed.len() > 0 && trimmed.len() < 60 && 
                           (trimmed.chars().filter(|c| c.is_alphabetic()).count() > 0) &&
                           (trimmed.chars().filter(|c| c.is_uppercase()).count() as f32 / 
                            trimmed.chars().filter(|c| c.is_alphabetic()).count() as f32 > 0.7);
            
            if !is_header && !trimmed.starts_with("    ") { // Don't indent blockquotes
                formatted.push_str("    "); // 4-space indent for paragraphs
            }
            
            // Process lines within the paragraph
            let lines: Vec<&str> = paragraph.lines().collect();
            for (j, line) in lines.iter().enumerate() {
                formatted.push_str(line);
                if j < lines.len() - 1 {
                    formatted.push('\n');
                }
            }
            
            // Add spacing between paragraphs
            if i < paragraphs.len() - 1 {
                formatted.push_str("\n\n"); // 2 newlines = 1 empty line between paragraphs
            }
        }
        
        formatted
    }

    fn update_content(&mut self) {
        if let Some(doc) = &mut self.current_epub {
            if let Ok(content) = doc.get_current_str() {
                debug!("Raw content length: {} bytes", content.len());
                
                // Extract chapter title before processing
                self.current_chapter_title = self.extract_chapter_title(&content);
                
                // First pass: Remove CSS and script blocks entirely
                let style_re = regex::Regex::new(r"(?s)<style[^>]*>.*?</style>").unwrap();
                let script_re = regex::Regex::new(r"(?s)<script[^>]*>.*?</script>").unwrap();
                let content = style_re.replace_all(&content, "");
                let content = script_re.replace_all(&content, "");
                
                // Second pass: Replace HTML entities
                let text = content
                    .replace("&nbsp;", " ")
                    .replace("&amp;", "&")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&mdash;", "—")
                    .replace("&ndash;", "–")
                    .replace("&hellip;", "...")
                    .replace("&ldquo;", "\u{201C}")  // Opening double quote
                    .replace("&rdquo;", "\u{201D}")  // Closing double quote
                    .replace("&lsquo;", "\u{2018}")  // Opening single quote
                    .replace("&rsquo;", "\u{2019}"); // Closing single quote

                // Third pass: Convert semantic HTML elements to plain text with proper formatting
                let text = self.p_tag_re.replace_all(&text, "").to_string();
                
                let text = text
                    .replace("</p>", "\n\n")
                    // Preserve line breaks
                    .replace("<br>", "\n")
                    .replace("<br/>", "\n")
                    .replace("<br />", "\n")
                    // Handle blockquotes (for direct speech or citations)
                    .replace("<blockquote>", "\n    ")
                    .replace("</blockquote>", "\n")
                    // Handle emphasis
                    .replace("<em>", "_")
                    .replace("</em>", "_")
                    .replace("<i>", "_")
                    .replace("</i>", "_")
                    // Handle strong emphasis
                    .replace("<strong>", "**")
                    .replace("</strong>", "**")
                    .replace("<b>", "**")
                    .replace("</b>", "**");

                // Handle headers
                let text = self.h_open_re.replace_all(&text, "\n\n").to_string();
                let text = self.h_close_re.replace_all(&text, "\n\n").to_string();

                // Fourth pass: Remove any remaining HTML tags
                let text = self.remaining_tags_re.replace_all(&text, "").to_string();

                // Fifth pass: Clean up whitespace while preserving intentional formatting
                let text = self.multi_space_re.replace_all(&text, " ").to_string();
                let text = self.multi_newline_re.replace_all(&text, "\n\n").to_string();
                let text = self.leading_space_re.replace_all(&text, "").to_string();
                let text = self.line_leading_space_re.replace_all(&text, "\n").to_string();
                let text = text.trim().to_string();

                debug!("Text after HTML cleanup: {}", text.chars().take(100).collect::<String>());
                
                if text.is_empty() {
                    warn!("Converted text is empty");
                    self.current_content = Some("No content available in this chapter.".to_string());
                    self.content_length = 0;
                } else {
                    debug!("Final text length: {} bytes", text.len());
                    // Apply typography formatting with line spacing and indentation
                    let formatted_text = self.format_text_with_spacing(&text);
                    self.current_content = Some(formatted_text);
                    self.content_length = self.current_content.as_ref().unwrap().len();
                }
            } else {
                error!("Failed to get current chapter content");
                self.current_content = Some("Error reading chapter content.".to_string());
                self.content_length = 0;
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

    fn draw(&mut self, f: &mut ratatui::Frame) {
        // Define colors based on current mode
        let (interface_color, text_color, border_color, highlight_bg, highlight_fg) = if self.mode == Mode::Content {
            // In reading mode, gray out interface elements but keep text prominent
            (Color::DarkGray, Color::White, Color::DarkGray, Color::DarkGray, Color::White)
        } else {
            // In file list mode, use normal colors
            (Color::White, Color::White, Color::White, Color::White, Color::Black)
        };
        
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
            .epub_files
            .iter()
            .map(|file| {
                let bookmark = self.bookmarks.get_bookmark(file);
                let last_read = bookmark
                    .map(|b| b.last_read.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Never".to_string());
                
                // Get filename without path and extension for display
                let display_name = Path::new(file)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                
                let content = Line::from(vec![
                    Span::styled(
                        display_name,
                        Style::default().fg(interface_color),
                    ),
                    Span::styled(
                        format!(" ({})", last_read),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(content)
            })
            .collect();

        let files = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Books").border_style(Style::default().fg(border_color)))
            .highlight_style(Style::default().bg(highlight_bg).fg(highlight_fg));

        f.render_stateful_widget(files, main_chunks[0], &mut self.list_state.clone());

        // Draw content with margins
        let content_area = main_chunks[1];
        
        let content = self
            .current_content
            .as_deref()
            .unwrap_or("Select a file to view its content");
        
        let title = if self.current_epub.is_some() {
            let chapter_progress = if self.content_length > 0 {
                // Get the visible area width and height (accounting for margins and borders)
                // We subtract 8 for left/right margins (4 each) and 2 for borders
                let visible_width = content_area.width.saturating_sub(10) as usize;
                // We subtract 2 for borders and 1 for top margin
                let visible_height = content_area.height.saturating_sub(3) as usize;
                
                // Calculate total scrollable lines by counting actual content lines
                let content = self.current_content.as_ref().unwrap();
                let total_lines = content
                    .lines()
                    .filter(|line| !line.trim().is_empty()) // Skip empty lines
                    .map(|line| {
                        // Calculate how many terminal lines this content line will take
                        (line.len() as f32 / visible_width as f32).ceil() as usize
                    })
                    .sum::<usize>();
                
                // Calculate current visible line based on scroll offset
                let current_line = self.scroll_offset;
                
                // Calculate the maximum scroll position (when last line becomes visible at the bottom)
                let max_scroll = if total_lines > visible_height {
                    total_lines - visible_height
                } else {
                    0
                };
                
                // Calculate percentage based on how far we've scrolled to the max position
                let progress = if max_scroll > 0 {
                    ((current_line as f32 / max_scroll as f32) * 100.0).min(100.0) as u32
                } else {
                    100 // If content fits in one screen, we're at 100%
                };
                
                progress
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
            
            let chapter_info = if let Some(ref title) = self.current_chapter_title {
                format!("Chapter {}: {}", self.current_chapter + 1, title)
            } else {
                format!("Chapter {}/{}", self.current_chapter + 1, self.total_chapters)
            };
            
            format!("{} {}%{}", chapter_info, chapter_progress, time_text)
        } else {
            "Content".to_string()
        };

        // Draw the border with title around the entire content area
        let content_border = Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(border_color));
        
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
                Constraint::Length(4),  // Left margin
                Constraint::Min(0),     // Content area
                Constraint::Length(4),  // Right margin
            ])
            .split(vertical_margined_area[1]);
        
        // Render the actual content in the margined area
        let content_paragraph = Paragraph::new(content)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll_offset as u16, 0))
            .style(Style::default().fg(text_color));
            
        f.render_widget(content_paragraph, margined_content_area[1]);

        // Draw help bar
        let help_text = match self.mode {
            Mode::FileList => "j/k: Navigate | Enter: Select | Tab: Switch View | q: Quit",
            Mode::Content => "j/k: Scroll | h/l: Change Chapter | Tab: Switch View | q: Quit",
        };
        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(border_color)))
            .style(Style::default().fg(Color::DarkGray));
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
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('j') => {
                        if app.mode == Mode::FileList {
                            if app.selected < app.epub_files.len().saturating_sub(1) {
                                app.selected += 1;
                                app.list_state.select(Some(app.selected));
                            }
                        } else {
                            app.scroll_down();
                        }
                    }
                    KeyCode::Char('k') => {
                        if app.mode == Mode::FileList {
                            if app.selected > 0 {
                                app.selected -= 1;
                                app.list_state.select(Some(app.selected));
                            }
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
                            if let Some(path) = app.epub_files.get(app.selected).cloned() {
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
                                if let Some(pos) = app.epub_files.iter().position(|f| f == current_file) {
                                    app.selected = pos;
                                    app.list_state.select(Some(pos));
                                }
                            }
                            app.mode = Mode::FileList;
                            app.start_animation(0.0); // Contract to file list mode
                        };
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
