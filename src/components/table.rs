use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};
use std::cmp::max;

use crate::types::LinkInfo;

/// Inline style information for table cell rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
}

/// Structured inline content for table cells
#[derive(Debug, Clone)]
pub enum InlineSpan {
    Text { text: String, style: InlineStyle },
    Link { text: Vec<InlineSpan>, url: String },
    SoftBreak,
    HardBreak,
}

impl InlineSpan {
    pub fn plain<T: Into<String>>(text: T) -> Self {
        Self::Text {
            text: text.into(),
            style: InlineStyle::default(),
        }
    }
}

fn inline_from_plain_text(text: &str) -> Vec<InlineSpan> {
    let mut inlines = Vec::new();
    let normalized = text.replace("<br/> ", "\n").replace("<br/>", "\n");
    let mut parts = normalized.split('\n').peekable();

    while let Some(part) = parts.next() {
        if !part.is_empty() {
            inlines.push(InlineSpan::plain(part));
        }
        if parts.peek().is_some() {
            inlines.push(InlineSpan::HardBreak);
        }
    }

    if inlines.is_empty() {
        inlines.push(InlineSpan::plain(""));
    }

    inlines
}

/// Cell data with content and colspan information
#[derive(Debug, Clone)]
pub struct CellData {
    pub content: Vec<InlineSpan>,
    pub colspan: u32,
}

impl CellData {
    pub fn new(content: Vec<InlineSpan>) -> Self {
        Self {
            content,
            colspan: 1,
        }
    }

    pub fn with_colspan(content: Vec<InlineSpan>, colspan: u32) -> Self {
        Self { content, colspan }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }
}

impl From<String> for CellData {
    fn from(content: String) -> Self {
        Self::new(inline_from_plain_text(&content))
    }
}

impl From<&str> for CellData {
    fn from(content: &str) -> Self {
        Self::new(inline_from_plain_text(content))
    }
}

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
    rows: Vec<Vec<CellData>>,
    header: Option<Vec<CellData>>,
    constraints: Vec<Constraint>,
    config: TableConfig,
    block: Option<Block<'static>>,
    /// Store link information for click handling
    links: Vec<LinkInfo>,
    /// Base line number where this table starts (for absolute positioning)
    base_line: usize,
}

impl Table {
    pub fn new(rows: Vec<Vec<String>>) -> Self {
        let cell_rows: Vec<Vec<CellData>> = rows
            .into_iter()
            .map(|row| row.into_iter().map(CellData::from).collect())
            .collect();

        Self {
            rows: cell_rows,
            header: None,
            constraints: Vec::new(),
            config: TableConfig::default(),
            block: None,
            links: Vec::new(),
            base_line: 0,
        }
    }

    /// Create a table with colspan support
    pub fn new_with_colspans(rows: Vec<Vec<CellData>>) -> Self {
        Self {
            rows,
            header: None,
            constraints: Vec::new(),
            config: TableConfig::default(),
            block: None,
            links: Vec::new(),
            base_line: 0,
        }
    }

    pub fn header(mut self, header: Vec<String>) -> Self {
        let cells = header.into_iter().map(CellData::from).collect();
        self.header = Some(cells);
        self
    }

    /// Set header with colspan support
    pub fn header_with_colspans(mut self, header: Vec<CellData>) -> Self {
        self.header = Some(header);
        self
    }

    /// Set rows with colspan support (for use with Table::new)
    pub fn rows_with_colspans(mut self, rows: Vec<Vec<CellData>>) -> Self {
        self.rows = rows;
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

    pub fn base_line(mut self, base_line: usize) -> Self {
        self.base_line = base_line;
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
    /// If colspans is provided, skip column separators within colspan regions
    fn render_top_border(&self, widths: &[u16], colspans: Option<&[u32]>) -> Line<'static> {
        self.render_horizontal_border(widths, colspans, '┌', '┬', '┐')
    }

    /// Render a horizontal border with colspan support
    fn render_horizontal_border(
        &self,
        widths: &[u16],
        colspans: Option<&[u32]>,
        left: char,
        middle: char,
        right: char,
    ) -> Line<'static> {
        if widths.is_empty() {
            return Line::from("");
        }

        // Build a set of grid column indices where we should NOT draw a separator
        // These are columns that are "inside" a colspan (not the first column of a cell)
        let mut skip_separator_after: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        if let Some(cs) = colspans {
            let mut grid_col = 0;
            for colspan in cs {
                let span = (*colspan).max(1) as usize;
                // Skip separators for columns inside this cell (not the last column of this cell)
                for offset in 0..span.saturating_sub(1) {
                    skip_separator_after.insert(grid_col + offset);
                }
                grid_col += span;
            }
        }

        let mut line = String::new();
        line.push(left);

        for (i, &width) in widths.iter().enumerate() {
            line.push_str(&"─".repeat(width as usize));
            if i < widths.len() - 1 {
                if skip_separator_after.contains(&i) {
                    line.push('─'); // Continue the horizontal line
                } else {
                    line.push(middle);
                }
            }
        }

        line.push(right);
        Line::from(Span::styled(
            line,
            Style::default().fg(self.config.border_color),
        ))
    }

    /// Render middle border (between header and data rows)
    /// Takes colspans from both the row above and the row below to determine separators
    fn render_middle_border(
        &self,
        widths: &[u16],
        colspans_above: Option<&[u32]>,
        colspans_below: Option<&[u32]>,
    ) -> Line<'static> {
        if widths.is_empty() {
            return Line::from("");
        }

        // Build sets of grid columns where separators should be skipped
        let mut skip_above: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut skip_below: std::collections::HashSet<usize> = std::collections::HashSet::new();

        if let Some(cs) = colspans_above {
            let mut grid_col = 0;
            for colspan in cs {
                let span = (*colspan).max(1) as usize;
                for offset in 0..span.saturating_sub(1) {
                    skip_above.insert(grid_col + offset);
                }
                grid_col += span;
            }
        }

        if let Some(cs) = colspans_below {
            let mut grid_col = 0;
            for colspan in cs {
                let span = (*colspan).max(1) as usize;
                for offset in 0..span.saturating_sub(1) {
                    skip_below.insert(grid_col + offset);
                }
                grid_col += span;
            }
        }

        let mut line = String::new();
        line.push('├');

        for (i, &width) in widths.iter().enumerate() {
            line.push_str(&"─".repeat(width as usize));
            if i < widths.len() - 1 {
                let skip_a = skip_above.contains(&i);
                let skip_b = skip_below.contains(&i);
                let ch = match (skip_a, skip_b) {
                    (true, true) => '─',   // Both rows span over this column
                    (true, false) => '┬',  // Row above spans, row below has separator
                    (false, true) => '┴',  // Row below spans, row above has separator
                    (false, false) => '┼', // Both rows have a separator here
                };
                line.push(ch);
            }
        }

        line.push('┤');
        Line::from(Span::styled(
            line,
            Style::default().fg(self.config.border_color),
        ))
    }

    /// Render bottom border
    fn render_bottom_border(&self, widths: &[u16], colspans: Option<&[u32]>) -> Line<'static> {
        self.render_horizontal_border(widths, colspans, '└', '┴', '┘')
    }

    fn style_for_inline(&self, base_color: Color, inline_style: InlineStyle) -> Style {
        let mut style = Style::default().fg(base_color);

        if inline_style.code {
            style = Style::default()
                .fg(Color::Rgb(166, 226, 46))
                .bg(Color::Rgb(50, 50, 50));
        }
        if inline_style.bold {
            style = style.bold();
        }
        if inline_style.italic {
            style = style.italic();
        }
        if inline_style.strike {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }

        style
    }

    fn inline_to_spans_flat(
        &self,
        inlines: &[InlineSpan],
        base_color: Color,
        link_style: bool,
    ) -> Vec<Span<'static>> {
        let mut spans = Vec::new();

        for inline in inlines {
            match inline {
                InlineSpan::Text { text, style } => {
                    let mut span_style = self.style_for_inline(base_color, *style);
                    if link_style {
                        span_style = span_style
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::UNDERLINED);
                    }
                    spans.push(Span::styled(text.clone(), span_style));
                }
                InlineSpan::Link { text, .. } => {
                    spans.extend(self.inline_to_spans_flat(text, base_color, true));
                }
                InlineSpan::SoftBreak | InlineSpan::HardBreak => {
                    spans.push(Span::styled(
                        " ".to_string(),
                        Style::default().fg(base_color),
                    ));
                }
            }
        }

        spans
    }

    fn inline_to_lines(
        &self,
        inlines: &[InlineSpan],
        base_color: Color,
    ) -> Vec<Vec<Span<'static>>> {
        let mut lines = Vec::new();
        let mut current_line = Vec::new();

        for inline in inlines {
            match inline {
                InlineSpan::HardBreak => {
                    lines.push(current_line);
                    current_line = Vec::new();
                }
                InlineSpan::SoftBreak => {
                    current_line.push(Span::styled(
                        " ".to_string(),
                        Style::default().fg(base_color),
                    ));
                }
                InlineSpan::Text { .. } | InlineSpan::Link { .. } => {
                    current_line.extend(self.inline_to_spans_flat(
                        std::slice::from_ref(inline),
                        base_color,
                        false,
                    ));
                }
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        if lines.is_empty() {
            lines.push(vec![Span::styled(
                String::new(),
                Style::default().fg(base_color),
            )]);
        }

        lines
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
            let span_width = span_content.chars().count();

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
                    current_line.push(Span::styled(chunk, span.style));

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

                        let chunk_display_width = chunk.chars().count();
                        current_line.push(Span::styled(chunk, span.style));

                        if start + width < chars.len() {
                            result.push(current_line.clone());
                            current_line.clear();
                            current_width = 0;
                        } else {
                            current_width = chunk_display_width;
                        }

                        start = end;
                    }
                }
            }
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }

        if result.is_empty() {
            result.push(vec![Span::styled(
                String::new(),
                Style::default().fg(base_color),
            )]);
        }

        result
    }

    /// Render a data row with proper cell formatting, wrapping, and colspan support
    fn render_row(&self, row: &[CellData], widths: &[u16], is_header: bool) -> Vec<Line<'static>> {
        if widths.is_empty() || row.is_empty() {
            return vec![Line::from("")];
        }

        let text_color = if is_header {
            self.config.header_color
        } else {
            self.config.text_color
        };

        // Calculate effective width for each cell (accounting for colspan)
        // Also track which grid columns each cell occupies
        let mut cell_widths: Vec<usize> = Vec::new();
        let mut grid_col = 0;

        for cell in row {
            let colspan = cell.colspan.max(1) as usize;

            // Calculate total width for this cell (sum of spanned columns + separators between them)
            let mut total_width = 0usize;
            for offset in 0..colspan {
                if grid_col + offset < widths.len() {
                    total_width += widths[grid_col + offset] as usize;
                    if offset > 0 {
                        total_width += 1; // Add 1 for the separator we're spanning over
                    }
                }
            }

            cell_widths.push(total_width);
            grid_col += colspan;
        }

        // Wrap each cell content and find the maximum height
        let mut wrapped_cells: Vec<Vec<Vec<Span<'static>>>> = Vec::new();
        let mut max_height = 1;

        for (cell_idx, cell) in row.iter().enumerate() {
            let width = cell_widths.get(cell_idx).copied().unwrap_or(0);
            if width == 0 {
                wrapped_cells.push(vec![vec![]]);
                continue;
            }

            let base_lines = self.inline_to_lines(&cell.content, text_color);
            let mut all_wrapped_lines = Vec::new();

            for line_spans in base_lines {
                let display_width: usize =
                    line_spans.iter().map(|s| s.content.chars().count()).sum();
                if display_width <= width {
                    all_wrapped_lines.push(line_spans);
                } else {
                    let wrapped_spans =
                        self.wrap_spans_with_formatting(&line_spans, width, text_color);
                    for wrapped_line in wrapped_spans {
                        all_wrapped_lines.push(wrapped_line);
                    }
                }
            }

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

            for (cell_idx, cell_lines) in wrapped_cells.iter().enumerate() {
                let width = cell_widths[cell_idx];
                let cell_spans = cell_lines.get(line_idx).cloned().unwrap_or_default();
                let spans_width: usize = cell_spans.iter().map(|s| s.content.chars().count()).sum();

                if spans_width <= width {
                    for span in cell_spans {
                        line_spans.push(span);
                    }
                    if spans_width < width {
                        line_spans.push(Span::styled(
                            " ".repeat(width - spans_width),
                            Style::default().fg(text_color),
                        ));
                    }
                } else {
                    let mut remaining_width = width;
                    for span in cell_spans {
                        if remaining_width == 0 {
                            break;
                        }
                        let span_display_width = span.content.chars().count();
                        if span_display_width <= remaining_width {
                            line_spans.push(span);
                            remaining_width -= span_display_width;
                        } else if remaining_width > 0 {
                            let truncated_content: String =
                                span.content.chars().take(remaining_width).collect();
                            let truncated_width = truncated_content.chars().count();
                            line_spans.push(Span::styled(truncated_content, span.style));
                            remaining_width -= truncated_width;
                        }
                    }
                }

                // Column separator - only add if this is not the last cell
                if cell_idx < wrapped_cells.len() - 1 {
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

        // Top border - use header colspans if header exists, otherwise first row
        let header_colspans = self.header.as_ref().map(|h| {
            h.iter()
                .map(|cell| cell.colspan.max(1))
                .collect::<Vec<u32>>()
        });
        let first_row_colspans = self.rows.first().map(|row| {
            row.iter()
                .map(|cell| cell.colspan.max(1))
                .collect::<Vec<u32>>()
        });

        let top_colspans = if self.header.is_some() {
            header_colspans.as_deref()
        } else {
            first_row_colspans.as_deref()
        };
        lines.push(self.render_top_border(&widths, top_colspans));

        // Header if present
        if let Some(ref header) = self.header {
            let header_lines = self.render_row(header, &widths, true);
            lines.extend(header_lines);

            // Middle border between header and first data row
            lines.push(self.render_middle_border(
                &widths,
                header_colspans.as_deref(),
                first_row_colspans.as_deref(),
            ));
        }

        // Data rows
        for row in &self.rows {
            let row_lines = self.render_row(row, &widths, false);
            lines.extend(row_lines);
        }

        // Bottom border - use last row's colspans
        let bottom_colspans = self.rows.last().map(|row| {
            row.iter()
                .map(|cell| cell.colspan.max(1))
                .collect::<Vec<u32>>()
        });
        lines.push(self.render_bottom_border(&widths, bottom_colspans.as_deref()));

        lines
    }

    /// Get all links in this table
    pub fn get_links(&self) -> &Vec<LinkInfo> {
        &self.links
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

        assert_eq!(table.rows.len(), rows.len());
        assert_eq!(table.rows[0].len(), rows[0].len());
        match &table.rows[0][0].content[0] {
            InlineSpan::Text { text, .. } => assert_eq!(text, "Cell 1"),
            _ => panic!("Expected text span in first cell"),
        }
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

        assert!(table.header.is_some());
        let header_cells = table.header.as_ref().unwrap();
        assert_eq!(header_cells.len(), header.len());
        match &header_cells[0].content[0] {
            InlineSpan::Text { text, .. } => assert_eq!(text, "Header 1"),
            _ => panic!("Expected text span in header cell"),
        }
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
        let bold = InlineStyle {
            bold: true,
            ..Default::default()
        };
        let italic = InlineStyle {
            italic: true,
            ..Default::default()
        };

        let rows = vec![
            vec![
                CellData::new(vec![InlineSpan::Text {
                    text: "This is a very long bold text that should wrap across multiple lines"
                        .to_string(),
                    style: bold,
                }]),
                CellData::new(vec![InlineSpan::Text {
                    text: "This is a very long italic text that should also wrap".to_string(),
                    style: italic,
                }]),
            ],
            vec![
                CellData::new(vec![
                    InlineSpan::Text {
                        text: "Mixed ".to_string(),
                        style: InlineStyle::default(),
                    },
                    InlineSpan::Text {
                        text: "bold".to_string(),
                        style: bold,
                    },
                    InlineSpan::Text {
                        text: " and ".to_string(),
                        style: InlineStyle::default(),
                    },
                    InlineSpan::Text {
                        text: "italic".to_string(),
                        style: italic,
                    },
                    InlineSpan::Text {
                        text: " in a very long line that needs wrapping".to_string(),
                        style: InlineStyle::default(),
                    },
                ]),
                CellData::from("Normal text that is also quite long and should wrap properly"),
            ],
        ];

        let table = Table::new_with_colspans(rows).constraints(vec![
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
        let bold = InlineStyle {
            bold: true,
            ..Default::default()
        };
        let italic = InlineStyle {
            italic: true,
            ..Default::default()
        };

        let rows = vec![
            vec![
                CellData::new(vec![InlineSpan::Text {
                    text: "Bold text".to_string(),
                    style: bold,
                }]),
                CellData::new(vec![InlineSpan::Text {
                    text: "Italic text".to_string(),
                    style: italic,
                }]),
            ],
            vec![
                CellData::new(vec![InlineSpan::Text {
                    text: "Also italic".to_string(),
                    style: italic,
                }]),
                CellData::new(vec![InlineSpan::Text {
                    text: "Also bold".to_string(),
                    style: bold,
                }]),
            ],
            vec![
                CellData::new(vec![
                    InlineSpan::Text {
                        text: "Mixed ".to_string(),
                        style: InlineStyle::default(),
                    },
                    InlineSpan::Text {
                        text: "bold".to_string(),
                        style: bold,
                    },
                    InlineSpan::Text {
                        text: " and ".to_string(),
                        style: InlineStyle::default(),
                    },
                    InlineSpan::Text {
                        text: "italic".to_string(),
                        style: italic,
                    },
                ]),
                CellData::from("Normal text"),
            ],
        ];

        let table = Table::new_with_colspans(rows)
            .constraints(vec![Constraint::Length(25), Constraint::Length(15)]);

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
