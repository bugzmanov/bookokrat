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
            "ul" | "ol" => {
                self.handle_list(tag_name, attrs, node, document);
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
            // Skip tables for now as per the plan
            "table" => {}
            // Skip li at this level - they're handled within lists
            "li" => {}
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

    fn handle_list(
        &mut self,
        tag_name: &str,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let kind = if tag_name == "ol" {
            // Check for start attribute
            let start = self
                .get_attr_value(attrs, "start")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            crate::markdown::ListKind::Ordered { start }
        } else {
            crate::markdown::ListKind::Unordered
        };

        let items = self.extract_list_items(node);

        if !items.is_empty() {
            let list_block = Block::List { kind, items };
            let list_node = Node::new(list_block, 0..0); // TODO: proper source range
            document.blocks.push(list_node);
        }
    }

    fn extract_list_items(
        &mut self,
        list_node: &Rc<markup5ever_rcdom::Node>,
    ) -> Vec<crate::markdown::ListItem> {
        let mut items = Vec::new();

        for child in list_node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, .. } => {
                    if name.local.as_ref() == "li" {
                        let item = self.extract_list_item(child);
                        items.push(item);
                    }
                }
                NodeData::Text { contents } => {
                    // Skip whitespace-only text nodes between list items
                    if !contents.borrow().trim().is_empty() {
                        // If there's actual text content between list items (which shouldn't happen in valid HTML),
                        // we could handle it here, but for now we'll just skip it
                    }
                }
                _ => {}
            }
        }

        items
    }

    fn extract_list_item(
        &mut self,
        li_node: &Rc<markup5ever_rcdom::Node>,
    ) -> crate::markdown::ListItem {
        let mut content = Vec::new();
        let mut current_text = Text::default();

        for child in li_node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, attrs, .. } => {
                    let tag_name = name.local.as_ref();

                    match tag_name {
                        "ul" | "ol" => {
                            // Before flushing, trim trailing whitespace from current_text
                            // since we're about to hit a block element
                            self.trim_text_trailing_whitespace(&mut current_text);

                            // Flush current text as paragraph if it has actual content
                            if !current_text.is_empty() {
                                let has_content =
                                    current_text.clone().into_iter().any(|item| match item {
                                        TextOrInline::Text(node) => !node.content.trim().is_empty(),
                                        TextOrInline::Inline(_) => true,
                                    });

                                if has_content {
                                    let paragraph = Block::Paragraph {
                                        content: current_text.clone(),
                                    };
                                    content.push(Node::new(paragraph, 0..0));
                                }
                                current_text = Text::default();
                            }

                            let kind = if tag_name == "ol" {
                                let start = self
                                    .get_attr_value(attrs, "start")
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .unwrap_or(1);
                                crate::markdown::ListKind::Ordered { start }
                            } else {
                                crate::markdown::ListKind::Unordered
                            };

                            let nested_items = self.extract_list_items(child);
                            if !nested_items.is_empty() {
                                let nested_list = Block::List {
                                    kind,
                                    items: nested_items,
                                };
                                content.push(Node::new(nested_list, 0..0));
                            }
                        }
                        "p" => {
                            // Before flushing, trim trailing whitespace from current_text
                            // since we're about to hit a block element
                            self.trim_text_trailing_whitespace(&mut current_text);

                            // Flush current text as paragraph if it has actual content
                            if !current_text.is_empty() {
                                let has_content =
                                    current_text.clone().into_iter().any(|item| match item {
                                        TextOrInline::Text(node) => !node.content.trim().is_empty(),
                                        TextOrInline::Inline(_) => true,
                                    });

                                if has_content {
                                    let paragraph = Block::Paragraph {
                                        content: current_text.clone(),
                                    };
                                    content.push(Node::new(paragraph, 0..0));
                                }
                                current_text = Text::default();
                            }

                            let para_content = self.extract_formatted_content(child);
                            if !para_content.is_empty() {
                                let paragraph = Block::Paragraph {
                                    content: para_content,
                                };
                                content.push(Node::new(paragraph, 0..0));
                            }
                        }
                        _ => {
                            self.collect_formatted_content(child, &mut current_text, None);
                        }
                    }
                }
                NodeData::Text { contents } => {
                    // Process text nodes through the normal formatted content collector
                    // This preserves proper spacing and formatting
                    self.collect_formatted_content(child, &mut current_text, None);
                }
                _ => {
                    // Process other node types
                    for grandchild in child.children.borrow().iter() {
                        self.collect_formatted_content(grandchild, &mut current_text, None);
                    }
                }
            }
        }

        // Flush any remaining text as a paragraph
        // But only if it contains actual content (not just whitespace)
        if !current_text.is_empty() {
            // Check if the text has any non-whitespace content
            let has_content = current_text.clone().into_iter().any(|item| match item {
                TextOrInline::Text(node) => !node.content.trim().is_empty(),
                TextOrInline::Inline(_) => true,
            });

            if has_content {
                let paragraph = Block::Paragraph {
                    content: current_text,
                };
                content.push(Node::new(paragraph, 0..0));
            }
        }

        crate::markdown::ListItem {
            content,
            task_status: None, // HTML doesn't have task lists
        }
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
                    // Normalize whitespace while preserving meaningful leading/trailing spaces
                    let mut normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");

                    // Preserve leading space if original had it and we already have content
                    if !text.is_empty()
                        && content.chars().next().map_or(false, |c| c.is_whitespace())
                    {
                        normalized = format!(" {}", normalized);
                    }

                    // Preserve trailing space if original had it
                    if content.chars().last().map_or(false, |c| c.is_whitespace()) {
                        normalized.push(' ');
                    }

                    if normalized.trim().is_empty() {
                        return;
                    }

                    // Add spacing around inline code elements at the AST level
                    let adjusted_content = if current_style == Some(Style::Code) {
                        self.add_code_spacing(&normalized)
                    } else {
                        normalized
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

    fn trim_text_trailing_whitespace(&self, text: &mut Text) {
        // Find the last text node and trim its trailing whitespace
        let items: Vec<TextOrInline> = text.clone().into_iter().collect();
        if items.is_empty() {
            return;
        }

        // Process items in reverse to find the last text node
        let mut new_items = items.clone();
        for i in (0..new_items.len()).rev() {
            match &mut new_items[i] {
                TextOrInline::Text(node) => {
                    // Trim trailing whitespace from the last text node
                    node.content = node.content.trim_end().to_string();
                    break;
                }
                TextOrInline::Inline(_) => {
                    // Inline elements like links/images shouldn't have whitespace trimmed
                    // But if we hit an inline, we're done (no text nodes after it to trim)
                    break;
                }
            }
        }

        // Rebuild the Text from the modified items
        *text = Text::default();
        for item in new_items {
            text.push(item);
        }
    }

    fn normalize_text_whitespace(&self, content: &str, text: &Text) -> String {
        // If the text is all whitespace, check if we need to preserve a space
        if content.chars().all(|c| c.is_whitespace()) {
            // If we already have content in the Text, and this is whitespace between elements,
            // preserve a single space
            if !text.is_empty() {
                return " ".to_string();
            } else {
                return String::new();
            }
        }

        // For actual text content, normalize whitespace
        let mut result = String::new();

        // Check if we need a leading space (text doesn't start with whitespace-only beginning)
        if !text.is_empty() && content.starts_with(|c: char| c.is_whitespace()) {
            result.push(' ');
        }

        // Add the trimmed content
        result.push_str(content.trim());

        // Check if we need a trailing space
        if content.ends_with(|c: char| c.is_whitespace()) && !content.trim().is_empty() {
            result.push(' ');
        }

        result
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
    fn test_mixed_nested_lists() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = MarkdownRenderer::new();

        let html = r#"
            <ol>
                <li>
                    <p>Chapter 1 introduction paragraph.</p>
                    <p>This chapter covers the basics of our topic with detailed explanations.</p>
                </li>
                <li>Chapter 2: Advanced Topics
                    <ul>
                        <li>
                            <p>Section A - Theory</p>
                            <p>This section explains theoretical foundations that will be used throughout.</p>
                        </li>
                        <li>Section B - Practice
                            <ol>
                                <li>Exercise 1: Basic implementation</li>
                                <li>
                                    <p>Exercise 2: Advanced implementation</p>
                                    <p>This exercise builds upon the previous one and introduces new concepts.</p>
                                    <ul>
                                        <li>Subtask 2.1: Setup environment</li>
                                        <li>
                                            <p>Subtask 2.2: Write code</p>
                                            <p>Make sure to follow best practices.</p>
                                        </li>
                                        <li>Subtask 2.3: Test thoroughly</li>
                                    </ul>
                                </li>
                                <li>Exercise 3: Integration</li>
                            </ol>
                        </li>
                        <li>Section C - Review
                            <ul>
                                <li>Review point 1</li>
                                <li>
                                    <p>Review point 2</p>
                                    <p>Additional notes about this review point.</p>
                                </li>
                            </ul>
                        </li>
                    </ul>
                </li>
                <li>
                    <p>Chapter 3: Conclusion</p>
                    <p>This final chapter summarizes everything we learned.</p>
                    <ol>
                        <li>Summary of key points</li>
                        <li>
                            <p>Future directions</p>
                            <p>Where to go from here and additional resources.</p>
                        </li>
                    </ol>
                </li>
            </ol>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"1. Chapter 1 introduction paragraph.
  This chapter covers the basics of our topic with detailed explanations.
2. Chapter 2: Advanced Topics
  * Section A - Theory
    This section explains theoretical foundations that will be used throughout.
  * Section B - Practice
    1. Exercise 1: Basic implementation
    2. Exercise 2: Advanced implementation
      This exercise builds upon the previous one and introduces new concepts.
      - Subtask 2.1: Setup environment
      - Subtask 2.2: Write code
        Make sure to follow best practices.
      - Subtask 2.3: Test thoroughly
    3. Exercise 3: Integration
  * Section C - Review
    + Review point 1
    + Review point 2
      Additional notes about this review point.
3. Chapter 3: Conclusion
  This final chapter summarizes everything we learned.
  1. Summary of key points
  2. Future directions
    Where to go from here and additional resources.

"#;

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_lists() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = MarkdownRenderer::new();

        let html = r#"
            <ul>
                <li>Level 1 Item 1</li>
                <li>Level 1 Item 2
                    <ul>
                        <li>Level 2 Item 1</li>
                        <li>Level 2 Item 2
                            <ul>
                                <li>Level 3 Item 1</li>
                                <li>Level 3 Item 2</li>
                                <li>Level 3 Item 3</li>
                            </ul>
                        </li>
                        <li>Level 2 Item 3</li>
                    </ul>
                </li>
                <li>Level 1 Item 3</li>
                <li>Level 1 Item 4
                    <ul>
                        <li>Another Level 2 Item</li>
                        <li>Another Level 2 Item with nesting
                            <ul>
                                <li>Another Level 3 Item</li>
                            </ul>
                        </li>
                    </ul>
                </li>
            </ul>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);
        eprintln!("{:#?}", doc);
        let expected = r#"
- Level 1 Item 1
- Level 1 Item 2
  * Level 2 Item 1
  * Level 2 Item 2
    + Level 3 Item 1
    + Level 3 Item 2
    + Level 3 Item 3
  * Level 2 Item 3
- Level 1 Item 3
- Level 1 Item 4
  * Another Level 2 Item
  * Another Level 2 Item with nesting
    + Another Level 3 Item

"#
        .trim_start();

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_nested_ordered_lists() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = MarkdownRenderer::new();

        let html = r#"
            <ol>
                <li>Chapter 1</li>
                <li>Chapter 2
                    <ol>
                        <li>Section 2.1</li>
                        <li>Section 2.2
                            <ol>
                                <li>Subsection 2.2.1</li>
                                <li>Subsection 2.2.2</li>
                                <li>Subsection 2.2.3</li>
                            </ol>
                        </li>
                        <li>Section 2.3</li>
                    </ol>
                </li>
                <li>Chapter 3</li>
                <li>Chapter 4
                    <ol>
                        <li>Section 4.1</li>
                        <li>Section 4.2
                            <ol>
                                <li>Subsection 4.2.1</li>
                            </ol>
                        </li>
                    </ol>
                </li>
            </ol>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"1. Chapter 1
2. Chapter 2
  1. Section 2.1
  2. Section 2.2
    1. Subsection 2.2.1
    2. Subsection 2.2.2
    3. Subsection 2.2.3
  3. Section 2.3
3. Chapter 3
4. Chapter 4
  1. Section 4.1
  2. Section 4.2
    1. Subsection 4.2.1

"#;

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_div_with_heading_and_list() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = MarkdownRenderer::new();

        let html = r#"
            <div class="technical-note">
                <h4>Protocol ACADEMIC Architecture</h4>
                <p>The protocol operates on multiple layers:</p>
                <ul>
                    <li><strong>Transport Layer:</strong> Standard academic network protocols (HTTP/HTTPS, SSH, FTP)</li>
                    <li><strong>Encoding Layer:</strong> Steganographic embedding in computational data</li>
                    <li><strong>Routing Layer:</strong> Messages distributed through research collaboration networks</li>
                    <li><strong>Command Layer:</strong> Encrypted instructions disguised as algorithm parameters</li>
                </ul>
            </div>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"#### Protocol ACADEMIC Architecture

The protocol operates on multiple layers:

- **Transport Layer:** Standard academic network protocols (HTTP/HTTPS, SSH, FTP)
- **Encoding Layer:** Steganographic embedding in computational data
- **Routing Layer:** Messages distributed through research collaboration networks
- **Command Layer:** Encrypted instructions disguised as algorithm parameters

"#;

        assert_eq!(rendered, expected);
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

    #[test]
    fn test_mathml_with_subscripts_in_paragraph() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"<p><a contenteditable="false" data-primary="evaluation methodology" data-secondary="language model for computing text perplexity" data-type="indexterm" id="id903"></a><a contenteditable="false" data-primary="language models" data-type="indexterm" id="id904"></a>A model's perplexity with respect to a text measures how difficult it is for the model to predict that text. Given a language model <em>X</em>, and a sequence of tokens <math xmlns="http://www.w3.org/1998/Math/MathML" alttext="left-bracket x 1 comma x 2 comma period period period comma x Subscript n Baseline right-bracket">
  <mrow>
    <mo>[</mo>
    <msub><mi>x</mi> <mn>1</mn> </msub>
    <mo>,</mo>
    <msub><mi>x</mi> <mn>2</mn> </msub>
    <mo>,</mo>
    <mo>.</mo>
    <mo>.</mo>
    <mo>.</mo>
    <mo>,</mo>
    <msub><mi>x</mi> <mi>n</mi> </msub>
    <mo>]</mo>
  </mrow>
</math>, <em>X</em>'s perplexity for this sequence is good</p>"#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"A model's perplexity with respect to a text measures how difficult it is for the model to predict that text. Given a language model _X_, and a sequence of tokens [x₁,x₂,...,xₙ], _X_'s perplexity for this sequence is good

"#;

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_paragraph_with_link_and_data_attributes() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"<p>To compute perplexity, you need access to the probabilities (or logprobs) the language model assigns to each next token. Unfortunately, not all commercial models expose their models' logprobs, as discussed in <a data-type="xref" href="ch02.html#ch02_understanding_foundation_models_1730147895571359">Chapter 2</a> that is awesome.</p>"#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"To compute perplexity, you need access to the probabilities (or logprobs) the language model assigns to each next token. Unfortunately, not all commercial models expose their models' logprobs, as discussed in [Chapter 2](ch02.html#ch02_understanding_foundation_models_1730147895571359) that is awesome.

"#;

        assert_eq!(rendered, expected);
    }
}
