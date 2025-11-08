use super::types::*;
use crate::markdown::{
    Block as MarkdownBlock, Document, HeadingLevel, Inline, Node, Style, Text as MarkdownText,
    TextOrInline,
};
use crate::theme::Base16Palette;
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
                );
            }

            List { kind, items } => {
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

        let modifiers = match level {
            HeadingLevel::H3 => Modifier::BOLD | Modifier::UNDERLINED,
            HeadingLevel::H4 => Modifier::BOLD | Modifier::UNDERLINED,
            _ => Modifier::BOLD,
        };

        for wrapped_line in wrapped {
            let styled_spans = vec![Span::styled(
                wrapped_line.to_string(),
                RatatuiStyle::default()
                    .fg(heading_color)
                    .add_modifier(modifiers),
            )];

            lines.push(RenderedLine {
                spans: styled_spans,
                raw_text: wrapped_line.to_string(),
                line_type: LineType::Heading {
                    level: level.as_u8(),
                    needs_decoration: false,
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
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
            });

            self.raw_text_lines.push(decoration);
            *total_height += 1;
        }

        // Add empty line after heading
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
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

        let mut current_rich_spans = Vec::new();
        let mut has_content = false;

        for item in content.iter() {
            match item {
                TextOrInline::Inline(Inline::Image { url, .. }) => {
                    // If we have accumulated text before the image, render it first
                    if !current_rich_spans.is_empty() {
                        self.render_text_spans(
                            &current_rich_spans,
                            None, // no prefix
                            lines,
                            total_height,
                            width,
                            indent,
                            false, // don't add empty line after
                        );
                        current_rich_spans.clear();
                    }

                    // Render the image as a separate block
                    self.render_image_placeholder(url, lines, total_height, width, palette);
                    has_content = true;
                }
                _ => {
                    // Accumulate non-image content
                    let rich_spans = self.render_text_or_inline(item, palette, is_focused);
                    current_rich_spans.extend(rich_spans);
                }
            }
        }

        // Render any remaining text spans
        if !current_rich_spans.is_empty() {
            let add_empty_line = context == RenderContext::TopLevel;
            self.render_text_spans(
                &current_rich_spans,
                None,
                lines,
                total_height,
                width,
                indent,
                add_empty_line,
            );
        } else if !has_content {
            // Empty paragraph - just add an empty line
            lines.push(RenderedLine {
                spans: vec![Span::raw("")],
                raw_text: String::new(),
                line_type: LineType::Empty,
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
            });
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }

        if let Some(node_idx) = node_index {
            let comments_to_render = self.current_chapter_comments.get(&node_idx).cloned();
            if let Some(paragraph_comments) = comments_to_render {
                for comment in paragraph_comments {
                    self.render_comment_as_quote(
                        &comment,
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        indent,
                    );
                }
            }
        }
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

                    Inline::Image { alt_text, .. } => {
                        rich_spans.push(RichSpan::Text(Span::raw(format!("[image: {alt_text}]"))));
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

    #[allow(clippy::too_many_arguments)]
    pub fn render_code_block(
        &mut self,
        language: Option<&str>,
        content: &str,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        _width: usize, //todo: not supported yet
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        add_spacing_before: bool,
    ) {
        // TODO: Implement syntax highlighting if language is provided
        if add_spacing_before {
            lines.push(RenderedLine::empty());
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }

        let indent_str = "  ".repeat(indent);
        let code_lines: Vec<&str> = content.lines().collect();

        for code_line in code_lines {
            let mut spans = Vec::new();
            let mut display_text = String::new();

            if !indent_str.is_empty() {
                spans.push(Span::raw(indent_str.clone()));
                display_text.push_str(&indent_str);
            }

            let styled_span = Span::styled(
                code_line.to_string(),
                RatatuiStyle::default()
                    .fg(if is_focused {
                        palette.base_0b
                    } else {
                        palette.base_03
                    })
                    .bg(palette.base_00),
            );

            display_text.push_str(code_line);
            spans.push(styled_span);

            lines.push(RenderedLine {
                spans,
                raw_text: display_text.clone(),
                line_type: LineType::CodeBlock {
                    language: language.map(String::from),
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
            });

            self.raw_text_lines.push(display_text);
            *total_height += 1;
        }

        lines.push(RenderedLine::empty());

        self.raw_text_lines.push(String::new());
        *total_height += 1;
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
    ) {
        use crate::markdown::ListKind;

        for (idx, item) in items.iter().enumerate() {
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
                        MarkdownBlock::Paragraph { content } => {
                            let mut content_rich_spans = Vec::new();
                            for item in content.iter() {
                                content_rich_spans
                                    .extend(self.render_text_or_inline(item, palette, is_focused));
                            }

                            let lines_before = lines.len();

                            self.render_text_spans(
                                &content_rich_spans,
                                Some(&prefix),
                                lines,
                                total_height,
                                width,
                                indent,
                                false,
                            );

                            first_block_line_count = lines.len() - lines_before;

                            for (i, line) in lines[lines_before..].iter_mut().enumerate() {
                                line.line_type = LineType::ListItem {
                                    kind: kind.clone(),
                                    indent,
                                };
                                if i == 0 && idx == 0 {
                                    line.node_index = node_index;
                                }
                            }
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

            // Smart spacing: add empty line between items if first block is >2 lines
            if idx + 1 < items.len() && first_block_line_count > 2 {
                lines.push(RenderedLine::empty());
                self.raw_text_lines.push(String::new());
                *total_height += 1;
            }
        }

        // Render comments for the list if it has a node_index
        if let Some(node_idx) = node_index {
            let comments_to_render = self.current_chapter_comments.get(&node_idx).cloned();
            if let Some(paragraph_comments) = comments_to_render {
                if !paragraph_comments.is_empty() {
                    lines.push(RenderedLine::empty());
                    self.raw_text_lines.push(String::new());
                    *total_height += 1;

                    for comment in paragraph_comments {
                        self.render_comment_as_quote(
                            &comment,
                            lines,
                            total_height,
                            width,
                            palette,
                            is_focused,
                            indent,
                        );
                    }
                    return; // render_comment_as_quote already adds empty line after
                }
            }
        }

        // Add empty line after list (only if no comments were rendered)
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
    ) {
        // Convert markdown table to String format for Table widget
        let mut table_rows = Vec::new();
        let mut table_headers = Vec::new();

        // Process header
        if let Some(header_row) = header {
            table_headers = header_row
                .cells
                .iter()
                .map(|cell| Self::text_to_string(&cell.content))
                .collect();
        }

        // Process data rows
        for row in rows {
            let row_data: Vec<String> = row
                .cells
                .iter()
                .map(|cell| Self::text_to_string(&cell.content))
                .collect();
            table_rows.push(row_data);
        }

        // Get dimensions for embedded table tracking
        let num_cols = table_headers
            .len()
            .max(table_rows.iter().map(|r| r.len()).max().unwrap_or(0));

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

        // Create the table widget
        let mut custom_table = crate::table::Table::new(table_rows.clone())
            .constraints(constraints)
            .config(table_config);

        if !table_headers.is_empty() {
            custom_table = custom_table.header(table_headers.clone());
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
                line_type: LineType::Text, // Table widget handles its own styling
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
            };

            lines.push(rendered_line);
            self.raw_text_lines.push(raw_text);
            *total_height += 1;
        }

        // Extract and store links from the table
        let table_links = custom_table.get_links();
        self.links.extend(table_links.clone());

        // Store table info for click detection
        let table_height = *total_height - table_start_line;
        let num_data_rows = table_rows.len();
        self.embedded_tables.borrow_mut().push(EmbeddedTable {
            lines_before_table: table_start_line,
            num_rows: num_data_rows + if table_headers.is_empty() { 0 } else { 1 },
            num_cols,
            has_header: !table_headers.is_empty(),
            header_row: if table_headers.is_empty() {
                None
            } else {
                Some(table_headers)
            },
            data_rows: table_rows,
            height_cells: table_height,
        });

        // Add empty line after table
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    /// Calculate balanced column constraints for table rendering
    pub fn calculate_balanced_column_widths(
        &self,
        headers: &[String],
        data_rows: &[Vec<String>],
        available_width: usize,
    ) -> Vec<Constraint> {
        let num_cols = headers
            .len()
            .max(data_rows.iter().map(|r| r.len()).max().unwrap_or(0));

        if num_cols == 0 {
            return Vec::new();
        }

        let min_col_width = 8; // Minimum column width
        // Account for borders and column spacing
        let spacing_width = num_cols.saturating_sub(1);
        let total_available = available_width.saturating_sub(2 + spacing_width); // 2 for left/right borders

        // Calculate content-based widths by examining all rows
        let mut max_content_widths = vec![0; num_cols];

        // Check header row
        for (col_idx, cell) in headers.iter().enumerate() {
            if col_idx < max_content_widths.len() {
                let display_width = self.calculate_display_width(cell);
                max_content_widths[col_idx] = max_content_widths[col_idx].max(display_width);
            }
        }

        // Check all data rows
        for row in data_rows {
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx < max_content_widths.len() {
                    let display_width = self.calculate_display_width(cell);
                    max_content_widths[col_idx] = max_content_widths[col_idx].max(display_width);
                }
            }
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

    /// Calculate display width of text, excluding markdown formatting markers
    pub fn calculate_display_width(&self, text: &str) -> usize {
        // Strip markdown formatting markers for width calculation
        let mut display_text = text.to_string();

        // Handle <br/> tags - each represents a line break, so find the longest line
        if display_text.contains("<br/>") {
            return display_text
                .replace("<br/> ", "\n")
                .replace("<br/>", "\n")
                .lines()
                .map(|line| {
                    // Strip markdown from each line and get its width
                    let stripped = line
                        .replace("**", "")
                        .replace("__", "")
                        .replace("*", "")
                        .replace("_", "");
                    stripped.chars().count()
                })
                .max()
                .unwrap_or(0);
        }

        // Strip markdown formatting markers
        display_text = display_text.replace("**", "");
        display_text = display_text.replace("__", "");
        display_text = display_text.replace("*", "");
        display_text = display_text.replace("_", "");

        display_text.chars().count()
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
    ) {
        // Render quote content with "> " prefix
        for node in content {
            match &node.block {
                MarkdownBlock::Paragraph {
                    content: para_content,
                } => {
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

                    self.render_text_spans(
                        &styled_rich_spans,
                        Some("> "), // quote prefix
                        lines,
                        total_height,
                        width,
                        indent,
                        false, // don't add empty line after
                    );
                }
                _ => {
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
                }
            }
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
    ) {
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

            self.render_text_spans(
                &styled_term_rich_spans,
                None, // no prefix for terms
                lines,
                total_height,
                width,
                0,     // no indentation for terms
                false, // don't add empty line after
            );

            // Render each definition (dd) - as blocks with indentation
            for definition_blocks in &item.definitions {
                for block_node in definition_blocks {
                    self.render_node(
                        block_node,
                        lines,
                        total_height,
                        width.saturating_sub(4),
                        palette,
                        is_focused,
                        2,
                        None,
                        RenderContext::InsideContainer,
                    );
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

        // Add empty line after the entire definition list
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
        });
        self.raw_text_lines.push(separator_line);
        *total_height += 1;

        // Add empty line after the block
        lines.push(RenderedLine::empty());
        self.raw_text_lines.push(String::new());
        *total_height += 1;
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

        // Create lines from wrapped text
        for (line_idx, wrapped_line) in wrapped.iter().enumerate() {
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

            lines.push(RenderedLine {
                spans: line_spans,
                raw_text: final_raw_text.clone(),
                line_type: LineType::Text,
                link_nodes: line_links, // Captured links!
                node_anchor: None,
                node_index: None,
            });

            self.raw_text_lines.push(final_raw_text);
            *total_height += 1;
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
            lines.push(RenderedLine {
                spans: styled_line.spans,
                raw_text: raw_line,
                line_type: LineType::ImagePlaceholder {
                    src: url.to_string(),
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
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
