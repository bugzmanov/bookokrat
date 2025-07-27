use crate::theme::Base16Palette;
use crate::text_selection::TextSelection;
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

#[derive(Debug, Clone)]
struct AutoScrollState {
    direction: AutoScrollDirection,
    mouse_x: u16,
    mouse_y: u16,
    content_area: Rect,
    last_scroll_time: Instant,
}

#[derive(Debug, Clone, PartialEq)]
enum AutoScrollDirection {
    Up,
    Down,
}

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
    // Cache dimensions to prevent redundant calculations
    cached_text_width: usize,
    cached_content_hash: u64,
    // Cache styled content to avoid expensive re-parsing
    cached_styled_content: Option<Text<'static>>,
    cached_styled_width: usize,
    cached_chapter_title_hash: u64,
    // Text selection state
    pub text_selection: TextSelection,
    // Store raw text lines for selection extraction
    raw_text_lines: Vec<String>,
    // Store the last content area for mouse coordinate conversion
    pub last_content_area: Option<Rect>,
    // Auto-scroll state for continuous scrolling during text selection
    auto_scroll_state: Option<AutoScrollState>,
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
            cached_text_width: 0,
            cached_content_hash: 0,
            cached_styled_content: None,
            cached_styled_width: 0,
            cached_chapter_title_hash: 0,
            text_selection: TextSelection::new(),
            raw_text_lines: Vec::new(),
            last_content_area: None,
            auto_scroll_state: None,
        }
    }

    /// Simple hash function for content caching
    fn simple_hash(content: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// Calculate total wrapped lines for the given content and width
    pub fn update_wrapped_lines(&mut self, content: &str, width: usize, visible_height: usize) {
        self.visible_height = visible_height;
        self.cached_text_width = width;
        self.cached_content_hash = Self::simple_hash(content);

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
            "Updated wrapped lines: {} total, {} visible, width: {}",
            total_lines, visible_height, width
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

    pub fn parse_styled_text_cached(
        &mut self,
        text: &str,
        chapter_title: &Option<String>,
        palette: &Base16Palette,
        width: usize,
    ) -> Text {
        let content_hash = Self::simple_hash(text);
        let chapter_title_hash = chapter_title
            .as_ref()
            .map(|t| Self::simple_hash(t))
            .unwrap_or(0);

        // Check if we need to regenerate the cached content
        let needs_update = self.cached_styled_content.is_none()
            || self.cached_styled_width != width
            || self.cached_content_hash != content_hash
            || self.cached_chapter_title_hash != chapter_title_hash;

        if needs_update {
            debug!("Regenerating styled content cache: width {} -> {}, content changed: {}, title changed: {}",
                   self.cached_styled_width, width,
                   self.cached_content_hash != content_hash,
                   self.cached_chapter_title_hash != chapter_title_hash);

            let (styled_content, raw_lines) =
                self.parse_styled_text_internal_with_raw(text, chapter_title, palette, width);
            self.cached_styled_content = Some(styled_content);
            self.raw_text_lines = raw_lines;
            self.cached_styled_width = width;
            self.cached_content_hash = content_hash;
            self.cached_chapter_title_hash = chapter_title_hash;
        }

        let mut result = self.cached_styled_content.as_ref().unwrap().clone();
        
        // Apply selection highlighting if there's an active selection
        if self.text_selection.has_selection() {
            // Use theme color for selection - base_02 is typically used for selection/highlight
            let selection_bg_color = palette.base_02;
            let highlighted_lines: Vec<Line> = result
                .lines
                .into_iter()
                .enumerate()
                .map(|(line_idx, line)| {
                    self.text_selection.apply_selection_highlighting(
                        line_idx,
                        line.spans,
                        selection_bg_color,
                    )
                })
                .collect();
            result = ratatui::text::Text::from(highlighted_lines);
        }
        
        result
    }

    fn parse_styled_text_internal_with_raw(
        &mut self,
        text: &str,
        chapter_title: &Option<String>,
        palette: &Base16Palette,
        width: usize,
    ) -> (Text<'static>, Vec<String>) {
        let mut lines = Vec::new();
        let mut raw_lines = Vec::new();

        // Add chapter title lines to raw text
        if let Some(ref title) = chapter_title {
            raw_lines.push(title.clone());
            raw_lines.push(String::new());
            
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
                raw_lines.push(String::new());
                lines.push(Line::from(String::new()));
                continue;
            }

            // Wrap the line using textwrap
            let wrapped_lines = textwrap::wrap(line, width);
            for wrapped_line in wrapped_lines {
                let line_str = wrapped_line.to_string();
                raw_lines.push(line_str.clone());
                let styled_line = self.parse_line_styling_owned(&line_str, palette);
                lines.push(styled_line);
            }
        }

        // Return both styled content and raw lines
        (Text::from(lines), raw_lines)
    }

    /// Parse styling for a single line (bold, quotes, etc.) - owned version for caching
    fn parse_line_styling_owned(&self, line: &str, palette: &Base16Palette) -> Line<'static> {
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
            .map(|line| {
                // Use integer arithmetic for stable line wrapping calculation
                // This is equivalent to ceil(line.len() / visible_width)
                (line.len() + visible_width - 1) / visible_width
            })
            .sum::<usize>();

        let max_scroll = if total_lines > visible_height {
            total_lines - visible_height
        } else {
            0
        };

        if max_scroll > 0 {
            // Use integer arithmetic for stable rounding
            // This ensures consistent results across different runs
            let progress = (self.scroll_offset * 100) / max_scroll;
            progress.min(100) as u32
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
        // Clear caches when resetting (typically when changing chapters)
        self.cached_styled_content = None;
        self.cached_content_hash = 0;
        self.cached_chapter_title_hash = 0;
        // Clear text selection when changing chapters
        self.text_selection.clear_selection();
        self.raw_text_lines.clear();
        // Clear auto-scroll state
        self.auto_scroll_state = None;
    }

    pub fn restore_scroll_position(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn scroll_down(&mut self, _content: &str) {
        let max_offset = self.get_max_scroll_offset();

        // Early return if we're already at the bottom
        if self.scroll_offset >= max_offset {
            debug!(
                "Already at bottom, scroll_offset: {}, max_offset: {}",
                self.scroll_offset, max_offset
            );
            return;
        }

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
        let new_offset = self.scroll_offset.saturating_add(self.scroll_speed);
        self.scroll_offset = new_offset.min(max_offset);

        debug!(
            "Scrolling down to offset: {}/{} (speed: {}, max: {})",
            self.scroll_offset, self.total_wrapped_lines, self.scroll_speed, max_offset
        );
    }

    pub fn scroll_up(&mut self, _content: &str) {
        // Early return if we're already at the top
        if self.scroll_offset == 0 {
            debug!("Already at top, scroll_offset: 0");
            return;
        }

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
        let max_offset = self.get_max_scroll_offset();
        let new_offset = self.scroll_offset.saturating_add(half_screen);
        self.scroll_offset = new_offset.min(max_offset);

        // Simply highlight the middle line of the current window
        let middle_line = screen_height / 2;

        debug!("Half-screen down to offset: {}/{}, highlighting middle line at screen position: {}, max: {}", 
               self.scroll_offset, self.total_wrapped_lines, middle_line, max_offset);

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

    /// Handle mouse down event for text selection
    pub fn handle_mouse_down(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            debug!("Mouse down at text coordinates: line {}, column {}", line, column);
            self.text_selection.start_selection(line, column);
        }
    }

    /// Handle mouse drag event for text selection
    pub fn handle_mouse_drag(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        // Check if we need to auto-scroll due to dragging outside the visible area
        let needs_scroll_up = screen_y < content_area.y;
        let needs_scroll_down = screen_y >= content_area.y + content_area.height;
        
        if needs_scroll_up {
            // Set up auto-scroll state for continuous upward scrolling
            self.auto_scroll_state = Some(AutoScrollState {
                direction: AutoScrollDirection::Up,
                mouse_x: screen_x,
                mouse_y: screen_y,
                content_area,
                last_scroll_time: Instant::now(),
            });
            // Perform initial scroll
            self.perform_auto_scroll();
        } else if needs_scroll_down {
            // Set up auto-scroll state for continuous downward scrolling
            self.auto_scroll_state = Some(AutoScrollState {
                direction: AutoScrollDirection::Down,
                mouse_x: screen_x,
                mouse_y: screen_y,
                content_area,
                last_scroll_time: Instant::now(),
            });
            // Perform initial scroll
            self.perform_auto_scroll();
        } else {
            // Mouse is back in content area - stop auto-scrolling
            self.auto_scroll_state = None;
            // Normal drag within visible area
            if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
                self.text_selection.update_selection(line, column);
            }
        }
    }

    /// Handle mouse up event for text selection
    pub fn handle_mouse_up(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        // Stop auto-scrolling when mouse is released
        self.auto_scroll_state = None;
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            self.text_selection.update_selection(line, column);
        }
        self.text_selection.end_selection();
    }

    /// Clear text selection
    pub fn clear_selection(&mut self) {
        self.text_selection.clear_selection();
        self.auto_scroll_state = None;
    }

    /// Handle double-click for word selection
    pub fn handle_double_click(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            debug!("Double-click at text coordinates: line {}, column {}", line, column);
            self.text_selection.select_word_at(line, column, &self.raw_text_lines);
        }
    }

    /// Handle triple-click for paragraph selection
    pub fn handle_triple_click(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            debug!("Triple-click at text coordinates: line {}, column {}", line, column);
            self.text_selection.select_paragraph_at(line, column, &self.raw_text_lines);
        }
    }

    /// Perform auto-scroll based on current auto-scroll state
    fn perform_auto_scroll(&mut self) {
        if let Some(ref state) = self.auto_scroll_state.clone() {
            match state.direction {
                AutoScrollDirection::Up => {
                    if self.scroll_offset > 0 {
                        // Calculate scroll distance based on how far above the content area the mouse is
                        let distance_above = state.content_area.y.saturating_sub(state.mouse_y) as usize;
                        // Use integer arithmetic for stable scroll amount calculation
                        let scroll_amount = ((distance_above + 9) / 10).max(1).min(3);
                        
                        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
                        debug!("Auto-scroll up by {} to offset: {}", scroll_amount, self.scroll_offset);
                        
                        // Update selection to the top line of the visible area
                        let top_line = self.scroll_offset;
                        let column = if state.mouse_x < state.content_area.x { 0 } else { (state.mouse_x - state.content_area.x) as usize };
                        self.text_selection.update_selection(top_line, column);
                    }
                }
                AutoScrollDirection::Down => {
                    let max_offset = self.get_max_scroll_offset();
                    if self.scroll_offset < max_offset {
                        // Calculate scroll distance based on how far below the content area the mouse is
                        let distance_below = state.mouse_y.saturating_sub(state.content_area.y + state.content_area.height) as usize;
                        // Use integer arithmetic for stable scroll amount calculation
                        let scroll_amount = ((distance_below + 9) / 10).max(1).min(3);
                        
                        let new_offset = (self.scroll_offset + scroll_amount).min(max_offset);
                        self.scroll_offset = new_offset;
                        debug!("Auto-scroll down by {} to offset: {}", scroll_amount, self.scroll_offset);
                        
                        // Update selection to the bottom line of the visible area
                        let bottom_line = self.scroll_offset + self.visible_height.saturating_sub(1);
                        let column = if state.mouse_x < state.content_area.x { 0 } else { (state.mouse_x - state.content_area.x) as usize };
                        self.text_selection.update_selection(bottom_line, column);
                    }
                }
            }
        }
    }

    /// Update auto-scroll - should be called continuously from the main loop
    pub fn update_auto_scroll(&mut self) -> bool {
        let should_scroll = if let Some(ref state) = self.auto_scroll_state {
            let now = Instant::now();
            // Auto-scroll every 100ms (10 FPS)
            now.duration_since(state.last_scroll_time) >= std::time::Duration::from_millis(100)
        } else {
            false
        };

        if should_scroll {
            self.perform_auto_scroll();
            if let Some(ref mut state) = self.auto_scroll_state {
                state.last_scroll_time = Instant::now();
            }
            true // Indicates that scrolling occurred and a redraw is needed
        } else {
            false
        }
    }

    /// Convert screen coordinates to logical text coordinates
    fn screen_to_text_coords(&self, screen_x: u16, screen_y: u16, content_area: Rect) -> Option<(usize, usize)> {
        self.text_selection.screen_to_text_coords(
            screen_x,
            screen_y,
            self.scroll_offset,
            content_area.x,
            content_area.y,
        )
    }

    /// Update wrapped lines if dimensions or content have changed
    pub fn update_wrapped_lines_if_needed(&mut self, content: &str, area: Rect) {
        let text_width = area.width.saturating_sub(12) as usize; // Account for margins
        let visible_height = area.height.saturating_sub(3) as usize; // Account for borders
        let content_hash = Self::simple_hash(content);

        // Only update if dimensions or content have changed
        if self.visible_height != visible_height
            || self.cached_text_width != text_width
            || self.cached_content_hash != content_hash
        {
            debug!("Triggering wrapped lines update: height {} -> {}, width {} -> {}, content changed: {}",
                   self.visible_height, visible_height, self.cached_text_width, text_width,
                   self.cached_content_hash != content_hash);
            self.update_wrapped_lines(content, text_width, visible_height);
        }
    }

    pub fn render(
        &mut self,
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
                        let truncated = chapter_title_str
                            .chars()
                            .take(max_chars)
                            .collect::<String>();
                        format!("{}...", truncated)
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

        // Render the actual content with manual wrapping and selection highlighting
        let text_width = margined_content_area[1].width as usize;
        let scroll_offset = self.scroll_offset;
        let styled_content =
            self.parse_styled_text_cached(content, chapter_title, palette, text_width);

        let content_paragraph = Paragraph::new(styled_content)
            .scroll((scroll_offset as u16, 0))
            .style(Style::default().fg(text_color).bg(palette.base_00));

        f.render_widget(content_paragraph, margined_content_area[1]);
        
        // Store the content area for mouse coordinate conversion
        self.last_content_area = Some(margined_content_area[1]);

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
