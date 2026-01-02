use log::debug;
use ratatui::layout::Rect;

impl crate::markdown_text_reader::MarkdownTextReader {
    pub fn handle_mouse_down(&mut self, x: u16, y: u16) {
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
        if self.text_selection.is_selecting && self.last_inner_text_area.is_some() {
            // Use the inner text area if available, otherwise fall back to the provided area
            let text_area = self.last_inner_text_area.unwrap();

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

    pub fn handle_mouse_up(&mut self, x: u16, y: u16) -> Option<String> {
        self.auto_scroll_active = false;

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

    pub fn copy_selection_to_clipboard(&self) -> Result<(), String> {
        if let Some(selected_text) = self
            .text_selection
            .extract_selected_text(&self.raw_text_lines)
        {
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

    pub fn copy_chapter_to_clipboard(&self) -> Result<(), String> {
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
        clipboard
            .set_text(text)
            .map_err(|e| format!("Failed to copy to clipboard: {e}"))
    }

    pub fn copy_to_clipboard(&self, text: String) -> Result<(), String> {
        use arboard::Clipboard;
        let mut clipboard =
            Clipboard::new().map_err(|e| format!("Failed to access clipboard: {e}"))?;
        clipboard
            .set_text(text)
            .map_err(|e| format!("Failed to copy to clipboard: {e}"))
    }

    //for debuggin purposes
    pub fn copy_raw_text_lines_to_clipboard(&self) -> Result<(), String> {
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

        use arboard::Clipboard;
        let mut clipboard =
            Clipboard::new().map_err(|e| format!("Failed to access clipboard: {e}"))?;
        clipboard
            .set_text(debug_output)
            .map_err(|e| format!("Failed to copy to clipboard: {e}"))?;

        Ok(())
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
}
