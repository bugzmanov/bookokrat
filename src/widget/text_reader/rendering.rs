use super::types::*;
use crate::comments::Comment;
use crate::markdown::{
    Block as MarkdownBlock, Document, HeadingLevel, Inline, Node, Style, Text as MarkdownText,
    TextOrInline,
};
use crate::theme::{Base16Palette, theme_background};
use crate::types::LinkInfo;
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style as RatatuiStyle},
    text::{Line, Span},
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderContext {
    TopLevel,
    InsideContainer,
}

#[allow(dead_code)]
pub struct RenderingContext {
    pub raw_text_lines: Vec<String>,
    pub anchor_positions: HashMap<String, usize>,
    pub links: Vec<LinkInfo>,
}

#[allow(dead_code)]
impl RenderingContext {
    pub fn new() -> Self {
        Self {
            raw_text_lines: Vec::new(),
            anchor_positions: HashMap::new(),
            links: Vec::new(),
        }
    }
}

impl crate::markdown_text_reader::MarkdownTextReader {
    fn last_line_has_content(lines: &[RenderedLine]) -> bool {
        lines
            .last()
            .map(|line| !line.raw_text.trim().is_empty())
            .unwrap_or(false)
    }

    /// Check if a Text contains an image and return its URL if found
    fn extract_image_from_text(text: &MarkdownText) -> Option<String> {
        for item in text.iter() {
            if let TextOrInline::Inline(Inline::Image { url, .. }) = item {
                return Some(url.clone());
            }
        }
        None
    }

    fn content_has_image(content: &MarkdownText) -> bool {
        content.iter().any(|item| match item {
            TextOrInline::Inline(Inline::Image { .. }) => true,
            TextOrInline::Inline(Inline::Link { text, .. }) => {
                Self::extract_image_from_text(text).is_some()
            }
            _ => false,
        })
    }

    pub fn render_document_to_lines(
        &mut self,
        doc: &Document,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) -> RenderedContent {
        let mut lines = Vec::new();
        let mut total_height = 0;

        self.raw_text_lines.clear();
        self.anchor_positions.clear();

        // Iterate through all blocks in the document
        for (node_idx, node) in doc.blocks.iter().enumerate() {
            self.extract_and_track_anchors_from_node(node, total_height);

            self.render_node(
                node,
                &mut lines,
                &mut total_height,
                width,
                palette,
                is_focused,
                0,
                Some(node_idx),
                RenderContext::TopLevel,
            );
        }

        self.links.clear();
        for rendered_line in &lines {
            self.links.extend(rendered_line.link_nodes.clone());
        }

        RenderedContent {
            lines,
            total_height,
            generation: self.cache_generation,
        }
    }

    pub fn extract_and_track_anchors_from_node(&mut self, node: &Node, current_line: usize) {
        if let Some(html_id) = &node.id {
            self.anchor_positions.insert(html_id.clone(), current_line);
        }

        match &node.block {
            MarkdownBlock::Heading { content, .. } => {
                if node.id.is_none() {
                    let heading_text = Self::text_to_string(content);
                    let anchor_id = self.generate_heading_anchor(&heading_text);
                    self.anchor_positions.insert(anchor_id, current_line);
                }
                self.extract_inline_anchors_from_text(content, current_line);
            }
            MarkdownBlock::Paragraph { content } => {
                self.extract_inline_anchors_from_text(content, current_line);
            }
            _ => {}
        }
    }

    /// Generate anchor ID from heading text (simplified version)
    pub fn generate_heading_anchor(&self, heading_text: &str) -> String {
        heading_text
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-')
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
    }

    /// Extract inline anchors from text content
    pub fn extract_inline_anchors_from_text(&mut self, text: &MarkdownText, current_line: usize) {
        for item in text.iter() {
            if let TextOrInline::Inline(Inline::Anchor { id }) = item {
                self.anchor_positions.insert(id.clone(), current_line);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_node(
        &mut self,
        node: &Node,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        node_index: Option<usize>,
        context: RenderContext,
    ) {
        use MarkdownBlock::*;

        // Store the current node's anchor to add to the first line rendered for this node
        let mut current_node_anchor = node.id.clone();
        let mut generated_heading_anchor: Option<String> = None;
        let initial_line_count = lines.len();

        // Remember the starting line count to assign node_index to first line only
        let start_lines_count = lines.len();

        match &node.block {
            Heading { level, content } => {
                if current_node_anchor.is_none() {
                    let heading_text = Self::text_to_string(content);
                    generated_heading_anchor = Some(self.generate_heading_anchor(&heading_text));
                }

                self.render_heading(
                    *level,
                    content,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    node_index,
                );
            }

            Paragraph { content } => {
                self.render_paragraph(
                    content,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    indent,
                    node_index,
                    context,
                );
            }

            CodeBlock { language, content } => {
                self.render_code_block(
                    language.as_deref(),
                    content,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    indent,
                    Self::last_line_has_content(lines),
                    node_index,
                );
            }

            List { kind, items } => {
                let mut list_char_pos = 0;
                self.render_list(
                    kind,
                    items,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    indent,
                    node_index,
                    &[],
                    &mut list_char_pos,
                );
            }

            Table {
                header,
                rows,
                alignment,
            } => {
                self.render_table(
                    header,
                    rows,
                    alignment,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    node_index,
                );
            }

            Quote { content } => {
                self.render_quote(
                    content,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    indent,
                    node_index,
                );
            }

            ThematicBreak => {
                self.render_thematic_break(lines, total_height, width, palette, is_focused);
            }

            DefinitionList { items: def_items } => {
                self.render_definition_list(
                    def_items,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    node_index,
                );
            }

            EpubBlock {
                epub_type,
                element_name,
                content,
            } => {
                self.render_epub_block(
                    epub_type,
                    element_name,
                    content,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                );
            }
        }

        if current_node_anchor.is_none() {
            current_node_anchor = generated_heading_anchor;
        }

        if let Some(anchor) = current_node_anchor {
            if initial_line_count < lines.len() {
                if let Some(line) = lines.get_mut(initial_line_count) {
                    line.node_anchor = Some(anchor);
                }
            }
        }

        if let Some(idx) = node_index {
            if start_lines_count < lines.len() {
                if let Some(line) = lines.get_mut(start_lines_count) {
                    line.node_index = Some(idx);
                }
            }
        }
    }

    // Helper method to convert Text AST to plain string
    pub fn text_to_string(text: &MarkdownText) -> String {
        let mut result = String::new();
        for item in text.iter() {
            match item {
                TextOrInline::Text(text_node) => {
                    result.push_str(&text_node.content);
                }
                TextOrInline::Inline(inline) => match inline {
                    Inline::Link {
                        text: link_text, ..
                    } => {
                        result.push_str(&Self::text_to_string(link_text));
                    }
                    Inline::Image { alt_text, .. } => {
                        result.push_str(alt_text);
                    }
                    Inline::Anchor { .. } => {
                        // Anchors don't contribute to text content
                    }
                    Inline::LineBreak => {
                        result.push('\n');
                    }
                    Inline::SoftBreak => {
                        result.push(' ');
                    }
                },
            }
        }
        result
    }

    fn text_or_inline_canonical_len(item: &TextOrInline) -> usize {
        match item {
            TextOrInline::Text(t) => t.content.chars().count(),
            TextOrInline::Inline(inline) => match inline {
                Inline::Link { text, .. } => Self::text_to_string(text).chars().count(),
                Inline::Image { alt_text, .. } => alt_text.chars().count(),
                Inline::Anchor { .. } => 0,
                Inline::LineBreak => 1,
                Inline::SoftBreak => 1,
            },
        }
    }

    /// Convert MarkdownText to structured inline spans
    pub fn text_to_inline_spans(text: &MarkdownText) -> Vec<crate::table::InlineSpan> {
        let mut result = Vec::new();
        let mut after_hard_break = false;

        fn trim_trailing_space_in_spans(spans: &mut [crate::table::InlineSpan]) {
            for span in spans.iter_mut().rev() {
                match span {
                    crate::table::InlineSpan::Text { text, .. } => {
                        while text.ends_with(' ') {
                            text.pop();
                        }
                        break;
                    }
                    crate::table::InlineSpan::Link { text, .. } => {
                        trim_trailing_space_in_spans(text);
                        break;
                    }
                    crate::table::InlineSpan::SoftBreak => {
                        continue;
                    }
                    crate::table::InlineSpan::HardBreak => {
                        break;
                    }
                }
            }
        }

        fn trim_leading_space_once(value: String, trim: &mut bool) -> String {
            if *trim && value.starts_with(' ') {
                *trim = false;
                value[1..].to_string()
            } else {
                *trim = false;
                value
            }
        }

        fn trim_leading_space_inlines(
            inlines: Vec<crate::table::InlineSpan>,
        ) -> Vec<crate::table::InlineSpan> {
            let mut trimmed = Vec::new();
            let mut applied = false;
            for inline in inlines {
                if applied {
                    trimmed.push(inline);
                    continue;
                }
                match inline {
                    crate::table::InlineSpan::Text { text, style } => {
                        let value = text.strip_prefix(' ').unwrap_or(&text).to_string();
                        trimmed.push(crate::table::InlineSpan::Text { text: value, style });
                        applied = true;
                    }
                    crate::table::InlineSpan::Link { text, url } => {
                        let link_text = trim_leading_space_inlines(text);
                        trimmed.push(crate::table::InlineSpan::Link {
                            text: link_text,
                            url,
                        });
                        applied = true;
                    }
                    other => {
                        trimmed.push(other);
                    }
                }
            }
            trimmed
        }

        for item in text.iter() {
            match item {
                TextOrInline::Text(text_node) => {
                    let mut style = crate::table::InlineStyle::default();
                    match &text_node.style {
                        Some(Style::Strong) => style.bold = true,
                        Some(Style::Emphasis) => style.italic = true,
                        Some(Style::Code) => style.code = true,
                        Some(Style::Strikethrough) => style.strike = true,
                        None => {}
                    }
                    let content = text_node.content.as_str();
                    if content.contains("<br/>") {
                        let parts: Vec<&str> = content.split("<br/>").collect();
                        for (idx, part) in parts.iter().enumerate() {
                            let mut slice = *part;
                            if idx + 1 < parts.len() {
                                slice = slice.trim_end();
                            }
                            if idx > 0 {
                                slice = slice.trim_start();
                            }
                            if !slice.is_empty() {
                                let mut trim = after_hard_break;
                                result.push(crate::table::InlineSpan::Text {
                                    text: trim_leading_space_once(slice.to_string(), &mut trim),
                                    style,
                                });
                                after_hard_break = trim;
                            }
                            if idx + 1 < parts.len() {
                                trim_trailing_space_in_spans(&mut result);
                                result.push(crate::table::InlineSpan::HardBreak);
                                after_hard_break = true;
                            }
                        }
                    } else {
                        let mut trim = after_hard_break;
                        let content = trim_leading_space_once(text_node.content.clone(), &mut trim);
                        after_hard_break = trim;
                        result.push(crate::table::InlineSpan::Text {
                            text: content,
                            style,
                        });
                    }
                }
                TextOrInline::Inline(inline) => match inline {
                    Inline::Link {
                        text: link_text,
                        url,
                        ..
                    } => {
                        let mut link_spans = Self::text_to_inline_spans(link_text);
                        if after_hard_break {
                            link_spans = trim_leading_space_inlines(link_spans);
                            after_hard_break = false;
                        }
                        result.push(crate::table::InlineSpan::Link {
                            text: link_spans,
                            url: url.clone(),
                        });
                    }
                    Inline::Image { url, .. } => {
                        result.push(crate::table::InlineSpan::Link {
                            text: vec![crate::table::InlineSpan::Text {
                                text: "[view image]".to_string(),
                                style: crate::table::InlineStyle::default(),
                            }],
                            url: url.clone(),
                        });
                    }
                    Inline::Anchor { .. } => {}
                    Inline::LineBreak => {
                        trim_trailing_space_in_spans(&mut result);
                        result.push(crate::table::InlineSpan::HardBreak);
                        after_hard_break = true;
                    }
                    Inline::SoftBreak => result.push(crate::table::InlineSpan::SoftBreak),
                },
            }
        }
        result
    }

    /// Convert table cell content to structured inline spans
    pub fn cell_content_to_inline(
        content: &crate::markdown::TableCellContent,
    ) -> Vec<crate::table::InlineSpan> {
        match content {
            crate::markdown::TableCellContent::Simple(text) => Self::text_to_inline_spans(text),
            crate::markdown::TableCellContent::Rich(nodes) => {
                let mut result = Vec::new();
                for (idx, node) in nodes.iter().enumerate() {
                    if idx > 0 {
                        result.push(crate::table::InlineSpan::HardBreak);
                    }
                    result.extend(Self::node_to_inline_spans(node));
                }
                result
            }
        }
    }

    /// Convert a Node to structured inline spans
    fn node_to_inline_spans(node: &crate::markdown::Node) -> Vec<crate::table::InlineSpan> {
        use crate::markdown::Block;
        match &node.block {
            Block::Paragraph { content } => Self::text_to_inline_spans(content),
            Block::Heading { content, .. } => Self::text_to_inline_spans(content),
            Block::CodeBlock { content, .. } => vec![crate::table::InlineSpan::Text {
                text: content.clone(),
                style: crate::table::InlineStyle {
                    code: true,
                    ..Default::default()
                },
            }],
            Block::Quote { content } => {
                let mut result = Vec::new();
                for (idx, node) in content.iter().enumerate() {
                    if idx > 0 {
                        result.push(crate::table::InlineSpan::HardBreak);
                    }
                    result.extend(Self::node_to_inline_spans(node));
                }
                result
            }
            Block::List { kind, items } => {
                use crate::markdown::ListKind;
                let mut result = Vec::new();
                for (idx, item) in items.iter().enumerate() {
                    let prefix = match kind {
                        ListKind::Unordered => "• ".to_string(),
                        ListKind::Ordered { start } => format!("{}. ", start + idx as u32),
                    };
                    result.push(crate::table::InlineSpan::Text {
                        text: prefix,
                        style: crate::table::InlineStyle::default(),
                    });

                    for node in &item.content {
                        result.extend(Self::node_to_inline_spans(node));
                    }

                    if idx + 1 < items.len() {
                        result.push(crate::table::InlineSpan::HardBreak);
                    }
                }
                result
            }
            Block::Table { header, rows, .. } => {
                let mut rows_inline: Vec<Vec<Vec<crate::table::InlineSpan>>> = Vec::new();
                let mut has_header = false;

                if let Some(h) = header {
                    let header_cells = h
                        .cells
                        .iter()
                        .map(|cell| Self::cell_content_to_inline(&cell.content))
                        .collect::<Vec<_>>();
                    rows_inline.push(header_cells);
                    has_header = true;
                }

                for row in rows {
                    let row_cells = row
                        .cells
                        .iter()
                        .map(|cell| Self::cell_content_to_inline(&cell.content))
                        .collect::<Vec<_>>();
                    rows_inline.push(row_cells);
                }

                let num_cols = rows_inline.iter().map(|row| row.len()).max().unwrap_or(0);
                if num_cols == 0 {
                    return Vec::new();
                }

                let mut col_widths = vec![0usize; num_cols];
                for row in &rows_inline {
                    for (idx, cell) in row.iter().enumerate() {
                        let width = Self::inline_spans_to_flat_text(cell).chars().count();
                        col_widths[idx] = col_widths[idx].max(width);
                    }
                }

                let mut result = Vec::new();
                for (row_idx, row) in rows_inline.iter().enumerate() {
                    let is_header = has_header && row_idx == 0;
                    for (col_idx, col_width) in col_widths.iter().enumerate() {
                        let cell = row.get(col_idx).cloned().unwrap_or_default();
                        let styled_cell = if is_header {
                            Self::apply_header_style(&cell)
                        } else {
                            cell
                        };
                        let cell_width = Self::inline_spans_to_flat_text(&styled_cell)
                            .chars()
                            .count();
                        let padding = col_width.saturating_sub(cell_width);

                        result.extend(styled_cell);
                        if padding > 0 {
                            result.push(crate::table::InlineSpan::Text {
                                text: " ".repeat(padding),
                                style: crate::table::InlineStyle::default(),
                            });
                        }

                        if col_idx + 1 < num_cols {
                            result.push(crate::table::InlineSpan::Text {
                                text: " | ".to_string(),
                                style: crate::table::InlineStyle::default(),
                            });
                        }
                    }

                    if row_idx + 1 < rows_inline.len() {
                        result.push(crate::table::InlineSpan::HardBreak);
                    }
                }

                result
            }
            Block::DefinitionList { items } => {
                let mut result = Vec::new();
                for (idx, item) in items.iter().enumerate() {
                    let term =
                        Self::inline_spans_to_plain_text(&Self::text_to_inline_spans(&item.term));
                    let defs: Vec<String> = item
                        .definitions
                        .iter()
                        .map(|def| {
                            def.iter()
                                .flat_map(Self::node_to_inline_spans)
                                .collect::<Vec<_>>()
                        })
                        .map(|inline| Self::inline_spans_to_plain_text(&inline))
                        .collect();

                    result.push(crate::table::InlineSpan::Text {
                        text: format!("{}: {}", term, defs.join("; ")),
                        style: crate::table::InlineStyle::default(),
                    });
                    if idx + 1 < items.len() {
                        result.push(crate::table::InlineSpan::HardBreak);
                    }
                }
                result
            }
            Block::EpubBlock { content, .. } => {
                let mut result = Vec::new();
                for (idx, node) in content.iter().enumerate() {
                    if idx > 0 {
                        result.push(crate::table::InlineSpan::HardBreak);
                    }
                    result.extend(Self::node_to_inline_spans(node));
                }
                result
            }
            Block::ThematicBreak => vec![crate::table::InlineSpan::Text {
                text: "---".to_string(),
                style: crate::table::InlineStyle::default(),
            }],
        }
    }

    fn inline_spans_to_plain_text(inlines: &[crate::table::InlineSpan]) -> String {
        let mut result = String::new();
        for inline in inlines {
            match inline {
                crate::table::InlineSpan::Text { text, .. } => result.push_str(text),
                crate::table::InlineSpan::Link { text, .. } => {
                    result.push_str(&Self::inline_spans_to_plain_text(text));
                }
                crate::table::InlineSpan::SoftBreak => result.push(' '),
                crate::table::InlineSpan::HardBreak => result.push('\n'),
            }
        }
        result
    }

    fn inline_spans_to_flat_text(inlines: &[crate::table::InlineSpan]) -> String {
        let mut result = String::new();
        for inline in inlines {
            match inline {
                crate::table::InlineSpan::Text { text, .. } => result.push_str(text),
                crate::table::InlineSpan::Link { text, .. } => {
                    result.push_str(&Self::inline_spans_to_flat_text(text));
                }
                crate::table::InlineSpan::SoftBreak | crate::table::InlineSpan::HardBreak => {
                    result.push(' ');
                }
            }
        }
        result
    }

    fn apply_header_style(inlines: &[crate::table::InlineSpan]) -> Vec<crate::table::InlineSpan> {
        inlines
            .iter()
            .map(|inline| match inline {
                crate::table::InlineSpan::Text { text, style } => {
                    let mut styled = *style;
                    styled.bold = true;
                    crate::table::InlineSpan::Text {
                        text: text.clone(),
                        style: styled,
                    }
                }
                crate::table::InlineSpan::Link { text, url } => crate::table::InlineSpan::Link {
                    text: Self::apply_header_style(text),
                    url: url.clone(),
                },
                crate::table::InlineSpan::SoftBreak => crate::table::InlineSpan::SoftBreak,
                crate::table::InlineSpan::HardBreak => crate::table::InlineSpan::HardBreak,
            })
            .collect()
    }

    /// Build a proper table grid for rendering with colspan information.
    /// Note: rowspan is already handled by the parser (extract_table_row_with_rowspan)
    /// which inserts empty placeholder cells.
    /// Returns: (header_cells, row_cells, grid_columns)
    fn build_table_grid(
        header: &Option<crate::markdown::TableRow>,
        rows: &[crate::markdown::TableRow],
    ) -> (
        Vec<crate::table::CellData>,
        Vec<Vec<crate::table::CellData>>,
        usize,
    ) {
        // Collect all source rows (header + data rows)
        let mut source_rows: Vec<&crate::markdown::TableRow> = Vec::new();
        if let Some(h) = header {
            source_rows.push(h);
        }
        for row in rows {
            source_rows.push(row);
        }

        if source_rows.is_empty() {
            return (Vec::new(), Vec::new(), 0);
        }

        // Determine max grid columns (accounting for colspan)
        let mut max_grid_cols = 0;
        for row in &source_rows {
            let mut grid_cols = 0;
            for cell in &row.cells {
                grid_cols += cell.colspan.max(1) as usize;
            }
            max_grid_cols = max_grid_cols.max(grid_cols);
        }

        if max_grid_cols == 0 {
            return (Vec::new(), Vec::new(), 0);
        }

        // Build grid - extract cell content and colspan info
        let mut result_rows: Vec<Vec<crate::table::CellData>> = Vec::new();

        for row in &source_rows {
            let mut grid_row: Vec<crate::table::CellData> = Vec::new();
            let mut total_grid_cols = 0;

            for cell in &row.cells {
                let content = Self::cell_content_to_inline(&cell.content);
                let colspan = cell.colspan.max(1);
                grid_row.push(crate::table::CellData::with_colspan(content, colspan));
                total_grid_cols += colspan as usize;
            }

            // Pad row to max_grid_cols if needed (add empty cells with colspan=1)
            while total_grid_cols < max_grid_cols {
                grid_row.push(crate::table::CellData::empty());
                total_grid_cols += 1;
            }

            result_rows.push(grid_row);
        }

        // Convert to output format - separate header from data rows
        if header.is_some() && !result_rows.is_empty() {
            let table_headers = result_rows.remove(0);
            (table_headers, result_rows, max_grid_cols)
        } else {
            (Vec::new(), result_rows, max_grid_cols)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_heading(
        &mut self,
        level: HeadingLevel,
        content: &MarkdownText,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        node_index: Option<usize>,
    ) {
        let heading_text = Self::text_to_string(content);

        let display_text = if level == HeadingLevel::H1 {
            heading_text.to_uppercase()
        } else {
            heading_text.clone()
        };

        let wrapped = textwrap::wrap(&display_text, width);

        let heading_color = if is_focused {
            palette.base_0a // Yellow
        } else {
            palette.base_03 // Dimmed
        };

        // Check if this heading has annotations
        let annotation_ranges = self.get_annotation_ranges(node_index);

        let base_modifiers = match level {
            HeadingLevel::H3 => Modifier::BOLD | Modifier::UNDERLINED,
            HeadingLevel::H4 => Modifier::BOLD | Modifier::UNDERLINED,
            _ => Modifier::BOLD,
        };

        // Track cumulative character position for annotation ranges
        let mut cumulative_char_pos = 0;

        for (line_idx, wrapped_line) in wrapped.iter().enumerate() {
            let base_style = RatatuiStyle::default()
                .fg(heading_color)
                .add_modifier(base_modifiers);

            // Apply precise underlines at annotation boundaries
            let line_len = wrapped_line.chars().count();
            let styled_spans = if annotation_ranges.is_empty() {
                vec![Span::styled(wrapped_line.to_string(), base_style)]
            } else {
                Self::split_text_at_annotation_boundaries(
                    wrapped_line,
                    &annotation_ranges,
                    cumulative_char_pos,
                    base_style,
                    palette.base_0e,
                )
            };

            cumulative_char_pos += line_len;

            lines.push(RenderedLine {
                spans: styled_spans,
                raw_text: wrapped_line.to_string(),
                line_type: LineType::Heading {
                    level: level.as_u8(),
                    needs_decoration: false,
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: if line_idx == 0 { node_index } else { None },
                code_line: None,
                inline_code_comments: Vec::new(),
                canonical_content_start: None,
                content_column_start: 0,
            });

            self.raw_text_lines.push(wrapped_line.to_string());
            *total_height += 1;
        }

        // Add decoration line for H1-H3
        if matches!(level, HeadingLevel::H1 | HeadingLevel::H2) {
            let decoration = match level {
                HeadingLevel::H1 => "═".repeat(width),
                HeadingLevel::H2 => "─".repeat(width),
                _ => unreachable!(),
            };

            lines.push(RenderedLine {
                spans: vec![Span::styled(
                    decoration.clone(),
                    RatatuiStyle::default().fg(heading_color),
                )],
                raw_text: decoration.clone(),
                line_type: LineType::Heading {
                    level: level.as_u8(),
                    needs_decoration: true,
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
                code_line: None,
                inline_code_comments: Vec::new(),
                canonical_content_start: None,
                content_column_start: 0,
            });

            self.raw_text_lines.push(decoration);
            *total_height += 1;
        }

        // Render comments for this heading if any
        let heading_comments = self.get_node_comments(node_index);
        if !heading_comments.is_empty() {
            // Add empty line before comments
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;

            self.render_node_comments(
                node_index,
                lines,
                total_height,
                width,
                palette,
                is_focused,
                0,
            );
            // Comments already add empty line after, so we're done
            return;
        }

        // Add empty line after heading (only if no comments were rendered)
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
            canonical_content_start: None,
            content_column_start: 0,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_paragraph(
        &mut self,
        content: &MarkdownText,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        node_index: Option<usize>,
        context: RenderContext,
    ) {
        let _paragraph_lines_start = lines.len();
        if context == RenderContext::InsideContainer {
            let has_visible_content = content.iter().any(|item| match item {
                TextOrInline::Text(t) => !t.content.trim().is_empty(),
                TextOrInline::Inline(inline) => match inline {
                    Inline::Image { .. } => true,
                    Inline::Link { text, .. } => !Self::text_to_string(text).trim().is_empty(),
                    Inline::Anchor { .. } | Inline::LineBreak | Inline::SoftBreak => false,
                },
            });

            if !has_visible_content {
                return;
            }
        }

        // Collect annotation ranges for this node (for underline styling)
        let annotation_ranges = self.get_annotation_ranges(node_index);
        let underline_color = palette.base_0e; // Purple

        let mut current_rich_spans = Vec::new();
        let mut current_chunk_items: Vec<&TextOrInline> = Vec::new();
        let mut has_content = false;
        let mut has_visible_text = false;
        let mut has_anchor = false;
        let mut para_canonical_offset: usize = 0;

        for item in content.iter() {
            match item {
                TextOrInline::Text(t) => {
                    if !t.content.trim().is_empty() {
                        has_visible_text = true;
                    }
                    let rich_spans = self.render_text_or_inline(item, palette, is_focused);
                    current_rich_spans.extend(rich_spans);
                    current_chunk_items.push(item);
                }
                TextOrInline::Inline(Inline::Image { url, .. }) => {
                    // If we have accumulated text before the image, render it first
                    if !current_rich_spans.is_empty() {
                        let chunk_canonical_len = current_chunk_items
                            .iter()
                            .map(|i| Self::text_or_inline_canonical_len(i))
                            .sum::<usize>();
                        let styled_spans = Self::apply_annotation_underlines(
                            current_rich_spans,
                            &annotation_ranges,
                            underline_color,
                        );
                        self.render_text_spans(
                            &styled_spans,
                            None, // no prefix
                            lines,
                            total_height,
                            width,
                            indent,
                            false, // don't add empty line after
                            node_index,
                            Some(para_canonical_offset),
                        );
                        para_canonical_offset += chunk_canonical_len;
                        current_rich_spans = Vec::new();
                        current_chunk_items.clear();
                    }

                    // Render the image as a separate block
                    self.render_image_placeholder(url, lines, total_height, width, palette);
                    has_content = true;
                }
                TextOrInline::Inline(Inline::Anchor { .. }) => {
                    has_anchor = true;
                }
                // Handle links containing images - render as clickable image
                TextOrInline::Inline(Inline::Link {
                    text: link_text,
                    url,
                    link_type,
                    target_chapter,
                    target_anchor,
                    ..
                }) if Self::extract_image_from_text(link_text).is_some() => {
                    // If we have accumulated text before the image link, render it first
                    if !current_rich_spans.is_empty() {
                        let chunk_canonical_len = current_chunk_items
                            .iter()
                            .map(|i| Self::text_or_inline_canonical_len(i))
                            .sum::<usize>();
                        let styled_spans = Self::apply_annotation_underlines(
                            current_rich_spans,
                            &annotation_ranges,
                            underline_color,
                        );
                        self.render_text_spans(
                            &styled_spans,
                            None,
                            lines,
                            total_height,
                            width,
                            indent,
                            false,
                            node_index,
                            Some(para_canonical_offset),
                        );
                        para_canonical_offset += chunk_canonical_len;
                        current_rich_spans = Vec::new();
                        current_chunk_items.clear();
                    }

                    // Extract the image URL from the link text
                    let image_url = Self::extract_image_from_text(link_text).unwrap();

                    // Create link info for the image
                    let link_info = LinkInfo {
                        text: String::new(),
                        url: url.clone(),
                        line: 0,
                        start_col: 0,
                        end_col: 0,
                        link_type: link_type.clone(),
                        target_chapter: target_chapter.clone(),
                        target_anchor: target_anchor.clone(),
                    };

                    // Render the image with link info attached
                    self.render_image_placeholder_with_link(
                        &image_url,
                        lines,
                        total_height,
                        width,
                        palette,
                        Some(link_info),
                    );
                    has_content = true;
                }
                _ => {
                    if let TextOrInline::Inline(Inline::Link { text, .. }) = item {
                        if !Self::text_to_string(text).trim().is_empty() {
                            has_visible_text = true;
                        }
                    }
                    // Accumulate non-image content
                    let rich_spans = self.render_text_or_inline(item, palette, is_focused);
                    current_rich_spans.extend(rich_spans);
                    current_chunk_items.push(item);
                }
            }
        }

        // Render any remaining text spans
        if !current_rich_spans.is_empty() {
            let add_empty_line = context == RenderContext::TopLevel;
            let styled_spans = Self::apply_annotation_underlines(
                current_rich_spans,
                &annotation_ranges,
                underline_color,
            );
            self.render_text_spans(
                &styled_spans,
                None,
                lines,
                total_height,
                width,
                indent,
                add_empty_line,
                node_index,
                Some(para_canonical_offset),
            );
        } else if !has_content {
            let has_anchor_only = has_anchor && !has_visible_text;
            if has_anchor_only {
                return;
            }
            // Empty paragraph - just add an empty line
            lines.push(RenderedLine {
                spans: vec![Span::raw("")],
                raw_text: String::new(),
                line_type: LineType::Empty,
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
                code_line: None,
                inline_code_comments: Vec::new(),
                canonical_content_start: None,
                content_column_start: 0,
            });
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }

        let _paragraph_lines_end = lines.len();

        // Render any comments for this paragraph
        self.render_node_comments(
            node_index,
            lines,
            total_height,
            width,
            palette,
            is_focused,
            indent,
        );
    }

    pub fn render_text_or_inline(
        &mut self,
        item: &TextOrInline,
        palette: &Base16Palette,
        is_focused: bool,
    ) -> Vec<RichSpan> {
        let mut rich_spans = Vec::new();

        match item {
            TextOrInline::Text(text_node) => {
                let styled_span = self.style_text_node(text_node, palette, is_focused);
                rich_spans.push(RichSpan::Text(styled_span));
            }

            TextOrInline::Inline(inline) => {
                match inline {
                    Inline::Link {
                        text: link_text,
                        url,
                        link_type,
                        target_chapter,
                        target_anchor,
                        ..
                    } => {
                        let link_text_str = Self::text_to_string(link_text);

                        // Create link info (line and columns will be set during line creation)
                        let link_info = LinkInfo {
                            text: link_text_str.clone(),
                            url: url.clone(),
                            line: 0,      // Will be set when added to RenderedLine
                            start_col: 0, // Will be calculated when added to line
                            end_col: 0,   // Will be calculated when added to line
                            link_type: link_type.clone(),
                            target_chapter: target_chapter.clone(),
                            target_anchor: target_anchor.clone(),
                        };

                        // Determine styling based on link type
                        let (link_color, link_modifier) = if is_focused {
                            match link_type {
                                crate::markdown::LinkType::External => {
                                    (palette.base_0c, Modifier::UNDERLINED) // Cyan + underlined
                                }
                                crate::markdown::LinkType::InternalChapter => {
                                    (palette.base_0b, Modifier::UNDERLINED | Modifier::BOLD)
                                    // Green + bold underlined
                                }
                                crate::markdown::LinkType::InternalAnchor => {
                                    (palette.base_0a, Modifier::UNDERLINED | Modifier::ITALIC)
                                    // Yellow + italic underlined
                                }
                            }
                        } else {
                            // Unfocused state - use muted colors but maintain differentiation
                            match link_type {
                                crate::markdown::LinkType::External => {
                                    (palette.base_03, Modifier::UNDERLINED)
                                }
                                crate::markdown::LinkType::InternalChapter => {
                                    (palette.base_03, Modifier::UNDERLINED | Modifier::BOLD)
                                }
                                crate::markdown::LinkType::InternalAnchor => {
                                    (palette.base_03, Modifier::UNDERLINED | Modifier::ITALIC)
                                }
                            }
                        };

                        let styled_span = Span::styled(
                            link_text_str,
                            RatatuiStyle::default()
                                .fg(link_color)
                                .add_modifier(link_modifier),
                        );

                        rich_spans.push(RichSpan::Link {
                            span: styled_span,
                            info: link_info,
                        });
                    }

                    Inline::Image { url, .. } => {
                        let link_info = LinkInfo {
                            text: "[view image]".to_string(),
                            url: url.clone(),
                            line: 0,
                            start_col: 0,
                            end_col: 0,
                            link_type: crate::markdown::LinkType::External,
                            target_chapter: None,
                            target_anchor: None,
                        };
                        rich_spans.push(RichSpan::Link {
                            span: Span::styled(
                                "[view image]",
                                RatatuiStyle::default()
                                    .fg(if is_focused {
                                        palette.base_0c
                                    } else {
                                        palette.base_03
                                    })
                                    .add_modifier(Modifier::UNDERLINED),
                            ),
                            info: link_info,
                        });
                    }

                    Inline::Anchor { .. } => {
                        // Anchors don't produce visible content - position tracking is handled elsewhere
                    }

                    Inline::LineBreak => {
                        rich_spans.push(RichSpan::Text(Span::raw("\n")));
                    }

                    Inline::SoftBreak => {
                        rich_spans.push(RichSpan::Text(Span::raw(" ")));
                    }
                }
            }
        }

        rich_spans
    }

    pub fn style_text_node(
        &self,
        node: &crate::markdown::TextNode,
        palette: &Base16Palette,
        is_focused: bool,
    ) -> Span<'static> {
        let (normal_color, _, _) = palette.get_panel_colors(is_focused);

        let base_style = RatatuiStyle::default().fg(normal_color);

        let styled = match &node.style {
            Some(Style::Strong) => {
                let bold_color = if is_focused {
                    palette.base_08 // Red
                } else {
                    palette.base_01
                };
                base_style.fg(bold_color).add_modifier(Modifier::BOLD)
            }
            Some(Style::Emphasis) => base_style.add_modifier(Modifier::ITALIC),
            Some(Style::Code) => {
                // Inline code with background
                RatatuiStyle::default().fg(Color::Black).bg(Color::Gray)
            }
            Some(Style::Strikethrough) => base_style.add_modifier(Modifier::CROSSED_OUT),
            None => base_style,
        };

        Span::styled(node.content.clone(), styled)
    }

    /// Apply underline styling to spans that overlap with annotation character ranges.
    /// This provides visual feedback for annotated text passages.
    /// `char_offset` is added to span positions for cumulative tracking across multiple items.
    /// Spans are split at annotation boundaries so only the exact annotated characters are underlined.
    fn apply_annotation_underlines_with_offset(
        rich_spans: Vec<RichSpan>,
        annotation_ranges: &[(usize, usize)],
        underline_color: Color,
        char_offset: usize,
    ) -> (Vec<RichSpan>, usize) {
        let mut result = Vec::with_capacity(rich_spans.len());
        let mut char_pos = char_offset;

        if annotation_ranges.is_empty() {
            // Still need to calculate final position
            for rich_span in &rich_spans {
                let span_len = match rich_span {
                    RichSpan::Text(s) => s.content.chars().count(),
                    RichSpan::Link { span, .. } => span.content.chars().count(),
                };
                char_pos += span_len;
            }
            return (rich_spans, char_pos);
        }

        for rich_span in rich_spans {
            let (span, link_info) = match rich_span {
                RichSpan::Text(s) => (s, None),
                RichSpan::Link { span, info } => (span, Some(info)),
            };

            let span_len = span.content.chars().count();
            let span_start = char_pos;
            let span_end = char_pos + span_len;

            // Check if this span overlaps with any annotation range
            let overlapping_ranges: Vec<(usize, usize)> = annotation_ranges
                .iter()
                .filter(|&&(ann_start, ann_end)| span_start < ann_end && span_end > ann_start)
                .copied()
                .collect();

            if overlapping_ranges.is_empty() {
                // No overlap, keep span as-is
                let new_rich_span = match link_info {
                    Some(info) => RichSpan::Link { span, info },
                    None => RichSpan::Text(span),
                };
                result.push(new_rich_span);
            } else {
                // Split span at annotation boundaries for precise underlining
                let chars: Vec<char> = span.content.chars().collect();
                let base_style = span.style;
                let underlined_style = base_style
                    .add_modifier(Modifier::UNDERLINED)
                    .underline_color(underline_color);

                // Collect all boundary points within this span
                let mut boundaries: Vec<usize> = vec![0, span_len];
                for &(ann_start, ann_end) in &overlapping_ranges {
                    if ann_start > span_start && ann_start < span_end {
                        boundaries.push(ann_start - span_start);
                    }
                    if ann_end > span_start && ann_end < span_end {
                        boundaries.push(ann_end - span_start);
                    }
                }
                boundaries.sort_unstable();
                boundaries.dedup();

                // Create sub-spans for each segment
                for window in boundaries.windows(2) {
                    let seg_start = window[0];
                    let seg_end = window[1];
                    if seg_start >= seg_end {
                        continue;
                    }

                    let segment_text: String = chars[seg_start..seg_end].iter().collect();
                    let abs_start = span_start + seg_start;
                    let abs_end = span_start + seg_end;

                    // Check if this segment is within any annotation range
                    let is_annotated = overlapping_ranges
                        .iter()
                        .any(|&(ann_start, ann_end)| abs_start >= ann_start && abs_end <= ann_end);

                    let style = if is_annotated {
                        underlined_style
                    } else {
                        base_style
                    };

                    let sub_span = Span::styled(segment_text, style);
                    let sub_rich_span = match &link_info {
                        Some(info) => RichSpan::Link {
                            span: sub_span,
                            info: info.clone(),
                        },
                        None => RichSpan::Text(sub_span),
                    };
                    result.push(sub_rich_span);
                }
            }

            char_pos = span_end;
        }

        (result, char_pos)
    }

    /// Apply underline styling to spans that overlap with annotation character ranges.
    /// Convenience wrapper that starts at offset 0.
    fn apply_annotation_underlines(
        rich_spans: Vec<RichSpan>,
        annotation_ranges: &[(usize, usize)],
        underline_color: Color,
    ) -> Vec<RichSpan> {
        Self::apply_annotation_underlines_with_offset(
            rich_spans,
            annotation_ranges,
            underline_color,
            0,
        )
        .0
    }

    /// Split plain text into spans at annotation boundaries.
    /// Returns spans with underline styling only on annotated portions.
    fn split_text_at_annotation_boundaries(
        text: &str,
        annotation_ranges: &[(usize, usize)],
        char_offset: usize,
        base_style: RatatuiStyle,
        underline_color: Color,
    ) -> Vec<Span<'static>> {
        let chars: Vec<char> = text.chars().collect();
        let text_len = chars.len();
        let text_start = char_offset;
        let text_end = char_offset + text_len;

        // Find overlapping annotation ranges
        let overlapping_ranges: Vec<(usize, usize)> = annotation_ranges
            .iter()
            .filter(|&&(ann_start, ann_end)| text_start < ann_end && text_end > ann_start)
            .copied()
            .collect();

        if overlapping_ranges.is_empty() {
            return vec![Span::styled(text.to_string(), base_style)];
        }

        let underlined_style = base_style
            .add_modifier(Modifier::UNDERLINED)
            .underline_color(underline_color);

        // Collect all boundary points within this text
        let mut boundaries: Vec<usize> = vec![0, text_len];
        for &(ann_start, ann_end) in &overlapping_ranges {
            if ann_start > text_start && ann_start < text_end {
                boundaries.push(ann_start - text_start);
            }
            if ann_end > text_start && ann_end < text_end {
                boundaries.push(ann_end - text_start);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        // Create spans for each segment
        let mut result = Vec::new();
        for window in boundaries.windows(2) {
            let seg_start = window[0];
            let seg_end = window[1];
            if seg_start >= seg_end {
                continue;
            }

            let segment_text: String = chars[seg_start..seg_end].iter().collect();
            let abs_start = text_start + seg_start;
            let abs_end = text_start + seg_end;

            // Check if this segment is within any annotation range
            let is_annotated = overlapping_ranges
                .iter()
                .any(|&(ann_start, ann_end)| abs_start >= ann_start && abs_end <= ann_end);

            let style = if is_annotated {
                underlined_style
            } else {
                base_style
            };

            result.push(Span::styled(segment_text, style));
        }

        result
    }

    /// Apply underline styling to spans in a rendered line.
    /// Works on already-rendered Span<'static> instead of RichSpan.
    /// Returns the updated character position after processing.
    /// Spans are split at annotation boundaries so only the exact annotated characters are underlined.
    fn apply_underlines_to_line_spans(
        spans: &mut Vec<Span<'static>>,
        annotation_ranges: &[(usize, usize)],
        underline_color: Color,
        char_offset: usize,
    ) -> usize {
        if annotation_ranges.is_empty() {
            let mut char_pos = char_offset;
            for span in spans.iter() {
                char_pos += span.content.chars().count();
            }
            return char_pos;
        }

        let mut result = Vec::with_capacity(spans.len());
        let mut char_pos = char_offset;

        for span in spans.drain(..) {
            let span_len = span.content.chars().count();
            let span_start = char_pos;
            let span_end = char_pos + span_len;

            // Check if this span overlaps with any annotation range
            let overlapping_ranges: Vec<(usize, usize)> = annotation_ranges
                .iter()
                .filter(|&&(ann_start, ann_end)| span_start < ann_end && span_end > ann_start)
                .copied()
                .collect();

            if overlapping_ranges.is_empty() {
                result.push(span);
            } else {
                // Split span at annotation boundaries for precise underlining
                let chars: Vec<char> = span.content.chars().collect();
                let base_style = span.style;
                let underlined_style = base_style
                    .add_modifier(Modifier::UNDERLINED)
                    .underline_color(underline_color);

                // Collect all boundary points within this span
                let mut boundaries: Vec<usize> = vec![0, span_len];
                for &(ann_start, ann_end) in &overlapping_ranges {
                    if ann_start > span_start && ann_start < span_end {
                        boundaries.push(ann_start - span_start);
                    }
                    if ann_end > span_start && ann_end < span_end {
                        boundaries.push(ann_end - span_start);
                    }
                }
                boundaries.sort_unstable();
                boundaries.dedup();

                // Create sub-spans for each segment
                for window in boundaries.windows(2) {
                    let seg_start = window[0];
                    let seg_end = window[1];
                    if seg_start >= seg_end {
                        continue;
                    }

                    let segment_text: String = chars[seg_start..seg_end].iter().collect();
                    let abs_start = span_start + seg_start;
                    let abs_end = span_start + seg_end;

                    // Check if this segment is within any annotation range
                    let is_annotated = overlapping_ranges
                        .iter()
                        .any(|&(ann_start, ann_end)| abs_start >= ann_start && abs_end <= ann_end);

                    let style = if is_annotated {
                        underlined_style
                    } else {
                        base_style
                    };

                    result.push(Span::styled(segment_text, style));
                }
            }

            char_pos = span_end;
        }

        *spans = result;

        char_pos
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_code_block(
        &mut self,
        language: Option<&str>,
        content: &str,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        add_spacing_before: bool,
        node_index: Option<usize>,
    ) {
        // TODO: Implement syntax highlighting if language is provided
        if add_spacing_before {
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }

        let indent_str = "  ".repeat(indent);
        let code_lines: Vec<&str> = if content.is_empty() {
            vec![""]
        } else {
            content.lines().collect()
        };
        let total_code_lines = code_lines.len();

        let mut coverage_counts = vec![0usize; total_code_lines];
        let mut inline_comments: Vec<Vec<Comment>> = vec![Vec::new(); total_code_lines];

        if let Some(node_idx) = node_index {
            if let Some(node_comments) = self.current_chapter_comments.get(&node_idx) {
                for comment in node_comments {
                    if let Some(line_range) = comment.target.line_range() {
                        if total_code_lines == 0 {
                            continue;
                        }
                        let mut start = line_range.0.min(total_code_lines.saturating_sub(1));
                        let mut end = line_range.1.min(total_code_lines.saturating_sub(1));
                        if end < start {
                            std::mem::swap(&mut start, &mut end);
                        }
                        for count in coverage_counts.iter_mut().take(end + 1).skip(start) {
                            *count = count.saturating_add(1);
                        }
                        inline_comments[end].push(comment.clone());
                    }
                }
            }
        }

        let indent_width = indent_str.chars().count();
        let available_width = width.saturating_sub(indent_width).max(1);

        let split_code_line = |line: &str| -> Vec<String> {
            if line.is_empty() {
                return vec![String::new()];
            }

            let mut segments = Vec::new();
            let mut current = String::new();
            let mut count = 0usize;

            for ch in line.chars() {
                current.push(ch);
                count += 1;
                if count >= available_width {
                    segments.push(current);
                    current = String::new();
                    count = 0;
                }
            }

            if !current.is_empty() {
                segments.push(current);
            }

            segments
        };

        for (line_idx, code_line) in code_lines.iter().enumerate() {
            let segments = split_code_line(code_line);

            let mut single_line_comments: Vec<Comment> = Vec::new();
            let mut multi_line_comments: Vec<Comment> = Vec::new();
            if let Some(comments) = inline_comments.get(line_idx) {
                for comment in comments {
                    if let Some(line_range) = comment.target.line_range() {
                        let single_line_range = line_range.0 == line_range.1;
                        let comment_body = comment.content.trim_end_matches(['\n', '\r']);
                        let multiline_text = comment_body.contains('\n');

                        if single_line_range && !multiline_text {
                            single_line_comments.push(comment.clone());
                        } else {
                            multi_line_comments.push(comment.clone());
                        }
                    }
                }
            }

            for (segment_idx, segment) in segments.iter().enumerate() {
                let mut spans = Vec::new();
                let mut display_text = String::new();

                if !indent_str.is_empty() {
                    spans.push(Span::raw(indent_str.clone()));
                    display_text.push_str(&indent_str);
                }

                let mut style = RatatuiStyle::default().fg(if is_focused {
                    palette.base_0b
                } else {
                    palette.base_03
                });
                style = style.bg(theme_background());

                if coverage_counts.get(line_idx).copied().unwrap_or(0) > 0 {
                    style = style
                        .underline_color(palette.base_0e)
                        .add_modifier(Modifier::UNDERLINED);
                }

                let styled_span = Span::styled(segment.clone(), style);
                display_text.push_str(segment);
                spans.push(styled_span);

                let mut inline_fragments = Vec::new();
                let is_last_segment = segment_idx + 1 == segments.len();
                if is_last_segment && !single_line_comments.is_empty() {
                    let comment_style = RatatuiStyle::default().fg(palette.base_0e);
                    let mut appended_chars = display_text.chars().count();

                    for (idx, comment) in single_line_comments.iter().enumerate() {
                        let prefix = if idx == 0 { "  ⟵ " } else { " | ⟵ " };
                        let prefix_len = prefix.chars().count();
                        let available_width = width.saturating_sub(appended_chars);

                        let mut piece = prefix.to_string();
                        let fragment_start = appended_chars;

                        let mut comment_line = comment
                            .content
                            .lines()
                            .find(|line| !line.trim().is_empty())
                            .unwrap_or("(comment)")
                            .trim()
                            .to_string();

                        let available_for_text = available_width.saturating_sub(prefix_len);
                        if available_for_text == 0 {
                            // Only room for prefix arrow
                            appended_chars += piece.chars().count();
                            display_text.push_str(&piece);
                            spans.push(Span::styled(piece.clone(), comment_style));
                            inline_fragments.push(InlineCodeCommentFragment {
                                chapter_href: comment.chapter_href.clone(),
                                target: comment.target.clone(),
                                comment_id: comment.id.clone(),
                                start_column: fragment_start,
                                end_column: appended_chars,
                            });
                            continue;
                        }

                        if comment_line.chars().count() > available_for_text {
                            let allowed = available_for_text.saturating_sub(1);
                            if allowed == 0 {
                                comment_line = "…".to_string();
                            } else {
                                let truncated: String =
                                    comment_line.chars().take(allowed).collect();
                                comment_line = format!("{truncated}…");
                            }
                        }

                        piece.push_str(&comment_line);
                        appended_chars += piece.chars().count();
                        display_text.push_str(&piece);
                        spans.push(Span::styled(piece.clone(), comment_style));

                        inline_fragments.push(InlineCodeCommentFragment {
                            chapter_href: comment.chapter_href.clone(),
                            target: comment.target.clone(),
                            comment_id: comment.id.clone(),
                            start_column: fragment_start,
                            end_column: appended_chars,
                        });
                    }
                }

                let rendered_line = RenderedLine {
                    spans,
                    raw_text: display_text.clone(),
                    line_type: LineType::CodeBlock {
                        language: language.map(String::from),
                    },
                    link_nodes: vec![],
                    node_anchor: None,
                    node_index,
                    code_line: node_index.map(|idx| CodeLineMetadata {
                        node_index: idx,
                        line_index: line_idx,
                        total_lines: total_code_lines,
                    }),
                    inline_code_comments: inline_fragments,
                    canonical_content_start: None,
                    content_column_start: 0,
                };

                lines.push(rendered_line);
                self.raw_text_lines.push(display_text);
                *total_height += 1;
            }

            for comment in multi_line_comments {
                self.render_inline_code_comment(
                    &comment,
                    lines,
                    total_height,
                    width,
                    indent,
                    palette,
                );
            }
        }

        lines.push(RenderedLine::empty());

        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_inline_code_comment(
        &mut self,
        comment: &Comment,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        indent: usize,
        palette: &Base16Palette,
    ) {
        let indent_prefix = "  ".repeat(indent);
        let arrow_prefix = format!("{indent_prefix}⟵ ");
        let continuation_prefix = format!("{indent_prefix}   ");
        let available_width = width.saturating_sub(arrow_prefix.len()).max(10);
        let style = RatatuiStyle::default().fg(palette.base_0e);
        let normalized_content = comment.content.trim_end_matches(['\n', '\r']).to_string();

        let mut wrapped_lines = Vec::new();
        let mut previous_blank = false;
        if normalized_content.trim().is_empty() {
            wrapped_lines.push("(no content)".to_string());
        } else {
            for raw_line in normalized_content.split('\n') {
                let line_no_cr = raw_line.trim_end_matches('\r');
                if line_no_cr.trim().is_empty() {
                    if !previous_blank {
                        wrapped_lines.push(String::new());
                        previous_blank = true;
                    }
                    continue;
                }

                for seg in textwrap::wrap(line_no_cr, available_width) {
                    wrapped_lines.push(seg.into_owned());
                }
                previous_blank = false;
            }
        }

        for (idx, segment) in wrapped_lines.iter().enumerate() {
            let prefix = if idx == 0 {
                arrow_prefix.clone()
            } else {
                continuation_prefix.clone()
            };
            let raw_text = if segment.is_empty() {
                prefix.clone()
            } else {
                format!("{prefix}{segment}")
            };
            lines.push(RenderedLine {
                spans: vec![
                    Span::styled(prefix.clone(), style),
                    Span::styled(segment.clone(), style),
                ],
                raw_text: raw_text.clone(),
                line_type: LineType::Comment {
                    chapter_href: comment.chapter_href.clone(),
                    target: comment.target.clone(),
                    comment_id: comment.id.clone(),
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
                canonical_content_start: None,
                content_column_start: 0,
                code_line: None,
                inline_code_comments: Vec::new(),
            });
            self.raw_text_lines.push(raw_text);
            *total_height += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_list(
        &mut self,
        kind: &crate::markdown::ListKind,
        items: &[crate::markdown::ListItem],
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        node_index: Option<usize>,
        list_path: &[usize],
        global_char_pos: &mut usize,
    ) {
        use crate::markdown::ListKind;

        // Collect comments and annotation ranges for this list node
        let annotation_ranges = if list_path.is_empty() {
            self.get_annotation_ranges_for_legacy_list(node_index)
        } else {
            Vec::new()
        };
        let underline_color = palette.base_0e; // Purple

        for (idx, item) in items.iter().enumerate() {
            let mut item_path = list_path.to_vec();
            item_path.push(idx);
            let item_line_path = if list_path.is_empty() {
                Vec::new()
            } else {
                item_path.clone()
            };
            let item_annotation_ranges =
                self.get_annotation_ranges_for_list_item_path(node_index, &item_path);
            let mut item_char_pos = 0;

            // Determine bullet/number for this item
            let prefix = match kind {
                ListKind::Unordered => "• ".to_string(),
                ListKind::Ordered { start } => {
                    let num = start + idx as u32;
                    format!("{num}. ")
                }
            };

            let mut first_block_line_count = 0;

            // Render the list item content
            // List items can contain multiple blocks (paragraphs, nested lists, etc.)
            for (block_idx, block_node) in item.content.iter().enumerate() {
                if block_idx == 0 {
                    // First block gets the bullet/number prefix
                    match &block_node.block {
                        MarkdownBlock::Paragraph { content }
                            if Self::content_has_image(content) =>
                        {
                            let canonical_len = Self::text_to_string(content).chars().count();
                            let lines_before = lines.len();
                            let canon_offset = if list_path.is_empty() {
                                Some(*global_char_pos)
                            } else {
                                Some(item_char_pos)
                            };
                            self.render_content_with_images(
                                content,
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                Some(&prefix),
                                indent,
                                node_index,
                                canon_offset,
                            );
                            first_block_line_count = lines.len() - lines_before;
                            for line in lines[lines_before..].iter_mut() {
                                if !matches!(line.line_type, LineType::ImagePlaceholder { .. }) {
                                    line.line_type = LineType::ListItem {
                                        kind: kind.clone(),
                                        indent,
                                        item_index: idx,
                                        list_path: item_line_path.clone(),
                                    };
                                }
                                line.node_index = node_index;
                            }
                            if !list_path.is_empty() {
                                item_char_pos += canonical_len;
                            }
                            *global_char_pos += canonical_len;
                        }
                        MarkdownBlock::Paragraph { content } => {
                            let canonical_len = Self::text_to_string(content).chars().count();
                            let mut content_rich_spans = Vec::new();
                            for item in content.iter() {
                                content_rich_spans
                                    .extend(self.render_text_or_inline(item, palette, is_focused));
                            }

                            let (styled_spans, _) = if list_path.is_empty() {
                                Self::apply_annotation_underlines_with_offset(
                                    content_rich_spans,
                                    &annotation_ranges,
                                    underline_color,
                                    *global_char_pos,
                                )
                            } else {
                                Self::apply_annotation_underlines_with_offset(
                                    content_rich_spans,
                                    &item_annotation_ranges,
                                    underline_color,
                                    item_char_pos,
                                )
                            };

                            let lines_before = lines.len();

                            let canon_offset = if list_path.is_empty() {
                                Some(*global_char_pos)
                            } else {
                                Some(item_char_pos)
                            };
                            self.render_text_spans(
                                &styled_spans,
                                Some(&prefix),
                                lines,
                                total_height,
                                width,
                                indent,
                                false,
                                None,
                                canon_offset,
                            );

                            first_block_line_count = lines.len() - lines_before;

                            for line in lines[lines_before..].iter_mut() {
                                line.line_type = LineType::ListItem {
                                    kind: kind.clone(),
                                    indent,
                                    item_index: idx,
                                    list_path: item_line_path.clone(),
                                };
                                line.node_index = node_index;
                            }
                            if !list_path.is_empty() {
                                item_char_pos += canonical_len;
                            }
                            *global_char_pos += canonical_len;
                        }
                        MarkdownBlock::List {
                            kind: nested_kind,
                            items: nested_items,
                        } => {
                            let lines_before = lines.len();
                            self.render_list(
                                nested_kind,
                                nested_items,
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                indent + 1,
                                node_index,
                                &item_path,
                                global_char_pos,
                            );
                            first_block_line_count = lines.len() - lines_before;
                        }
                        _ => {
                            let lines_before = lines.len();
                            self.render_node(
                                block_node,
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                indent + 1,
                                None,
                                RenderContext::InsideContainer,
                            );
                            first_block_line_count = lines.len() - lines_before;
                        }
                    }
                } else {
                    match &block_node.block {
                        MarkdownBlock::Paragraph { content }
                            if Self::content_has_image(content) =>
                        {
                            let canonical_len = Self::text_to_string(content).chars().count();
                            let lines_before = lines.len();
                            let canon_offset = if list_path.is_empty() {
                                Some(*global_char_pos)
                            } else {
                                Some(item_char_pos)
                            };
                            self.render_content_with_images(
                                content,
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                None,
                                indent + 1,
                                node_index,
                                canon_offset,
                            );
                            for line in lines[lines_before..].iter_mut() {
                                if !matches!(line.line_type, LineType::ImagePlaceholder { .. }) {
                                    line.line_type = LineType::ListItem {
                                        kind: kind.clone(),
                                        indent,
                                        item_index: idx,
                                        list_path: item_line_path.clone(),
                                    };
                                }
                                line.node_index = node_index;
                            }
                            if !list_path.is_empty() {
                                item_char_pos += canonical_len;
                            }
                            *global_char_pos += canonical_len;
                        }
                        MarkdownBlock::Paragraph { content } => {
                            let canonical_len = Self::text_to_string(content).chars().count();
                            let mut content_rich_spans = Vec::new();
                            for item in content.iter() {
                                content_rich_spans
                                    .extend(self.render_text_or_inline(item, palette, is_focused));
                            }
                            let (styled_spans, _) = if list_path.is_empty() {
                                Self::apply_annotation_underlines_with_offset(
                                    content_rich_spans,
                                    &annotation_ranges,
                                    underline_color,
                                    *global_char_pos,
                                )
                            } else {
                                Self::apply_annotation_underlines_with_offset(
                                    content_rich_spans,
                                    &item_annotation_ranges,
                                    underline_color,
                                    item_char_pos,
                                )
                            };

                            let canon_offset = if list_path.is_empty() {
                                Some(*global_char_pos)
                            } else {
                                Some(item_char_pos)
                            };
                            let lines_before = lines.len();
                            self.render_text_spans(
                                &styled_spans,
                                None,
                                lines,
                                total_height,
                                width,
                                indent + 1,
                                false,
                                None,
                                canon_offset,
                            );

                            for line in lines[lines_before..].iter_mut() {
                                line.line_type = LineType::ListItem {
                                    kind: kind.clone(),
                                    indent,
                                    item_index: idx,
                                    list_path: item_line_path.clone(),
                                };
                                line.node_index = node_index;
                            }
                            if !list_path.is_empty() {
                                item_char_pos += canonical_len;
                            }
                            *global_char_pos += canonical_len;
                        }
                        MarkdownBlock::List {
                            kind: nested_kind,
                            items: nested_items,
                        } => {
                            self.render_list(
                                nested_kind,
                                nested_items,
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                indent + 1,
                                node_index,
                                &item_path,
                                global_char_pos,
                            );
                        }
                        _ => {
                            self.render_node(
                                block_node,
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                indent + 1,
                                None,
                                RenderContext::InsideContainer,
                            );
                        }
                    }
                }
            }

            // Smart spacing: add empty line between items if first block is >2 lines
            if idx + 1 < items.len() && first_block_line_count > 2 {
                lines.push(RenderedLine::empty());
                self.raw_text_lines.push(String::new());
                *total_height += 1;
            }
        }

        // Render comments for the list at the end (only at the outermost list level)
        let list_comments = if list_path.is_empty() {
            self.get_node_comments(node_index)
        } else {
            Vec::new()
        };
        if !list_comments.is_empty() {
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;

            self.render_node_comments(
                node_index,
                lines,
                total_height,
                width,
                palette,
                is_focused,
                indent,
            );
            return;
        }

        // Add empty line after list
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_table(
        &mut self,
        header: &Option<crate::markdown::TableRow>,
        rows: &[crate::markdown::TableRow],
        _alignment: &[crate::markdown::TableAlignment],
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        node_index: Option<usize>,
    ) {
        // Build a proper grid that accounts for colspan and rowspan
        let (table_headers, table_rows, grid_columns) = Self::build_table_grid(header, rows);

        // Get dimensions for embedded table tracking (count grid columns including colspan)
        let num_cols = grid_columns;

        if num_cols == 0 {
            return; // Empty table
        }

        // Store table position
        let table_start_line = *total_height;

        // Create balanced column constraints based on content
        let constraints = self.calculate_balanced_column_widths(&table_headers, &table_rows, width);

        // Create table widget configuration
        let table_config = crate::table::TableConfig {
            border_color: palette.base_03,
            header_color: if is_focused {
                palette.base_0a
            } else {
                palette.base_03
            },
            text_color: if is_focused {
                palette.base_05
            } else {
                palette.base_04
            },
            use_block: false,
        };

        let header_plain = if table_headers.is_empty() {
            None
        } else {
            Some(
                table_headers
                    .iter()
                    .map(|cell| Self::inline_spans_to_plain_text(&cell.content))
                    .collect::<Vec<String>>(),
            )
        };
        let rows_plain: Vec<Vec<String>> = table_rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| Self::inline_spans_to_plain_text(&cell.content))
                    .collect()
            })
            .collect();

        // Collect image URLs from [view image] links before moving cells into table widget
        let mut image_urls: Vec<String> = Vec::new();
        fn collect_view_image_urls(inlines: &[crate::table::InlineSpan], urls: &mut Vec<String>) {
            for inline in inlines {
                if let crate::table::InlineSpan::Link { text, url } = inline {
                    let is_view_image = text.iter().any(|s| {
                        matches!(s, crate::table::InlineSpan::Text { text, .. } if text == "[view image]")
                    });
                    if is_view_image {
                        urls.push(url.clone());
                    }
                }
            }
        }
        for cell in &table_headers {
            collect_view_image_urls(&cell.content, &mut image_urls);
        }
        for row in &table_rows {
            for cell in row {
                collect_view_image_urls(&cell.content, &mut image_urls);
            }
        }

        // Create the table widget with colspan support
        let mut custom_table = crate::table::Table::new_with_colspans(table_rows)
            .constraints(constraints)
            .config(table_config);

        if !table_headers.is_empty() {
            custom_table = custom_table.header_with_colspans(table_headers);
        }

        // Set base line for link tracking
        custom_table = custom_table.base_line(table_start_line);

        // Render the table to lines
        let rendered_lines = custom_table.render_to_lines(width as u16);

        // Convert ratatui Lines to RenderedLines
        for line in rendered_lines {
            // Get raw text before moving spans
            let raw_text = self.line_to_plain_text(&line);

            // Convert line spans to our format
            let rendered_line = RenderedLine {
                spans: line.spans,
                raw_text: raw_text.clone(),
                line_type: LineType::Text,
                link_nodes: vec![],
                node_anchor: None,
                node_index,
                code_line: None,
                inline_code_comments: Vec::new(),
                canonical_content_start: None,
                content_column_start: 0,
            };

            lines.push(rendered_line);
            self.raw_text_lines.push(raw_text);
            *total_height += 1;
        }

        // Scan rendered lines for [view image] text and create LinkInfo entries
        {
            let mut url_iter = image_urls.into_iter();
            for (offset, line) in lines[table_start_line..].iter_mut().enumerate() {
                let raw = line.raw_text.clone();
                let mut search_start = 0;
                while let Some(pos) = raw[search_start..].find("[view image]") {
                    let byte_start = search_start + pos;
                    let col_start = raw[..byte_start].chars().count();
                    let col_end = col_start + "[view image]".len() - 1;
                    if let Some(url) = url_iter.next() {
                        line.link_nodes.push(LinkInfo {
                            text: "[view image]".to_string(),
                            url,
                            line: table_start_line + offset,
                            start_col: col_start,
                            end_col: col_end,
                            link_type: crate::markdown::LinkType::External,
                            target_chapter: None,
                            target_anchor: None,
                        });
                    }
                    search_start = byte_start + "[view image]".len();
                }
            }
        }

        // Store table info for click detection
        let table_height = *total_height - table_start_line;
        let num_data_rows = rows_plain.len();
        self.embedded_tables.borrow_mut().push(EmbeddedTable {
            lines_before_table: table_start_line,
            num_rows: num_data_rows + if header_plain.is_none() { 0 } else { 1 },
            num_cols,
            has_header: header_plain.is_some(),
            header_row: header_plain,
            data_rows: rows_plain,
            height_cells: table_height,
        });

        // Render comments for this table if any
        let table_comments = self.get_node_comments(node_index);
        if !table_comments.is_empty() {
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;

            self.render_node_comments(
                node_index,
                lines,
                total_height,
                width,
                palette,
                is_focused,
                0,
            );
            return; // Comments already add empty line after
        }

        // Add empty line after table
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
            canonical_content_start: None,
            content_column_start: 0,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    /// Calculate balanced column constraints for table rendering
    pub fn calculate_balanced_column_widths(
        &self,
        headers: &[crate::table::CellData],
        data_rows: &[Vec<crate::table::CellData>],
        available_width: usize,
    ) -> Vec<Constraint> {
        let header_cols: usize = headers
            .iter()
            .map(|cell| cell.colspan.max(1) as usize)
            .sum();
        let data_cols = data_rows
            .iter()
            .map(|row| row.iter().map(|cell| cell.colspan.max(1) as usize).sum())
            .max()
            .unwrap_or(0);
        let num_cols = header_cols.max(data_cols);

        if num_cols == 0 {
            return Vec::new();
        }

        let min_col_width = 8; // Minimum column width
        // Account for borders and column spacing
        let spacing_width = num_cols.saturating_sub(1);
        let total_available = available_width.saturating_sub(2 + spacing_width); // 2 for left/right borders

        // Calculate content-based widths by examining all rows
        let mut max_content_widths = vec![0; num_cols];

        let mut apply_row = |row: &[crate::table::CellData]| {
            let mut grid_col = 0usize;
            for cell in row {
                let span = cell.colspan.max(1) as usize;
                if grid_col >= num_cols {
                    break;
                }
                let span = span.min(num_cols - grid_col);
                let display_width = Self::calculate_inline_display_width(&cell.content);

                if span == 1 {
                    max_content_widths[grid_col] = max_content_widths[grid_col].max(display_width);
                } else {
                    let current_sum: usize =
                        max_content_widths[grid_col..grid_col + span].iter().sum();
                    if current_sum < display_width {
                        let deficit = display_width - current_sum;
                        let per_col = deficit / span;
                        let mut extra = deficit % span;
                        for i in 0..span {
                            max_content_widths[grid_col + i] += per_col;
                            if extra > 0 {
                                max_content_widths[grid_col + i] += 1;
                                extra -= 1;
                            }
                        }
                    }
                }

                grid_col += span;
            }
        };

        if !headers.is_empty() {
            apply_row(headers);
        }
        for row in data_rows {
            apply_row(row);
        }

        // Apply minimum width constraint and calculate total desired width
        let mut desired_widths: Vec<usize> = max_content_widths
            .into_iter()
            .map(|w| w.max(min_col_width))
            .collect();

        let total_desired: usize = desired_widths.iter().sum();

        // If total desired width exceeds available space, scale down proportionally
        if total_desired > total_available {
            let scale = total_available as f32 / total_desired as f32;
            for width in &mut desired_widths {
                *width = (*width as f32 * scale).max(min_col_width as f32) as usize;
            }

            // Ensure we don't exceed available width after scaling
            let scaled_total: usize = desired_widths.iter().sum();
            if scaled_total > total_available {
                let excess = scaled_total - total_available;
                // Remove excess from the largest column
                if let Some(max_idx) = desired_widths
                    .iter()
                    .position(|&w| w == *desired_widths.iter().max().unwrap())
                {
                    desired_widths[max_idx] = desired_widths[max_idx].saturating_sub(excess);
                }
            }
        }

        // Convert to ratatui constraints
        desired_widths
            .into_iter()
            .map(|w| Constraint::Length(w as u16))
            .collect()
    }

    /// Calculate display width of structured inline spans
    pub fn calculate_inline_display_width(inlines: &[crate::table::InlineSpan]) -> usize {
        let mut max_line = 0usize;
        let mut current = 0usize;

        for inline in inlines {
            match inline {
                crate::table::InlineSpan::Text { text, .. } => {
                    current += text.chars().count();
                }
                crate::table::InlineSpan::Link { text, .. } => {
                    current += Self::calculate_inline_display_width(text);
                }
                crate::table::InlineSpan::SoftBreak => {
                    current += 1;
                }
                crate::table::InlineSpan::HardBreak => {
                    max_line = max_line.max(current);
                    current = 0;
                }
            }
        }

        max_line.max(current)
    }

    /// Convert ratatui Line to plain text string
    pub fn line_to_plain_text(&self, line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_quote(
        &mut self,
        content: &[Node],
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        node_index: Option<usize>,
    ) {
        let lines_before_quote = lines.len();

        // Collect annotation ranges for this quote (for underline styling)
        let annotation_ranges = self.get_annotation_ranges(node_index);
        let underline_color = palette.base_0e; // Purple

        // Track cumulative character position for annotation ranges
        let mut cumulative_char_pos = 0;
        // Track paragraph index within the quote
        let mut quote_para_idx = 0;

        // Render quote content with "> " prefix
        for node in content {
            match &node.block {
                MarkdownBlock::Paragraph {
                    content: para_content,
                } if Self::content_has_image(para_content) => {
                    let canonical_len = Self::text_to_string(para_content).chars().count();
                    let lines_before_para = lines.len();
                    let current_para_idx = quote_para_idx;
                    quote_para_idx += 1;

                    self.render_content_with_images(
                        para_content,
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        Some("> "),
                        indent,
                        node_index,
                        Some(cumulative_char_pos),
                    );

                    let quote_color = if is_focused {
                        palette.base_03
                    } else {
                        palette.base_02
                    };
                    for line in lines[lines_before_para..].iter_mut() {
                        line.node_index = node_index;
                        if !matches!(line.line_type, LineType::ImagePlaceholder { .. }) {
                            line.line_type = LineType::QuoteParagraph {
                                paragraph_index: current_para_idx,
                            };
                            for span in &mut line.spans {
                                let new_style =
                                    span.style.fg(quote_color).add_modifier(Modifier::ITALIC);
                                *span = Span::styled(span.content.clone(), new_style);
                            }
                        }
                    }
                    cumulative_char_pos += canonical_len;
                }
                MarkdownBlock::Paragraph {
                    content: para_content,
                } => {
                    let canonical_len = Self::text_to_string(para_content).chars().count();
                    let lines_before_para = lines.len();
                    let current_para_idx = quote_para_idx;
                    quote_para_idx += 1;

                    // Render the rich text content with "> " prefix
                    let mut content_rich_spans = Vec::new();
                    for item in para_content.iter() {
                        content_rich_spans
                            .extend(self.render_text_or_inline(item, palette, is_focused));
                    }

                    // Apply quote styling to all rich spans
                    let quote_color = if is_focused {
                        palette.base_03
                    } else {
                        palette.base_02
                    };

                    let styled_rich_spans: Vec<RichSpan> = content_rich_spans
                        .into_iter()
                        .map(|rich_span| match rich_span {
                            RichSpan::Text(span) => RichSpan::Text(Span::styled(
                                span.content.clone(),
                                span.style.fg(quote_color).add_modifier(Modifier::ITALIC),
                            )),
                            RichSpan::Link { span, info } => RichSpan::Link {
                                span: Span::styled(
                                    span.content.clone(),
                                    span.style.fg(quote_color).add_modifier(Modifier::ITALIC),
                                ),
                                info,
                            },
                        })
                        .collect();

                    let (underlined_spans, _) = Self::apply_annotation_underlines_with_offset(
                        styled_rich_spans,
                        &annotation_ranges,
                        underline_color,
                        cumulative_char_pos,
                    );

                    self.render_text_spans(
                        &underlined_spans,
                        Some("> "),
                        lines,
                        total_height,
                        width,
                        indent,
                        false, // don't add empty line after
                        node_index,
                        Some(cumulative_char_pos),
                    );

                    for line in lines[lines_before_para..].iter_mut() {
                        line.node_index = node_index;
                        line.line_type = LineType::QuoteParagraph {
                            paragraph_index: current_para_idx,
                        };
                    }
                    cumulative_char_pos += canonical_len;
                }
                _ => {
                    let lines_before_block = lines.len();
                    self.render_node(
                        node,
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        indent + 1,
                        None,
                        RenderContext::InsideContainer,
                    );

                    for line in lines[lines_before_block..].iter_mut() {
                        line.node_index = node_index;
                        cumulative_char_pos = Self::apply_underlines_to_line_spans(
                            &mut line.spans,
                            &annotation_ranges,
                            underline_color,
                            cumulative_char_pos,
                        );
                    }
                }
            }
        }

        // Assign node_index to all quote lines
        for line in lines[lines_before_quote..].iter_mut() {
            line.node_index = node_index;
        }

        // Render comments for this quote if any
        let quote_comments = self.get_node_comments(node_index);
        if !quote_comments.is_empty() {
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;

            self.render_node_comments(
                node_index,
                lines,
                total_height,
                width,
                palette,
                is_focused,
                indent,
            );
            return; // Comments already add empty line after
        }

        // Add empty line after quote
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    pub fn render_thematic_break(
        &mut self,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        let hr_line = "─".repeat(width);

        lines.push(RenderedLine {
            spans: vec![Span::styled(
                hr_line.clone(),
                RatatuiStyle::default().fg(if is_focused {
                    palette.base_03
                } else {
                    palette.base_02
                }),
            )],
            raw_text: hr_line.clone(),
            line_type: LineType::HorizontalRule,
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
            canonical_content_start: None,
            content_column_start: 0,
        });

        self.raw_text_lines.push(hr_line);
        *total_height += 1;

        // Add empty line after horizontal rule
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_definition_list(
        &mut self,
        items: &[crate::markdown::DefinitionListItem],
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        node_index: Option<usize>,
    ) {
        // Collect annotation ranges for this definition list (for underline styling)
        let annotation_ranges = self.get_annotation_ranges(node_index);
        let underline_color = palette.base_0e; // Purple

        // Track cumulative character position for annotation ranges
        let mut cumulative_char_pos = 0;

        // Render each term-definition pair
        for (idx, item) in items.iter().enumerate() {
            // Track lines for this entire definition item
            let item_start_line = lines.len();

            // Render the term (dt) - bold and possibly colored
            let term_color = if is_focused {
                palette.base_0c // Yellow for focused
            } else {
                palette.base_03 // Dimmed when not focused
            };

            let mut term_rich_spans = Vec::new();
            for term_item in item.term.iter() {
                term_rich_spans.extend(self.render_text_or_inline(term_item, palette, is_focused));
            }

            // Apply bold styling to all term rich spans
            let styled_term_rich_spans: Vec<RichSpan> = term_rich_spans
                .into_iter()
                .map(|rich_span| match rich_span {
                    RichSpan::Text(span) => RichSpan::Text(Span::styled(
                        span.content.clone(),
                        span.style.fg(term_color).add_modifier(Modifier::BOLD),
                    )),
                    RichSpan::Link { span, info } => RichSpan::Link {
                        span: Span::styled(
                            span.content.clone(),
                            span.style.fg(term_color).add_modifier(Modifier::BOLD),
                        ),
                        info,
                    },
                })
                .collect();

            // Apply annotation underlines to the term
            let (final_term_spans, _) = Self::apply_annotation_underlines_with_offset(
                styled_term_rich_spans,
                &annotation_ranges,
                underline_color,
                cumulative_char_pos,
            );

            let lines_before_term = lines.len();

            let canonical_len = Self::text_to_string(&item.term).chars().count();

            self.render_text_spans(
                &final_term_spans,
                None, // no prefix for terms
                lines,
                total_height,
                width,
                0,     // no indentation for terms
                false, // don't add empty line after
                None,
                Some(cumulative_char_pos),
            );

            for line in lines[lines_before_term..].iter_mut() {
                line.node_index = node_index;
                line.line_type = LineType::DefinitionListItem {
                    item_index: idx,
                    is_term: true,
                };
            }
            cumulative_char_pos += canonical_len;

            // Render each definition (dd) - as blocks with indentation
            for definition_blocks in &item.definitions {
                for block_node in definition_blocks {
                    let lines_before_def = lines.len();

                    self.render_node(
                        block_node,
                        lines,
                        total_height,
                        width.saturating_sub(4),
                        palette,
                        is_focused,
                        2,
                        None, // Definition blocks get their own node handling
                        RenderContext::InsideContainer,
                    );

                    // Assign node_index, line_type, apply underlines, and update cumulative position
                    for line in lines[lines_before_def..].iter_mut() {
                        line.node_index = node_index;
                        line.line_type = LineType::DefinitionListItem {
                            item_index: idx,
                            is_term: false,
                        };
                        // Align canonical_content_start with the underline coordinate space
                        // (apply_underlines_to_line_spans counts all chars including indent)
                        line.canonical_content_start =
                            Some(cumulative_char_pos + line.content_column_start);
                        cumulative_char_pos = Self::apply_underlines_to_line_spans(
                            &mut line.spans,
                            &annotation_ranges,
                            underline_color,
                            cumulative_char_pos,
                        );
                    }
                }
            }

            // Smart spacing: add empty line between items if this entire item (term + definitions) is >2 lines
            if idx + 1 < items.len() {
                let item_line_count = lines.len() - item_start_line;
                if item_line_count > 2 {
                    lines.push(RenderedLine::empty());
                    self.raw_text_lines.push(String::new());
                    *total_height += 1;
                }
            }
        }

        // Render comments for this definition list if any
        let def_comments = self.get_node_comments(node_index);
        if !def_comments.is_empty() {
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;

            self.render_node_comments(
                node_index,
                lines,
                total_height,
                width,
                palette,
                is_focused,
                0,
            );
            return; // Comments already add empty line after
        }

        // Add empty line after the entire definition list (only if no comments)
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_epub_block(
        &mut self,
        _epub_type: &str,
        _element_name: &str,
        content: &[crate::markdown::Node],
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Add line separator before the block
        let separator_line = ".".repeat(width);
        lines.push(RenderedLine {
            spans: vec![Span::styled(
                separator_line.clone(),
                RatatuiStyle::default().fg(if is_focused {
                    palette.base_03
                } else {
                    palette.base_02
                }),
            )],
            raw_text: separator_line.clone(),
            line_type: LineType::HorizontalRule,
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
            canonical_content_start: None,
            content_column_start: 0,
        });
        self.raw_text_lines.push(separator_line.clone());
        *total_height += 1;
        //
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;

        // Render the content blocks with controlled spacing
        for (idx, content_node) in content.iter().enumerate() {
            // Render the content node normally
            match &content_node.block {
                MarkdownBlock::Heading { content, .. } => {
                    self.render_heading(
                        HeadingLevel::H5, // remap to always same time of heading to avoid visual hierarchy issues
                        content,
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        None, // No annotation support inside containers
                    );
                }
                _ => {
                    self.render_node(
                        content_node,
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        0,
                        None,
                        RenderContext::InsideContainer,
                    );
                }
            }

            let next_block = content.get(idx + 1).map(|n| &n.block);
            let needs_spacing = matches!(&content_node.block, MarkdownBlock::Paragraph { .. })
                && next_block.is_some()
                || matches!(
                    (&content_node.block, next_block),
                    (
                        MarkdownBlock::CodeBlock { .. },
                        Some(MarkdownBlock::Paragraph { .. })
                    )
                );

            if needs_spacing
                && !matches!(
                    lines.last(),
                    Some(RenderedLine {
                        line_type: LineType::Empty,
                        ..
                    })
                )
            {
                lines.push(RenderedLine::empty());
                self.raw_text_lines.push(String::new());
                *total_height += 1;
            }
        }

        // Add line separator after the block
        lines.push(RenderedLine {
            spans: vec![Span::styled(
                separator_line.clone(),
                RatatuiStyle::default().fg(if is_focused {
                    palette.base_03
                } else {
                    palette.base_02
                }),
            )],
            raw_text: separator_line.clone(),
            line_type: LineType::HorizontalRule,
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
            canonical_content_start: None,
            content_column_start: 0,
        });
        self.raw_text_lines.push(separator_line);
        *total_height += 1;

        // Add empty line after the block
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    /// Render paragraph content that may contain images, splitting at image boundaries.
    /// Used by render_list and render_quote to handle images inside containers.
    #[allow(clippy::too_many_arguments)]
    fn render_content_with_images(
        &mut self,
        content: &MarkdownText,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        first_prefix: Option<&str>,
        indent: usize,
        node_index: Option<usize>,
        canonical_offset: Option<usize>,
    ) {
        let mut current_rich_spans: Vec<RichSpan> = Vec::new();
        let mut current_chunk_items: Vec<&TextOrInline> = Vec::new();
        let mut is_first_chunk = true;
        let mut para_canonical_offset: usize = canonical_offset.unwrap_or(0);

        for item in content.iter() {
            match item {
                TextOrInline::Inline(Inline::Image { url, .. }) => {
                    // Flush accumulated text before image
                    if !current_rich_spans.is_empty() {
                        let chunk_canonical_len = current_chunk_items
                            .iter()
                            .map(|i| Self::text_or_inline_canonical_len(i))
                            .sum::<usize>();
                        let prefix = if is_first_chunk { first_prefix } else { None };
                        self.render_text_spans(
                            &current_rich_spans,
                            prefix,
                            lines,
                            total_height,
                            width,
                            indent,
                            false,
                            node_index,
                            Some(para_canonical_offset),
                        );
                        para_canonical_offset += chunk_canonical_len;
                        current_rich_spans = Vec::new();
                        current_chunk_items.clear();
                        is_first_chunk = false;
                    }
                    self.render_image_placeholder(url, lines, total_height, width, palette);
                }
                TextOrInline::Inline(Inline::Link {
                    text: link_text,
                    url,
                    link_type,
                    target_chapter,
                    target_anchor,
                    ..
                }) if Self::extract_image_from_text(link_text).is_some() => {
                    // Flush accumulated text before image link
                    if !current_rich_spans.is_empty() {
                        let chunk_canonical_len = current_chunk_items
                            .iter()
                            .map(|i| Self::text_or_inline_canonical_len(i))
                            .sum::<usize>();
                        let prefix = if is_first_chunk { first_prefix } else { None };
                        self.render_text_spans(
                            &current_rich_spans,
                            prefix,
                            lines,
                            total_height,
                            width,
                            indent,
                            false,
                            node_index,
                            Some(para_canonical_offset),
                        );
                        para_canonical_offset += chunk_canonical_len;
                        current_rich_spans = Vec::new();
                        current_chunk_items.clear();
                        is_first_chunk = false;
                    }
                    let image_url = Self::extract_image_from_text(link_text).unwrap();
                    let link_info = LinkInfo {
                        text: String::new(),
                        url: url.clone(),
                        line: 0,
                        start_col: 0,
                        end_col: 0,
                        link_type: link_type.clone(),
                        target_chapter: target_chapter.clone(),
                        target_anchor: target_anchor.clone(),
                    };
                    self.render_image_placeholder_with_link(
                        &image_url,
                        lines,
                        total_height,
                        width,
                        palette,
                        Some(link_info),
                    );
                }
                _ => {
                    let rich_spans = self.render_text_or_inline(item, palette, is_focused);
                    current_rich_spans.extend(rich_spans);
                    current_chunk_items.push(item);
                }
            }
        }

        // Flush remaining text
        if !current_rich_spans.is_empty() {
            let prefix = if is_first_chunk { first_prefix } else { None };
            self.render_text_spans(
                &current_rich_spans,
                prefix,
                lines,
                total_height,
                width,
                indent,
                false,
                node_index,
                Some(para_canonical_offset),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_text_spans(
        &mut self,
        rich_spans: &[RichSpan],
        prefix: Option<&str>,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        indent: usize,
        add_empty_line_after: bool,
        node_index: Option<usize>,
        canonical_base_offset: Option<usize>,
    ) {
        let prefix_text = prefix.unwrap_or("");
        let prefix_width = prefix_text.chars().count();
        let prefix_padding = if prefix_width > 0 {
            " ".repeat(prefix_width)
        } else {
            String::new()
        };
        let wrappable_rich_spans: Vec<RichSpan> = rich_spans.to_vec();

        // Convert rich spans to plain text for wrapping
        let plain_text = wrappable_rich_spans
            .iter()
            .map(|rs| match rs {
                RichSpan::Text(span) => span.content.as_ref(),
                RichSpan::Link { span, .. } => span.content.as_ref(),
            })
            .collect::<String>();

        // Calculate available width after accounting for indentation
        let indent_str = "  ".repeat(indent);
        let indent_width_chars = indent_str.chars().count();
        let mut available_width = width.saturating_sub(indent_width_chars);
        if prefix_width > 0 {
            available_width = available_width.saturating_sub(prefix_width);
        }
        let available_width = available_width.max(1);

        // Wrap the text
        let wrapped = textwrap::wrap(&plain_text, available_width);

        let plain_chars: Vec<char> = plain_text.chars().collect();
        let mut plain_idx: usize = 0;

        // Create lines from wrapped text
        for (line_idx, wrapped_line) in wrapped.iter().enumerate() {
            let wl_chars_count = wrapped_line.chars().count();

            // Skip whitespace consumed by wrapping between lines
            if line_idx > 0 {
                while plain_idx < plain_chars.len() && plain_chars[plain_idx] == ' ' {
                    plain_idx += 1;
                }
            }

            let mut line_spans = Vec::new();
            let mut line_links = Vec::new();

            // Map wrapped line back to rich spans
            let rich_spans_for_line = if line_idx == 0 && wrapped.len() == 1 {
                // Single line - use all rich spans
                wrappable_rich_spans.clone()
            } else {
                // Multi-line content: map wrapped line back to rich spans
                self.map_wrapped_line_to_rich_spans(wrapped_line, &wrappable_rich_spans)
            };

            // Extract spans and links, calculating positions
            let mut current_col = 0;
            for rich_span in rich_spans_for_line {
                match rich_span {
                    RichSpan::Text(span) => {
                        let len = span.content.chars().count();
                        line_spans.push(span);
                        current_col += len;
                    }
                    RichSpan::Link { span, mut info } => {
                        let len = span.content.chars().count();
                        info.line = lines.len(); // Set to current line being created
                        info.start_col = current_col;
                        info.end_col = current_col + len;

                        line_links.push(info);
                        line_spans.push(span);
                        current_col += len;
                    }
                }
            }

            // Apply indentation by prepending indent span
            if indent > 0 {
                line_spans.insert(0, Span::raw(indent_str.clone()));
                // Adjust link positions for indentation
                for link in &mut line_links {
                    link.start_col += indent_width_chars;
                    link.end_col += indent_width_chars;
                }
            }

            if prefix_width > 0 {
                let insert_at = if indent > 0 { 1 } else { 0 };
                if line_idx == 0 {
                    line_spans.insert(insert_at, Span::raw(prefix_text.to_string()));
                } else {
                    line_spans.insert(insert_at, Span::raw(prefix_padding.clone()));
                }
                for link in &mut line_links {
                    link.start_col += prefix_width;
                    link.end_col += prefix_width;
                }
            }

            // Build the final raw text with indentation and prefix padding
            let mut final_raw_text = String::new();
            if indent > 0 {
                final_raw_text.push_str(&indent_str);
            }
            if prefix_width > 0 {
                if line_idx == 0 {
                    final_raw_text.push_str(prefix_text);
                } else {
                    final_raw_text.push_str(&prefix_padding);
                }
            }
            final_raw_text.push_str(wrapped_line.as_ref());

            let content_col_start = indent_width_chars + prefix_width;

            lines.push(RenderedLine {
                spans: line_spans,
                raw_text: final_raw_text.clone(),
                line_type: LineType::Text,
                link_nodes: line_links, // Captured links!
                node_anchor: None,
                node_index,
                code_line: None,
                inline_code_comments: Vec::new(),
                canonical_content_start: canonical_base_offset.map(|base| base + plain_idx),
                content_column_start: content_col_start,
            });

            self.raw_text_lines.push(final_raw_text);
            *total_height += 1;

            plain_idx += wl_chars_count;
        }

        // Add empty line after if requested
        if add_empty_line_after {
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }
    }

    /// Map a wrapped line back to its rich spans, preserving links
    pub fn map_wrapped_line_to_rich_spans(
        &self,
        wrapped_line: &str,
        original_rich_spans: &[RichSpan],
    ) -> Vec<RichSpan> {
        // Build a flattened representation with rich span info
        #[derive(Clone)]
        struct CharWithRichSpan {
            ch: char,
            rich_span_idx: usize, // Index into original_rich_spans
            #[allow(dead_code)]
            char_idx_in_span: usize, // Position within the span's text
        }

        let mut chars_with_rich = Vec::new();
        for (span_idx, rich_span) in original_rich_spans.iter().enumerate() {
            let span_text = match rich_span {
                RichSpan::Text(span) => &span.content,
                RichSpan::Link { span, .. } => &span.content,
            };
            for (char_idx, ch) in span_text.chars().enumerate() {
                chars_with_rich.push(CharWithRichSpan {
                    ch,
                    rich_span_idx: span_idx,
                    char_idx_in_span: char_idx,
                });
            }
        }

        // Find where this wrapped line starts in the original content
        let wrapped_chars: Vec<char> = wrapped_line.chars().collect();
        if wrapped_chars.is_empty() {
            return vec![RichSpan::Text(Span::raw(""))];
        }

        // Find the starting position
        let mut start_pos = None;
        for i in 0..=chars_with_rich.len().saturating_sub(wrapped_chars.len()) {
            let mut matches = true;
            for (j, &wrapped_ch) in wrapped_chars.iter().enumerate() {
                if i + j >= chars_with_rich.len() || chars_with_rich[i + j].ch != wrapped_ch {
                    matches = false;
                    break;
                }
            }
            if matches {
                start_pos = Some(i);
                break;
            }
        }

        // If we found the position, reconstruct the rich spans
        if let Some(pos) = start_pos {
            let mut result_spans = Vec::new();
            let mut current_span_idx = None;
            let mut current_text = String::new();

            for i in pos..pos + wrapped_chars.len() {
                if i >= chars_with_rich.len() {
                    break;
                }

                let char_info = &chars_with_rich[i];

                if current_span_idx != Some(char_info.rich_span_idx) {
                    // Span changed, push accumulated span
                    if !current_text.is_empty() {
                        if let Some(idx) = current_span_idx {
                            // Clone the original rich span but with new text
                            let new_rich_span = match &original_rich_spans[idx] {
                                RichSpan::Text(original_span) => RichSpan::Text(Span::styled(
                                    current_text.clone(),
                                    original_span.style,
                                )),
                                RichSpan::Link {
                                    span: original_span,
                                    info,
                                } => RichSpan::Link {
                                    span: Span::styled(current_text.clone(), original_span.style),
                                    info: info.clone(),
                                },
                            };
                            result_spans.push(new_rich_span);
                        }
                        current_text.clear();
                    }
                    current_span_idx = Some(char_info.rich_span_idx);
                }

                current_text.push(char_info.ch);
            }

            // Push final accumulated span
            if !current_text.is_empty() {
                if let Some(idx) = current_span_idx {
                    let new_rich_span = match &original_rich_spans[idx] {
                        RichSpan::Text(original_span) => {
                            RichSpan::Text(Span::styled(current_text, original_span.style))
                        }
                        RichSpan::Link {
                            span: original_span,
                            info,
                        } => RichSpan::Link {
                            span: Span::styled(current_text, original_span.style),
                            info: info.clone(),
                        },
                    };
                    result_spans.push(new_rich_span);
                }
            }

            if result_spans.is_empty() {
                vec![RichSpan::Text(Span::raw(wrapped_line.to_string()))]
            } else {
                result_spans
            }
        } else {
            // Fallback if we can't find the position
            vec![RichSpan::Text(Span::raw(wrapped_line.to_string()))]
        }
    }

    fn render_image_placeholder(
        &mut self,
        url: &str,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
    ) {
        self.render_image_placeholder_with_link(url, lines, total_height, width, palette, None);
    }

    fn render_image_placeholder_with_link(
        &mut self,
        url: &str,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        link_info: Option<LinkInfo>,
    ) {
        // Add empty line before image
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;

        // Store the position where the image will be rendered
        let lines_before_image = *total_height;

        // Check if we have image dimensions already loaded
        let (image_height, loading_status) =
            if let Some(embedded_image) = self.embedded_images.borrow().get(url) {
                let height = embedded_image.height_cells;
                let status = match &embedded_image.state {
                    ImageLoadState::Loaded { .. } => {
                        crate::images::image_placeholder::LoadingStatus::Loaded
                    }
                    ImageLoadState::Failed { .. } => {
                        crate::images::image_placeholder::LoadingStatus::Failed
                    }
                    ImageLoadState::Unsupported => {
                        crate::images::image_placeholder::LoadingStatus::Unsupported
                    }
                    ImageLoadState::NotLoaded | ImageLoadState::Loading => {
                        crate::images::image_placeholder::LoadingStatus::Loading
                    }
                };
                (height, status)
            } else {
                // Image not in map yet - check if picker exists to determine status
                let status = if self.image_picker.is_none() {
                    crate::images::image_placeholder::LoadingStatus::Unsupported
                } else {
                    crate::images::image_placeholder::LoadingStatus::Loading
                };
                (IMAGE_HEIGHT_WIDE, status)
            };

        // Update or insert the embedded image info
        self.embedded_images
            .borrow_mut()
            .entry(url.to_string())
            .or_insert_with(|| EmbeddedImage {
                src: url.to_string(),
                lines_before_image,
                height_cells: image_height,
                width: 200,  // Default width, will be updated when loaded
                height: 200, // Default height, will be updated when loaded
                state: ImageLoadState::NotLoaded,
            })
            .lines_before_image = lines_before_image;

        // Create image placeholder configuration
        let config = crate::images::image_placeholder::ImagePlaceholderConfig {
            internal_padding: 4,
            total_height: image_height as usize,
            border_color: palette.base_03,
        };

        // Create the placeholder
        let placeholder = crate::images::image_placeholder::ImagePlaceholder::new(
            url,
            width,
            &config,
            loading_status != crate::images::image_placeholder::LoadingStatus::Loaded,
            loading_status,
        );

        // Add all the placeholder lines
        for (raw_line, styled_line) in placeholder
            .raw_lines
            .into_iter()
            .zip(placeholder.styled_lines.into_iter())
        {
            // Update link coordinates for this specific line
            let line_link_nodes: Vec<_> = link_info
                .clone()
                .map(|mut info| {
                    info.line = lines.len();
                    info.start_col = 0;
                    info.end_col = width.saturating_sub(1);
                    info
                })
                .into_iter()
                .collect();

            lines.push(RenderedLine {
                spans: styled_line.spans,
                raw_text: raw_line,
                line_type: LineType::ImagePlaceholder {
                    src: url.to_string(),
                },
                link_nodes: line_link_nodes,
                node_anchor: None,
                node_index: None,
                code_line: None,
                inline_code_comments: Vec::new(),
                canonical_content_start: None,
                content_column_start: 0,
            });

            self.raw_text_lines.push(String::new()); // Keep raw_text_lines in sync
            *total_height += 1;
        }

        // Add empty line after image
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }
}

#[cfg(test)]
mod tests {
    use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
    use crate::theme;

    fn assert_eq_multiline(name: &str, left: &str, right: &str) {
        if left == right {
            return;
        }

        let left_lines: Vec<&str> = left.lines().collect();
        let right_lines: Vec<&str> = right.lines().collect();
        let max_lines = left_lines.len().max(right_lines.len());

        let mut message = String::new();
        message.push_str(&format!("{name}\n"));
        message.push_str("Lines: left | right\n");

        for i in 0..max_lines {
            let left_line = left_lines.get(i).copied().unwrap_or("");
            let right_line = right_lines.get(i).copied().unwrap_or("");
            let marker = if left_line == right_line { " " } else { "!" };
            message.push_str(&format!(
                "{marker} {ln:>3} L: {left_line}\n    {spacer} R: {right_line}\n",
                marker = marker,
                ln = i + 1,
                left_line = left_line,
                spacer = " ",
                right_line = right_line
            ));
        }

        panic!("{message}");
    }

    fn split_code_line(line: &str, width: usize) -> Vec<String> {
        if line.is_empty() {
            return vec![String::new()];
        }

        let mut segments = Vec::new();
        let mut current = String::new();
        let mut count = 0usize;

        for ch in line.chars() {
            current.push(ch);
            count += 1;
            if count >= width {
                segments.push(current);
                current = String::new();
                count = 0;
            }
        }

        if !current.is_empty() {
            segments.push(current);
        }

        segments
    }

    #[test]
    fn test_render_simple_table_3x3() {
        let html = r#"
        <table>
            <tr><th>H1</th><th>H2</th><th>H3</th></tr>
            <tr><td>R1C1</td><td>R1C2</td><td>R1C3</td></tr>
            <tr><td>R2C1</td><td>R2C2</td><td>R2C3</td></tr>
        </table>
        "#;

        let mut converter = HtmlToMarkdownConverter::new();
        let doc = converter.convert(html);

        let mut reader = crate::markdown_text_reader::MarkdownTextReader::new();
        let rendered = reader.render_document_to_lines(&doc, 40, theme::current_theme(), true);

        let rendered_text = rendered
            .lines
            .iter()
            .map(|line| line.raw_text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let expected_lines = [
            "┌────────┬────────┬────────┐",
            "│H1      │H2      │H3      │",
            "├────────┼────────┼────────┤",
            "│R1C1    │R1C2    │R1C3    │",
            "│R2C1    │R2C2    │R2C3    │",
            "└────────┴────────┴────────┘",
            "",
        ];
        let expected = expected_lines.join("\n");

        assert_eq!(rendered_text, expected);
    }

    #[test]
    fn test_render_table_with_lists() {
        let html = r#"
        <table>
            <tr><th>Type</th><th>Items</th></tr>
            <tr>
                <td>Unordered</td>
                <td>
                    <ul>
                        <li>Apple</li>
                        <li>Banana</li>
                    </ul>
                </td>
            </tr>
            <tr>
                <td>Ordered</td>
                <td>
                    <ol>
                        <li>One</li>
                        <li>Two</li>
                    </ol>
                </td>
            </tr>
        </table>
        "#;

        let mut converter = HtmlToMarkdownConverter::new();
        let doc = converter.convert(html);

        let mut reader = crate::markdown_text_reader::MarkdownTextReader::new();
        let rendered = reader.render_document_to_lines(&doc, 60, theme::current_theme(), true);

        let rendered_text = rendered
            .lines
            .iter()
            .map(|line| line.raw_text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let expected = r#"┌─────────┬────────┐
│Type     │Items   │
├─────────┼────────┤
│Unordered│• Apple │
│         │• Banana│
│Ordered  │1. One  │
│         │2. Two  │
└─────────┴────────┘
"#;
        assert_eq_multiline("test_render_table_with_lists", &rendered_text, expected);
    }

    #[test]
    fn test_render_table_with_nested_table() {
        let html = r#"
        <table>
            <tr><th>Outer</th><th>Details</th></tr>
            <tr>
                <td>Row 1</td>
                <td>
                    <table>
                        <tr><th>H1</th><th>H2</th></tr>
                        <tr><td>A</td><td>B</td></tr>
                    </table>
                </td>
            </tr>
        </table>
        "#;

        let mut converter = HtmlToMarkdownConverter::new();
        let doc = converter.convert(html);

        let mut reader = crate::markdown_text_reader::MarkdownTextReader::new();
        let rendered = reader.render_document_to_lines(&doc, 80, theme::current_theme(), true);

        let rendered_text = rendered
            .lines
            .iter()
            .map(|line| line.raw_text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let expected = r#"┌────────┬────────┐
│Outer   │Details │
├────────┼────────┤
│Row 1   │H1 | H2 │
│        │A  | B  │
└────────┴────────┘
"#;
        assert_eq_multiline(
            "test_render_table_with_nested_table",
            &rendered_text,
            expected,
        );
    }

    #[test]
    fn test_render_code_block_wraps_and_preserves_blank_lines() {
        let html = r#"<pre>alpha
 
beta beta beta beta beta</pre>"#;

        let mut converter = HtmlToMarkdownConverter::new();
        let doc = converter.convert(html);

        let mut reader = crate::markdown_text_reader::MarkdownTextReader::new();
        let rendered = reader.render_document_to_lines(&doc, 12, theme::current_theme(), true);

        let rendered_text = rendered
            .lines
            .iter()
            .map(|line| line.raw_text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let code_lines = vec!["alpha", " ", "beta beta beta beta beta"];
        let mut expected_lines = Vec::new();
        for line in code_lines {
            let segments = split_code_line(line, 12);
            for segment in segments {
                expected_lines.push(segment);
            }
        }
        expected_lines.push(String::new());

        let expected = expected_lines.join("\n");
        assert_eq_multiline(
            "test_render_code_block_wraps_and_preserves_blank_lines",
            &rendered_text,
            &expected,
        );
    }
}
