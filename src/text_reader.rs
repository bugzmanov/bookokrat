use ratatui::{
    style::{Modifier, Style, Color},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    layout::{Layout, Direction, Constraint, Rect},
    Frame,
};
use std::time::Instant;
use log::{debug};
use crate::theme::Base16Palette;

pub struct TextReader {
    pub words_per_minute: f32,
    pub scroll_offset: usize,
    pub content_length: usize,
    last_scroll_time: Instant,
    scroll_speed: usize,
    // Highlight state for navigation aid
    pub highlight_visual_line: Option<usize>,  // Visual line number to highlight (0-based)
    pub highlight_end_time: Instant,  // When to stop highlighting
}

impl TextReader {
    pub fn new() -> Self {
        Self {
            words_per_minute: 250.0,
            scroll_offset: 0,
            content_length: 0,
            last_scroll_time: Instant::now(),
            scroll_speed: 1,
            highlight_visual_line: None,
            highlight_end_time: Instant::now(),
        }
    }
    
    pub fn calculate_reading_time(&self, text: &str) -> (u32, u32) {
        let word_count = text.split_whitespace().count() as f32;
        let total_minutes = word_count / self.words_per_minute;
        let hours = (total_minutes / 60.0) as u32;
        let minutes = (total_minutes % 60.0) as u32;
        (hours, minutes)
    }
    
    pub fn parse_styled_text<'a>(&self, text: &str, chapter_title: &Option<String>, palette: &Base16Palette) -> Text<'a> {
        let mut lines = Vec::new();
        
        if let Some(ref title) = chapter_title {
            lines.push(Line::from(vec![
                Span::styled(title.clone(), Style::default().fg(palette.base_0d).add_modifier(Modifier::BOLD))
            ]));
            lines.push(Line::from(""));
        }
        
        let text_lines: Vec<&str> = text.lines().collect();
        for line in text_lines.iter() {
            let mut spans = Vec::new();
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            let mut current_text = String::new();
            
            while i < chars.len() {
                if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                    if !current_text.is_empty() {
                        spans.push(Span::styled(current_text.clone(), Style::default().fg(palette.base_07)));
                        current_text.clear();
                    }
                    
                    i += 2;
                    let mut bold_text = String::new();
                    let mut found_closing = false;
                    while i + 1 < chars.len() {
                        if chars[i] == '*' && chars[i + 1] == '*' {
                            found_closing = true;
                            i += 2;
                            break;
                        } else {
                            bold_text.push(chars[i]);
                            i += 1;
                        }
                    }
                    if found_closing {
                        spans.push(Span::styled(
                            bold_text,
                            Style::default().fg(palette.base_08).add_modifier(Modifier::BOLD)
                        ));
                    } else {
                        current_text.push_str("**");
                        current_text.push_str(&bold_text);
                    }
                }
                else if (chars[i] == '"' || chars[i] == '\u{201C}' || chars[i] == '\u{201D}') &&
                        chars[i] != '\'' && chars[i] != '\u{2018}' && chars[i] != '\u{2019}' {
                    let quote_char = chars[i];
                    let closing_quote = match quote_char {
                        '"' => '"',
                        '\u{201C}' => '\u{201D}',
                        '\u{201D}' => '\u{201D}',
                        _ => quote_char
                    };
                    
                    if !current_text.is_empty() {
                        spans.push(Span::styled(current_text.clone(), Style::default().fg(palette.base_07)));
                        current_text.clear();
                    }
                    
                    let start_pos = i;
                    i += 1;
                    let mut quoted_text = String::new();
                    let mut found_closing = false;
                    
                    let max_quote_length = 200;
                    let search_limit = (i + max_quote_length).min(chars.len());
                    
                    while i < search_limit {
                        if chars[i] == closing_quote || chars[i] == quote_char {
                            spans.push(Span::styled(
                                format!("{}{}{}", quote_char, quoted_text, chars[i]),
                                Style::default().fg(palette.base_0d).add_modifier(Modifier::BOLD)
                            ));
                            i += 1;
                            found_closing = true;
                            break;
                        } else {
                            quoted_text.push(chars[i]);
                            i += 1;
                        }
                    }
                    
                    if !found_closing {
                        current_text.push(chars[start_pos]);
                        i = start_pos + 1;
                    }
                }
                else {
                    current_text.push(chars[i]);
                    i += 1;
                }
            }
            
            if !current_text.is_empty() {
                spans.push(Span::styled(current_text, Style::default().fg(palette.base_07)));
            }
            
            if spans.is_empty() {
                lines.push(Line::from(""));
            } else {
                lines.push(Line::from(spans));
            }
        }
        
        Text::from(lines)
    }
    
    pub fn calculate_progress(&self, content: &str, visible_width: usize, visible_height: usize) -> u32 {
        let total_lines = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                (line.len() as f32 / visible_width as f32).ceil() as usize
            })
            .sum::<usize>();
        
        let max_scroll = if total_lines > visible_height {
            total_lines - visible_height
        } else {
            0
        };
        
        if max_scroll > 0 {
            ((self.scroll_offset as f32 / max_scroll as f32) * 100.0).min(100.0) as u32
        } else {
            100
        }
    }
    
    pub fn set_content_length(&mut self, length: usize) {
        self.content_length = length;
    }
    
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
        self.scroll_speed = 1;
    }
    
    pub fn restore_scroll_position(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }
    
    pub fn scroll_down(&mut self, content: &str) {
        // Check if we're scrolling continuously
        let now = Instant::now();
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
    }
    
    pub fn scroll_up(&mut self, content: &str) {
        // Check if we're scrolling continuously
        let now = Instant::now();
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
    }
    
    pub fn scroll_half_screen_down(&mut self, content: &str, screen_height: usize) {
        let half_screen = (screen_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_add(half_screen);
        
        // Simply highlight the middle line of the current window
        let middle_line = screen_height / 2;
        
        let total_lines = content.lines().count();
        debug!("Half-screen down to offset: {}/{}, highlighting middle line at screen position: {}", 
               self.scroll_offset, total_lines, middle_line);
        
        // Set up highlighting for 1 second
        self.highlight_visual_line = Some(middle_line);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_secs(1);
    }
    
    pub fn scroll_half_screen_up(&mut self, content: &str, screen_height: usize) {
        let half_screen = (screen_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(half_screen);
        
        // Simply highlight the middle line of the current window
        let middle_line = screen_height / 2;
        
        let total_lines = content.lines().count();
        debug!("Half-screen up to offset: {}/{}, highlighting middle line at screen position: {}", 
               self.scroll_offset, total_lines, middle_line);
        
        // Set up highlighting for 1 second
        self.highlight_visual_line = Some(middle_line);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_secs(1);
    }
    
    pub fn update_highlight(&mut self) {
        // Clear expired highlight
        if self.highlight_visual_line.is_some() && Instant::now() >= self.highlight_end_time {
            self.highlight_visual_line = None;
        }
    }
    
    pub fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        content: &str,
        chapter_title: &Option<String>,
        current_chapter: usize,
        total_chapters: usize,
        palette: &Base16Palette,
        is_active: bool,
    ) {
        let (_, text_color, border_color, _, _) = palette.get_interface_colors(is_active);
        
        // Calculate reading progress
        let visible_width = area.width.saturating_sub(12) as usize;
        let visible_height = area.height.saturating_sub(3) as usize;
        let chapter_progress = if self.content_length > 0 {
            self.calculate_progress(content, visible_width, visible_height)
        } else {
            0
        };
        
        // Calculate remaining reading time
        let remaining_content = if self.scroll_offset < content.lines().count() {
            let lines: Vec<&str> = content.lines().collect();
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
        
        let title = format!(
            "Chapter {}/{} {}%{}",
            current_chapter + 1,
            total_chapters,
            chapter_progress,
            time_text
        );
        
        // Draw the border with title
        let content_border = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(palette.base_00));
        
        let inner_area = content_border.inner(area);
        f.render_widget(content_border, area);
        
        // Create vertical margins
        let vertical_margined_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // Top margin
                Constraint::Min(0),     // Content area
            ])
            .split(inner_area);
        
        // Create horizontal margins
        let margined_content_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(5),  // Left margin
                Constraint::Min(0),     // Content area
                Constraint::Length(5),  // Right margin
            ])
            .split(vertical_margined_area[1]);
        
        // Render the actual content
        let styled_content = self.parse_styled_text(content, chapter_title, palette);
        
        let content_paragraph = Paragraph::new(styled_content)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll_offset as u16, 0))
            .style(Style::default().fg(text_color).bg(palette.base_00));
            
        f.render_widget(content_paragraph, margined_content_area[1]);
        
        // Draw highlight overlay if active
        if let Some(highlight_line) = self.highlight_visual_line {
            if Instant::now() < self.highlight_end_time {
                let content_area = margined_content_area[1];
                if highlight_line < content_area.height as usize {
                    let highlight_area = Rect {
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
    }
}