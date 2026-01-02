use super::{LineType, MarkdownTextReader};
use crate::theme::Base16Palette;
use ratatui::{style::Style as RatatuiStyle, text::Span};

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

#[derive(Debug)]
pub struct NormalModeState {
    pub active: bool,
    pub cursor: CursorPosition,
    pub scrolloff: usize,
    pub pending_motion: PendingCharMotion,
    pub last_find: Option<(PendingCharMotion, char)>,
}

impl Default for NormalModeState {
    fn default() -> Self {
        Self {
            active: false,
            cursor: CursorPosition::default(),
            scrolloff: 3,
            pending_motion: PendingCharMotion::None,
            last_find: None,
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
    }

    pub fn deactivate(&mut self) {
        self.active = false;
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

            if cursor_in_viewport && !self.should_skip_line(previous_line) {
                // Restore previous position, just clamp column
                self.normal_mode.active = true;
                self.clamp_column_to_line_length();
            } else {
                // Position outside viewport or on invalid line - place at top-left
                let mut initial_line = self.scroll_offset;
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

    fn is_image_line(&self, line: usize) -> bool {
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

    fn clamp_column_to_line_length(&mut self) {
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

    fn ensure_cursor_visible(&mut self) {
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
}
