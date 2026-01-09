use super::{LineType, MarkdownTextReader};
use crate::theme::Base16Palette;
use ratatui::{style::Style as RatatuiStyle, text::Span};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CursorPosition {
    pub line: usize,
    pub column: usize,
}

impl CursorPosition {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PendingCharMotion {
    #[default]
    None,
    FindForward,  // f
    FindBackward, // F
    TillForward,  // t
    TillBackward, // T
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VisualMode {
    #[default]
    None,
    CharacterWise, // v
    LineWise,      // V
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PendingYank {
    #[default]
    None,
    WaitingForMotion,                      // after 'y'
    WaitingForInnerObject,                 // after 'yi'
    WaitingForAroundObject,                // after 'ya'
    WaitingForFindChar(PendingCharMotion), // after 'yf', 'yF', 'yt', 'yT'
    WaitingForG,                           // after 'yg' (for ygg)
}

#[derive(Debug, Clone)]
pub struct YankHighlight {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub timestamp: Instant,
}

impl YankHighlight {
    pub fn is_expired(&self) -> bool {
        self.timestamp.elapsed().as_millis() > 250
    }

    pub fn contains(&self, line: usize, col: usize) -> bool {
        if line < self.start_line || line > self.end_line {
            return false;
        }
        if self.start_line == self.end_line {
            return col >= self.start_col && col < self.end_col;
        }
        if line == self.start_line {
            return col >= self.start_col;
        }
        if line == self.end_line {
            return col < self.end_col;
        }
        true
    }
}

#[derive(Debug)]
pub struct NormalModeState {
    pub active: bool,
    pub cursor: CursorPosition,
    /// Tracks if cursor was ever explicitly set (not just default position)
    pub cursor_was_set: bool,
    pub scrolloff: usize,
    pub pending_motion: PendingCharMotion,
    pub last_find: Option<(PendingCharMotion, char)>,
    pub pending_yank: PendingYank,
    pub yank_highlight: Option<YankHighlight>,
    pub count: Option<usize>,
    pub visual_mode: VisualMode,
    pub visual_anchor: Option<CursorPosition>,
}

impl Default for NormalModeState {
    fn default() -> Self {
        Self {
            active: false,
            cursor: CursorPosition::default(),
            cursor_was_set: false,
            scrolloff: 3,
            pending_motion: PendingCharMotion::None,
            last_find: None,
            pending_yank: PendingYank::None,
            yank_highlight: None,
            count: None,
            visual_mode: VisualMode::None,
            visual_anchor: None,
        }
    }
}

impl NormalModeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn activate(&mut self, initial_line: usize, initial_column: usize) {
        self.active = true;
        self.cursor = CursorPosition::new(initial_line, initial_column);
        self.cursor_was_set = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.visual_mode = VisualMode::None;
        self.visual_anchor = None;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl MarkdownTextReader {
    pub fn toggle_normal_mode(&mut self) {
        if self.normal_mode.is_active() {
            self.normal_mode.deactivate();
        } else {
            let previous_line = self.normal_mode.cursor.line;
            let viewport_top = self.scroll_offset;
            let viewport_bottom = self.scroll_offset + self.visible_height;

            // Check if previous cursor position is within current viewport
            let cursor_in_viewport =
                previous_line >= viewport_top && previous_line < viewport_bottom;

            // Only restore previous position if it was explicitly set before
            // and is still within the current viewport
            if self.normal_mode.cursor_was_set
                && cursor_in_viewport
                && !self.should_skip_line(previous_line)
            {
                // Restore previous position, just clamp column
                self.normal_mode.active = true;
                self.clamp_column_to_line_length();
            } else {
                // First activation, position outside viewport, or on invalid line
                // Place cursor at scrolloff margin from top of viewport
                let scrolloff = self.normal_mode.scrolloff;
                let mut initial_line = self.scroll_offset + scrolloff;
                // Clamp to valid range
                initial_line = initial_line.min(self.total_wrapped_lines.saturating_sub(1));
                initial_line = self.find_next_valid_line(initial_line, 1);
                let initial_column = self.get_first_non_whitespace_column(initial_line);
                self.normal_mode.activate(initial_line, initial_column);
            }
        }
    }

    pub fn is_normal_mode_active(&self) -> bool {
        self.normal_mode.is_active()
    }

    pub fn update_normal_mode_cursor(&mut self, line: usize, column: usize) {
        if self.normal_mode.is_active() {
            self.normal_mode.cursor.line = line;
            self.normal_mode.cursor.column = column;
        }
    }

    pub fn append_count_digit(&mut self, digit: char) -> bool {
        if !self.normal_mode.active {
            return false;
        }
        if let Some(d) = digit.to_digit(10) {
            let current = self.normal_mode.count.unwrap_or(0);
            // Prevent overflow and unreasonably large counts
            if current < 10000 {
                self.normal_mode.count = Some(current * 10 + d as usize);
            }
            true
        } else {
            false
        }
    }

    pub fn get_count(&self) -> usize {
        self.normal_mode.count.unwrap_or(1)
    }

    pub fn take_count(&mut self) -> usize {
        self.normal_mode.count.take().unwrap_or(1)
    }

    pub fn has_pending_count(&self) -> bool {
        self.normal_mode.count.is_some()
    }

    pub fn clear_count(&mut self) {
        self.normal_mode.count = None;
    }

    pub fn normal_mode_left(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        if self.normal_mode.cursor.column > 0 {
            self.normal_mode.cursor.column -= 1;
        }
    }

    pub fn normal_mode_right(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let line_len = self.get_line_char_count(self.normal_mode.cursor.line);
        if self.normal_mode.cursor.column < line_len.saturating_sub(1) {
            self.normal_mode.cursor.column += 1;
        }
    }

    pub fn normal_mode_down(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let max_line = self.raw_text_lines.len().saturating_sub(1);
        if self.normal_mode.cursor.line < max_line {
            let mut new_line = self.normal_mode.cursor.line + 1;
            // Skip over image lines
            new_line = self.find_next_valid_line(new_line, 1);
            if new_line <= max_line {
                self.normal_mode.cursor.line = new_line;
                self.clamp_column_to_line_length();
                self.ensure_cursor_visible();
            }
        }
    }

    pub fn normal_mode_up(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        if self.normal_mode.cursor.line > 0 {
            let mut new_line = self.normal_mode.cursor.line - 1;
            // Skip over image lines
            new_line = self.find_next_valid_line(new_line, -1);
            self.normal_mode.cursor.line = new_line;
            self.clamp_column_to_line_length();
            self.ensure_cursor_visible();
        }
    }

    pub fn normal_mode_word_forward(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let (mut new_line, new_col) =
            self.find_next_word_start(self.normal_mode.cursor.line, self.normal_mode.cursor.column);
        // Skip image lines
        if self.is_image_line(new_line) {
            new_line = self.find_next_valid_line(new_line, 1);
        }
        self.normal_mode.cursor.line = new_line;
        self.normal_mode.cursor.column = if self.is_image_line(new_line) {
            0
        } else {
            new_col
        };
        self.clamp_column_to_line_length();
        self.ensure_cursor_visible();
    }

    pub fn normal_mode_word_end(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let (mut new_line, new_col) =
            self.find_word_end(self.normal_mode.cursor.line, self.normal_mode.cursor.column);
        // Skip image lines
        if self.is_image_line(new_line) {
            new_line = self.find_next_valid_line(new_line, 1);
        }
        self.normal_mode.cursor.line = new_line;
        self.normal_mode.cursor.column = if self.is_image_line(new_line) {
            0
        } else {
            new_col
        };
        self.clamp_column_to_line_length();
        self.ensure_cursor_visible();
    }

    pub fn normal_mode_word_backward(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let (mut new_line, new_col) =
            self.find_prev_word_start(self.normal_mode.cursor.line, self.normal_mode.cursor.column);
        // Skip image lines
        if self.is_image_line(new_line) {
            new_line = self.find_next_valid_line(new_line, -1);
        }
        self.normal_mode.cursor.line = new_line;
        self.normal_mode.cursor.column = if self.is_image_line(new_line) {
            0
        } else {
            new_col
        };
        self.clamp_column_to_line_length();
        self.ensure_cursor_visible();
    }

    pub fn normal_mode_line_start(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        self.normal_mode.cursor.column = 0;
    }

    pub fn normal_mode_first_non_whitespace(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        self.normal_mode.cursor.column =
            self.get_first_non_whitespace_column(self.normal_mode.cursor.line);
    }

    pub fn normal_mode_line_end(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let line_len = self.get_line_char_count(self.normal_mode.cursor.line);
        self.normal_mode.cursor.column = line_len.saturating_sub(1);
    }

    pub fn set_pending_find_forward(&mut self) {
        if self.normal_mode.active {
            self.normal_mode.pending_motion = PendingCharMotion::FindForward;
        }
    }

    pub fn set_pending_find_backward(&mut self) {
        if self.normal_mode.active {
            self.normal_mode.pending_motion = PendingCharMotion::FindBackward;
        }
    }

    pub fn set_pending_till_forward(&mut self) {
        if self.normal_mode.active {
            self.normal_mode.pending_motion = PendingCharMotion::TillForward;
        }
    }

    pub fn set_pending_till_backward(&mut self) {
        if self.normal_mode.active {
            self.normal_mode.pending_motion = PendingCharMotion::TillBackward;
        }
    }

    pub fn has_pending_motion(&self) -> bool {
        self.normal_mode.active && self.normal_mode.pending_motion != PendingCharMotion::None
    }

    pub fn clear_pending_motion(&mut self) {
        self.normal_mode.pending_motion = PendingCharMotion::None;
    }

    pub fn execute_pending_find(&mut self, ch: char) -> bool {
        if !self.normal_mode.active {
            return false;
        }

        let motion = self.normal_mode.pending_motion;
        self.normal_mode.pending_motion = PendingCharMotion::None;

        let result = match motion {
            PendingCharMotion::FindForward => self.find_char_forward(ch),
            PendingCharMotion::FindBackward => self.find_char_backward(ch),
            PendingCharMotion::TillForward => self.find_char_till_forward(ch),
            PendingCharMotion::TillBackward => self.find_char_till_backward(ch),
            PendingCharMotion::None => false,
        };

        // Save last find for ; repeat
        if motion != PendingCharMotion::None {
            self.normal_mode.last_find = Some((motion, ch));
        }

        result
    }

    pub fn repeat_last_find(&mut self) -> bool {
        if !self.normal_mode.active {
            return false;
        }

        if let Some((motion, ch)) = self.normal_mode.last_find {
            match motion {
                PendingCharMotion::FindForward => self.find_char_forward(ch),
                PendingCharMotion::FindBackward => self.find_char_backward(ch),
                PendingCharMotion::TillForward => self.find_char_till_forward(ch),
                PendingCharMotion::TillBackward => self.find_char_till_backward(ch),
                PendingCharMotion::None => false,
            }
        } else {
            false
        }
    }

    fn find_char_forward(&mut self, ch: char) -> bool {
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;

        let chars: Vec<char> = self
            .raw_text_lines
            .get(line)
            .map(|s| s.chars().collect())
            .unwrap_or_default();

        // Search forward from current position + 1
        for (i, &c) in chars.iter().enumerate().skip(col + 1) {
            if c == ch {
                self.normal_mode.cursor.column = i;
                return true;
            }
        }
        false
    }

    fn find_char_backward(&mut self, ch: char) -> bool {
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;

        let chars: Vec<char> = self
            .raw_text_lines
            .get(line)
            .map(|s| s.chars().collect())
            .unwrap_or_default();

        // Search backward from current position - 1
        for i in (0..col).rev() {
            if chars.get(i).copied() == Some(ch) {
                self.normal_mode.cursor.column = i;
                return true;
            }
        }
        false
    }

    fn find_char_till_forward(&mut self, ch: char) -> bool {
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;

        let chars: Vec<char> = self
            .raw_text_lines
            .get(line)
            .map(|s| s.chars().collect())
            .unwrap_or_default();

        // Search forward from current position + 1, stop one before the char
        for (i, &c) in chars.iter().enumerate().skip(col + 1) {
            if c == ch && i > 0 {
                self.normal_mode.cursor.column = i - 1;
                return true;
            }
        }
        false
    }

    fn find_char_till_backward(&mut self, ch: char) -> bool {
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;

        let chars: Vec<char> = self
            .raw_text_lines
            .get(line)
            .map(|s| s.chars().collect())
            .unwrap_or_default();

        // Search backward from current position - 1, stop one after the char
        for i in (0..col).rev() {
            if chars.get(i).copied() == Some(ch) {
                self.normal_mode.cursor.column = i + 1;
                return true;
            }
        }
        false
    }

    pub fn normal_mode_paragraph_up(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let new_line = self.find_prev_paragraph_boundary(self.normal_mode.cursor.line);
        self.normal_mode.cursor.line = new_line;
        self.normal_mode.cursor.column = 0;
        self.ensure_cursor_visible();
    }

    pub fn normal_mode_paragraph_down(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let new_line = self.find_next_paragraph_boundary(self.normal_mode.cursor.line);
        self.normal_mode.cursor.line = new_line;
        self.normal_mode.cursor.column = 0;
        self.ensure_cursor_visible();
    }

    pub fn normal_mode_document_top(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let mut line = 0;
        // Skip image lines at the start
        line = self.find_next_valid_line(line, 1);
        self.normal_mode.cursor.line = line;
        self.normal_mode.cursor.column = self.get_first_non_whitespace_column(line);
        self.scroll_offset = 0;
    }

    pub fn normal_mode_document_bottom(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let mut last_line = self.raw_text_lines.len().saturating_sub(1);
        // Skip image lines at the end
        last_line = self.find_next_valid_line(last_line, -1);
        self.normal_mode.cursor.line = last_line;
        self.normal_mode.cursor.column = self.get_first_non_whitespace_column(last_line);
        self.scroll_offset = self.get_max_scroll_offset();
    }

    pub fn normal_mode_half_page_down(&mut self, screen_height: usize) {
        if !self.normal_mode.active {
            return;
        }
        let scroll_amount = screen_height / 2;
        let max_line = self.raw_text_lines.len().saturating_sub(1);

        let mut new_line = (self.normal_mode.cursor.line + scroll_amount).min(max_line);
        // Skip image lines
        new_line = self.find_next_valid_line(new_line, 1);
        self.normal_mode.cursor.line = new_line;
        self.scroll_offset = (self.scroll_offset + scroll_amount).min(self.get_max_scroll_offset());

        self.clamp_column_to_line_length();
    }

    pub fn normal_mode_half_page_up(&mut self, screen_height: usize) {
        if !self.normal_mode.active {
            return;
        }
        let scroll_amount = screen_height / 2;

        let mut new_line = self.normal_mode.cursor.line.saturating_sub(scroll_amount);
        // Skip image lines
        new_line = self.find_next_valid_line(new_line, -1);
        self.normal_mode.cursor.line = new_line;
        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);

        self.clamp_column_to_line_length();
    }

    fn get_line_char_count(&self, line: usize) -> usize {
        self.raw_text_lines
            .get(line)
            .map(|s| s.chars().count())
            .unwrap_or(0)
    }

    fn get_first_non_whitespace_column(&self, line: usize) -> usize {
        self.raw_text_lines
            .get(line)
            .map(|s| s.chars().position(|c| !c.is_whitespace()).unwrap_or(0))
            .unwrap_or(0)
    }

    pub(super) fn is_image_line(&self, line: usize) -> bool {
        self.rendered_content
            .lines
            .get(line)
            .map(|l| matches!(l.line_type, LineType::ImagePlaceholder { .. }))
            .unwrap_or(false)
    }

    fn is_empty_line(&self, line: usize) -> bool {
        self.raw_text_lines
            .get(line)
            .map(|s| s.is_empty())
            .unwrap_or(true)
    }

    fn should_skip_line(&self, line: usize) -> bool {
        self.is_image_line(line) || self.is_empty_line(line)
    }

    fn find_next_valid_line(&self, from: usize, direction: i32) -> usize {
        let max_line = self.raw_text_lines.len().saturating_sub(1);
        let mut line = from;
        loop {
            if !self.should_skip_line(line) {
                return line;
            }
            if direction > 0 {
                if line >= max_line {
                    return max_line;
                }
                line += 1;
            } else {
                if line == 0 {
                    return 0;
                }
                line -= 1;
            }
        }
    }

    pub(super) fn clamp_column_to_line_length(&mut self) {
        let line_len = self.get_line_char_count(self.normal_mode.cursor.line);
        if line_len == 0 {
            self.normal_mode.cursor.column = 0;
        } else {
            self.normal_mode.cursor.column = self
                .normal_mode
                .cursor
                .column
                .min(line_len.saturating_sub(1));
        }
    }

    pub(super) fn ensure_cursor_visible(&mut self) {
        let scrolloff = self.normal_mode.scrolloff;
        let cursor_line = self.normal_mode.cursor.line;
        let viewport_top = self.scroll_offset;
        let viewport_bottom = self.scroll_offset + self.visible_height;

        if cursor_line < viewport_top + scrolloff {
            self.scroll_offset = cursor_line.saturating_sub(scrolloff);
        } else if cursor_line >= viewport_bottom.saturating_sub(scrolloff) {
            let target = cursor_line + scrolloff + 1;
            self.scroll_offset = target
                .saturating_sub(self.visible_height)
                .min(self.get_max_scroll_offset());
        }
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    fn find_next_word_start(&self, line: usize, col: usize) -> (usize, usize) {
        let mut current_line = line;
        let mut current_col = col;
        let total_lines = self.raw_text_lines.len();

        while current_line < total_lines {
            let chars: Vec<char> = self
                .raw_text_lines
                .get(current_line)
                .map(|s| s.chars().collect())
                .unwrap_or_default();

            // Skip current word (if on a word char)
            while current_col < chars.len() {
                if !Self::is_word_char(chars[current_col]) {
                    break;
                }
                current_col += 1;
            }

            // Skip whitespace/non-word chars
            while current_col < chars.len() {
                if Self::is_word_char(chars[current_col]) {
                    return (current_line, current_col);
                }
                current_col += 1;
            }

            current_line += 1;
            current_col = 0;
        }

        let last_line = total_lines.saturating_sub(1);
        (
            last_line,
            self.get_line_char_count(last_line).saturating_sub(1),
        )
    }

    fn find_word_end(&self, line: usize, col: usize) -> (usize, usize) {
        let mut current_line = line;
        let mut current_col = col + 1;
        let total_lines = self.raw_text_lines.len();

        while current_line < total_lines {
            let chars: Vec<char> = self
                .raw_text_lines
                .get(current_line)
                .map(|s| s.chars().collect())
                .unwrap_or_default();

            // Skip leading whitespace/non-word
            while current_col < chars.len() && !Self::is_word_char(chars[current_col]) {
                current_col += 1;
            }

            // Find end of word
            while current_col < chars.len() {
                if current_col + 1 >= chars.len() || !Self::is_word_char(chars[current_col + 1]) {
                    return (current_line, current_col);
                }
                current_col += 1;
            }

            current_line += 1;
            current_col = 0;
        }

        let last_line = total_lines.saturating_sub(1);
        (
            last_line,
            self.get_line_char_count(last_line).saturating_sub(1),
        )
    }

    fn find_prev_word_start(&self, line: usize, col: usize) -> (usize, usize) {
        let mut current_line = line;
        let mut current_col = col;

        // If at column 0, must go to previous line
        if current_col == 0 {
            if current_line == 0 {
                return (0, 0);
            }
            current_line -= 1;
            current_col = self.get_line_char_count(current_line);
        } else {
            current_col -= 1;
        }

        loop {
            let chars: Vec<char> = self
                .raw_text_lines
                .get(current_line)
                .map(|s| s.chars().collect())
                .unwrap_or_default();

            // If line is empty, go to previous line
            if chars.is_empty() {
                if current_line == 0 {
                    return (0, 0);
                }
                current_line -= 1;
                current_col = self.get_line_char_count(current_line);
                continue;
            }

            // Clamp column to valid range
            current_col = current_col.min(chars.len().saturating_sub(1));

            // Skip whitespace/non-word going backward
            while current_col > 0
                && !Self::is_word_char(chars.get(current_col).copied().unwrap_or(' '))
            {
                current_col -= 1;
            }

            // Check if we found a word char
            if Self::is_word_char(chars.get(current_col).copied().unwrap_or(' ')) {
                // Find start of this word
                while current_col > 0
                    && Self::is_word_char(chars.get(current_col - 1).copied().unwrap_or(' '))
                {
                    current_col -= 1;
                }
                return (current_line, current_col);
            }

            // No word found, go to previous line
            if current_line == 0 {
                return (0, 0);
            }
            current_line -= 1;
            current_col = self.get_line_char_count(current_line);
        }
    }

    fn find_prev_paragraph_boundary(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }

        let mut current = line;

        // Go back to start of current paragraph first
        while current > 0 && !self.is_line_blank(current - 1) {
            current -= 1;
        }

        // If already at start and not line 0, go to previous paragraph
        if current > 0 {
            current -= 1; // Move into blank area
            // Skip consecutive blank lines
            while current > 0 && self.is_line_blank(current) {
                current -= 1;
            }
            // Now on non-blank, find start of this paragraph
            while current > 0 && !self.is_line_blank(current - 1) {
                current -= 1;
            }
        }

        current
    }

    fn find_next_paragraph_boundary(&self, line: usize) -> usize {
        let total = self.raw_text_lines.len();
        if total == 0 {
            return 0;
        }
        let max_line = total - 1;
        let mut current = line;

        // Skip current non-blank lines to find blank
        while current < total && !self.is_line_blank(current) {
            current += 1;
        }

        // Skip blank lines to find start of next paragraph
        while current < total && self.is_line_blank(current) {
            current += 1;
        }

        current.min(max_line)
    }

    fn is_line_blank(&self, line: usize) -> bool {
        self.raw_text_lines
            .get(line)
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    }

    // Big word (WORD) = whitespace-delimited (everything between spaces is one WORD)
    fn find_next_big_word_start(&self, line: usize, col: usize) -> (usize, usize) {
        let mut current_line = line;
        let mut current_col = col;
        let total_lines = self.raw_text_lines.len();

        while current_line < total_lines {
            let chars: Vec<char> = self
                .raw_text_lines
                .get(current_line)
                .map(|s| s.chars().collect())
                .unwrap_or_default();

            // Skip current non-whitespace
            while current_col < chars.len() && !chars[current_col].is_whitespace() {
                current_col += 1;
            }

            // Skip whitespace
            while current_col < chars.len() && chars[current_col].is_whitespace() {
                current_col += 1;
            }

            // Found start of next WORD
            if current_col < chars.len() {
                return (current_line, current_col);
            }

            current_line += 1;
            current_col = 0;
        }

        let last_line = total_lines.saturating_sub(1);
        (
            last_line,
            self.get_line_char_count(last_line).saturating_sub(1),
        )
    }

    pub fn normal_mode_big_word_forward(&mut self) {
        if !self.normal_mode.active {
            return;
        }
        let (new_line, new_col) = self
            .find_next_big_word_start(self.normal_mode.cursor.line, self.normal_mode.cursor.column);
        self.normal_mode.cursor.line = new_line;
        self.normal_mode.cursor.column = new_col;
        self.ensure_cursor_visible();
    }

    fn find_big_word_bounds(&self, line: usize, col: usize) -> Option<(usize, usize)> {
        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();

        if col >= chars.len() {
            return None;
        }

        // If on whitespace, return None (no WORD here)
        if chars[col].is_whitespace() {
            return None;
        }

        // Find start of WORD (first non-whitespace going back)
        let mut start = col;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        // Find end of WORD (first whitespace or end of line)
        let mut end = col;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        Some((start, end))
    }

    pub fn apply_normal_mode_cursor(
        &self,
        line_idx: usize,
        spans: Vec<Span<'static>>,
        palette: &Base16Palette,
    ) -> Vec<Span<'static>> {
        if !self.normal_mode.is_active() || line_idx != self.normal_mode.cursor.line {
            return spans;
        }

        // Don't render cursor on image placeholder lines (but DO render on empty lines)
        if self.is_image_line(line_idx) {
            return spans;
        }

        let cursor_col = self.normal_mode.cursor.column;
        let mut result_spans = Vec::new();
        let mut current_column = 0;

        for span in spans {
            let span_text: Vec<char> = span.content.chars().collect();
            let span_len = span_text.len();
            let span_end = current_column + span_len;

            if cursor_col >= current_column && cursor_col < span_end {
                let relative_pos = cursor_col - current_column;

                if relative_pos > 0 {
                    let before: String = span_text[..relative_pos].iter().collect();
                    result_spans.push(Span::styled(before, span.style));
                }

                let cursor_char = span_text
                    .get(relative_pos)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| " ".to_string());
                let cursor_style = RatatuiStyle::default()
                    .fg(palette.base_00)
                    .bg(palette.base_05);
                result_spans.push(Span::styled(cursor_char, cursor_style));

                if relative_pos + 1 < span_len {
                    let after: String = span_text[relative_pos + 1..].iter().collect();
                    result_spans.push(Span::styled(after, span.style));
                }
            } else {
                result_spans.push(span);
            }

            current_column = span_end;
        }

        if cursor_col >= current_column {
            let cursor_style = RatatuiStyle::default()
                .fg(palette.base_00)
                .bg(palette.base_05);
            result_spans.push(Span::styled(" ", cursor_style));
        }

        result_spans
    }

    // ==================== YANKING ====================

    pub fn start_yank(&mut self) {
        if self.normal_mode.active {
            self.normal_mode.pending_yank = PendingYank::WaitingForMotion;
        }
    }

    pub fn has_pending_yank(&self) -> bool {
        self.normal_mode.active && self.normal_mode.pending_yank != PendingYank::None
    }

    pub fn clear_pending_yank(&mut self) {
        self.normal_mode.pending_yank = PendingYank::None;
    }

    pub fn get_pending_yank(&self) -> PendingYank {
        self.normal_mode.pending_yank
    }

    pub fn set_pending_yank(&mut self, state: PendingYank) {
        self.normal_mode.pending_yank = state;
    }

    pub fn clear_expired_yank_highlight(&mut self) {
        if let Some(ref highlight) = self.normal_mode.yank_highlight {
            if highlight.is_expired() {
                self.normal_mode.yank_highlight = None;
            }
        }
    }

    pub fn has_yank_highlight(&self) -> bool {
        self.normal_mode
            .yank_highlight
            .as_ref()
            .map(|h| !h.is_expired())
            .unwrap_or(false)
    }

    fn set_yank_highlight(
        &mut self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) {
        self.normal_mode.yank_highlight = Some(YankHighlight {
            start_line,
            start_col,
            end_line,
            end_col,
            timestamp: Instant::now(),
        });
    }

    // Yank current line(s) (yy, 4yy)
    pub fn yank_line(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let start_line = self.normal_mode.cursor.line;
        let end_line = (start_line + count - 1).min(self.raw_text_lines.len().saturating_sub(1));

        let text = self.extract_lines(start_line, end_line)?;
        let end_len = self
            .raw_text_lines
            .get(end_line)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        self.set_yank_highlight(start_line, 0, end_line, end_len);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to end of line (y$)
    pub fn yank_to_line_end(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();
        if col >= chars.len() {
            return None;
        }
        let text: String = chars[col..].iter().collect();
        self.set_yank_highlight(line, col, line, chars.len());
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to start of line (y0)
    pub fn yank_to_line_start(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();
        if col == 0 {
            return None;
        }
        let text: String = chars[..col].iter().collect();
        self.set_yank_highlight(line, 0, line, col);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to first non-whitespace (y^)
    pub fn yank_to_first_non_whitespace(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let first_non_ws = self.get_first_non_whitespace_column(line);
        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();

        let (start, end) = if col <= first_non_ws {
            (col, first_non_ws)
        } else {
            (first_non_ws, col)
        };

        let text: String = chars.get(start..end)?.iter().collect();
        self.set_yank_highlight(line, start, line, end);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank word forward (yw, 2yw)
    pub fn yank_word_forward(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let start_line = self.normal_mode.cursor.line;
        let start_col = self.normal_mode.cursor.column;
        let (mut end_line, mut end_col) = (start_line, start_col);
        for _ in 0..count {
            (end_line, end_col) = self.find_next_word_start(end_line, end_col);
        }

        let text = self.extract_text(start_line, start_col, end_line, end_col)?;
        self.set_yank_highlight(start_line, start_col, end_line, end_col);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank big word forward (yW, 2yW)
    pub fn yank_big_word_forward(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let start_line = self.normal_mode.cursor.line;
        let start_col = self.normal_mode.cursor.column;
        let (mut end_line, mut end_col) = (start_line, start_col);
        for _ in 0..count {
            (end_line, end_col) = self.find_next_big_word_start(end_line, end_col);
        }

        let text = self.extract_text(start_line, start_col, end_line, end_col)?;
        self.set_yank_highlight(start_line, start_col, end_line, end_col);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to word end (ye, 2ye)
    pub fn yank_word_end(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let start_line = self.normal_mode.cursor.line;
        let start_col = self.normal_mode.cursor.column;
        let (mut end_line, mut end_col) = (start_line, start_col);
        for _ in 0..count {
            (end_line, end_col) = self.find_word_end(end_line, end_col);
        }

        let text = self.extract_text(start_line, start_col, end_line, end_col + 1)?;
        self.set_yank_highlight(start_line, start_col, end_line, end_col + 1);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank word backward (yb, 2yb)
    pub fn yank_word_backward(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let end_line = self.normal_mode.cursor.line;
        let end_col = self.normal_mode.cursor.column;
        let (mut start_line, mut start_col) = (end_line, end_col);
        for _ in 0..count {
            (start_line, start_col) = self.find_prev_word_start(start_line, start_col);
        }

        let text = self.extract_text(start_line, start_col, end_line, end_col)?;
        self.set_yank_highlight(start_line, start_col, end_line, end_col);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to previous paragraph (y{, 2y{)
    pub fn yank_paragraph_up(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let end_line = self.normal_mode.cursor.line;
        let mut start_line = end_line;
        for _ in 0..count {
            start_line = self.find_prev_paragraph_boundary(start_line);
        }

        let text = self.extract_lines(start_line, end_line)?;
        let end_len = self
            .raw_text_lines
            .get(end_line)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        self.set_yank_highlight(start_line, 0, end_line, end_len);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to next paragraph (y}, 2y})
    pub fn yank_paragraph_down(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let start_line = self.normal_mode.cursor.line;
        let mut end_line = start_line;
        for _ in 0..count {
            end_line = self.find_next_paragraph_boundary(end_line);
        }

        let text = self.extract_lines(start_line, end_line)?;
        let end_len = self
            .raw_text_lines
            .get(end_line)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        self.set_yank_highlight(start_line, 0, end_line, end_len);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to document top (ygg)
    pub fn yank_to_document_top(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let end_line = self.normal_mode.cursor.line;
        let text = self.extract_lines(0, end_line)?;
        let end_len = self
            .raw_text_lines
            .get(end_line)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        self.set_yank_highlight(0, 0, end_line, end_len);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank to document bottom (yG)
    pub fn yank_to_document_bottom(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let start_line = self.normal_mode.cursor.line;
        let end_line = self.raw_text_lines.len().saturating_sub(1);
        let text = self.extract_lines(start_line, end_line)?;
        let end_len = self
            .raw_text_lines
            .get(end_line)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        self.set_yank_highlight(start_line, 0, end_line, end_len);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Yank with find char (yf, yF, yt, yT) - kept for compatibility
    pub fn yank_find_char(&mut self, motion: PendingCharMotion, ch: char) -> Option<String> {
        self.yank_find_char_with_count(motion, ch, 1)
    }

    // Yank with find char and count (2yfa = yank to 2nd 'a')
    pub fn yank_find_char_with_count(
        &mut self,
        motion: PendingCharMotion,
        ch: char,
        count: usize,
    ) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let start_col = self.normal_mode.cursor.column;

        let end_col = match motion {
            PendingCharMotion::FindForward => self.find_nth_char_forward(ch, count)?,
            PendingCharMotion::FindBackward => self.find_nth_char_backward(ch, count)?,
            PendingCharMotion::TillForward => {
                self.find_nth_char_forward(ch, count)?.saturating_sub(1)
            }
            PendingCharMotion::TillBackward => self.find_nth_char_backward(ch, count)? + 1,
            PendingCharMotion::None => return None,
        };

        let (from, to) = if start_col <= end_col {
            (start_col, end_col + 1)
        } else {
            (end_col, start_col + 1)
        };

        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();
        let text: String = chars.get(from..to)?.iter().collect();

        self.set_yank_highlight(line, from, line, to);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    fn find_nth_char_forward(&self, ch: char, count: usize) -> Option<usize> {
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let chars: Vec<char> = self.raw_text_lines.get(line)?.chars().collect();

        let mut found = 0;
        for (i, &c) in chars.iter().enumerate().skip(col + 1) {
            if c == ch {
                found += 1;
                if found == count {
                    return Some(i);
                }
            }
        }
        None
    }

    fn find_nth_char_backward(&self, ch: char, count: usize) -> Option<usize> {
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let chars: Vec<char> = self.raw_text_lines.get(line)?.chars().collect();

        let mut found = 0;
        for i in (0..col).rev() {
            if chars.get(i).copied() == Some(ch) {
                found += 1;
                if found == count {
                    return Some(i);
                }
            }
        }
        None
    }

    // ==================== TEXT OBJECTS ====================

    // Inner word (iw)
    pub fn yank_inner_word(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let (start, end) = self.find_word_bounds(line, col)?;

        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();
        let text: String = chars.get(start..end)?.iter().collect();

        self.set_yank_highlight(line, start, line, end);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // A word (aw) - includes trailing whitespace
    pub fn yank_a_word(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let (start, mut end) = self.find_word_bounds(line, col)?;

        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();

        // Include trailing whitespace
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        let text: String = chars.get(start..end)?.iter().collect();
        self.set_yank_highlight(line, start, line, end);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Inner big word (iW) - whitespace-delimited
    pub fn yank_inner_big_word(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let (start, end) = self.find_big_word_bounds(line, col)?;

        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();
        let text: String = chars.get(start..end)?.iter().collect();

        self.set_yank_highlight(line, start, line, end);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // A big word (aW) - includes trailing whitespace
    pub fn yank_a_big_word(&mut self) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let (start, mut end) = self.find_big_word_bounds(line, col)?;

        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();

        // Include trailing whitespace
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        let text: String = chars.get(start..end)?.iter().collect();
        self.set_yank_highlight(line, start, line, end);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    fn find_word_bounds(&self, line: usize, col: usize) -> Option<(usize, usize)> {
        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();

        if col >= chars.len() {
            return None;
        }

        let at_word = Self::is_word_char(chars[col]);

        if at_word {
            let mut start = col;
            while start > 0 && Self::is_word_char(chars[start - 1]) {
                start -= 1;
            }
            let mut end = col;
            while end < chars.len() && Self::is_word_char(chars[end]) {
                end += 1;
            }
            Some((start, end))
        } else {
            // On whitespace/punctuation - get that sequence
            let mut start = col;
            while start > 0
                && !Self::is_word_char(chars[start - 1])
                && !chars[start - 1].is_whitespace()
            {
                start -= 1;
            }
            let mut end = col;
            while end < chars.len()
                && !Self::is_word_char(chars[end])
                && !chars[end].is_whitespace()
            {
                end += 1;
            }
            if start == end {
                // Just whitespace
                while start > 0 && chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                while end < chars.len() && chars[end].is_whitespace() {
                    end += 1;
                }
            }
            Some((start, end))
        }
    }

    // Inner paragraph (ip, 2ip)
    pub fn yank_inner_paragraph(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let (start, mut end) = self.find_paragraph_bounds(line)?;

        // Extend to additional paragraphs
        for _ in 1..count {
            let next = self.find_next_paragraph_boundary(end);
            if next > end {
                if let Some((_, new_end)) = self.find_paragraph_bounds(next) {
                    end = new_end;
                }
            }
        }

        let text = self.extract_lines(start, end)?;
        let end_len = self
            .raw_text_lines
            .get(end)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        self.set_yank_highlight(start, 0, end, end_len);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // A paragraph (ap, 2ap) - includes trailing blank lines
    pub fn yank_a_paragraph(&mut self, count: usize) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let (start, mut end) = self.find_paragraph_bounds(line)?;

        // Extend to additional paragraphs
        for _ in 1..count {
            // Skip trailing blanks first
            let total = self.raw_text_lines.len();
            while end + 1 < total && self.is_line_blank(end + 1) {
                end += 1;
            }
            // Find next paragraph
            let next = end + 1;
            if next < total {
                if let Some((_, new_end)) = self.find_paragraph_bounds(next) {
                    end = new_end;
                }
            }
        }

        // Include trailing blank lines for the final paragraph
        let total = self.raw_text_lines.len();
        while end + 1 < total && self.is_line_blank(end + 1) {
            end += 1;
        }

        let text = self.extract_lines(start, end)?;
        let end_len = self
            .raw_text_lines
            .get(end)
            .map(|s| s.chars().count())
            .unwrap_or(0);
        self.set_yank_highlight(start, 0, end, end_len);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    fn find_paragraph_bounds(&self, line: usize) -> Option<(usize, usize)> {
        let total = self.raw_text_lines.len();
        if total == 0 {
            return None;
        }

        // Find start of paragraph
        let mut start = line;
        while start > 0 && !self.is_line_blank(start - 1) {
            start -= 1;
        }

        // Find end of paragraph
        let mut end = line;
        while end + 1 < total && !self.is_line_blank(end + 1) {
            end += 1;
        }

        Some((start, end))
    }

    // Inner quotes (i", i', i`)
    pub fn yank_inner_quotes(&mut self, quote: char) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let (start, end) = self.find_quote_bounds(line, col, quote, false)?;

        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();
        let text: String = chars.get(start..end)?.iter().collect();

        self.set_yank_highlight(line, start, line, end);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Around quotes (a", a', a`)
    pub fn yank_around_quotes(&mut self, quote: char) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let line = self.normal_mode.cursor.line;
        let col = self.normal_mode.cursor.column;
        let (start, end) = self.find_quote_bounds(line, col, quote, true)?;

        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();
        let text: String = chars.get(start..end)?.iter().collect();

        self.set_yank_highlight(line, start, line, end);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    fn find_quote_bounds(
        &self,
        line: usize,
        col: usize,
        quote: char,
        include_quotes: bool,
    ) -> Option<(usize, usize)> {
        let line_text = self.raw_text_lines.get(line)?;
        let chars: Vec<char> = line_text.chars().collect();

        // Find opening quote (search backward from cursor, or forward if on quote)
        let mut open_pos = None;
        let mut close_pos = None;

        // Check if cursor is on a quote
        if chars.get(col) == Some(&quote) {
            // Could be opening or closing - need to count
            let quotes_before: usize = chars[..col].iter().filter(|&&c| c == quote).count();
            if quotes_before % 2 == 0 {
                // This is an opening quote
                open_pos = Some(col);
            } else {
                // This is a closing quote
                close_pos = Some(col);
            }
        }

        // Search backward for opening quote if not found
        if open_pos.is_none() {
            for i in (0..col).rev() {
                if chars[i] == quote {
                    let quotes_before: usize = chars[..i].iter().filter(|&&c| c == quote).count();
                    if quotes_before % 2 == 0 {
                        open_pos = Some(i);
                        break;
                    }
                }
            }
        }

        let open = open_pos?;

        // Search forward for closing quote if not found
        if close_pos.is_none() {
            for (i, ch) in chars.iter().enumerate().skip(open + 1) {
                if *ch == quote {
                    close_pos = Some(i);
                    break;
                }
            }
        }

        let close = close_pos?;

        if include_quotes {
            Some((open, close + 1))
        } else {
            Some((open + 1, close))
        }
    }

    // Inner brackets (i(, i), i[, i], i{, i}, i<, i>)
    pub fn yank_inner_brackets(&mut self, open: char, close: char) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let (start_line, start_col, end_line, end_col) =
            self.find_bracket_bounds(open, close, false)?;

        let text = self.extract_text(start_line, start_col, end_line, end_col)?;
        self.set_yank_highlight(start_line, start_col, end_line, end_col);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    // Around brackets (a(, a), a[, a], a{, a}, a<, a>)
    pub fn yank_around_brackets(&mut self, open: char, close: char) -> Option<String> {
        if !self.normal_mode.active {
            return None;
        }
        let (start_line, start_col, end_line, end_col) =
            self.find_bracket_bounds(open, close, true)?;

        let text = self.extract_text(start_line, start_col, end_line, end_col)?;
        self.set_yank_highlight(start_line, start_col, end_line, end_col);
        self.normal_mode.pending_yank = PendingYank::None;
        Some(text)
    }

    fn find_bracket_bounds(
        &self,
        open: char,
        close: char,
        include_brackets: bool,
    ) -> Option<(usize, usize, usize, usize)> {
        let cursor_line = self.normal_mode.cursor.line;
        let cursor_col = self.normal_mode.cursor.column;

        // Search backward for opening bracket
        let mut depth = 0;
        let mut open_pos: Option<(usize, usize)> = None;

        // Start from cursor position
        for line in (0..=cursor_line).rev() {
            let line_text = self.raw_text_lines.get(line)?;
            let chars: Vec<char> = line_text.chars().collect();

            let start_col = if line == cursor_line {
                cursor_col
            } else {
                chars.len().saturating_sub(1)
            };

            for col in (0..=start_col).rev() {
                if let Some(&ch) = chars.get(col) {
                    if ch == close {
                        depth += 1;
                    } else if ch == open {
                        if depth == 0 {
                            open_pos = Some((line, col));
                            break;
                        }
                        depth -= 1;
                    }
                }
            }
            if open_pos.is_some() {
                break;
            }
        }

        let (open_line, open_col) = open_pos?;

        // Search forward for closing bracket
        depth = 0;
        let mut close_pos: Option<(usize, usize)> = None;

        for line in open_line..self.raw_text_lines.len() {
            let line_text = self.raw_text_lines.get(line)?;
            let chars: Vec<char> = line_text.chars().collect();

            let start_col = if line == open_line { open_col } else { 0 };

            for col in start_col..chars.len() {
                if let Some(&ch) = chars.get(col) {
                    if ch == open {
                        depth += 1;
                    } else if ch == close {
                        if depth == 1 {
                            close_pos = Some((line, col));
                            break;
                        }
                        depth -= 1;
                    }
                }
            }
            if close_pos.is_some() {
                break;
            }
        }

        let (close_line, close_col) = close_pos?;

        if include_brackets {
            Some((open_line, open_col, close_line, close_col + 1))
        } else {
            // Inner: skip the brackets
            Some((open_line, open_col + 1, close_line, close_col))
        }
    }

    // ==================== HELPERS ====================

    fn extract_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Option<String> {
        if start_line == end_line {
            let line_text = self.raw_text_lines.get(start_line)?;
            let chars: Vec<char> = line_text.chars().collect();
            let text: String = chars.get(start_col..end_col)?.iter().collect();
            return Some(text);
        }

        let mut result = String::new();

        for line in start_line..=end_line {
            let line_text = self.raw_text_lines.get(line)?;
            let chars: Vec<char> = line_text.chars().collect();

            if line == start_line {
                let text: String = chars.get(start_col..)?.iter().collect();
                result.push_str(&text);
            } else if line == end_line {
                result.push('\n');
                let text: String = chars.get(..end_col)?.iter().collect();
                result.push_str(&text);
            } else {
                result.push('\n');
                result.push_str(line_text);
            }
        }

        Some(result)
    }

    fn extract_lines(&self, start_line: usize, end_line: usize) -> Option<String> {
        let mut result = String::new();

        for line in start_line..=end_line {
            if line > start_line {
                result.push('\n');
            }
            if let Some(text) = self.raw_text_lines.get(line) {
                result.push_str(text);
            }
        }

        Some(result)
    }

    fn apply_highlight_with_predicate<F>(
        &self,
        line_idx: usize,
        spans: Vec<Span<'static>>,
        palette: &Base16Palette,
        is_highlighted: F,
    ) -> Vec<Span<'static>>
    where
        F: Fn(usize, usize) -> bool,
    {
        let mut result_spans = Vec::new();
        let mut current_column = 0;

        let highlight_style = RatatuiStyle::default().bg(palette.base_02);

        for span in spans {
            let span_text: Vec<char> = span.content.chars().collect();
            let span_len = span_text.len();
            let span_end = current_column + span_len;

            let mut segments: Vec<(usize, usize, bool)> = vec![];
            let mut pos = 0;

            while pos < span_len {
                let global_col = current_column + pos;
                let highlighted = is_highlighted(line_idx, global_col);

                let mut end_pos = pos + 1;
                while end_pos < span_len {
                    let next_global = current_column + end_pos;
                    if is_highlighted(line_idx, next_global) != highlighted {
                        break;
                    }
                    end_pos += 1;
                }

                segments.push((pos, end_pos, highlighted));
                pos = end_pos;
            }

            for (start, end, highlighted) in segments {
                let text: String = span_text[start..end].iter().collect();
                let style = if highlighted {
                    highlight_style
                } else {
                    span.style
                };
                result_spans.push(Span::styled(text, style));
            }

            current_column = span_end;
        }

        result_spans
    }

    // Apply yank highlight to rendered spans
    pub fn apply_yank_highlight(
        &self,
        line_idx: usize,
        spans: Vec<Span<'static>>,
        palette: &Base16Palette,
    ) -> Vec<Span<'static>> {
        let highlight = match &self.normal_mode.yank_highlight {
            Some(h) if !h.is_expired() => h,
            _ => return spans,
        };

        if line_idx < highlight.start_line || line_idx > highlight.end_line {
            return spans;
        }

        self.apply_highlight_with_predicate(line_idx, spans, palette, |line, col| {
            highlight.contains(line, col)
        })
    }

    // ==================== VISUAL MODE ====================

    pub fn enter_visual_mode(&mut self, mode: VisualMode) {
        if !self.normal_mode.active {
            return;
        }
        self.normal_mode.visual_mode = mode;
        self.normal_mode.visual_anchor = Some(self.normal_mode.cursor.clone());
    }

    pub fn exit_visual_mode(&mut self) {
        self.normal_mode.visual_mode = VisualMode::None;
        self.normal_mode.visual_anchor = None;
    }

    pub fn is_visual_mode_active(&self) -> bool {
        self.normal_mode.visual_mode != VisualMode::None
    }

    pub fn get_visual_mode(&self) -> VisualMode {
        self.normal_mode.visual_mode
    }

    pub fn get_visual_selection_range(&self) -> Option<(usize, usize, usize, usize)> {
        let anchor = self.normal_mode.visual_anchor.as_ref()?;
        let cursor = &self.normal_mode.cursor;

        match self.normal_mode.visual_mode {
            VisualMode::None => None,
            VisualMode::CharacterWise => {
                let (start_line, start_col, end_line, end_col) = if anchor.line < cursor.line
                    || (anchor.line == cursor.line && anchor.column <= cursor.column)
                {
                    (anchor.line, anchor.column, cursor.line, cursor.column + 1)
                } else {
                    (cursor.line, cursor.column, anchor.line, anchor.column + 1)
                };
                Some((start_line, start_col, end_line, end_col))
            }
            VisualMode::LineWise => {
                let (start_line, end_line) = if anchor.line <= cursor.line {
                    (anchor.line, cursor.line)
                } else {
                    (cursor.line, anchor.line)
                };
                let end_len = self
                    .raw_text_lines
                    .get(end_line)
                    .map(|s| s.chars().count())
                    .unwrap_or(0);
                Some((start_line, 0, end_line, end_len))
            }
        }
    }

    pub fn yank_visual_selection(&mut self) -> Option<String> {
        let (start_line, start_col, end_line, end_col) = self.get_visual_selection_range()?;

        let text = match self.normal_mode.visual_mode {
            VisualMode::LineWise => self.extract_lines(start_line, end_line)?,
            VisualMode::CharacterWise => {
                self.extract_text(start_line, start_col, end_line, end_col)?
            }
            VisualMode::None => return None,
        };

        self.set_yank_highlight(start_line, start_col, end_line, end_col);

        // Move cursor to start of selection (vim behavior)
        self.normal_mode.cursor.line = start_line;
        self.normal_mode.cursor.column = start_col;

        self.exit_visual_mode();
        Some(text)
    }

    pub fn is_in_visual_selection(&self, line: usize, col: usize) -> bool {
        let Some((start_line, start_col, end_line, end_col)) = self.get_visual_selection_range()
        else {
            return false;
        };

        if line < start_line || line > end_line {
            return false;
        }

        match self.normal_mode.visual_mode {
            VisualMode::LineWise => true,
            VisualMode::CharacterWise => {
                if start_line == end_line {
                    col >= start_col && col < end_col
                } else if line == start_line {
                    col >= start_col
                } else if line == end_line {
                    col < end_col
                } else {
                    true
                }
            }
            VisualMode::None => false,
        }
    }

    pub fn apply_visual_highlight(
        &self,
        line_idx: usize,
        spans: Vec<Span<'static>>,
        palette: &Base16Palette,
    ) -> Vec<Span<'static>> {
        if !self.is_visual_mode_active() {
            return spans;
        }

        let Some((start_line, _, end_line, _)) = self.get_visual_selection_range() else {
            return spans;
        };

        if line_idx < start_line || line_idx > end_line {
            return spans;
        }

        self.apply_highlight_with_predicate(line_idx, spans, palette, |line, col| {
            self.is_in_visual_selection(line, col)
        })
    }
}
