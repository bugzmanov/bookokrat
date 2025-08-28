use crate::markdown::{Block, Document, HeadingLevel, Inline, Node, Text, TextNode, TextOrInline};

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
/// ```rust
/// let renderer = MarkdownRenderer::new();
/// let output_text = renderer.render(&markdown_document);
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
            Block::CodeBlock {
                language: _,
                content,
            } => {
                self.render_code_block(content, output);
            }
            Block::Quote { content } => {
                self.render_quote(content, output);
            }
            Block::List { kind: _, items: _ } => {
                // Skip lists for now as per the plan
            }
            Block::Table {
                header: _,
                rows: _,
                alignment: _,
            } => {
                // Skip tables for now as per the plan
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

    fn render_code_block(&self, content: &str, output: &mut String) {
        output.push_str("```\n");
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

    fn render_text(&self, text: &Text) -> String {
        let mut output = String::new();
        let items: Vec<TextOrInline> = text.clone().into_iter().collect();

        for (i, item) in items.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == items.len() - 1;
            let prev_item = if i > 0 { Some(&items[i - 1]) } else { None };
            let next_item = if i < items.len() - 1 { Some(&items[i + 1]) } else { None };

            match item {
                TextOrInline::Text(text_node) => {
                    // Handle spacing for text nodes adjacent to inline elements
                    if let Some(TextOrInline::Inline(_)) = prev_item {
                        // If previous was inline and current text doesn't start with space or punctuation
                        let starts_with_punctuation = text_node.content.starts_with(&[' ', '.', ',', '!', '?', ':', ';', ')', ']', '}']);
                        if !starts_with_punctuation {
                            // Add space between inline element and following text
                            if !output.ends_with(' ') {
                                output.push(' ');
                            }
                        }
                    }
                    self.render_text_node(text_node, &mut output);
                }
                TextOrInline::Inline(inline) => {
                    self.render_inline_with_spacing(inline, &mut output, prev_item, next_item, is_first, is_last);
                }
            }
        }

        // Clean up any double spaces that might have been introduced
        let cleaned = output.replace("  ", " ");
        cleaned
    }

    fn render_text_node(&self, text_node: &TextNode, output: &mut String) {
        match &text_node.style {
            Some(style) => match style {
                crate::markdown::Style::Code => {
                    // Add space before inline code if the previous character isn't whitespace
                    if !output.is_empty() {
                        let last_char = output.chars().last().unwrap();
                        if !last_char.is_whitespace() && !matches!(last_char, '(' | '[' | '{') {
                            output.push(' ');
                        }
                    }
                    output.push('`');
                    output.push_str(&text_node.content);
                    output.push('`');
                    // Only add space after if we're not at the end or followed immediately by punctuation
                    // Let the next element handle its own spacing
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

    fn render_inline_with_spacing(&self, inline: &Inline, output: &mut String, _prev_item: Option<&TextOrInline>, _next_item: Option<&TextOrInline>, _is_first: bool, _is_last: bool) {
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
                output.push_str(&format!("[image src=\"{}\"]", url));
                output.push(' ');
            }
            Inline::Link {
                text,
                url,
                title: _,
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
