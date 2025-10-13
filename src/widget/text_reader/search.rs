use crate::search::{SearchState, SearchablePanel, find_matches_in_text};
use crate::theme::Base16Palette;
use ratatui::style::{Color, Style as RatatuiStyle};
use ratatui::text::Span;

impl crate::markdown_text_reader::MarkdownTextReader {
    /// Get searchable content (visible lines as text)
    pub fn get_visible_text(&self) -> Vec<String> {
        self.rendered_content
            .lines
            .iter()
            .map(|line| line.raw_text.clone())
            .collect()
    }

    /// Apply search highlighting to a line's spans
    pub fn apply_search_highlighting(
        &self,
        line_idx: usize,
        line_spans: Vec<Span<'static>>,
        _palette: &Base16Palette,
    ) -> Vec<Span<'static>> {
        if !self.search_state.active || self.search_state.matches.is_empty() {
            return line_spans;
        }

        // Check if this line has any search matches
        let line_matches: Vec<_> = self
            .search_state
            .matches
            .iter()
            .filter(|m| m.index == line_idx)
            .collect();

        if line_matches.is_empty() {
            return line_spans;
        }

        // Get the raw text for this line to calculate positions
        let _raw_text = self
            .rendered_content
            .lines
            .get(line_idx)
            .map(|l| l.raw_text.clone())
            .unwrap_or_default();

        let mut result_spans = Vec::new();
        let mut char_offset = 0;

        for span in line_spans {
            let span_text = span.content.to_string();
            let span_len = span_text.len();
            let span_end = char_offset + span_len;

            // Check if any highlights overlap with this span
            let mut segments = vec![];
            let mut last_pos = 0;

            for match_item in &line_matches {
                for (highlight_start, highlight_end) in &match_item.highlight_ranges {
                    // Check if this highlight overlaps with the current span
                    if *highlight_end > char_offset && *highlight_start < span_end {
                        // Calculate relative positions within the span
                        let rel_start = highlight_start.saturating_sub(char_offset).min(span_len);
                        let rel_end = highlight_end.saturating_sub(char_offset).min(span_len);

                        if rel_start > last_pos {
                            // Add non-highlighted segment before this match
                            segments.push((last_pos, rel_start, false));
                        }

                        // Add highlighted segment
                        segments.push((rel_start, rel_end, true));
                        last_pos = rel_end;
                    }
                }
            }

            // Add any remaining non-highlighted text
            if last_pos < span_len {
                segments.push((last_pos, span_len, false));
            }

            // Create new spans based on segments
            if segments.is_empty() {
                result_spans.push(span);
            } else {
                for (start, end, is_highlighted) in segments {
                    if start >= end {
                        continue;
                    }

                    let text_segment = span_text[start..end].to_string();
                    let style = if is_highlighted {
                        let is_current = self.search_state.is_current_match(line_idx);
                        if is_current {
                            // Current match: bright yellow background with black text
                            RatatuiStyle::default().bg(Color::Yellow).fg(Color::Black)
                        } else {
                            // Other matches: dim yellow background, preserve original fg
                            span.style.bg(Color::Rgb(100, 100, 0))
                        }
                    } else {
                        span.style
                    };

                    result_spans.push(Span::styled(text_segment, style));
                }
            }

            char_offset = span_end;
        }

        result_spans
    }
}

impl SearchablePanel for crate::markdown_text_reader::MarkdownTextReader {
    fn start_search(&mut self) {
        self.search_state.start_search(self.scroll_offset);
    }

    fn cancel_search(&mut self) {
        let original_position = self.search_state.cancel_search();
        self.scroll_offset = original_position;
    }

    fn confirm_search(&mut self) {
        self.search_state.confirm_search();
        if !self.search_state.active {
            let original_position = self.search_state.original_position;
            self.scroll_offset = original_position;
        }
    }

    fn exit_search(&mut self) {
        self.search_state.exit_search();
        // Keep current position
    }

    fn update_search_query(&mut self, query: &str) {
        self.search_state.update_query(query.to_string());

        // Find matches in visible text
        let searchable = self.get_searchable_content();
        let matches = find_matches_in_text(query, &searchable);
        self.search_state.set_matches(matches);

        // Jump to match if found
        if let Some(match_index) = self.search_state.get_current_match() {
            self.jump_to_match(match_index);
        }
    }

    fn next_match(&mut self) {
        if self.search_state.matches.is_empty() {
            return;
        }

        if let Some(current_idx) = self.search_state.current_match_index {
            let next_idx = (current_idx + 1) % self.search_state.matches.len();
            self.search_state.current_match_index = Some(next_idx);

            if let Some(search_match) = self.search_state.matches.get(next_idx) {
                self.jump_to_match(search_match.index);
            }
        } else {
            let current_position = self.scroll_offset;

            let mut next_match_idx = None;
            for (idx, search_match) in self.search_state.matches.iter().enumerate() {
                if search_match.index > current_position {
                    next_match_idx = Some(idx);
                    break;
                }
            }

            // If no match found after current position, wrap to beginning
            let target_idx = next_match_idx.unwrap_or(0);
            self.search_state.current_match_index = Some(target_idx);

            if let Some(search_match) = self.search_state.matches.get(target_idx) {
                self.jump_to_match(search_match.index);
            }
        }
    }

    fn previous_match(&mut self) {
        if self.search_state.matches.is_empty() {
            return;
        }

        if let Some(current_idx) = self.search_state.current_match_index {
            let prev_idx = if current_idx == 0 {
                self.search_state.matches.len() - 1
            } else {
                current_idx - 1
            };
            self.search_state.current_match_index = Some(prev_idx);

            if let Some(search_match) = self.search_state.matches.get(prev_idx) {
                self.jump_to_match(search_match.index);
            }
        } else {
            let current_position = self.scroll_offset;

            let mut prev_match_idx = None;
            for (idx, search_match) in self.search_state.matches.iter().enumerate().rev() {
                if search_match.index < current_position {
                    prev_match_idx = Some(idx);
                    break;
                }
            }

            let target_idx = prev_match_idx.unwrap_or(self.search_state.matches.len() - 1);
            self.search_state.current_match_index = Some(target_idx);

            if let Some(search_match) = self.search_state.matches.get(target_idx) {
                self.jump_to_match(search_match.index);
            }
        }
    }

    fn get_search_state(&self) -> &SearchState {
        &self.search_state
    }

    fn is_searching(&self) -> bool {
        self.search_state.active
    }

    fn has_matches(&self) -> bool {
        !self.search_state.matches.is_empty()
    }

    fn jump_to_match(&mut self, match_index: usize) {
        self.jump_to_line(match_index);
    }

    fn get_searchable_content(&self) -> Vec<String> {
        self.get_visible_text()
    }
}
