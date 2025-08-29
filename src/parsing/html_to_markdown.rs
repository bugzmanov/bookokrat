use crate::markdown::{
    Block, Document, HeadingLevel, Inline, Node, Style, Text, TextNode, TextOrInline,
};
use crate::mathml_renderer::mathml_to_ascii;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
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
/// ```rust,no_run
/// use bookrat::parsing::html_to_markdown::HtmlToMarkdownConverter;
/// # fn main() {
/// let mut converter = HtmlToMarkdownConverter::new();
/// # let html_content = "<p>Hello world</p>";
/// let markdown_doc = converter.convert(html_content);
/// # }
/// ```
pub struct HtmlToMarkdownConverter {
    // Conversion state and placeholders
}

impl HtmlToMarkdownConverter {
    pub fn new() -> Self {
        HtmlToMarkdownConverter {}
    }

    pub fn convert(&mut self, html: &str) -> Document {
        let dom = parse_document(RcDom::default(), Default::default())
            .from_utf8()
            .read_from(&mut html.as_bytes())
            .unwrap();

        let mut document = Document::new();
        self.visit_node(&dom.document, &mut document);

        // Post-process:
        self.group_dialog_paragraphs(&mut document);

        document
    }

    fn visit_node(&mut self, node: &Rc<markup5ever_rcdom::Node>, document: &mut Document) {
        match node.data {
            NodeData::Document => {
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
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let tag_name = name.local.as_ref();

        match tag_name {
            "html" | "body" => {
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
                self.handle_image(attrs, document);
            }
            "math" => {
                self.handle_mathml(node, document);
            }
            "style" | "script" | "head" => {
                // Do nothing
            }
            // Handle inline formatting elements within other content
            "strong" | "b" | "em" | "i" | "code" | "a" | "br" | "del" | "s" | "strike" => {
                // These are handled within extract_formatted_content, skip at block level
                for child in node.children.borrow().iter() {
                    self.visit_node(child, document);
                }
            }
            // For now, skip tables and lists as per the plan
            "table" | "ul" | "ol" | "li" => {}
            _ => {
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

        let content = self.extract_formatted_content(node);

        let heading_block = Block::Heading { level, content };
        let heading_node = Node::new(heading_block, 0..0); // TODO: proper source range
        document.blocks.push(heading_node);
    }

    fn handle_paragraph(&mut self, node: &Rc<markup5ever_rcdom::Node>, document: &mut Document) {
        let content = self.extract_formatted_content(node);
        if !content.is_empty() {
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

            let mut content = Text::default();
            content.push_inline(image_inline);
            let paragraph_block = Block::Paragraph { content };
            let paragraph_node = Node::new(paragraph_block, 0..0);
            document.blocks.push(paragraph_node);
        }
    }

    fn handle_mathml(&mut self, node: &Rc<markup5ever_rcdom::Node>, document: &mut Document) {
        let mathml_html = self.serialize_node_to_html(node);

        let content = match mathml_to_ascii(&mathml_html, true) {
            Ok(ascii_math) => {
                let content = Text::from(ascii_math);
                let paragraph_block = Block::Paragraph { content };
                Node::new(paragraph_block, 0..0)
            }
            Err(e) => {
                let paragraph_block = Block::CodeBlock {
                    language: Some(format!("failed to extract mathml: {:?}", e)),
                    content: mathml_html,
                };
                Node::new(paragraph_block, 0..0)
            }
        };

        document.blocks.push(content);
    }

    fn extract_formatted_content(&self, node: &Rc<markup5ever_rcdom::Node>) -> Text {
        let mut text = Text::default();
        self.collect_formatted_content(node, &mut text, None);
        text
    }

    fn collect_formatted_content(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        text: &mut Text,
        current_style: Option<Style>,
    ) {
        match &node.data {
            NodeData::Text { contents } => {
                let content = contents.borrow().to_string();
                if !content.trim().is_empty() {
                    // Add spacing around inline code elements at the AST level
                    let adjusted_content = if current_style == Some(Style::Code) {
                        self.add_code_spacing(&content)
                    } else {
                        content
                    };
                    let text_node = TextNode::new(adjusted_content, current_style.clone());
                    text.push_text(text_node);
                }
            }
            NodeData::Element { name, attrs, .. } => {
                let tag_name = name.local.as_ref();

                let style = match tag_name {
                    "strong" | "b" => Some(Style::Strong),
                    "em" | "i" => Some(Style::Emphasis),
                    "code" => Some(Style::Code),
                    "del" | "s" | "strike" => Some(Style::Strikethrough),
                    "a" => {
                        // Handle links as inline elements
                        if let Some(href) = self.get_attr_value(attrs, "href") {
                            let mut link_text = Text::default();
                            for child in node.children.borrow().iter() {
                                self.collect_formatted_content(
                                    child,
                                    &mut link_text,
                                    current_style.clone(),
                                );
                            }
                            let title = self.get_attr_value(attrs, "title");
                            let link_inline = Inline::Link {
                                text: link_text,
                                url: href,
                                title,
                            };
                            text.push_inline(link_inline);
                        }
                        return; // Don't process children again
                    }
                    "math" => {
                        let math_html = self.serialize_node_to_html(node);
                        match mathml_to_ascii(&math_html, true) {
                            Ok(math_ascii) => {
                                text.push_text(TextNode::new(math_ascii, None));
                            }
                            Err(e) => {
                                text.push_text(TextNode::new(
                                    format!("<fail to parse math: {:?}", e),
                                    None,
                                ));
                            }
                        }
                        return;
                    }
                    "br" => {
                        text.push_inline(Inline::LineBreak);
                        return;
                    }
                    _ => current_style.clone(),
                };

                // Process children with the new or inherited style
                for child in node.children.borrow().iter() {
                    self.collect_formatted_content(child, text, style.clone());
                }
            }
            _ => {
                // For other node types, process children with inherited style
                for child in node.children.borrow().iter() {
                    self.collect_formatted_content(child, text, current_style.clone());
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
        fn serialize_node_recursive(node: &Rc<markup5ever_rcdom::Node>, html: &mut String) {
            match node.data {
                NodeData::Element {
                    ref name,
                    ref attrs,
                    ..
                } => {
                    html.push('<');
                    html.push_str(&name.local);

                    for attr in attrs.borrow().iter() {
                        html.push(' ');
                        html.push_str(&attr.name.local);
                        html.push_str("=\"");
                        html.push_str(&attr.value);
                        html.push('"');
                    }
                    html.push('>');

                    for child in node.children.borrow().iter() {
                        serialize_node_recursive(child, html);
                    }

                    html.push_str("</");
                    html.push_str(&name.local);
                    html.push('>');
                }
                NodeData::Text { ref contents } => {
                    html.push_str(&contents.borrow());
                }
                _ => {
                    for child in node.children.borrow().iter() {
                        serialize_node_recursive(child, html);
                    }
                }
            }
        }
        let mut html = String::new();
        serialize_node_recursive(node, &mut html);
        html
    }

    fn add_code_spacing(&self, content: &str) -> String {
        format!("  {}  ", content)
    }

    //todo: this should be done on a parsing/convertion phase
    pub fn decode_entities(&self, text: &str) -> String {
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

    /// Groups consecutive dialog paragraphs into single paragraphs with line breaks.
    ///
    /// This method identifies dialog lines (starting with various dash characters)
    /// and merges consecutive dialog paragraphs into a single paragraph block,
    /// preserving the dialog structure while reducing fragmentation.
    fn group_dialog_paragraphs(&self, document: &mut Document) {
        let mut i = 0;

        while i < document.blocks.len() {
            if let Block::Paragraph { content } = &document.blocks[i].block {
                if self.is_dialog_content(content) {
                    let mut dialog_contents = vec![content.clone()];
                    let mut j = i + 1;

                    while j < document.blocks.len() {
                        if let Block::Paragraph { content } = &document.blocks[j].block {
                            if self.is_dialog_content(content) {
                                dialog_contents.push(content.clone());
                                j += 1;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }

                    if dialog_contents.len() > 1 {
                        let merged_content = self.merge_dialog_group(&dialog_contents);
                        if let Block::Paragraph { content } = &mut document.blocks[i].block {
                            *content = merged_content;
                        }

                        document.blocks.drain(i + 1..j);
                    }
                }
            }

            i += 1;
        }
    }

    /// Check if a Text content represents dialog (contains dialog lines)
    fn is_dialog_content(&self, content: &Text) -> bool {
        for item in content.clone().into_iter() {
            match item {
                TextOrInline::Text(text_node) => {
                    let trimmed = text_node.content.trim_start();
                    if !trimmed.is_empty() {
                        return trimmed.starts_with('-')
                            || trimmed.starts_with('–')
                            || trimmed.starts_with('—')
                            || trimmed.starts_with('\u{2010}')
                            || trimmed.starts_with('\u{2011}')
                            || trimmed.starts_with('\u{2012}')
                            || trimmed.starts_with('\u{2013}')
                            || trimmed.starts_with('\u{2014}');
                    }
                }
                TextOrInline::Inline(Inline::Link { text, .. }) => {
                    // Recursively check link text
                    if self.is_dialog_content(&text) {
                        return true;
                    }
                }
                _ => {
                    // Skip images, line breaks, etc. and continue looking
                }
            }
        }
        false
    }

    /// Merge a group of dialog Text contents into a single Text with line breaks
    fn merge_dialog_group(&self, dialog_group: &[Text]) -> Text {
        let mut merged = Text::default();

        for (i, dialog_text) in dialog_group.iter().enumerate() {
            for item in dialog_text.clone().into_iter() {
                merged.push(item);
            }

            if i < dialog_group.len() - 1 {
                merged.push_inline(Inline::LineBreak);
            }
        }

        merged
    }
}

impl Default for HtmlToMarkdownConverter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        markdown::{Style, TextNode, TextOrInline},
        parsing::html5ever_text_generator::TextGenerator,
        parsing::markdown_renderer::MarkdownRenderer,
    };

    #[test]
    fn test_nested_formatting_in_paragraph() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r#"<p>Ololo <strong>olo</strong> or not to ololo</p>"#;
        let doc = converter.convert(html);

        assert_eq!(doc.blocks.len(), 1);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            // Convert content to vector to inspect
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            // Should have at least 3 text items: "Ololo ", "olo" (bold), " or not to ololo"
            assert!(
                items.len() >= 3,
                "Expected at least 3 text items, got {}",
                items.len()
            );

            // Check for bold formatting
            let has_bold = items.iter().any(|item| {
                if let TextOrInline::Text(text_node) = item {
                    text_node.style == Some(Style::Strong)
                } else {
                    false
                }
            });
            assert!(has_bold, "Expected bold text in paragraph");
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_deeply_nested_formatting() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r#"<p>Text with <strong>bold and <em>italic</em> here</strong> end</p>"#;
        let doc = converter.convert(html);

        assert_eq!(doc.blocks.len(), 1);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            // Should contain both strong and emphasis styles
            let has_bold = items.iter().any(|item| {
                if let TextOrInline::Text(text_node) = item {
                    text_node.style == Some(Style::Strong)
                } else {
                    false
                }
            });

            let has_italic = items.iter().any(|item| {
                if let TextOrInline::Text(text_node) = item {
                    text_node.style == Some(Style::Emphasis)
                } else {
                    false
                }
            });

            assert!(has_bold, "Expected bold text");
            assert!(has_italic, "Expected italic text");
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_dialog_grouping_with_markdown_rendering() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"
            <p>Вот пример из жизни. Молодой человек знакомится с родителями невесты.</p>
            <p>— А кем работаешь?</p>
            <p>— Я аналитик, работаю на рынке ценных бумаг.</p>
            <p>— Пирамиды, что ли? Ваучеры?</p>
            <p>Видя, что тесть не понимает, молодой человек меняет тактику:</p>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        // Should have 3 paragraphs in the final output
        let paragraphs: Vec<&str> = rendered
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .collect();
        assert_eq!(
            paragraphs.len(),
            3,
            "Should have exactly 3 paragraphs in rendered output"
        );

        // Check that the second paragraph contains grouped dialog with line breaks
        let dialog_paragraph = paragraphs[1];
        assert!(
            dialog_paragraph.contains("— А кем работаешь?"),
            "Dialog paragraph should contain first dialog line"
        );
        assert!(
            dialog_paragraph.contains("— Я аналитик"),
            "Dialog paragraph should contain second dialog line"
        );
        assert!(
            dialog_paragraph.contains("— Пирамиды"),
            "Dialog paragraph should contain third dialog line"
        );

        // Should contain line break markers from markdown rendering
        let line_breaks_count = dialog_paragraph.matches("  \n").count();
        assert!(
            line_breaks_count >= 2,
            "Dialog paragraph should contain line break markers between dialog lines"
        );
    }
}
