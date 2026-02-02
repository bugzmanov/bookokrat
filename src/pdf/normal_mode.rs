//! Normal mode state for vim-like cursor navigation in PDFs

use super::types::LineBounds;

/// Normal mode state for cursor navigation
#[derive(Debug, Default)]
pub struct NormalModeState {
    /// Whether normal mode is active
    pub active: bool,
    /// Current cursor position
    pub cursor: CursorPosition,
    /// Pending character motion (f/F/t/T)
    pub pending_motion: PendingMotion,
    /// Pending g key for gg motion
    pub pending_g: bool,
    /// Last find motion for repeat
    pub last_find: Option<(PendingMotion, char)>,
    /// Visual mode state
    pub visual_mode: VisualMode,
    /// Anchor position for visual selection
    pub visual_anchor: Option<CursorPosition>,
    /// Last position before deactivation
    pub last_position: Option<CursorPosition>,
}

/// Visual mode type
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum VisualMode {
    #[default]
    None,
    /// Character-wise visual mode (v)
    CharacterWise,
    /// Line-wise visual mode (V)
    LineWise,
}

/// Cursor position in a PDF page
#[derive(Debug, Default, Clone, Copy)]
pub struct CursorPosition {
    /// Page index
    pub page: usize,
    /// Line index within the page
    pub line_idx: usize,
    /// Character index within the line
    pub char_idx: usize,
}

/// Pending character motion type
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PendingMotion {
    #[default]
    None,
    /// f - find forward
    FindForward,
    /// F - find backward
    FindBackward,
    /// t - till forward
    TillForward,
    /// T - till backward
    TillBackward,
}

/// Result of a cursor move operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveResult {
    /// Cursor moved successfully
    Moved,
    /// Cursor wants to move to previous page
    WantsPrevPage,
    /// Cursor wants to move to next page
    WantsNextPage,
}

/// Rectangle for cursor display
#[derive(Clone, Debug)]
pub struct CursorRect {
    pub page: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Rectangle for visual selection display
#[derive(Clone, Debug)]
pub struct VisualRect {
    pub page: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl NormalModeState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle normal mode
    pub fn toggle(&mut self) {
        self.active = !self.active;
        if !self.active {
            self.pending_motion = PendingMotion::None;
            self.pending_g = false;
            self.visual_mode = VisualMode::None;
            self.visual_anchor = None;
        }
    }

    /// Activate normal mode at a specific position
    pub fn activate_at(&mut self, page: usize, line_idx: usize, char_idx: usize) {
        self.active = true;
        self.cursor = CursorPosition {
            page,
            line_idx,
            char_idx,
        };
        self.pending_motion = PendingMotion::None;
        self.pending_g = false;
        self.visual_mode = VisualMode::None;
        self.visual_anchor = None;
    }

    /// Deactivate normal mode
    pub fn deactivate(&mut self) {
        self.last_position = Some(self.cursor);
        self.active = false;
        self.pending_motion = PendingMotion::None;
        self.pending_g = false;
        self.visual_mode = VisualMode::None;
        self.visual_anchor = None;
    }

    /// Get the last cursor position
    #[must_use]
    pub fn get_last_position(&self) -> Option<CursorPosition> {
        self.last_position
    }

    /// Check if there's a pending character motion
    #[must_use]
    pub fn has_pending_char_motion(&self) -> bool {
        self.pending_motion != PendingMotion::None
    }

    /// Check if visual mode is active
    #[must_use]
    pub fn is_visual_active(&self) -> bool {
        self.visual_mode != VisualMode::None
    }

    /// Toggle character-wise visual mode
    pub fn toggle_visual_char(&mut self) {
        if self.visual_mode == VisualMode::CharacterWise {
            self.visual_mode = VisualMode::None;
            self.visual_anchor = None;
        } else {
            self.visual_mode = VisualMode::CharacterWise;
            self.visual_anchor = Some(self.cursor);
        }
    }

    /// Toggle line-wise visual mode
    pub fn toggle_visual_line(&mut self) {
        if self.visual_mode == VisualMode::LineWise {
            self.visual_mode = VisualMode::None;
            self.visual_anchor = None;
        } else {
            self.visual_mode = VisualMode::LineWise;
            self.visual_anchor = Some(self.cursor);
        }
    }

    /// Exit visual mode
    pub fn exit_visual(&mut self) {
        self.visual_mode = VisualMode::None;
        self.visual_anchor = None;
    }

    /// Get the visual selection range
    #[must_use]
    pub fn get_visual_range(&self) -> Option<(CursorPosition, CursorPosition)> {
        let anchor = self.visual_anchor?;
        if self.visual_mode == VisualMode::None {
            return None;
        }

        let (start, end) = if (anchor.page, anchor.line_idx, anchor.char_idx)
            <= (self.cursor.page, self.cursor.line_idx, self.cursor.char_idx)
        {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        };

        Some((start, end))
    }

    /// Move cursor left
    pub fn move_left(&mut self, line_bounds: &[LineBounds]) {
        if !self.active {
            return;
        }
        if self.cursor.char_idx > 0 {
            self.cursor.char_idx -= 1;
        } else if self.cursor.line_idx > 0 {
            self.cursor.line_idx -= 1;
            if let Some(line) = line_bounds.get(self.cursor.line_idx) {
                self.cursor.char_idx = line.chars.len().saturating_sub(1);
            }
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self, line_bounds: &[LineBounds]) {
        if !self.active {
            return;
        }
        if let Some(line) = line_bounds.get(self.cursor.line_idx) {
            if self.cursor.char_idx < line.chars.len().saturating_sub(1) {
                self.cursor.char_idx += 1;
            } else if self.cursor.line_idx < line_bounds.len().saturating_sub(1) {
                self.cursor.line_idx += 1;
                self.cursor.char_idx = 0;
            }
        }
    }

    /// Move cursor up
    pub fn move_up(&mut self, line_bounds: &[LineBounds]) -> MoveResult {
        if !self.active {
            return MoveResult::Moved;
        }
        if self.cursor.line_idx > 0 {
            self.cursor.line_idx -= 1;
            if let Some(line) = line_bounds.get(self.cursor.line_idx) {
                self.cursor.char_idx = self.cursor.char_idx.min(line.chars.len().saturating_sub(1));
            }
            MoveResult::Moved
        } else {
            MoveResult::WantsPrevPage
        }
    }

    /// Move cursor down
    pub fn move_down(&mut self, line_bounds: &[LineBounds]) -> MoveResult {
        if !self.active {
            return MoveResult::Moved;
        }
        if self.cursor.line_idx < line_bounds.len().saturating_sub(1) {
            self.cursor.line_idx += 1;
            if let Some(line) = line_bounds.get(self.cursor.line_idx) {
                self.cursor.char_idx = self.cursor.char_idx.min(line.chars.len().saturating_sub(1));
            }
            MoveResult::Moved
        } else {
            MoveResult::WantsNextPage
        }
    }

    /// Move cursor to page start
    pub fn move_to_page_start(&mut self, page: usize, line_bounds: &[LineBounds]) {
        self.cursor.page = page;
        self.cursor.line_idx = 0;
        if let Some(line) = line_bounds.first() {
            self.cursor.char_idx = self.cursor.char_idx.min(line.chars.len().saturating_sub(1));
        }
    }

    /// Move cursor to page end
    pub fn move_to_page_end(&mut self, page: usize, line_bounds: &[LineBounds]) {
        self.cursor.page = page;
        self.cursor.line_idx = line_bounds.len().saturating_sub(1);
        if let Some(line) = line_bounds.last() {
            self.cursor.char_idx = self.cursor.char_idx.min(line.chars.len().saturating_sub(1));
        }
    }

    /// Move to line start
    pub fn move_line_start(&mut self) {
        if !self.active {
            return;
        }
        self.cursor.char_idx = 0;
    }

    /// Move to line end
    pub fn move_line_end(&mut self, line_bounds: &[LineBounds]) {
        if !self.active {
            return;
        }
        if let Some(line) = line_bounds.get(self.cursor.line_idx) {
            self.cursor.char_idx = line.chars.len().saturating_sub(1);
        }
    }

    /// Move to first non-whitespace character
    pub fn move_first_non_whitespace(&mut self, line_bounds: &[LineBounds]) {
        if !self.active {
            return;
        }
        if let Some(line) = line_bounds.get(self.cursor.line_idx) {
            self.cursor.char_idx = line
                .chars
                .iter()
                .position(|c| !c.c.is_whitespace())
                .unwrap_or(0);
        }
    }

    /// Move word forward
    pub fn move_word_forward(&mut self, line_bounds: &[LineBounds]) {
        if !self.active {
            return;
        }
        let Some(line) = line_bounds.get(self.cursor.line_idx) else {
            return;
        };

        let mut idx = self.cursor.char_idx;
        while idx < line.chars.len() && !line.chars[idx].c.is_whitespace() {
            idx += 1;
        }
        while idx < line.chars.len() && line.chars[idx].c.is_whitespace() {
            idx += 1;
        }

        if idx < line.chars.len() {
            self.cursor.char_idx = idx;
        } else if self.cursor.line_idx < line_bounds.len().saturating_sub(1) {
            self.cursor.line_idx += 1;
            self.move_first_non_whitespace(line_bounds);
        }
    }

    /// Move word backward
    pub fn move_word_backward(&mut self, line_bounds: &[LineBounds]) {
        if !self.active {
            return;
        }
        let Some(line) = line_bounds.get(self.cursor.line_idx) else {
            return;
        };

        if self.cursor.char_idx == 0 {
            if self.cursor.line_idx > 0 {
                self.cursor.line_idx -= 1;
                self.move_line_end(line_bounds);
            }
            return;
        }

        let mut idx = self.cursor.char_idx.saturating_sub(1);
        while idx > 0 && line.chars[idx].c.is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !line.chars[idx - 1].c.is_whitespace() {
            idx -= 1;
        }

        self.cursor.char_idx = idx;
    }

    /// Move to page top
    pub fn move_page_top(&mut self) {
        if !self.active {
            return;
        }
        self.cursor.line_idx = 0;
        self.cursor.char_idx = 0;
    }

    /// Move to page bottom
    pub fn move_page_bottom(&mut self, line_bounds: &[LineBounds]) {
        if !self.active {
            return;
        }
        self.cursor.line_idx = line_bounds.len().saturating_sub(1);
        self.cursor.char_idx = 0;
    }

    /// Find character forward (f motion)
    pub fn find_char_forward(&mut self, ch: char, line_bounds: &[LineBounds]) -> bool {
        if !self.active {
            return false;
        }
        let Some(line) = line_bounds.get(self.cursor.line_idx) else {
            return false;
        };

        for (i, c) in line.chars.iter().enumerate().skip(self.cursor.char_idx + 1) {
            if c.c == ch {
                self.cursor.char_idx = i;
                self.last_find = Some((PendingMotion::FindForward, ch));
                return true;
            }
        }
        false
    }

    /// Find character backward (F motion)
    pub fn find_char_backward(&mut self, ch: char, line_bounds: &[LineBounds]) -> bool {
        if !self.active {
            return false;
        }
        let Some(line) = line_bounds.get(self.cursor.line_idx) else {
            return false;
        };

        for i in (0..self.cursor.char_idx).rev() {
            if line.chars[i].c == ch {
                self.cursor.char_idx = i;
                self.last_find = Some((PendingMotion::FindBackward, ch));
                return true;
            }
        }
        false
    }

    /// Till character forward (t motion)
    pub fn till_char_forward(&mut self, ch: char, line_bounds: &[LineBounds]) -> bool {
        if !self.active {
            return false;
        }
        let Some(line) = line_bounds.get(self.cursor.line_idx) else {
            return false;
        };

        for (i, c) in line.chars.iter().enumerate().skip(self.cursor.char_idx + 1) {
            if c.c == ch && i > 0 {
                self.cursor.char_idx = i - 1;
                self.last_find = Some((PendingMotion::TillForward, ch));
                return true;
            }
        }
        false
    }

    /// Till character backward (T motion)
    pub fn till_char_backward(&mut self, ch: char, line_bounds: &[LineBounds]) -> bool {
        if !self.active {
            return false;
        }
        let Some(line) = line_bounds.get(self.cursor.line_idx) else {
            return false;
        };

        for i in (0..self.cursor.char_idx).rev() {
            if line.chars[i].c == ch {
                self.cursor.char_idx = (i + 1).min(line.chars.len().saturating_sub(1));
                self.last_find = Some((PendingMotion::TillBackward, ch));
                return true;
            }
        }
        false
    }

    /// Repeat last find motion
    pub fn repeat_find(&mut self, line_bounds: &[LineBounds]) -> bool {
        if let Some((motion, ch)) = self.last_find {
            match motion {
                PendingMotion::FindForward => self.find_char_forward(ch, line_bounds),
                PendingMotion::FindBackward => self.find_char_backward(ch, line_bounds),
                PendingMotion::TillForward => self.till_char_forward(ch, line_bounds),
                PendingMotion::TillBackward => self.till_char_backward(ch, line_bounds),
                PendingMotion::None => false,
            }
        } else {
            false
        }
    }

    /// Get cursor rectangle for display
    #[must_use]
    pub fn get_cursor_rect(&self, line_bounds: &[LineBounds]) -> Option<CursorRect> {
        if !self.active {
            return None;
        }

        let line = line_bounds.get(self.cursor.line_idx)?;
        let char_info = line.chars.get(self.cursor.char_idx)?;

        let char_width = if self.cursor.char_idx + 1 < line.chars.len() {
            (line.chars[self.cursor.char_idx + 1].x - char_info.x).max(1.0)
        } else {
            (line.x1 - char_info.x).max(8.0)
        };

        Some(CursorRect {
            page: self.cursor.page,
            x: char_info.x as u32,
            y: line.y0 as u32,
            width: char_width as u32,
            height: (line.y1 - line.y0).max(1.0) as u32,
        })
    }

    /// Get visual rectangles for display
    #[must_use]
    pub fn get_visual_rects(&self, line_bounds: &[LineBounds]) -> Vec<VisualRect> {
        self.get_visual_rects_multi(&[line_bounds.to_vec()])
    }

    /// Get visual rectangles for multiple pages
    #[must_use]
    pub fn get_visual_rects_multi(&self, all_line_bounds: &[Vec<LineBounds>]) -> Vec<VisualRect> {
        let Some((start, end)) = self.get_visual_range() else {
            return vec![];
        };

        visual_rects_for_range(start, end, self.visual_mode, all_line_bounds)
    }
}

/// Generate visual rectangles for a selection range
pub fn visual_rects_for_range(
    start: CursorPosition,
    end: CursorPosition,
    mode: VisualMode,
    all_line_bounds: &[Vec<LineBounds>],
) -> Vec<VisualRect> {
    let (start, end) =
        if (start.page, start.line_idx, start.char_idx) <= (end.page, end.line_idx, end.char_idx) {
            (start, end)
        } else {
            (end, start)
        };

    let mut rects = Vec::new();

    match mode {
        VisualMode::None => {}
        VisualMode::CharacterWise => {
            for page in start.page..=end.page {
                let Some(line_bounds) = all_line_bounds.get(page) else {
                    continue;
                };

                let first_line = if page == start.page {
                    start.line_idx
                } else {
                    0
                };
                let last_line = if page == end.page {
                    end.line_idx
                } else {
                    line_bounds.len().saturating_sub(1)
                };

                for line_idx in first_line..=last_line {
                    let Some(line) = line_bounds.get(line_idx) else {
                        continue;
                    };
                    if line.chars.is_empty() {
                        continue;
                    }

                    let start_char = if page == start.page && line_idx == start.line_idx {
                        start.char_idx
                    } else {
                        0
                    };
                    let end_char = if page == end.page && line_idx == end.line_idx {
                        end.char_idx
                    } else {
                        line.chars.len().saturating_sub(1)
                    };

                    let Some(start_info) = line.chars.get(start_char) else {
                        continue;
                    };

                    let end_x = if end_char + 1 < line.chars.len() {
                        line.chars[end_char + 1].x
                    } else {
                        line.x1
                    };

                    rects.push(VisualRect {
                        page,
                        x: start_info.x as u32,
                        y: line.y0 as u32,
                        width: (end_x - start_info.x).max(1.0) as u32,
                        height: (line.y1 - line.y0).max(1.0) as u32,
                    });
                }
            }
        }
        VisualMode::LineWise => {
            for page in start.page..=end.page {
                let Some(line_bounds) = all_line_bounds.get(page) else {
                    continue;
                };

                let first_line = if page == start.page {
                    start.line_idx
                } else {
                    0
                };
                let last_line = if page == end.page {
                    end.line_idx
                } else {
                    line_bounds.len().saturating_sub(1)
                };

                for line_idx in first_line..=last_line {
                    let Some(line) = line_bounds.get(line_idx) else {
                        continue;
                    };

                    rects.push(VisualRect {
                        page,
                        x: line.x0 as u32,
                        y: line.y0 as u32,
                        width: (line.x1 - line.x0).max(1.0) as u32,
                        height: (line.y1 - line.y0).max(1.0) as u32,
                    });
                }
            }
        }
    }

    rects
}
