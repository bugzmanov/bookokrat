use crate::markdown::{Block, Document, HeadingLevel, Inline, Node, Text, TextOrInline};
use crate::mathml_renderer::mathml_to_ascii;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use regex::Regex;
use std::rc::Rc;

/// Converts HTML content to clean Markdown AST with text formatting and cleanup.
///
/// This converter is responsible for the first phase of the HTML→Markdown→Text pipeline.
/// It handles HTML parsing using html5ever, AST creation, and text cleanup during conversion.
///
/// # Responsibilities
///
/// ## HTML Parsing and DOM Traversal
/// - Parses HTML using html5ever with proper DOM handling
/// - Traverses DOM nodes and converts to Markdown AST structures
/// - Handles various HTML elements (headings, paragraphs, images, MathML, etc.)
///
/// # Usage
///
/// ```rust
/// let mut converter = HtmlToMarkdownConverter::new();
/// let markdown_doc = converter.convert_with_cleanup(html_content);
/// ```
pub struct HtmlToMarkdownConverter {
    // Conversion state and placeholders
    mathml_content: Vec<(String, String)>, // (placeholder, preserved_content)

    // Regex patterns for text cleanup during conversion
    multi_space_re: Regex,
    multi_newline_re: Regex,
    leading_space_re: Regex,
    line_leading_space_re: Regex,
}

impl HtmlToMarkdownConverter {
    pub fn new() -> Self {
        HtmlToMarkdownConverter {
            mathml_content: Vec::new(),
            multi_space_re: Regex::new(r" +").expect("Failed to compile multi space regex"),
            multi_newline_re: Regex::new(r"\n{3,}").expect("Failed to compile multi newline regex"),
            leading_space_re: Regex::new(r"^ +").expect("Failed to compile leading space regex"),
            line_leading_space_re: Regex::new(r"\n +")
                .expect("Failed to compile line leading space regex"),
        }
    }

    pub fn convert(&mut self, html: &str) -> Document {
        let dom = parse_document(RcDom::default(), Default::default())
            .from_utf8()
            .read_from(&mut html.as_bytes())
            .unwrap();

        let mut document = Document::new();
        self.visit_node(&dom.document, &mut document);
        document
    }

    /// Converts HTML to Markdown AST with integrated text cleanup.
    pub fn convert_with_cleanup(&mut self, html: &str) -> Document {
        // 1. First do basic HTML to Markdown AST conversion
        let mut document = self.convert(html);

        // 2. Apply text formatting and cleanup to the AST content
        self.cleanup_document_text(&mut document);

        document
    }

    fn visit_node(&mut self, node: &Rc<markup5ever_rcdom::Node>, document: &mut Document) {
        match node.data {
            NodeData::Document => {
                // Visit all children of the document
                for child in node.children.borrow().iter() {
                    self.visit_node(child, document);
                }
            }
            NodeData::Element {
                ref name,
                ref attrs,
                ..
            } => {
                self.visit_element(name, attrs, node, document);
            }
            NodeData::Text { contents: _ } => {
                // For now, we'll handle text within element contexts
                // This is a placeholder for the actual implementation
            }
            _ => {
                // Handle comments, doctypes, etc. by visiting children
                for child in node.children.borrow().iter() {
                    self.visit_node(child, document);
                }
            }
        }
    }

    fn visit_element(
        &mut self,
        name: &html5ever::QualName,
        _attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let tag_name = name.local.as_ref();

        match tag_name {
            "html" | "body" => {
                // Visit children for these container elements
                for child in node.children.borrow().iter() {
                    self.visit_node(child, document);
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.handle_heading(tag_name, node, document);
            }
            "p" => {
                self.handle_paragraph(node, document);
            }
            "img" => {
                self.handle_image(_attrs, document);
            }
            "math" => {
                self.handle_mathml(node, document);
            }
            // Skip these elements entirely
            "style" | "script" | "head" => {
                // Do nothing - skip these elements
            }
            // For now, skip tables and lists as per the plan
            "table" | "ul" | "ol" | "li" => {
                // Do nothing - skip these elements initially
            }
            _ => {
                // For other elements, just visit children
                for child in node.children.borrow().iter() {
                    self.visit_node(child, document);
                }
            }
        }
    }

    fn handle_heading(
        &mut self,
        tag_name: &str,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let level = match tag_name {
            "h1" => HeadingLevel::H1,
            "h2" => HeadingLevel::H2,
            "h3" => HeadingLevel::H3,
            "h4" => HeadingLevel::H4,
            "h5" => HeadingLevel::H5,
            "h6" => HeadingLevel::H6,
            _ => HeadingLevel::H1,
        };

        let content_text = self.extract_text_content(node);
        let content = Text::from(content_text);

        let heading_block = Block::Heading { level, content };
        let heading_node = Node::new(heading_block, 0..0); // TODO: proper source range
        document.blocks.push(heading_node);
    }

    fn handle_paragraph(&mut self, node: &Rc<markup5ever_rcdom::Node>, document: &mut Document) {
        let content_text = self.extract_text_content(node);
        if !content_text.trim().is_empty() {
            let content = Text::from(content_text);
            let paragraph_block = Block::Paragraph { content };
            let paragraph_node = Node::new(paragraph_block, 0..0); // TODO: proper source range
            document.blocks.push(paragraph_node);
        }
    }

    fn handle_image(
        &mut self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        document: &mut Document,
    ) {
        if let Some(src) = self.get_attr_value(attrs, "src") {
            let alt_text = self.get_attr_value(attrs, "alt").unwrap_or_default();
            let title = self.get_attr_value(attrs, "title");

            // Create an Inline::Image
            let image_inline = Inline::Image {
                alt_text,
                url: src,
                title,
            };

            // Add as a paragraph containing just the image
            let mut content = Text::default();
            content.push_inline(image_inline);
            let paragraph_block = Block::Paragraph { content };
            let paragraph_node = Node::new(paragraph_block, 0..0);
            document.blocks.push(paragraph_node);
        }
    }

    fn handle_mathml(&mut self, node: &Rc<markup5ever_rcdom::Node>, document: &mut Document) {
        // Serialize the entire MathML node as HTML
        let mathml_html = self.serialize_node_to_html(node);

        // Convert MathML directly to ASCII using mathml_renderer
        let ascii_content = match mathml_to_ascii(&mathml_html, true) {
            Ok(ascii_math) => {
                // Create a placeholder to protect the content from whitespace cleanup
                let placeholder = format!("__MATHML_PROTECTED_{}__", self.mathml_content.len());
                self.mathml_content.push((placeholder.clone(), ascii_math));

                // Use the placeholder temporarily
                placeholder
            }
            Err(_) => {
                // Fall back to the original MathML HTML if conversion fails
                mathml_html
            }
        };

        // Add as a paragraph containing the placeholder (to be restored later)
        let content = Text::from(ascii_content);
        let paragraph_block = Block::Paragraph { content };
        let paragraph_node = Node::new(paragraph_block, 0..0);
        document.blocks.push(paragraph_node);
    }

    fn extract_text_content(&self, node: &Rc<markup5ever_rcdom::Node>) -> String {
        let mut text = String::new();
        self.collect_text_recursive(node, &mut text);
        text.trim().to_string()
    }

    fn collect_text_recursive(&self, node: &Rc<markup5ever_rcdom::Node>, text: &mut String) {
        match node.data {
            NodeData::Text { ref contents } => {
                text.push_str(&contents.borrow());
            }
            NodeData::Element { .. } => {
                for child in node.children.borrow().iter() {
                    self.collect_text_recursive(child, text);
                }
            }
            _ => {
                for child in node.children.borrow().iter() {
                    self.collect_text_recursive(child, text);
                }
            }
        }
    }

    fn get_attr_value(
        &self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        name: &str,
    ) -> Option<String> {
        attrs
            .borrow()
            .iter()
            .find(|attr| attr.name.local.as_ref() == name)
            .map(|attr| attr.value.to_string())
    }

    fn serialize_node_to_html(&self, node: &Rc<markup5ever_rcdom::Node>) -> String {
        let mut html = String::new();
        self.serialize_node_recursive(node, &mut html);
        html
    }

    fn serialize_node_recursive(&self, node: &Rc<markup5ever_rcdom::Node>, html: &mut String) {
        match node.data {
            NodeData::Element {
                ref name,
                ref attrs,
                ..
            } => {
                // Opening tag
                html.push('<');
                html.push_str(&name.local);

                // Attributes
                for attr in attrs.borrow().iter() {
                    html.push(' ');
                    html.push_str(&attr.name.local);
                    html.push_str("=\"");
                    html.push_str(&attr.value);
                    html.push('"');
                }
                html.push('>');

                // Children
                for child in node.children.borrow().iter() {
                    self.serialize_node_recursive(child, html);
                }

                // Closing tag
                html.push_str("</");
                html.push_str(&name.local);
                html.push('>');
            }
            NodeData::Text { ref contents } => {
                html.push_str(&contents.borrow());
            }
            _ => {
                // For other node types, just serialize children
                for child in node.children.borrow().iter() {
                    self.serialize_node_recursive(child, html);
                }
            }
        }
    }

    fn cleanup_document_text(&self, document: &mut Document) {
        // Apply text formatting and cleanup to all text nodes in the document
        for node in &mut document.blocks {
            self.cleanup_node_text(node);
        }
    }

    fn cleanup_node_text(&self, node: &mut Node) {
        match &mut node.block {
            Block::Heading { content, .. } => {
                self.cleanup_text(content);
            }
            Block::Paragraph { content } => {
                self.cleanup_text(content);
            }
            Block::Quote { content } => {
                for child_node in content {
                    self.cleanup_node_text(child_node);
                }
            }
            _ => {} // Other block types don't need text cleanup
        }
    }

    fn cleanup_text(&self, text: &mut Text) {
        // Apply dialog formatting and regex cleanup to text content
        for item in text.iter_mut() {
            match item {
                TextOrInline::Text(text_node) => {
                    let cleaned = self.apply_text_cleanup(&text_node.content);
                    text_node.content = cleaned;
                }
                TextOrInline::Inline(_) => {
                    // Don't modify inline elements during cleanup
                }
            }
        }
    }

    fn apply_text_cleanup(&self, text: &str) -> String {
        let formatted = self.format_text_with_spacing(text);
        self.apply_final_regex_cleanup(formatted)
    }

    fn format_text_with_spacing(&self, text: &str) -> String {
        let mut formatted = String::new();
        let normalized_text = self.multi_newline_re.replace_all(text, "\n\n");
        let paragraphs: Vec<&str> = normalized_text.split("\n\n").collect();
        let mut i = 0;
        while i < paragraphs.len() {
            let paragraph = paragraphs[i];
            if paragraph.trim().is_empty() {
                i += 1;
                continue;
            }
            // Check if this is the start of a list block
            if self.is_list_item(paragraph) {
                // Collect consecutive list items
                let mut list_items = vec![paragraph];
                let mut j = i + 1;
                while j < paragraphs.len() && self.is_list_item(paragraphs[j]) {
                    list_items.push(paragraphs[j]);
                    j += 1;
                }
                // Format list block without empty lines between items
                for (idx, list_item) in list_items.iter().enumerate() {
                    // Process each line in the list item (in case it spans multiple lines)
                    let lines: Vec<&str> = list_item.lines().collect();
                    for (line_idx, line) in lines.iter().enumerate() {
                        formatted.push_str(line);
                        if line_idx < lines.len() - 1 {
                            formatted.push('\n');
                        }
                    }
                    if idx < list_items.len() - 1 {
                        formatted.push('\n');
                    }
                }
                // Add empty line after list block
                if j < paragraphs.len() {
                    formatted.push_str("\n\n");
                }
                i = j;
                continue;
            }
            // Check if this is the start of a dialog block
            if self.is_dialog_line(paragraph) {
                // Collect consecutive dialog lines
                let mut dialog_lines = vec![paragraph];
                let mut j = i + 1;
                while j < paragraphs.len() && self.is_dialog_line(paragraphs[j]) {
                    dialog_lines.push(paragraphs[j]);
                    j += 1;
                }
                // Only treat as dialog if we have at least 2 consecutive dialog lines
                if dialog_lines.len() >= 2 {
                    // Format dialog block without empty lines between responses
                    for (idx, dialog_line) in dialog_lines.iter().enumerate() {
                        formatted.push_str(dialog_line);
                        if idx < dialog_lines.len() - 1 {
                            formatted.push('\n');
                        }
                    }
                    // Add empty line after dialog block
                    if j < paragraphs.len() {
                        formatted.push_str("\n\n");
                    }
                    i = j;
                    continue;
                }
            }
            // Regular paragraph formatting
            let lines: Vec<&str> = paragraph.lines().collect();
            for (j, line) in lines.iter().enumerate() {
                formatted.push_str(line);
                if j < lines.len() - 1 {
                    formatted.push('\n');
                }
            }
            if i < paragraphs.len() - 1 {
                formatted.push_str("\n\n");
            }
            i += 1;
        }

        formatted
    }

    fn apply_final_regex_cleanup(&self, mut text: String) -> String {
        // Apply regex cleanup
        text = self.multi_space_re.replace_all(&text, " ").into_owned();
        text = self
            .multi_newline_re
            .replace_all(&text, "\n\n")
            .into_owned();
        text = self.leading_space_re.replace_all(&text, "").into_owned();
        text = self
            .line_leading_space_re
            .replace_all(&text, "\n")
            .into_owned();

        text.trim().to_string()
    }

    fn is_dialog_line(&self, text: &str) -> bool {
        let trimmed = text.trim_start();
        // Check for various dash types that indicate dialog
        trimmed.starts_with('-')
            || trimmed.starts_with('–')
            || trimmed.starts_with('—')
            || trimmed.starts_with('\u{2010}')
            || trimmed.starts_with('\u{2011}')
            || trimmed.starts_with('\u{2012}')
            || trimmed.starts_with('\u{2013}')
            || trimmed.starts_with('\u{2014}')
    }

    fn is_list_item(&self, text: &str) -> bool {
        let trimmed = text.trim_start();
        if trimmed.starts_with("• ") || trimmed.starts_with("- ") {
            return true;
        }
        if let Some(captures) = Regex::new(r"^\d+\. ").unwrap().captures(trimmed) {
            return captures.get(0).is_some();
        }
        false
    }

    fn decode_html_entities(&self, text: &str) -> String {
        text.replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&mdash;", "—")
            .replace("&ndash;", "–")
            .replace("&hellip;", "...")
            .replace("&ldquo;", "\u{201C}")
            .replace("&rdquo;", "\u{201D}")
            .replace("&lsquo;", "\u{2018}")
            .replace("&rsquo;", "\u{2019}")
    }

    /// Decodes HTML entities in rendered text.
    ///
    /// This method performs HTML entity decoding after the Markdown AST
    /// has been rendered to string.
    ///
    /// # Arguments
    /// * `text` - Rendered text containing HTML entities
    ///
    /// # Returns
    /// Text with HTML entities decoded
    pub fn decode_entities(&self, text: &str) -> String {
        self.decode_html_entities(text)
    }

    pub fn restore_mathml_content(&self, mut text: String) -> String {
        // Restore MathML content after whitespace cleanup
        for (placeholder, mathml_content) in &self.mathml_content {
            text = text.replace(placeholder, mathml_content);
        }
        text
    }
}

impl Default for HtmlToMarkdownConverter {
    fn default() -> Self {
        Self::new()
    }
}
