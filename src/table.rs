use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};
use std::cmp::max;

/// Configuration for table appearance
#[derive(Debug, Clone)]
pub struct TableConfig {
    pub border_color: Color,
    pub header_color: Color,
    pub text_color: Color,
    pub use_block: bool,
}

impl Default for TableConfig {
    fn default() -> Self {
        Self {
            border_color: Color::White,
            header_color: Color::Yellow,
            text_color: Color::White,
            use_block: false,
        }
    }
}

/// A custom table widget that renders with solid Unicode box-drawing characters
#[derive(Debug, Clone)]
pub struct Table {
    rows: Vec<Vec<String>>,
    header: Option<Vec<String>>,
    constraints: Vec<Constraint>,
    config: TableConfig,
    block: Option<Block<'static>>,
}

impl Table {
    pub fn new(rows: Vec<Vec<String>>) -> Self {
        Self {
            rows,
            header: None,
            constraints: Vec::new(),
            config: TableConfig::default(),
            block: None,
        }
    }

    pub fn header(mut self, header: Vec<String>) -> Self {
        self.header = Some(header);
        self
    }

    pub fn constraints(mut self, constraints: Vec<Constraint>) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn config(mut self, config: TableConfig) -> Self {
        self.config = config;
        self
    }

    pub fn block(mut self, block: Block<'static>) -> Self {
        self.block = Some(block);
        self
    }

    /// Calculate column widths based on constraints and available space
    fn calculate_column_widths(&self, available_width: u16) -> Vec<u16> {
        let num_cols = self.constraints.len();
        if num_cols == 0 {
            return Vec::new();
        }

        // Account for borders: left border (1) + column separators (num_cols - 1) + right border (1)
        let border_width = 1 + (num_cols - 1) + 1;
        let content_width = available_width.saturating_sub(border_width as u16);

        let mut widths = Vec::new();
        let mut remaining_width = content_width;
        let mut length_constraints = Vec::new();

        // First pass: handle Length constraints
        for constraint in &self.constraints {
            match constraint {
                Constraint::Length(len) => {
                    let width = (*len).min(remaining_width);
                    widths.push(width);
                    remaining_width = remaining_width.saturating_sub(width);
                    length_constraints.push(None);
                }
                _ => {
                    widths.push(0);
                    length_constraints.push(Some(constraint));
                }
            }
        }

        // Second pass: distribute remaining width among percentage/ratio constraints
        let flexible_count = length_constraints.iter().filter(|c| c.is_some()).count();
        if flexible_count > 0 && remaining_width > 0 {
            let width_per_flexible = remaining_width / flexible_count as u16;
            let mut extra = remaining_width % flexible_count as u16;

            for (i, constraint_opt) in length_constraints.iter().enumerate() {
                if constraint_opt.is_some() {
                    let mut width = width_per_flexible;
                    if extra > 0 {
                        width += 1;
                        extra -= 1;
                    }
                    widths[i] = width;
                }
            }
        }

        widths
    }

    /// Render top border with proper Unicode box-drawing characters
    fn render_top_border(&self, widths: &[u16]) -> Line<'static> {
        if widths.is_empty() {
            return Line::from("");
        }

        let mut line = String::new();
        line.push('┌'); // Top-left corner

        for (i, &width) in widths.iter().enumerate() {
            line.push_str(&"─".repeat(width as usize));
            if i < widths.len() - 1 {
                line.push('┬'); // Top tee
            }
        }

        line.push('┐'); // Top-right corner
        Line::from(Span::styled(
            line,
            Style::default().fg(self.config.border_color),
        ))
    }

    /// Render middle border (between header and data rows)
    fn render_middle_border(&self, widths: &[u16]) -> Line<'static> {
        if widths.is_empty() {
            return Line::from("");
        }

        let mut line = String::new();
        line.push('├'); // Left tee

        for (i, &width) in widths.iter().enumerate() {
            line.push_str(&"─".repeat(width as usize));
            if i < widths.len() - 1 {
                line.push('┼'); // Cross
            }
        }

        line.push('┤'); // Right tee
        Line::from(Span::styled(
            line,
            Style::default().fg(self.config.border_color),
        ))
    }

    /// Render bottom border
    fn render_bottom_border(&self, widths: &[u16]) -> Line<'static> {
        if widths.is_empty() {
            return Line::from("");
        }

        let mut line = String::new();
        line.push('└'); // Bottom-left corner

        for (i, &width) in widths.iter().enumerate() {
            line.push_str(&"─".repeat(width as usize));
            if i < widths.len() - 1 {
                line.push('┴'); // Bottom tee
            }
        }

        line.push('┘'); // Bottom-right corner
        Line::from(Span::styled(
            line,
            Style::default().fg(self.config.border_color),
        ))
    }

    /// Wrap spans while preserving formatting
    fn wrap_spans_with_formatting(
        &self,
        spans: &[Span<'static>],
        width: usize,
        base_color: Color,
    ) -> Vec<Vec<Span<'static>>> {
        let mut result = Vec::new();
        let mut current_line = Vec::new();
        let mut current_width = 0;

        for span in spans {
            let span_content = span.content.as_ref();
            let span_width = span_content.len();

            if current_width + span_width <= width {
                // Span fits on current line
                current_line.push(span.clone());
                current_width += span_width;
            } else if current_width == 0 && span_width > width {
                // Single span that's too wide - need to break it
                let chars: Vec<char> = span_content.chars().collect();
                let mut start = 0;

                while start < chars.len() {
                    let end = (start + width).min(chars.len());
                    let chunk: String = chars[start..end].iter().collect();

                    current_line.push(Span::styled(chunk, span.style.clone()));

                    if !current_line.is_empty() {
                        result.push(current_line.clone());
                        current_line.clear();
                    }

                    start = end;
                }
                current_width = 0;
            } else {
                // Current line is full, start new line
                if !current_line.is_empty() {
                    result.push(current_line.clone());
                    current_line.clear();
                }

                // Add the span to the new line
                if span_width <= width {
                    current_line.push(span.clone());
                    current_width = span_width;
                } else {
                    // Span is too wide, break it as above
                    let chars: Vec<char> = span_content.chars().collect();
                    let mut start = 0;

                    while start < chars.len() {
                        let end = (start + width).min(chars.len());
                        let chunk: String = chars[start..end].iter().collect();

                        let chunk_len = chunk.len();
                        current_line.push(Span::styled(chunk, span.style.clone()));

                        if start + width < chars.len() {
                            // More content to come, finish this line
                            result.push(current_line.clone());
                            current_line.clear();
                            current_width = 0;
                        } else {
                            // This is the last chunk
                            current_width = chunk_len;
                        }

                        start = end;
                    }
                }
            }
        }

        // Add any remaining content
        if !current_line.is_empty() {
            result.push(current_line);
        }

        // Ensure we have at least one line
        if result.is_empty() {
            result.push(vec![Span::styled(
                String::new(),
                Style::default().fg(base_color),
            )]);
        }

        result
    }

    /// Parse markdown formatting in text and return styled spans
    fn parse_markdown_formatting(&self, text: &str, base_color: Color) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut current_text = String::new();

        while i < chars.len() {
            // Check for bold (**text** or __text__)
            if i + 1 < chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_'))
            {
                // Save any accumulated text
                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(base_color),
                    ));
                    current_text.clear();
                }

                let marker = chars[i];
                i += 2; // Skip opening markers

                // Find closing markers
                let mut j = i;
                while j + 1 < chars.len() {
                    if chars[j] == marker && chars[j + 1] == marker {
                        // Found closing markers
                        let bold_text: String = chars[i..j].iter().collect();
                        spans.push(Span::styled(
                            bold_text,
                            Style::default().fg(base_color).bold(),
                        ));
                        i = j + 2;
                        break;
                    }
                    j += 1;
                }

                if j + 1 >= chars.len() {
                    // No closing markers found, treat as normal text
                    current_text.push(marker);
                    current_text.push(marker);
                }
            }
            // Check for italic (*text* or _text_) - but not bold
            else if (chars[i] == '*' || chars[i] == '_')
                && (i == 0 || (i > 0 && chars[i - 1] != chars[i]))
                && (i + 1 < chars.len() && chars[i + 1] != chars[i])
            {
                // Save any accumulated text
                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(base_color),
                    ));
                    current_text.clear();
                }

                let marker = chars[i];
                i += 1; // Skip opening marker

                // Find closing marker
                let mut j = i;
                while j < chars.len() {
                    if chars[j] == marker && (j + 1 >= chars.len() || chars[j + 1] != marker) {
                        // Found closing marker
                        let italic_text: String = chars[i..j].iter().collect();
                        spans.push(Span::styled(
                            italic_text,
                            Style::default().fg(base_color).italic(),
                        ));
                        i = j + 1;
                        break;
                    }
                    j += 1;
                }

                if j >= chars.len() {
                    // No closing marker found, treat as normal text
                    current_text.push(marker);
                }
            } else {
                current_text.push(chars[i]);
                i += 1;
            }
        }

        // Add any remaining text
        if !current_text.is_empty() {
            spans.push(Span::styled(current_text, Style::default().fg(base_color)));
        }

        spans
    }

    /// Render a data row with proper cell formatting and wrapping
    fn render_row(&self, row: &[String], widths: &[u16], is_header: bool) -> Vec<Line<'static>> {
        if widths.is_empty() || row.is_empty() {
            return vec![Line::from("")];
        }

        let text_color = if is_header {
            self.config.header_color
        } else {
            self.config.text_color
        };

        // Wrap each cell content and find the maximum height
        let mut wrapped_cells: Vec<Vec<Vec<Span<'static>>>> = Vec::new();
        let mut max_height = 1;

        for (i, cell) in row.iter().enumerate() {
            let width = widths.get(i).copied().unwrap_or(0) as usize;
            if width == 0 {
                wrapped_cells.push(vec![vec![]]);
                continue;
            }

            // First process <br/> tags by replacing them with actual newlines
            let cell_with_newlines = cell.replace("<br/> ", "\n").replace("<br/>", "\n");
            let cell_lines_from_br: Vec<&str> = cell_with_newlines.split('\n').collect();

            // Then wrap each line separately and parse markdown
            let mut all_wrapped_lines = Vec::new();
            for br_line in cell_lines_from_br {
                // Parse markdown formatting first
                let spans = self.parse_markdown_formatting(br_line, text_color);

                // Calculate the actual display width of the parsed spans
                let display_width: usize = spans.iter().map(|s| s.content.len()).sum();

                if display_width <= width {
                    // Fits in one line, use the parsed spans directly
                    all_wrapped_lines.push(spans);
                } else {
                    // Need to wrap - use the new method that preserves formatting
                    let wrapped_spans = self.wrap_spans_with_formatting(&spans, width, text_color);
                    for line_spans in wrapped_spans {
                        all_wrapped_lines.push(line_spans);
                    }
                }
            }

            // If we had no content, ensure at least one empty line
            if all_wrapped_lines.is_empty() {
                all_wrapped_lines.push(vec![Span::styled(
                    String::new(),
                    Style::default().fg(text_color),
                )]);
            }

            max_height = max(max_height, all_wrapped_lines.len());
            wrapped_cells.push(all_wrapped_lines);
        }

        // Render each line of the row
        let mut lines = Vec::new();
        for line_idx in 0..max_height {
            let mut line_spans = Vec::new();

            // Left border
            line_spans.push(Span::styled(
                "│".to_string(),
                Style::default().fg(self.config.border_color),
            ));

            for (col_idx, cell_lines) in wrapped_cells.iter().enumerate() {
                let width = widths[col_idx] as usize;
                let cell_spans = cell_lines.get(line_idx).cloned().unwrap_or_default();

                // Calculate the actual width of the spans
                let spans_width: usize = cell_spans.iter().map(|s| s.content.len()).sum();

                // Ensure we don't exceed the column width
                if spans_width <= width {
                    // Add the cell spans
                    for span in cell_spans {
                        line_spans.push(span);
                    }
                    
                    // Pad to fill the column width
                    if spans_width < width {
                        line_spans.push(Span::styled(
                            " ".repeat(width - spans_width),
                            Style::default().fg(text_color),
                        ));
                    }
                } else {
                    // Spans exceed width - truncate to fit exactly
                    let mut remaining_width = width;
                    
                    for span in cell_spans {
                        if remaining_width == 0 {
                            break;
                        }
                        
                        let span_len = span.content.len();
                        if span_len <= remaining_width {
                            line_spans.push(span);
                            remaining_width -= span_len;
                        } else if remaining_width > 0 {
                            // Truncate this span to fit
                            let truncated_content: String = span.content.chars().take(remaining_width).collect();
                            line_spans.push(Span::styled(
                                truncated_content,
                                span.style.clone(),
                            ));
                            remaining_width = 0;
                        }
                    }
                }

                // Column separator
                if col_idx < wrapped_cells.len() - 1 {
                    line_spans.push(Span::styled(
                        "│".to_string(),
                        Style::default().fg(self.config.border_color),
                    ));
                }
            }

            // Right border
            line_spans.push(Span::styled(
                "│".to_string(),
                Style::default().fg(self.config.border_color),
            ));

            lines.push(Line::from(line_spans));
        }

        lines
    }

    /// Calculate the total height the table will occupy
    pub fn calculate_height(&self, available_width: u16) -> u16 {
        let widths = self.calculate_column_widths(available_width);
        let mut height = 0;

        // Top border
        height += 1;

        // Header if present
        if let Some(ref header) = self.header {
            let header_lines = self.render_row(header, &widths, true);
            height += header_lines.len() as u16;
            height += 1; // Middle border
        }

        // Data rows
        for row in &self.rows {
            let row_lines = self.render_row(row, &widths, false);
            height += row_lines.len() as u16;
        }

        // Bottom border
        height += 1;

        height
    }

    /// Render the table into a vector of lines for integration with Paragraph widget
    pub fn render_to_lines(&self, available_width: u16) -> Vec<Line<'static>> {
        self.render_to_lines_with_offset(available_width, 0, None)
    }

    /// Render the table with optional line offset and height limit for scrolling
    pub fn render_to_lines_with_offset(
        &self,
        available_width: u16,
        line_offset: usize,
        max_lines: Option<usize>,
    ) -> Vec<Line<'static>> {
        // First, render all lines normally
        let all_lines = self.render_all_lines(available_width);

        // Then apply offset and limit
        let start_index = line_offset.min(all_lines.len());
        let end_index = if let Some(limit) = max_lines {
            (start_index + limit).min(all_lines.len())
        } else {
            all_lines.len()
        };

        all_lines[start_index..end_index].to_vec()
    }

    /// Render all table lines without any offset or limit
    fn render_all_lines(&self, available_width: u16) -> Vec<Line<'static>> {
        let widths = self.calculate_column_widths(available_width);
        let mut lines = Vec::new();

        // Top border
        lines.push(self.render_top_border(&widths));

        // Header if present
        if let Some(ref header) = self.header {
            let header_lines = self.render_row(header, &widths, true);
            lines.extend(header_lines);
            lines.push(self.render_middle_border(&widths));
        }

        // Data rows
        for row in &self.rows {
            let row_lines = self.render_row(row, &widths, false);
            lines.extend(row_lines);
        }

        // Bottom border
        lines.push(self.render_bottom_border(&widths));

        lines
    }
}

impl Widget for Table {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let lines = self.render_to_lines(area.width);

        // Use Paragraph to render the table lines
        let paragraph = ratatui::widgets::Paragraph::new(ratatui::text::Text::from(lines));

        if let Some(block) = self.block {
            paragraph.block(block).render(area, buf);
        } else {
            paragraph.render(area, buf);
        }
    }
}

/// State for stateful table widget (currently minimal, but allows for future extensions)
#[derive(Debug, Default)]
pub struct TableState {
    pub selected_row: Option<usize>,
}

impl StatefulWidget for Table {
    type State = TableState;

    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer, _state: &mut Self::State) {
        // For now, stateful rendering is the same as stateless
        // Future enhancements could include row selection highlighting
        Widget::render(self, area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Constraint;

    #[test]
    fn test_table_creation() {
        let rows = vec![
            vec!["Cell 1".to_string(), "Cell 2".to_string()],
            vec!["Cell 3".to_string(), "Cell 4".to_string()],
        ];

        let table = Table::new(rows.clone())
            .constraints(vec![Constraint::Length(10), Constraint::Length(10)]);

        assert_eq!(table.rows, rows);
        assert_eq!(table.constraints.len(), 2);
        assert!(table.header.is_none());
    }

    #[test]
    fn test_table_with_header() {
        let header = vec!["Header 1".to_string(), "Header 2".to_string()];
        let rows = vec![vec!["Cell 1".to_string(), "Cell 2".to_string()]];

        let table = Table::new(rows)
            .header(header.clone())
            .constraints(vec![Constraint::Length(10), Constraint::Length(10)]);

        assert_eq!(table.header, Some(header));
    }

    #[test]
    fn test_column_width_calculation() {
        let table = Table::new(vec![]).constraints(vec![
            Constraint::Length(10),
            Constraint::Length(15),
            Constraint::Length(5),
        ]);

        // Available width: 40, borders: 1 + 2 + 1 = 4, content: 36
        // Should fit exactly: 10 + 15 + 5 = 30, with 6 remaining distributed
        let widths = table.calculate_column_widths(40);

        assert_eq!(widths.len(), 3);
        assert_eq!(widths[0], 10);
        assert_eq!(widths[1], 15);
        assert_eq!(widths[2], 5);
    }

    #[test]
    fn test_height_calculation() {
        let rows = vec![
            vec![
                "Short".to_string(),
                "This is a longer text that will wrap".to_string(),
            ],
            vec!["Row 2".to_string(), "Simple".to_string()],
        ];

        let table = Table::new(rows)
            .header(vec!["Col 1".to_string(), "Col 2".to_string()])
            .constraints(vec![Constraint::Length(8), Constraint::Length(12)]);

        let height = table.calculate_height(30);

        // Should include: top border (1) + header (1) + middle border (1) + data rows (varies based on wrapping) + bottom border (1)
        assert!(height >= 5); // Minimum expected height
    }

    #[test]
    fn test_render_to_lines() {
        let rows = vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["C".to_string(), "D".to_string()],
        ];

        let table =
            Table::new(rows).constraints(vec![Constraint::Length(3), Constraint::Length(3)]);

        let lines = table.render_to_lines(20);

        // Should have at least: top border + 2 data rows + bottom border = 4 lines
        assert!(lines.len() >= 4);

        // First line should be top border
        let first_line_content = &lines[0].spans[0].content;
        assert!(first_line_content.contains('┌'));
        assert!(first_line_content.contains('┐'));
    }

    #[test]
    fn test_unicode_borders() {
        let table =
            Table::new(vec![vec!["Test".to_string()]]).constraints(vec![Constraint::Length(5)]);

        let lines = table.render_to_lines(15);

        // Check that we're using proper Unicode box-drawing characters
        let top_border = &lines[0].spans[0].content;
        assert!(top_border.contains('┌')); // Top-left corner
        assert!(top_border.contains('─')); // Horizontal line
        assert!(top_border.contains('┐')); // Top-right corner

        let bottom_border = &lines[lines.len() - 1].spans[0].content;
        assert!(bottom_border.contains('└')); // Bottom-left corner
        assert!(bottom_border.contains('┘')); // Bottom-right corner
    }

    #[test]
    fn test_table_with_wrapping_content() {
        let rows = vec![
            vec!["Short".to_string(), "This is a very long text that should wrap across multiple lines when rendered in a narrow column".to_string()],
            vec!["Row 2".to_string(), "Another long text that will also wrap and demonstrate how table height is calculated".to_string()],
        ];

        let table = Table::new(rows)
            .header(vec!["Col 1".to_string(), "Col 2".to_string()])
            .constraints(vec![Constraint::Length(8), Constraint::Length(20)]);

        // Test with narrow width to force wrapping
        let height_narrow = table.calculate_height(35);

        // Test with wide width - should be shorter due to less wrapping
        let height_wide = table.calculate_height(80);

        // Height with narrow width should be greater than height with wide width
        // because narrow columns cause more text wrapping
        assert!(height_narrow >= height_wide);

        // Should be at least the minimum (top border + header + middle border + 2 data rows + bottom border)
        assert!(height_narrow >= 5);
        assert!(height_wide >= 5);
    }

    #[test]
    fn test_table_with_br_tags() {
        let rows = vec![
            vec![
                "Cell with<br/>line break".to_string(),
                "Normal cell".to_string(),
            ],
            vec![
                "Another<br/>multi<br/>line".to_string(),
                "Simple".to_string(),
            ],
        ];

        let table =
            Table::new(rows).constraints(vec![Constraint::Length(15), Constraint::Length(10)]);

        let height = table.calculate_height(30);

        // Should account for line breaks in cells
        // Top border (1) + first row with <br/> (2) + second row with 2 <br/> (3) + bottom border (1) = 7
        assert!(height >= 7);
    }

    #[test]
    fn test_table_with_newlines() {
        let rows = vec![
            vec![
                "Cell with\nactual newline".to_string(),
                "Normal cell".to_string(),
            ],
            vec!["Another\nmulti\nline".to_string(), "Simple".to_string()],
        ];

        let table =
            Table::new(rows).constraints(vec![Constraint::Length(15), Constraint::Length(10)]);

        let height = table.calculate_height(30);

        // Should account for newlines in cells
        // Top border (1) + first row with newline (2) + second row with 2 newlines (3) + bottom border (1) = 7
        assert!(height >= 7);

        let lines = table.render_to_lines(30);

        // Verify the content shows up correctly (should split on newlines)
        let content_lines: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        // Should contain the split content
        let full_content = content_lines.join("");
        assert!(full_content.contains("Cell with"));
        assert!(full_content.contains("actual newline"));
        assert!(full_content.contains("multi"));
    }

    #[test]
    fn test_table_with_markdown_formatting_and_wrapping() {
        let rows = vec![
            vec![
                "**This is a very long bold text that should wrap across multiple lines**"
                    .to_string(),
                "_This is a very long italic text that should also wrap_".to_string(),
            ],
            vec![
                "Mixed **bold** and _italic_ in a very long line that needs wrapping".to_string(),
                "Normal text that is also quite long and should wrap properly".to_string(),
            ],
        ];

        let table = Table::new(rows).constraints(vec![
            Constraint::Length(25), // Force wrapping
            Constraint::Length(20), // Force wrapping
        ]);

        let lines = table.render_to_lines(50);

        // Verify that the table renders without panic
        assert!(lines.len() >= 4); // At least some content

        // Check that wrapped lines still contain styled spans
        let mut found_bold_spans = 0;
        let mut found_italic_spans = 0;

        for line in &lines {
            for span in &line.spans {
                if span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::BOLD)
                {
                    found_bold_spans += 1;
                }
                if span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::ITALIC)
                {
                    found_italic_spans += 1;
                }
            }
        }

        // Should find bold and italic formatting even after wrapping
        assert!(
            found_bold_spans > 0,
            "Bold formatting should be preserved after wrapping"
        );
        assert!(
            found_italic_spans > 0,
            "Italic formatting should be preserved after wrapping"
        );
    }

    #[test]
    fn test_table_with_markdown_formatting() {
        let rows = vec![
            vec!["**Bold text**".to_string(), "_Italic text_".to_string()],
            vec!["*Also italic*".to_string(), "__Also bold__".to_string()],
            vec![
                "Mixed **bold** and _italic_".to_string(),
                "Normal text".to_string(),
            ],
        ];

        let table =
            Table::new(rows).constraints(vec![Constraint::Length(25), Constraint::Length(15)]);

        let lines = table.render_to_lines(45);

        // Verify that the table renders without panic
        assert!(lines.len() >= 5); // Top border + 3 data rows + bottom border

        // Check that lines contain styled spans
        for line in &lines[1..lines.len() - 1] {
            // Skip borders
            if line.spans.len() > 1 {
                // Data rows should have multiple spans due to formatting
                assert!(line.spans.len() >= 3); // At least border spans + content
            }
        }
    }

    #[test]
    fn test_table_scrolling_with_offset() {
        let rows = vec![
            vec![
                "Row 1 Col 1".to_string(),
                "Row 1<br/>with break".to_string(),
            ],
            vec!["Row 2".to_string(), "Simple".to_string()],
            vec![
                "Row 3 with<br/>multiple<br/>breaks".to_string(),
                "Col 2".to_string(),
            ],
        ];

        let table = Table::new(rows)
            .header(vec!["Header 1".to_string(), "Header 2".to_string()])
            .constraints(vec![Constraint::Length(15), Constraint::Length(15)]);

        // Render full table
        let all_lines = table.render_to_lines(35);
        let full_height = all_lines.len();

        // Render with offset of 2 lines (should skip top border and first header line)
        let offset_lines = table.render_to_lines_with_offset(35, 2, None);

        // Should have fewer lines
        assert!(offset_lines.len() < full_height);
        assert_eq!(offset_lines.len(), full_height - 2);

        // Render with offset and limit
        let limited_lines = table.render_to_lines_with_offset(35, 1, Some(3));
        assert_eq!(limited_lines.len(), 3);

        // Verify that the content is different (offset should show different lines)
        if full_height > 3 {
            let first_3_lines = &all_lines[0..3];
            let offset_3_lines = &all_lines[1..4];
            assert_ne!(
                first_3_lines
                    .iter()
                    .map(|l| &l.spans[0].content)
                    .collect::<Vec<_>>(),
                offset_3_lines
                    .iter()
                    .map(|l| &l.spans[0].content)
                    .collect::<Vec<_>>()
            );
        }
    }
}
