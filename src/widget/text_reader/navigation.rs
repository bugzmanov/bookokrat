use super::types::*;
use crate::main_app::VimNavMotions;
use crate::search::SearchMode;
use std::time::Instant;

impl crate::markdown_text_reader::MarkdownTextReader {
    pub(super) fn dual_line_to_vrow_for(
        line: usize,
        page_height: usize,
        stride: usize,
    ) -> Option<usize> {
        if page_height == 0 || stride == 0 {
            return None;
        }
        let page = line / page_height;
        let spread = page / 2;
        let offset = line % page_height;
        Some(spread * stride + offset)
    }

    fn dual_line_to_vrow(&self, line: usize) -> Option<usize> {
        Self::dual_line_to_vrow_for(line, self.dual.page_height, self.dual.stride)
    }

    pub(super) fn dual_scroll_to_line(&mut self, line: usize) {
        if let Some(vrow) = self.dual_line_to_vrow(line) {
            self.dual.vtop = vrow.min(self.dual.max_vtop);
            self.sync_dual_scroll();
        } else {
            self.scroll_offset = line.min(self.rendered_content.lines.len().saturating_sub(1));
            self.dual.last_synced_scroll = usize::MAX;
        }
    }

    pub(super) fn dual_center_line(&mut self, line: usize) {
        if let Some(vrow) = self.dual_line_to_vrow(line) {
            let half_page = self.dual.page_height / 2;
            self.dual.vtop = vrow.saturating_sub(half_page).min(self.dual.max_vtop);
            self.sync_dual_scroll();
        } else {
            self.scroll_offset = line.min(self.rendered_content.lines.len().saturating_sub(1));
            self.dual.last_synced_scroll = usize::MAX;
        }
    }

    pub(super) fn ensure_dual_line_visible(&mut self, line: usize, scrolloff: usize) {
        let Some(vrow) = self.dual_line_to_vrow(line) else {
            self.scroll_offset = line.min(self.rendered_content.lines.len().saturating_sub(1));
            self.dual.last_synced_scroll = usize::MAX;
            return;
        };
        let page_height = self.dual.page_height.max(1);
        let top = self.dual.vtop;
        let bottom = top + page_height;
        let scrolloff = scrolloff.min(page_height.saturating_sub(1));

        if vrow < top + scrolloff {
            self.dual.vtop = vrow.saturating_sub(scrolloff).min(self.dual.max_vtop);
            self.sync_dual_scroll();
        } else if vrow >= bottom.saturating_sub(scrolloff) {
            let target = vrow + scrolloff + 1;
            self.dual.vtop = target.saturating_sub(page_height).min(self.dual.max_vtop);
            self.sync_dual_scroll();
        }
    }

    /// Scroll the dual-column page grid by `delta` screen rows (line-by-line),
    /// then keep `scroll_offset` in sync with the new top row.
    pub(super) fn dual_scroll(&mut self, delta: isize) {
        let new_vtop =
            (self.dual.vtop as isize + delta).clamp(0, self.dual.max_vtop as isize) as usize;
        self.dual.vtop = new_vtop;
        self.sync_dual_scroll();
        self.last_scroll_time = Instant::now();
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    /// Derive `scroll_offset` (the top-left visible buffer line) from the grid's
    /// virtual scroll position so bookmarks, marks, and "current node" stay
    /// correct in dual mode.
    pub(super) fn sync_dual_scroll(&mut self) {
        let total = self.rendered_content.lines.len();
        let top = Self::dual_body_line(
            self.dual.vtop,
            self.dual.page_height,
            self.dual.stride,
            total,
        );
        self.scroll_offset = top;
        self.dual.last_synced_scroll = top;
    }

    /// Step size for half-screen motions in the page grid.
    fn dual_half_step(&self) -> isize {
        (self.dual.page_height / 2).max(1) as isize
    }

    pub fn scroll_up(&mut self) {
        if self.dual.active {
            self.dual_scroll(-(self.scroll_speed as isize));
            return;
        }
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);
            self.last_scroll_time = Instant::now();
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    pub fn scroll_down(&mut self) {
        if self.dual.active {
            self.dual_scroll(self.scroll_speed as isize);
            return;
        }
        let max_offset = self.get_max_scroll_offset();
        if self.scroll_offset < max_offset {
            self.scroll_offset = (self.scroll_offset + self.scroll_speed).min(max_offset);
            self.last_scroll_time = Instant::now();
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    pub fn scroll_paragraph_up(&mut self) {
        if self.dual.active {
            self.dual_scroll(-self.dual_half_step());
            return;
        }
        let target = self.find_prev_paragraph_boundary(self.scroll_offset);
        self.scroll_offset = target;
    }

    pub fn scroll_paragraph_down(&mut self) {
        if self.dual.active {
            self.dual_scroll(self.dual_half_step());
            return;
        }
        let target = self.find_next_paragraph_boundary(self.scroll_offset);
        self.scroll_offset = target.min(self.get_max_scroll_offset());
    }

    pub fn scroll_half_screen_up(&mut self, screen_height: usize) {
        if self.dual.active {
            self.dual_scroll(-self.dual_half_step());
            return;
        }
        let scroll_amount = screen_height / 2;
        let old_offset = self.scroll_offset;
        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
        self.highlight_visual_line = Some(Self::overlap_highlight_after_scroll_up(
            old_offset,
            self.scroll_offset,
            screen_height,
        ));
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        // Clear current match when manually scrolling so next 'n' finds from new position
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    pub fn scroll_half_screen_down(&mut self, screen_height: usize) {
        if self.dual.active {
            self.dual_scroll(self.dual_half_step());
            return;
        }
        let scroll_amount = screen_height / 2;
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
        self.highlight_visual_line = Some(Self::overlap_highlight_after_scroll_down());
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        // Clear current match when manually scrolling so next 'n' finds from new position
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    pub fn scroll_full_screen_up(&mut self, screen_height: usize) {
        if self.dual.active {
            self.dual_scroll(-(self.dual.page_height as isize));
            return;
        }
        let scroll_amount = screen_height.saturating_sub(1);
        let old_offset = self.scroll_offset;
        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
        self.highlight_visual_line = Some(Self::overlap_highlight_after_scroll_up(
            old_offset,
            self.scroll_offset,
            screen_height,
        ));
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        // Clear current match when manually scrolling so next 'n' finds from new position
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    pub fn scroll_full_screen_down(&mut self, screen_height: usize) {
        if self.dual.active {
            self.dual_scroll(self.dual.page_height as isize);
            return;
        }
        let scroll_amount = screen_height.saturating_sub(1);
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
        self.highlight_visual_line = Some(Self::overlap_highlight_after_scroll_down());
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    fn overlap_highlight_after_scroll_down() -> usize {
        0
    }

    fn overlap_highlight_after_scroll_up(
        old_offset: usize,
        new_offset: usize,
        screen_height: usize,
    ) -> usize {
        old_offset
            .saturating_sub(new_offset)
            .min(screen_height.saturating_sub(1))
    }

    pub fn get_scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn get_max_scroll_offset(&self) -> usize {
        if self.dual.active {
            return self.rendered_content.lines.len().saturating_sub(1);
        }
        self.total_wrapped_lines.saturating_sub(self.visible_height)
    }

    pub fn scroll_to_line(&mut self, target_line: usize) {
        if self.dual.active {
            self.dual_scroll_to_line(target_line);
            return;
        }
        // Center target line in viewport if possible
        let desired_offset = if target_line > self.visible_height / 2 {
            target_line // - self.visible_height  / 2
        } else {
            0
        };

        self.scroll_offset = desired_offset.min(self.get_max_scroll_offset());
    }

    pub fn jump_to_line(&mut self, line_idx: usize) {
        if line_idx < self.rendered_content.lines.len() {
            if self.dual.active {
                self.dual_center_line(line_idx);
                return;
            }
            // Center the line in the viewport if possible
            let half_height = self.visible_height / 2;
            self.scroll_offset = line_idx.saturating_sub(half_height);

            // Ensure we don't scroll past the end
            let max_scroll = self
                .rendered_content
                .total_height
                .saturating_sub(self.visible_height);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        }
    }

    pub fn scroll_to_paragraph(&mut self, paragraph_index: usize) {
        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line.node_index == Some(paragraph_index) {
                self.jump_to_line(line_idx);
                return;
            }
        }
    }

    pub fn get_anchor_position(&self, anchor_id: &str) -> Option<usize> {
        self.anchor_positions.get(anchor_id).copied()
    }

    pub fn store_pending_anchor_scroll(&mut self, pending_anchor: String) {
        // Store the pending anchor to be processed after anchors are collected
        self.pending_anchor_scroll = Some(pending_anchor);
    }

    //todo: remove
    pub fn highlight_line_temporarily(&mut self, line: usize, duration: std::time::Duration) {
        if line >= self.scroll_offset && line < self.scroll_offset + self.visible_height {
            let visible_line = line - self.scroll_offset;
            self.highlight_visual_line = Some(visible_line);
            self.highlight_end_time = Instant::now() + duration;
        }
    }

    //todo: remove
    pub fn update_highlight(&mut self) -> bool {
        let mut changed = false;

        if self.highlight_visual_line.is_some() && Instant::now() > self.highlight_end_time {
            self.highlight_visual_line = None;
            changed = true;
        }

        // Check for expired yank highlight
        if let Some(ref highlight) = self.normal_mode.yank_highlight {
            if highlight.is_expired() {
                self.normal_mode.yank_highlight = None;
                changed = true;
            }
        }

        changed
    }

    pub fn perform_auto_scroll(&mut self) {
        if self.auto_scroll_active {
            let scroll_amount = self.auto_scroll_speed.abs() as usize;

            if self.auto_scroll_speed < 0.0 && self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
            } else if self.auto_scroll_speed > 0.0 {
                let max_offset = self.get_max_scroll_offset();
                if self.scroll_offset < max_offset {
                    self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
                }
            }
        }
    }

    pub fn update_auto_scroll(&mut self) -> bool {
        if self.auto_scroll_active {
            let scroll_amount = self.auto_scroll_speed.abs() as usize;

            if self.auto_scroll_speed < 0.0 && self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
                return true;
            } else if self.auto_scroll_speed > 0.0 {
                let max_offset = self.get_max_scroll_offset();
                if self.scroll_offset < max_offset {
                    self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
                    return true;
                }
            }
        }
        false
    }

    pub fn clear_active_anchor(&mut self) {
        self.last_active_anchor = None;
    }

    pub fn set_active_anchor(&mut self, anchor: Option<String>) {
        self.last_active_anchor = anchor;
    }

    pub fn get_active_section(
        &mut self,
        current_chapter: usize,
        chapter_href: Option<&str>,
        available_anchors: &[String],
    ) -> ActiveSection {
        let chapter_href = if let Some(href) = chapter_href {
            href.to_string()
        } else if let Some(ref file) = self.current_chapter_file {
            file.clone()
        } else {
            format!("chapter_{current_chapter}")
        };

        let visible_start = self.scroll_offset;
        let total_lines = self.rendered_content.lines.len();
        if total_lines == 0 {
            return ActiveSection::new(current_chapter, chapter_href, None);
        }

        let viewport_mid = (visible_start + self.visible_height / 2).min(total_lines);
        let mut latest_heading_anchor: Option<String> = None;

        for line_idx in visible_start..viewport_mid {
            if let Some(line) = self.rendered_content.lines.get(line_idx) {
                if matches!(line.line_type, LineType::Heading { .. }) {
                    if let Some(anchor) = line.node_anchor.clone() {
                        latest_heading_anchor = Some(anchor);
                    }
                }
            }
        }

        if let Some(anchor) = latest_heading_anchor {
            if let Some(matched) = Self::match_available_anchor(&anchor, available_anchors) {
                self.last_active_anchor = Some(matched.clone());
                return ActiveSection::new(current_chapter, chapter_href, Some(matched));
            }
        }

        if let Some(ref anchor) = self.last_active_anchor {
            if let Some(matched) = Self::match_available_anchor(anchor, available_anchors) {
                self.last_active_anchor = Some(matched.clone());
                return ActiveSection::new(current_chapter, chapter_href, Some(matched));
            }
        }

        self.last_active_anchor = None;
        ActiveSection::new(current_chapter, chapter_href, None)
    }

    fn match_available_anchor(anchor: &str, available: &[String]) -> Option<String> {
        if available.iter().any(|a| a == anchor) {
            return Some(anchor.to_string());
        }

        available
            .iter()
            .find(|a| a.eq_ignore_ascii_case(anchor))
            .cloned()
    }

    /// Get the index of the first visible node in the viewport
    pub fn get_current_node_index(&self) -> usize {
        let visible_start = self.scroll_offset;

        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line_idx >= visible_start {
                if let Some(node_idx) = line.node_index {
                    return node_idx;
                }
            }
        }

        0
    }

    /// Node index at a specific wrapped-line, if any. Walks forward from the
    /// requested line so a hit on an "empty" line (no node_index) falls
    /// through to the next content line — matching how the viewport-based
    /// `get_current_node_index` behaves.
    pub fn node_index_at_line(&self, line: usize) -> Option<usize> {
        for rendered_line in self.rendered_content.lines.iter().skip(line) {
            if let Some(node_idx) = rendered_line.node_index {
                return Some(node_idx);
            }
        }
        None
    }

    /// Node index at the user's "current focus": cursor line in normal mode,
    /// top-of-viewport in scroll mode. Used by mark capture and any other
    /// code that wants "where is the user actually looking".
    pub fn focused_node_index(&self) -> usize {
        if self.normal_mode.is_active() {
            self.node_index_at_line(self.normal_mode.cursor.line)
                .unwrap_or_else(|| self.get_current_node_index())
        } else {
            self.get_current_node_index()
        }
    }

    /// Wrapped-line index at the user's current focus. Mirrors
    /// `focused_node_index` but returns the line itself, walking forward
    /// past any blank/separator lines (no `node_index`) so we land on a
    /// real content line.
    pub fn focused_line_index(&self) -> usize {
        let start = if self.normal_mode.is_active() {
            self.normal_mode.cursor.line
        } else {
            self.scroll_offset
        };
        for (offset, line) in self.rendered_content.lines.iter().skip(start).enumerate() {
            if line.node_index.is_some() {
                return start + offset;
            }
        }
        start
    }

    /// Canonical char offset of the focused line within its paragraph. None
    /// if the focused line isn't part of an annotatable block (separator,
    /// horizontal rule, etc.).
    pub fn focused_node_char_offset(&self) -> Option<usize> {
        let line_idx = self.focused_line_index();
        self.rendered_content
            .lines
            .get(line_idx)
            .and_then(|l| l.canonical_content_start)
    }

    /// Find the wrapped line within `node_index` whose canonical content
    /// start best matches `char_offset` (largest start <= offset). Falls
    /// back to the first line of the node when nothing fits.
    pub fn find_line_for_node_offset(
        &self,
        node_index: usize,
        char_offset: usize,
    ) -> Option<usize> {
        let mut best: Option<usize> = None;
        let mut first_in_node: Option<usize> = None;
        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line.node_index != Some(node_index) {
                if first_in_node.is_some() {
                    break; // walked past the node
                }
                continue;
            }
            if first_in_node.is_none() {
                first_in_node = Some(line_idx);
            }
            if let Some(start) = line.canonical_content_start {
                if start <= char_offset {
                    best = Some(line_idx);
                } else {
                    break;
                }
            }
        }
        best.or(first_in_node)
    }

    /// Restore scroll to the wrapped line within `node_index` matching
    /// `char_offset`. Performs immediately and queues a pending restore so
    /// it re-runs after the next render — needed for cross-chapter jumps
    /// where `rendered_content.lines` is empty when the jump fires.
    pub fn restore_to_node_position(&mut self, node_index: usize, char_offset: usize) {
        self.perform_node_position_restore(node_index, char_offset);
        self.pending_node_restore = Some((node_index, Some(char_offset)));
    }

    /// Synchronous half of `restore_to_node_position`. Does not queue any
    /// pending state; safe to call from the render-time pending processor.
    pub fn perform_node_position_restore(&mut self, node_index: usize, char_offset: usize) {
        if let Some(line_idx) = self.find_line_for_node_offset(node_index, char_offset) {
            if self.dual.active {
                self.dual_scroll_to_line(line_idx);
                return;
            }
            self.scroll_offset = line_idx.min(self.get_max_scroll_offset());
        } else {
            self.perform_node_restore(node_index);
        }
    }

    /// Briefly highlight the wrapped line at `(node_index, char_offset)`.
    /// Defers to a render-time hook if `rendered_content.lines` is empty
    /// (cross-chapter jump where content hasn't re-rendered yet).
    pub fn flash_node_position_highlight(
        &mut self,
        node_index: usize,
        char_offset: usize,
        duration: std::time::Duration,
    ) {
        if self.rendered_content.lines.is_empty() {
            self.pending_node_highlight = Some((node_index, Some(char_offset), duration));
            return;
        }
        if let Some(line_idx) = self.find_line_for_node_offset(node_index, char_offset) {
            self.highlight_line_temporarily(line_idx, duration);
        }
    }

    /// Restore scroll position to show a specific node
    pub fn restore_to_node_index(&mut self, node_index: usize) {
        // Perform immediately for jump list navigation
        self.perform_node_restore(node_index);
        // Also set pending in case content hasn't been rendered yet
        self.pending_node_restore = Some((node_index, None));
    }

    pub fn perform_node_restore(&mut self, node_index: usize) {
        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if let Some(node_idx) = line.node_index {
                if node_idx >= node_index {
                    if self.dual.active {
                        self.dual_scroll_to_line(line_idx);
                        return;
                    }
                    self.scroll_offset = line_idx.min(self.get_max_scroll_offset());
                    return;
                }
            }
        }
    }

    /// Briefly background-highlight the line that maps to `node_index`. Used
    /// after a mark jump so the user can see *which* line they jumped to.
    ///
    /// If `rendered_content.lines` is still empty (cross-chapter jump where
    /// content was cleared but the next render hasn't run), defers the
    /// highlight to fire after the render via `pending_node_highlight`.
    pub fn flash_node_highlight(&mut self, node_index: usize, duration: std::time::Duration) {
        if self.rendered_content.lines.is_empty() {
            self.pending_node_highlight = Some((node_index, None, duration));
            return;
        }
        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if let Some(node_idx) = line.node_index {
                if node_idx >= node_index {
                    self.highlight_line_temporarily(line_idx, duration);
                    return;
                }
            }
        }
    }

    pub fn set_current_chapter_file(&mut self, chapter_file: Option<String>) {
        self.current_chapter_file = chapter_file;
        self.rebuild_chapter_comments();
    }

    pub fn get_current_chapter_file(&self) -> &Option<String> {
        &self.current_chapter_file
    }

    pub fn get_visible_height(&self) -> usize {
        self.visible_height
    }

    /// Capture a short text excerpt for use as a mark label.
    ///
    /// In normal mode: starts at the cursor line (the line the user
    /// explicitly chose). In scroll mode: starts at the top of the viewport
    /// but skips any leading heading or horizontal-rule lines so the snippet
    /// is the first body content the reader sees, not a section title that
    /// happens to be at the top of the screen.
    pub fn current_text_snippet(&self, max_chars: usize) -> Option<String> {
        let (start, skip_headings) = if self.normal_mode.is_active() {
            (self.normal_mode.cursor.line, false)
        } else {
            (self.scroll_offset, true)
        };
        self.text_snippet_from(start, max_chars, skip_headings)
    }

    fn text_snippet_from(
        &self,
        start_line: usize,
        max_chars: usize,
        skip_headings: bool,
    ) -> Option<String> {
        use super::types::LineType;

        let mut included_any = false;
        let filtered = self
            .rendered_content
            .lines
            .iter()
            .skip(start_line)
            .filter_map(|line| {
                let trimmed = line.raw_text.trim();
                if trimmed.is_empty() {
                    return None;
                }
                if !included_any
                    && skip_headings
                    && matches!(
                        line.line_type,
                        LineType::Heading { .. } | LineType::HorizontalRule
                    )
                {
                    return None;
                }
                included_any = true;
                Some(line.raw_text.as_str())
            });
        crate::marks::build_text_snippet(filtered, max_chars)
    }
}

impl VimNavMotions for crate::markdown_text_reader::MarkdownTextReader {
    fn handle_h(&mut self) {
        // do nothing - handled at App level
    }

    fn handle_l(&mut self) {
        // do nothing - handled at App level
    }

    fn handle_j(&mut self) {
        self.scroll_down();
    }

    fn handle_k(&mut self) {
        self.scroll_up();
    }

    fn handle_ctrl_d(&mut self) {
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_half_screen_down(screen_height);
        }
    }

    fn handle_ctrl_u(&mut self) {
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_half_screen_up(screen_height);
        }
    }

    fn handle_ctrl_f(&mut self) {
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_full_screen_down(screen_height);
        }
    }

    fn handle_ctrl_b(&mut self) {
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_full_screen_up(screen_height);
        }
    }

    fn handle_gg(&mut self) {
        if self.dual.active {
            self.dual.vtop = 0;
            self.sync_dual_scroll();
            return;
        }
        self.scroll_offset = 0;
    }

    fn handle_upper_g(&mut self) {
        if self.dual.active {
            self.dual.vtop = self.dual.max_vtop;
            self.sync_dual_scroll();
            return;
        }
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = max_offset;
    }
}

#[cfg(test)]
mod tests {
    use crate::markdown_text_reader::MarkdownTextReader;

    #[test]
    fn full_page_scroll_down_is_noop_for_zero_height() {
        let mut reader = MarkdownTextReader::new_without_image_support();
        reader.total_wrapped_lines = 10;
        reader.visible_height = 0;

        reader.scroll_full_screen_down(0);

        assert_eq!(reader.scroll_offset, 0);
        assert_eq!(reader.highlight_visual_line, Some(0));
    }
}
