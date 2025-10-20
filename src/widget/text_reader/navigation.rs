use super::types::*;
use crate::main_app::VimNavMotions;
use crate::search::SearchMode;
use std::time::Instant;

impl crate::markdown_text_reader::MarkdownTextReader {
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);
            self.last_scroll_time = Instant::now();
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    pub fn scroll_down(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        if self.scroll_offset < max_offset {
            self.scroll_offset = (self.scroll_offset + self.scroll_speed).min(max_offset);
            self.last_scroll_time = Instant::now();
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    pub fn scroll_half_screen_up(&mut self, screen_height: usize) {
        let scroll_amount = screen_height / 2;
        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
        self.highlight_visual_line = Some(0);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        // Clear current match when manually scrolling so next 'n' finds from new position
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    pub fn scroll_half_screen_down(&mut self, screen_height: usize) {
        let scroll_amount = screen_height / 2;
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
        self.highlight_visual_line = Some(screen_height - 1);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        // Clear current match when manually scrolling so next 'n' finds from new position
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    pub fn get_scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn get_max_scroll_offset(&self) -> usize {
        self.total_wrapped_lines.saturating_sub(self.visible_height)
    }

    pub fn scroll_to_line(&mut self, target_line: usize) {
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
        if self.highlight_visual_line.is_some() && Instant::now() > self.highlight_end_time {
            self.highlight_visual_line = None;
            return true;
        }
        false
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

    /// Restore scroll position to show a specific node
    pub fn restore_to_node_index(&mut self, node_index: usize) {
        self.pending_node_restore = Some(node_index);
    }

    pub fn perform_node_restore(&mut self, node_index: usize) {
        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if let Some(node_idx) = line.node_index {
                if node_idx >= node_index {
                    self.scroll_offset = line_idx.min(self.get_max_scroll_offset());
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

    fn handle_gg(&mut self) {
        self.scroll_offset = 0;
    }

    fn handle_upper_g(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = max_offset;
    }
}
