use log::debug;
use ratatui::layout::Rect;

impl crate::markdown_text_reader::MarkdownTextReader {
    pub fn handle_mouse_down(&mut self, x: u16, y: u16) {
        if self.is_normal_mode_active() {
            self.text_selection.clear_selection();
            self.exit_visual_mode();

            if let Some(text_area) = self.last_inner_text_area {
                if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                    self.set_normal_mode_cursor(line, column);
                }
            }
            return;
        }

        if let Some(text_area) = self.last_inner_text_area {
            if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                if self.get_link_at_position(line, column).is_some() {
                    debug!("Mouse down on link, skipping text selection");
                    return;
                }

                self.text_selection.start_selection(line, column);
            }
        }
    }

    pub fn handle_mouse_drag(&mut self, x: u16, y: u16) {
        if self.is_normal_mode_active() {
            if let Some(text_area) = self.last_inner_text_area {
                if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                    if !self.is_image_line(line) {
                        if self.normal_mode.visual_mode == super::VisualMode::None {
                            if self.normal_mode.visual_anchor.is_none() {
                                self.normal_mode.visual_anchor =
                                    Some(self.normal_mode.cursor.clone());
                            }
                            self.normal_mode.visual_mode = super::VisualMode::CharacterWise;
                        }
                        self.set_normal_mode_cursor(line, column);
                    }
                }

                // Check if we need to auto-scroll due to dragging outside the visible area
                const SCROLL_MARGIN: u16 = 3;
                let needs_scroll_up = y <= text_area.y + SCROLL_MARGIN && self.scroll_offset > 0;
                let needs_scroll_down = y >= text_area.y + text_area.height - SCROLL_MARGIN;

                if needs_scroll_up {
                    self.auto_scroll_active = true;
                    self.auto_scroll_speed = -1.0;
                    // Perform immediate scroll like text_reader.rs does
                    self.perform_auto_scroll();
                } else if needs_scroll_down {
                    self.auto_scroll_active = true;
                    self.auto_scroll_speed = 1.0;
                    // Perform immediate scroll like text_reader.rs does
                    self.perform_auto_scroll();
                } else {
                    self.auto_scroll_active = false;
                }
                return;
            }
        }

        if self.text_selection.is_selecting {
            if let Some(text_area) = self.last_inner_text_area {
                // Always try to update text selection first, regardless of auto-scroll
                if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                    self.text_selection.update_selection(line, column);
                }

                // Check if we need to auto-scroll due to dragging outside the visible area
                const SCROLL_MARGIN: u16 = 3;
                let needs_scroll_up = y <= text_area.y + SCROLL_MARGIN && self.scroll_offset > 0;
                let needs_scroll_down = y >= text_area.y + text_area.height - SCROLL_MARGIN;

                if needs_scroll_up {
                    self.auto_scroll_active = true;
                    self.auto_scroll_speed = -1.0;
                    // Perform immediate scroll like text_reader.rs does
                    self.perform_auto_scroll();
                } else if needs_scroll_down {
                    self.auto_scroll_active = true;
                    self.auto_scroll_speed = 1.0;
                    // Perform immediate scroll like text_reader.rs does
                    self.perform_auto_scroll();
                } else {
                    self.auto_scroll_active = false;
                }
            }
        }
    }

    pub fn handle_mouse_up(&mut self, x: u16, y: u16) -> Option<String> {
        self.auto_scroll_active = false;

        if self.is_normal_mode_active() {
            let text_area = self.last_inner_text_area?;

            if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                if let Some(link) = self.get_link_at_position(line, column) {
                    let url = link.url.clone();
                    return Some(url);
                }
            }

            if !self.is_visual_mode_active() {
                self.normal_mode.visual_anchor = None;
            }

            return self.check_image_click(x, y);
        }

        let text_area = self.last_inner_text_area?;

        if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
            if let Some(link) = self.get_link_at_position(line, column) {
                let url = link.url.clone();
                self.text_selection.clear_selection();
                return Some(url);
            }
        }

        if self.text_selection.is_selecting {
            self.text_selection.end_selection();
        }

        self.check_image_click(x, y)
    }

    pub fn handle_double_click(&mut self, x: u16, y: u16) {
        if self.is_normal_mode_active() {
            if let Some(text_area) = self.last_inner_text_area {
                if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                    let mut temp_selection = super::text_selection::TextSelection::new();
                    temp_selection.select_word_at(line, column, &self.raw_text_lines);
                    if let (Some(start), Some(end)) = (temp_selection.start, temp_selection.end) {
                        if !self.is_image_line(start.line) {
                            self.normal_mode.visual_mode = super::VisualMode::CharacterWise;
                            self.normal_mode.visual_anchor = Some(
                                super::normal_mode::CursorPosition::new(start.line, start.column),
                            );
                            let end_col = end.column.saturating_sub(1);
                            self.set_normal_mode_cursor(end.line, end_col);
                        }
                    }
                }
            }
            return;
        }

        if let Some(text_area) = self.last_inner_text_area {
            if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                if line < self.raw_text_lines.len() {
                    self.text_selection
                        .select_word_at(line, column, &self.raw_text_lines);
                }
            }
        }
    }

    pub fn handle_triple_click(&mut self, x: u16, y: u16) {
        if self.is_normal_mode_active() {
            if let Some(text_area) = self.last_inner_text_area {
                if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                    let mut temp_selection = super::text_selection::TextSelection::new();
                    temp_selection.select_paragraph_at(line, column, &self.raw_text_lines);
                    if let (Some(start), Some(end)) = (temp_selection.start, temp_selection.end) {
                        if !self.is_image_line(start.line) {
                            self.normal_mode.visual_mode = super::VisualMode::LineWise;
                            self.normal_mode.visual_anchor =
                                Some(super::normal_mode::CursorPosition::new(start.line, 0));
                            self.set_normal_mode_cursor(end.line, 0);
                        }
                    }
                }
            }
            return;
        }

        if let Some(text_area) = self.last_inner_text_area {
            if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                if line < self.raw_text_lines.len() {
                    self.text_selection
                        .select_paragraph_at(line, column, &self.raw_text_lines);
                }
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.text_selection.clear_selection();
    }

    pub fn has_text_selection(&self) -> bool {
        self.text_selection.has_selection()
    }

    pub fn copy_selection_to_clipboard(&mut self) -> Result<(), String> {
        if let Some(selected_text) = self
            .text_selection
            .extract_selected_text(&self.raw_text_lines)
        {
            self.last_copied_text = Some(selected_text.clone());
            use arboard::Clipboard;
            let mut clipboard =
                Clipboard::new().map_err(|e| format!("Failed to access clipboard: {e}"))?;
            clipboard
                .set_text(selected_text)
                .map_err(|e| format!("Failed to copy to clipboard: {e}"))?;
            Ok(())
        } else {
            Err("No text selected".to_string())
        }
    }

    pub fn copy_chapter_to_clipboard(&mut self) -> Result<(), String> {
        use arboard::Clipboard;
        let mut clipboard =
            Clipboard::new().map_err(|e| format!("Failed to access clipboard: {e}"))?;
        let text = if self.show_raw_html {
            self.raw_html_content
                .as_ref()
                .unwrap_or(&"<failed to get raw html>".to_string())
                .to_string()
        } else {
            self.raw_text_lines.join("\n")
        };
        self.last_copied_text = Some(text.clone());
        clipboard
            .set_text(text)
            .map_err(|e| format!("Failed to copy to clipboard: {e}"))
    }

    pub fn copy_to_clipboard(&mut self, text: String) -> Result<(), String> {
        self.last_copied_text = Some(text.clone());
        use arboard::Clipboard;
        let mut clipboard =
            Clipboard::new().map_err(|e| format!("Failed to access clipboard: {e}"))?;
        clipboard
            .set_text(text)
            .map_err(|e| format!("Failed to copy to clipboard: {e}"))
    }

    //for debuggin purposes
    pub fn copy_raw_text_lines_to_clipboard(&mut self) -> Result<(), String> {
        if self.raw_text_lines.is_empty() {
            return Err("No content to copy".to_string());
        }

        let mut debug_output = String::new();
        debug_output.push_str(&format!(
            "=== raw_text_lines debug (total {} lines) ===\n",
            self.raw_text_lines.len()
        ));

        for (idx, line) in self.raw_text_lines.iter().enumerate() {
            debug_output.push_str(&format!("{idx:4}: {line}\n"));
        }

        self.last_copied_text = Some(debug_output.clone());
        use arboard::Clipboard;
        let mut clipboard =
            Clipboard::new().map_err(|e| format!("Failed to access clipboard: {e}"))?;
        clipboard
            .set_text(debug_output)
            .map_err(|e| format!("Failed to copy to clipboard: {e}"))?;

        Ok(())
    }

    pub fn get_last_copied_text(&self) -> Option<String> {
        self.last_copied_text.clone()
    }

    /// Convert screen coordinates to logical text coordinates (like TextReader does)
    pub fn screen_to_text_coords(
        &self,
        screen_x: u16,
        screen_y: u16,
        content_area: Rect,
    ) -> Option<(usize, usize)> {
        self.text_selection.screen_to_text_coords(
            screen_x,
            screen_y,
            self.scroll_offset,
            content_area.x,
            content_area.y,
        )
    }

    fn set_normal_mode_cursor(&mut self, line: usize, column: usize) {
        if self.raw_text_lines.is_empty() {
            return;
        }

        let max_line = self.raw_text_lines.len().saturating_sub(1);
        let clamped_line = line.min(max_line);
        if self.is_image_line(clamped_line) {
            return;
        }

        self.normal_mode.cursor.line = clamped_line;
        self.normal_mode.cursor.column = column;
        self.normal_mode.cursor_was_set = true;
        self.clamp_column_to_line_length();
        self.ensure_cursor_visible();
    }
}
