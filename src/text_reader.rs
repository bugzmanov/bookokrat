use crate::theme::Base16Palette;
use log::debug;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::time::Instant;
use textwrap;

pub struct TextReader {
    pub scroll_offset: usize,
    pub content_length: usize,
    last_scroll_time: Instant,
    scroll_speed: usize,
    // Highlight state for navigation aid
    pub highlight_visual_line: Option<usize>, // Visual line number to highlight (0-based)
    pub highlight_end_time: Instant,          // When to stop highlighting
    // Store the total wrapped lines for bounds checking
    pub total_wrapped_lines: usize,
    pub visible_height: usize,
}

impl TextReader {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            content_length: 0,
            last_scroll_time: Instant::now(),
            scroll_speed: 1,
            highlight_visual_line: None,
            highlight_end_time: Instant::now(),
            total_wrapped_lines: 0,
            visible_height: 0,
        }
    }

    /// Calculate total wrapped lines for the given content and width
    pub fn update_wrapped_lines(&mut self, content: &str, width: usize, visible_height: usize) {
        self.visible_height = visible_height;

        // Count lines including chapter title if present
        let mut total_lines = 0;

        // Add lines for chapter title (title + empty line)
        total_lines += 2;

        // Wrap each line of content
        for line in content.lines() {
            if line.trim().is_empty() {
                total_lines += 1;
            } else {
                let wrapped_lines = textwrap::wrap(line, width);
                total_lines += wrapped_lines.len();
            }
        }

        self.total_wrapped_lines = total_lines;
        debug!(
            "Updated wrapped lines: {} total, {} visible",
            total_lines, visible_height
        );
    }

    /// Get the maximum allowed scroll offset
    pub fn get_max_scroll_offset(&self) -> usize {
        if self.total_wrapped_lines > self.visible_height {
            self.total_wrapped_lines - self.visible_height
        } else {
            0
        }
    }

    /// Clamp scroll offset to valid bounds
    pub fn clamp_scroll_offset(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = self.scroll_offset.min(max_offset);
        debug!(
            "Clamped scroll offset to: {} (max: {})",
            self.scroll_offset, max_offset
        );
    }

    pub fn parse_styled_text(
        &self,
        text: &str,
        chapter_title: &Option<String>,
        palette: &Base16Palette,
        width: usize,
    ) -> Text {
        let mut lines = Vec::new();

        if let Some(ref title) = chapter_title {
            lines.push(Line::from(vec![Span::styled(
                title.clone(),
                Style::default()
                    .fg(palette.base_0d)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(String::new()));
        }

        // Process each line with manual wrapping
        for line in text.lines() {
            if line.trim().is_empty() {
                lines.push(Line::from(String::new()));
                continue;
            }

            // Wrap the line using textwrap
            let wrapped_lines = textwrap::wrap(line, width);
            for wrapped_line in wrapped_lines {
                let line_str = wrapped_line.to_string();
                let styled_line = self.parse_line_styling(&line_str, palette);
                lines.push(styled_line);
            }
        }

        Text::from(lines)
    }

    /// Parse styling for a single line (bold, quotes, etc.)
    fn parse_line_styling(&self, line: &str, palette: &Base16Palette) -> Line {
        let mut spans = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let mut current_text = String::new();

        while i < chars.len() {
            if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(palette.base_07),
                    ));
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
                        Style::default()
                            .fg(palette.base_08)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    current_text.push_str("**");
                    current_text.push_str(&bold_text);
                }
            } else if (chars[i] == '"' || chars[i] == '\u{201C}' || chars[i] == '\u{201D}')
                && chars[i] != '\''
                && chars[i] != '\u{2018}'
                && chars[i] != '\u{2019}'
            {
                let quote_char = chars[i];
                let closing_quote = match quote_char {
                    '"' => '"',
                    '\u{201C}' => '\u{201D}',
                    '\u{201D}' => '\u{201D}',
                    _ => quote_char,
                };

                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(palette.base_07),
                    ));
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
                            Style::default()
                                .fg(palette.base_0d)
                                .add_modifier(Modifier::BOLD),
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
            } else {
                current_text.push(chars[i]);
                i += 1;
            }
        }

        if !current_text.is_empty() {
            spans.push(Span::styled(
                current_text,
                Style::default().fg(palette.base_07),
            ));
        }

        if spans.is_empty() {
            Line::from(String::new())
        } else {
            Line::from(spans)
        }
    }

    pub fn calculate_progress(
        &self,
        content: &str,
        visible_width: usize,
        visible_height: usize,
    ) -> u32 {
        let total_lines = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| (line.len() as f32 / visible_width as f32).ceil() as usize)
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

    pub fn scroll_down(&mut self, _content: &str) {
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

        // Apply scroll with current speed and clamp to bounds
        self.scroll_offset = self.scroll_offset.saturating_add(self.scroll_speed);
        self.clamp_scroll_offset();

        debug!(
            "Scrolling down to offset: {}/{} (speed: {}, max: {})",
            self.scroll_offset,
            self.total_wrapped_lines,
            self.scroll_speed,
            self.get_max_scroll_offset()
        );
    }

    pub fn scroll_up(&mut self, _content: &str) {
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

        // Apply scroll with current speed (saturating_sub already handles lower bound)
        self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);

        debug!(
            "Scrolling up to offset: {}/{} (speed: {}, max: {})",
            self.scroll_offset,
            self.total_wrapped_lines,
            self.scroll_speed,
            self.get_max_scroll_offset()
        );
    }

    pub fn scroll_half_screen_down(&mut self, _content: &str, screen_height: usize) {
        let half_screen = (screen_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_add(half_screen);
        self.clamp_scroll_offset();

        // Simply highlight the middle line of the current window
        let middle_line = screen_height / 2;

        debug!("Half-screen down to offset: {}/{}, highlighting middle line at screen position: {}, max: {}", 
               self.scroll_offset, self.total_wrapped_lines, middle_line, self.get_max_scroll_offset());

        // Set up highlighting for 1 second
        self.highlight_visual_line = Some(middle_line);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_secs(1);
    }

    pub fn scroll_half_screen_up(&mut self, _content: &str, screen_height: usize) {
        let half_screen = (screen_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(half_screen);

        // Simply highlight the middle line of the current window
        let middle_line = screen_height / 2;

        debug!("Half-screen up to offset: {}/{}, highlighting middle line at screen position: {}, max: {}", 
               self.scroll_offset, self.total_wrapped_lines, middle_line, self.get_max_scroll_offset());

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

    /// Update wrapped lines if dimensions have changed
    pub fn update_wrapped_lines_if_needed(&mut self, content: &str, area: Rect) {
        let text_width = area.width.saturating_sub(12) as usize; // Account for margins
        let visible_height = area.height.saturating_sub(3) as usize; // Account for borders

        // Only update if dimensions have changed
        if self.visible_height != visible_height {
            self.update_wrapped_lines(content, text_width, visible_height);
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

        let mut title = format!(
            "Chapter {}/{} {}%",
            current_chapter + 1,
            total_chapters,
            chapter_progress
        );

        // Add chapter title if available, with truncation if necessary
        if let Some(ref chapter_title_str) = chapter_title {
            let separator = " : ";
            let title_with_separator = format!("{}{}{}", title, separator, chapter_title_str);

            // Calculate available space for title (leave some padding for border)
            let available_width = area.width.saturating_sub(4) as usize; // Account for borders and padding

            if title_with_separator.len() <= available_width {
                title = title_with_separator;
            } else {
                // Truncate chapter title to fit
                let base_length = title.len() + separator.len();
                let available_for_chapter = available_width.saturating_sub(base_length);

                if available_for_chapter >= 4 {
                    // Minimum space for "..." + at least 1 char
                    let truncated_chapter = if available_for_chapter >= chapter_title_str.len() {
                        chapter_title_str.clone()
                    } else {
                        let max_chars = available_for_chapter.saturating_sub(3); // Reserve space for "..."
                        format!(
                            "{}...",
                            &chapter_title_str[..max_chars.min(chapter_title_str.len())]
                        )
                    };
                    title = format!("{}{}{}", title, separator, truncated_chapter);
                }
            }
        }

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
                Constraint::Length(1), // Top margin
                Constraint::Min(0),    // Content area
            ])
            .split(inner_area);

        // Create horizontal margins
        let margined_content_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(5), // Left margin
                Constraint::Min(0),    // Content area
                Constraint::Length(5), // Right margin
            ])
            .split(vertical_margined_area[1]);

        // Render the actual content with manual wrapping
        let text_width = margined_content_area[1].width as usize;
        let styled_content = self.parse_styled_text(content, chapter_title, palette, text_width);

        let content_paragraph = Paragraph::new(styled_content)
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
                    let highlight_block =
                        Block::default().style(Style::default().bg(Color::Yellow));
                    f.render_widget(highlight_block, highlight_area);
                }
            }
        }
    }
}
