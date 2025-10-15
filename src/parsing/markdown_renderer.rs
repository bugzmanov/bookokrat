use crate::markdown::{
    Block, DefinitionListItem, Document, HeadingLevel, Inline, Node, Text, TextNode, TextOrInline,
};

/// Simple Markdown AST to string renderer with no cleanup logic.
///
/// This renderer is responsible for the second phase of the HTML→Markdown→Text pipeline.
/// It takes a clean Markdown AST and converts it to a formatted string representation.
///
/// # Responsibilities
///
/// ## AST Traversal and Rendering
/// - Traverses Markdown AST structures (Document, Node, Block)
/// - Converts AST elements to their string representations
/// - Handles different block types (headings, paragraphs, code blocks, quotes)
/// - Manages text node formatting with style applications
///
/// ## Text Formatting
/// - Applies Markdown formatting syntax (`#` for headings, `**` for bold, etc.)
/// - Handles heading levels with proper hash prefixes (H1-H6)
/// - Applies H1 uppercase transformation for consistency
/// - Manages inline text styles (emphasis, strong, code, strikethrough)
///
/// ## Output Generation
/// - Produces clean, properly formatted Markdown text
/// - Adds appropriate spacing and line breaks between elements
/// - Ensures consistent formatting throughout the document
///
/// # Design Philosophy
///
/// The renderer is intentionally simple and focused solely on AST→string conversion.
///
/// # Usage
///
/// ```rust,no_run
/// use bookrat::parsing::markdown_renderer::MarkdownRenderer;
/// # use bookrat::markdown::Document;
/// # fn main() {
/// let renderer = MarkdownRenderer::new();
/// # let markdown_document = Document::new();
/// let output_text = renderer.render(&markdown_document);
/// # }
/// ```
pub struct MarkdownRenderer {
    // Simple renderer - no cleanup logic needed as it's handled during conversion
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        MarkdownRenderer {}
    }

    /// Renders a Markdown AST document to formatted string.
    pub fn render(&self, doc: &Document) -> String {
        let mut output = String::new();

        for node in &doc.blocks {
            self.render_node(node, &mut output);
        }

        output
    }

    fn render_node(&self, node: &Node, output: &mut String) {
        match &node.block {
            Block::Heading { level, content } => {
                self.render_heading(*level, content, output);
            }
            Block::Paragraph { content } => {
                self.render_paragraph(content, output);
            }
            Block::CodeBlock { language, content } => {
                self.render_code_block(content, language, output);
            }
            Block::Quote { content } => {
                self.render_quote(content, output);
            }
            Block::List { kind, items } => {
                self.render_list(kind, items, output, 0);
            }
            Block::Table {
                header,
                rows,
                alignment,
            } => {
                self.render_table(header, rows, alignment, output);
            }
            Block::DefinitionList { items } => {
                self.render_definition_list(items, output);
            }
            Block::EpubBlock {
                epub_type,
                element_name,
                content,
            } => {
                self.render_epub_block(epub_type, element_name, content, output);
            }
            Block::ThematicBreak => {
                output.push_str("---\n\n");
            }
        }
    }

    fn render_heading(&self, level: HeadingLevel, content: &Text, output: &mut String) {
        let content_str = self.render_text(content);

        // Add markdown hash prefixes based on heading level
        let hashes = match level {
            HeadingLevel::H1 => "#",
            HeadingLevel::H2 => "##",
            HeadingLevel::H3 => "###",
            HeadingLevel::H4 => "####",
            HeadingLevel::H5 => "#####",
            HeadingLevel::H6 => "######",
        };

        output.push_str(hashes);
        output.push(' ');

        // Apply h1 uppercase rule as in original implementation
        if level == HeadingLevel::H1 {
            output.push_str(&content_str.to_uppercase());
        } else {
            output.push_str(&content_str);
        }

        output.push_str("\n\n");
    }

    fn render_paragraph(&self, content: &Text, output: &mut String) {
        let content_str = self.render_text(content);
        if !content_str.trim().is_empty() {
            output.push_str(&content_str);
            output.push_str("\n\n");
        }
    }

    fn render_code_block(&self, content: &str, language: &Option<String>, output: &mut String) {
        output.push_str("```");
        if let Some(lang) = language {
            output.push_str(lang);
        }
        output.push('\n');
        output.push_str(content);
        output.push_str("\n```\n\n");
    }

    fn render_quote(&self, content: &[Node], output: &mut String) {
        for node in content {
            output.push_str("> ");
            self.render_node(node, output);
        }
        output.push('\n');
    }

    fn render_definition_list(&self, items: &[DefinitionListItem], output: &mut String) {
        for item in items {
            // Render term as H6 heading
            output.push_str("###### ");
            let term_str = self.render_text(&item.term);
            output.push_str(&term_str);
            output.push('\n');

            // Render each definition (now blocks) with : prefix
            for definition_blocks in &item.definitions {
                // Each definition is now a Vec<Node>
                output.push_str(": ");
                // Render all blocks in the definition
                for (i, block_node) in definition_blocks.iter().enumerate() {
                    if i > 0 {
                        output.push_str("  "); // Add indentation for continuation
                    }
                    self.render_node(block_node, output);
                }
            }

            output.push('\n');
        }
    }

    fn render_epub_block(
        &self,
        epub_type: &str,
        element_name: &str,
        content: &[Node],
        output: &mut String,
    ) {
        // Render as a special block with type annotation
        output.push_str(&format!("[{element_name} epub:type=\"{epub_type}\"]\n"));

        // Render nested content
        for node in content {
            self.render_node(node, output);
        }

        output.push_str(&format!("[/{element_name}]\n\n"));
    }

    fn render_list(
        &self,
        kind: &crate::markdown::ListKind,
        items: &[crate::markdown::ListItem],
        output: &mut String,
        depth: usize,
    ) {
        for (index, item) in items.iter().enumerate() {
            self.render_list_item(kind, item, index, output, depth);
        }

        // Add extra newline after top-level lists
        if depth == 0 {
            output.push('\n');
        }
    }

    fn render_table(
        &self,
        header: &Option<crate::markdown::TableRow>,
        rows: &[crate::markdown::TableRow],
        alignment: &[crate::markdown::TableAlignment],
        output: &mut String,
    ) {
        // Calculate table dimensions
        let num_rows = rows.len();
        let num_cols = if let Some(header_row) = header {
            header_row.cells.len()
        } else if !rows.is_empty() {
            rows[0].cells.len()
        } else {
            0
        };
        let has_header = header.is_some();

        // Add table metadata line
        output.push_str(&format!(
            "[table width=\"{num_cols}\" height=\"{num_rows}\" header=\"{has_header}\"]\n"
        ));

        // Calculate column widths for proper formatting
        let mut column_widths = vec![];

        // Consider header cells for width calculation
        if let Some(header_row) = header {
            for (i, cell) in header_row.cells.iter().enumerate() {
                let width = self.render_text(&cell.content).len();
                if i >= column_widths.len() {
                    column_widths.push(width);
                } else {
                    column_widths[i] = column_widths[i].max(width);
                }
            }
        }

        // Consider body cells for width calculation
        for row in rows {
            for (i, cell) in row.cells.iter().enumerate() {
                let width = self.render_text(&cell.content).len();
                if i >= column_widths.len() {
                    column_widths.push(width);
                } else {
                    column_widths[i] = column_widths[i].max(width);
                }
            }
        }

        // Ensure minimum width of 3 for each column (for alignment markers)
        for width in &mut column_widths {
            *width = (*width).max(3);
        }

        // Render header if present
        if let Some(header_row) = header {
            output.push('|');
            for (i, cell) in header_row.cells.iter().enumerate() {
                let content = self.render_text(&cell.content);
                let width = if i < column_widths.len() {
                    column_widths[i]
                } else {
                    content.len()
                };
                // Don't apply bold formatting to header row cells, even if they're marked as headers
                output.push_str(&format!(" {content:<width$} |"));
            }
            output.push('\n');

            // Render separator row with alignment
            output.push('|');
            for (i, width) in column_widths.iter().enumerate() {
                let align = if i < alignment.len() {
                    &alignment[i]
                } else {
                    &crate::markdown::TableAlignment::None
                };

                match align {
                    crate::markdown::TableAlignment::Left => {
                        output.push_str(&format!(" :{} |", "-".repeat(width - 1)));
                    }
                    crate::markdown::TableAlignment::Right => {
                        output.push_str(&format!(" {}: |", "-".repeat(width - 1)));
                    }
                    crate::markdown::TableAlignment::Center => {
                        output.push_str(&format!(" :{}: |", "-".repeat(width - 2)));
                    }
                    crate::markdown::TableAlignment::None => {
                        output.push_str(&format!(" {} |", "-".repeat(*width)));
                    }
                }
            }
            output.push('\n');
        } else if !rows.is_empty() {
            // If no header but we have rows, create a separator based on first row
            output.push('|');
            for width in &column_widths {
                output.push_str(&format!(" {} |", "-".repeat(*width)));
            }
            output.push('\n');
        }

        // Render body rows
        for row in rows {
            output.push('|');
            for (i, cell) in row.cells.iter().enumerate() {
                let content = self.render_text(&cell.content);
                let width = if i < column_widths.len() {
                    column_widths[i]
                } else {
                    content.len()
                };

                // Apply bold formatting for header cells in body rows
                if cell.is_header {
                    // Account for the ** markers when calculating width
                    let bold_content = format!("**{}**", content.trim());
                    output.push_str(&format!(" {bold_content:<width$} |"));
                } else {
                    output.push_str(&format!(" {content:<width$} |"));
                }
            }
            output.push('\n');
        }

        output.push('\n');
    }

    fn render_list_item(
        &self,
        kind: &crate::markdown::ListKind,
        item: &crate::markdown::ListItem,
        index: usize,
        output: &mut String,
        depth: usize,
    ) {
        // Indentation for nested lists
        let indent = "  ".repeat(depth);

        // Render the list marker
        output.push_str(&indent);
        match kind {
            crate::markdown::ListKind::Ordered { start } => {
                let number = start + index as u32;
                output.push_str(&format!("{number}. "));
            }
            crate::markdown::ListKind::Unordered => {
                // Use different bullets for different nesting levels
                let bullet = match depth % 3 {
                    0 => '-',
                    1 => '*',
                    2 => '+',
                    _ => '-',
                };
                output.push(bullet);
                output.push(' ');
            }
        }

        // Render the list item content
        let mut first_block = true;
        for node in &item.content {
            match &node.block {
                Block::Paragraph { content } => {
                    let content_str = self.render_text(content);
                    // Skip empty paragraphs (those with only whitespace)
                    if !content_str.trim().is_empty() {
                        if !first_block {
                            // Additional paragraphs in list items need indentation
                            output.push_str(&indent);
                            output.push_str("  ");
                        }
                        output.push_str(&content_str);
                        output.push('\n');
                    }
                }
                Block::List {
                    kind: nested_kind,
                    items: nested_items,
                } => {
                    // Render nested list with increased depth
                    self.render_list(nested_kind, nested_items, output, depth + 1);
                }
                Block::CodeBlock { language, content } => {
                    // Code blocks in lists need proper indentation
                    if !first_block {
                        output.push('\n');
                    }
                    // Add indented opening fence with language
                    //todo: this is BS as it doesn't take consideration nested lists.
                    output.push_str(&indent);
                    output.push_str("    ```");
                    if let Some(lang) = language {
                        output.push_str(lang);
                    }
                    output.push('\n');

                    // Add indented content lines
                    let lines: Vec<&str> = content.lines().collect();
                    for line in lines {
                        output.push_str(&indent);
                        output.push_str("    ");
                        output.push_str(line);
                        output.push('\n');
                    }

                    // Add indented closing fence
                    output.push_str(&indent);
                    output.push_str("    ```\n");
                }
                _ => {
                    // Other block types - render with indentation
                    if !first_block {
                        output.push('\n');
                    }
                    let mut temp_output = String::new();
                    self.render_node(node, &mut temp_output);
                    // Indent each line of the output
                    for line in temp_output.lines() {
                        if !line.is_empty() {
                            output.push_str(&indent);
                            output.push_str("  ");
                        }
                        output.push_str(line);
                        output.push('\n');
                    }
                }
            }
            first_block = false;
        }
    }

    pub fn render_text(&self, text: &Text) -> String {
        let mut output = String::new();

        for item in text.clone().into_iter() {
            match item {
                TextOrInline::Text(text_node) => {
                    self.render_text_node(&text_node, &mut output);
                }
                TextOrInline::Inline(inline) => {
                    self.render_inline(&inline, &mut output);
                }
            }
        }

        output
    }

    fn render_text_node(&self, text_node: &TextNode, output: &mut String) {
        match &text_node.style {
            Some(style) => match style {
                crate::markdown::Style::Code => {
                    output.push('`');
                    output.push_str(&text_node.content);
                    output.push('`');
                }
                crate::markdown::Style::Emphasis => {
                    output.push('_');
                    output.push_str(&text_node.content);
                    output.push('_');
                }
                crate::markdown::Style::Strong => {
                    output.push_str("**");
                    output.push_str(&text_node.content);
                    output.push_str("**");
                }
                crate::markdown::Style::Strikethrough => {
                    output.push_str("~~");
                    output.push_str(&text_node.content);
                    output.push_str("~~");
                }
            },
            None => {
                output.push_str(&text_node.content);
            }
        }
    }

    fn render_inline_with_spacing(
        &self,
        inline: &Inline,
        output: &mut String,
        _prev_item: Option<&TextOrInline>,
        _next_item: Option<&TextOrInline>,
        _is_first: bool,
        _is_last: bool,
    ) {
        match inline {
            Inline::Image {
                alt_text: _,
                url,
                title: _,
            } => {
                // Add spacing around image placeholders
                if !output.is_empty() && !output.ends_with(' ') && !output.ends_with('\n') {
                    output.push(' ');
                }
                output.push_str(&format!("[image src=\"{url}\"]"));
                output.push(' ');
            }
            Inline::Link {
                text,
                url,
                title: _,
                ..
            } => {
                output.push('[');
                output.push_str(&self.render_text(text));
                output.push_str("](");
                output.push_str(url);
                output.push(')');
            }
            Inline::LineBreak => {
                output.push_str("  \n");
            }
            Inline::SoftBreak => {
                output.push('\n');
            }
            Inline::Anchor { .. } => {
                // Anchors don't contribute to rendered text output
            }
        }
    }

    fn render_inline(&self, inline: &Inline, output: &mut String) {
        // Use the spacing version with default parameters for backward compatibility
        self.render_inline_with_spacing(inline, output, None, None, false, false);
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}
