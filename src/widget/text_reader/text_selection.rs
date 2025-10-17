use ratatui::{
    style::Color,
    text::{Line, Span},
};

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionPoint {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct TextSelection {
    pub start: Option<SelectionPoint>,
    pub end: Option<SelectionPoint>,
    pub is_selecting: bool,
}

impl TextSelection {
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            is_selecting: false,
        }
    }

    pub fn start_selection(&mut self, line: usize, column: usize) {
        self.start = Some(SelectionPoint { line, column });
        self.end = Some(SelectionPoint { line, column });
        self.is_selecting = true;
    }

    pub fn update_selection(&mut self, line: usize, column: usize) {
        if self.is_selecting {
            self.end = Some(SelectionPoint { line, column });
        }
    }

    pub fn end_selection(&mut self) {
        self.is_selecting = false;
    }

    pub fn clear_selection(&mut self) {
        self.start = None;
        self.end = None;
        self.is_selecting = false;
    }

    pub fn has_selection(&self) -> bool {
        if let (Some(start), Some(end)) = (&self.start, &self.end) {
            // This prevents flickering when just clicking (start == end)
            start.line != end.line || start.column != end.column
        } else {
            false
        }
    }

    pub fn get_selection_range(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        if let (Some(start), Some(end)) = (&self.start, &self.end) {
            if start.line < end.line || (start.line == end.line && start.column <= end.column) {
                Some((start.clone(), end.clone()))
            } else {
                Some((end.clone(), start.clone()))
            }
        } else {
            None
        }
    }

    pub fn is_point_in_selection(&self, line: usize, column: usize) -> bool {
        if let Some((start, end)) = self.get_selection_range() {
            if line < start.line || line > end.line {
                return false;
            }

            if line == start.line && line == end.line {
                // Single line selection
                column >= start.column && column < end.column
            } else if line == start.line {
                // First line of multi-line selection
                column >= start.column
            } else if line == end.line {
                // Last line of multi-line selection
                column < end.column
            } else {
                // Middle line of multi-line selection
                true
            }
        } else {
            false
        }
    }

    pub fn extract_selected_text(&self, lines: &[String]) -> Option<String> {
        if let Some((start, end)) = self.get_selection_range() {
            let mut selected_text = String::new();

            for line_idx in start.line..=end.line.min(lines.len().saturating_sub(1)) {
                if let Some(line) = lines.get(line_idx) {
                    let line_chars: Vec<char> = line.chars().collect();

                    let start_col = if line_idx == start.line {
                        start.column.min(line_chars.len())
                    } else {
                        0
                    };

                    let end_col = if line_idx == end.line {
                        end.column.min(line_chars.len())
                    } else {
                        line_chars.len()
                    };

                    if start_col < end_col {
                        let selected_part: String = line_chars[start_col..end_col].iter().collect();
                        selected_text.push_str(&selected_part);
                    }

                    if line_idx < end.line {
                        selected_text.push('\n');
                    }
                }
            }

            if !selected_text.is_empty() {
                Some(selected_text)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Apply selection highlighting to a line of spans
    pub fn apply_selection_highlighting<'a>(
        &self,
        line_idx: usize,
        spans: Vec<Span<'a>>,
        selection_bg_color: Color,
    ) -> Line<'a> {
        if !self.has_selection() {
            return Line::from(spans);
        }

        let mut result_spans = Vec::new();
        let mut current_column = 0;

        for span in spans {
            let span_text = span.content.to_string();
            let span_chars: Vec<char> = span_text.chars().collect();
            let span_end_column = current_column + span_chars.len();

            let has_selection_in_span = (current_column..span_end_column)
                .any(|col| self.is_point_in_selection(line_idx, col));

            if !has_selection_in_span {
                result_spans.push(span);
            } else {
                let mut i = 0;
                while i < span_chars.len() {
                    let char_column = current_column + i;
                    let is_selected = self.is_point_in_selection(line_idx, char_column);

                    let mut j = i + 1;
                    while j < span_chars.len() {
                        let next_char_column = current_column + j;
                        let next_is_selected =
                            self.is_point_in_selection(line_idx, next_char_column);
                        if next_is_selected != is_selected {
                            break;
                        }
                        j += 1;
                    }

                    let segment_text: String = span_chars[i..j].iter().collect();
                    let segment_style = if is_selected {
                        span.style.bg(selection_bg_color)
                    } else {
                        span.style
                    };

                    result_spans.push(Span::styled(segment_text, segment_style));
                    i = j;
                }
            }

            current_column = span_end_column;
        }

        Line::from(result_spans)
    }

    /// Select word at the given position
    pub fn select_word_at(&mut self, line: usize, column: usize, lines: &[String]) {
        if let Some(line_text) = lines.get(line) {
            let chars: Vec<char> = line_text.chars().collect();

            if column >= chars.len() {
                return;
            }

            let (word_start, word_end) = self.find_word_boundaries(&chars, column);

            self.start = Some(SelectionPoint {
                line,
                column: word_start,
            });
            self.end = Some(SelectionPoint {
                line,
                column: word_end,
            });
            self.is_selecting = false;
        }
    }

    /// Select paragraph at the given position
    pub fn select_paragraph_at(&mut self, line: usize, column: usize, lines: &[String]) {
        if line >= lines.len() {
            return;
        }

        if let Some(current_line) = lines.get(line) {
            let current_len = current_line.chars().count();
            if column > current_len && current_len > 0 {
                return;
            }
        }

        let (para_start, para_end) = self.find_paragraph_boundaries(lines, line);

        self.start = Some(SelectionPoint {
            line: para_start,
            column: 0,
        });
        let end_column = if let Some(last_line) = lines.get(para_end) {
            last_line.chars().count()
        } else {
            0
        };
        self.end = Some(SelectionPoint {
            line: para_end,
            column: end_column,
        });
        self.is_selecting = false;
    }

    /// Find word boundaries around the given column position
    fn find_word_boundaries(&self, chars: &[char], column: usize) -> (usize, usize) {
        if chars.is_empty() || column >= chars.len() {
            return (column, column);
        }

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';

        let clicked_char = chars[column];
        if !is_word_char(clicked_char) {
            return (column, column + 1);
        }

        let mut start = column;
        while start > 0 && is_word_char(chars[start - 1]) {
            start -= 1;
        }

        let mut end = column;
        while end < chars.len() && is_word_char(chars[end]) {
            end += 1;
        }

        (start, end)
    }

    /// Find paragraph boundaries around the given line
    fn find_paragraph_boundaries(&self, lines: &[String], line: usize) -> (usize, usize) {
        if lines.is_empty() || line >= lines.len() {
            return (line, line);
        }

        let mut start = line;
        while start > 0 {
            let prev_line = &lines[start - 1];
            if prev_line.trim().is_empty() {
                break; // Found empty line, paragraph starts at current line
            }
            start -= 1;
        }

        let mut end = line;
        while end < lines.len() {
            let current_line = &lines[end];
            if current_line.trim().is_empty() {
                break; // Found empty line, paragraph ends before this line
            }
            end += 1;
        }

        if end > 0 && end <= lines.len() {
            end -= 1; // Move back to the last non-empty line
        }

        (start, end)
    }

    /// Convert screen coordinates to logical text coordinates
    pub fn screen_to_text_coords(
        &self,
        screen_x: u16,
        screen_y: u16,
        scroll_offset: usize,
        content_area_x: u16,
        content_area_y: u16,
    ) -> Option<(usize, usize)> {
        if screen_y < content_area_y {
            return None;
        }

        let relative_y = (screen_y - content_area_y) as usize;

        let line = relative_y + scroll_offset;

        let column = if screen_x < content_area_x {
            0
        } else {
            (screen_x - content_area_x) as usize
        };

        Some((line, column))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_creation() {
        let selection = TextSelection::new();
        assert!(!selection.has_selection());
        assert!(!selection.is_selecting);
    }

    #[test]
    fn test_selection_workflow() {
        let mut selection = TextSelection::new();

        // Start selection
        selection.start_selection(1, 5);
        assert!(selection.is_selecting);

        // Update selection
        selection.update_selection(2, 10);
        assert_eq!(selection.start, Some(SelectionPoint { line: 1, column: 5 }));
        assert_eq!(
            selection.end,
            Some(SelectionPoint {
                line: 2,
                column: 10
            })
        );

        // End selection
        selection.end_selection();
        assert!(!selection.is_selecting);
        assert!(selection.has_selection());

        // Clear selection
        selection.clear_selection();
        assert!(!selection.has_selection());
    }

    #[test]
    fn test_selection_range_normalization() {
        let mut selection = TextSelection::new();

        // Test backward selection (end before start)
        selection.start_selection(2, 10);
        selection.update_selection(1, 5);

        let range = selection.get_selection_range().unwrap();
        assert_eq!(range.0, SelectionPoint { line: 1, column: 5 });
        assert_eq!(
            range.1,
            SelectionPoint {
                line: 2,
                column: 10
            }
        );
    }

    #[test]
    fn test_point_in_selection() {
        let mut selection = TextSelection::new();
        selection.start_selection(1, 5);
        selection.update_selection(3, 2);

        // Test points within selection
        assert!(selection.is_point_in_selection(1, 5)); // Start point
        assert!(selection.is_point_in_selection(1, 10)); // Same line as start
        assert!(selection.is_point_in_selection(2, 0)); // Middle line
        assert!(selection.is_point_in_selection(3, 1)); // End line
        assert!(!selection.is_point_in_selection(3, 2)); // After end point
        assert!(!selection.is_point_in_selection(0, 0)); // Before start
        assert!(!selection.is_point_in_selection(4, 0)); // After end
    }

    #[test]
    fn test_extract_selected_text() {
        let mut selection = TextSelection::new();
        let lines = vec![
            "First line".to_string(),
            "Second line with more text".to_string(),
            "Third line".to_string(),
        ];

        // Single line selection
        selection.start_selection(1, 7);
        selection.update_selection(1, 12);

        let selected = selection.extract_selected_text(&lines).unwrap();
        assert_eq!(selected, "line ");

        // Multi-line selection
        selection.clear_selection();
        selection.start_selection(0, 6);
        selection.update_selection(2, 5);

        let selected = selection.extract_selected_text(&lines).unwrap();
        assert_eq!(selected, "line\nSecond line with more text\nThird");
    }

    #[test]
    fn test_screen_to_text_coords() {
        let selection = TextSelection::new();

        let result = selection.screen_to_text_coords(15, 10, 5, 10, 5);
        assert_eq!(result, Some((10, 5))); // (relative_y + scroll_offset, relative_x)

        let result = selection.screen_to_text_coords(15, 3, 5, 10, 5);
        assert_eq!(result, None);

        let result = selection.screen_to_text_coords(5, 10, 5, 10, 5);
        assert_eq!(result, Some((10, 0))); // Click on left margin -> column 0

        let result = selection.screen_to_text_coords(0, 10, 5, 10, 5);
        assert_eq!(result, Some((10, 0))); // Still column 0
    }
}
