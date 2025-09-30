use crate::markdown::{
    Block, DefinitionListItem, Document, HeadingLevel, Inline, Node, Style, Text, TextNode,
    TextOrInline,
};
use crate::mathml_renderer::{MathMLParser, mathml_to_ascii};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use log::{debug, info};
use markup5ever_rcdom::{NodeData, RcDom};
use std::rc::Rc;

/// Strategy for content collection mode
#[derive(Debug, Clone)]
enum ContentCollectionMode {
    /// Collect as flat text (for headings, simple content)
    FlatText { in_table: bool },
    /// Collect as structured blocks (for complex content with math)
    StructuredBlocks { in_table: bool },
}

/// Result of content collection
enum ContentResult {
    Text(Text),
    Blocks(Vec<Node>),
}

impl ContentResult {
    fn into_text(self) -> Text {
        match self {
            ContentResult::Text(text) => text,
            ContentResult::Blocks(blocks) => {
                let mut text = Text::default();
                for block_node in blocks {
                    match block_node.block {
                        Block::Paragraph { content } => {
                            for item in content.into_iter() {
                                text.push(item);
                            }
                        }
                        Block::CodeBlock { content, .. } => {
                            text.push_text(TextNode::new(content, Some(Style::Code)));
                        }
                        _ => {} // Skip other block types for now
                    }
                }
                text
            }
        }
    }

    fn into_blocks(self) -> Vec<Node> {
        match self {
            ContentResult::Blocks(blocks) => blocks,
            ContentResult::Text(text) => {
                if text.is_empty() {
                    vec![Node::new(
                        Block::Paragraph {
                            content: Text::default(),
                        },
                        0..0,
                    )]
                } else {
                    vec![Node::new(Block::Paragraph { content: text }, 0..0)]
                }
            }
        }
    }
}

/// Math content handling result
enum MathContent {
    Inline(String),
    Block(String),
    Error(String),
}

/// Text transformation to apply during processing
#[derive(Debug, Clone, PartialEq)]
enum TextTransform {
    Subscript,
    Superscript,
}

/// Processing context for content collection
#[derive(Debug, Clone)]
struct ProcessingContext {
    in_table: bool,
    current_style: Option<Style>,
    text_transform: Option<TextTransform>,
}

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

    /// Unified content collection method that handles both text and block modes
    fn collect_content(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        mode: ContentCollectionMode,
        context: ProcessingContext,
    ) -> ContentResult {
        match mode {
            ContentCollectionMode::FlatText { .. } => {
                let mut text = Text::default();
                self.collect_as_text(node, &mut text, context);
                ContentResult::Text(text)
            }
            ContentCollectionMode::StructuredBlocks { .. } => {
                let mut blocks = Vec::new();
                let mut current_text = Text::default();
                self.collect_as_blocks(node, &mut blocks, &mut current_text, context);

                // Flush any remaining text
                if !current_text.is_empty() {
                    blocks.push(Node::new(
                        Block::Paragraph {
                            content: current_text,
                        },
                        0..0,
                    ));
                }

                // // Ensure we always have at least one block
                // if blocks.is_empty() {
                //     blocks.push(Node::new(
                //         Block::Paragraph {
                //             content: Text::from("<fallback ERROR>"), //Text::default(),
                //         },
                //         0..0,
                //     ));
                // }

                ContentResult::Blocks(blocks)
            }
        }
    }

    /// Handle math elements with appropriate mode-specific logic
    fn handle_math_element(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        mode: &ContentCollectionMode,
    ) -> MathContent {
        let math_html = self.serialize_node_to_html(node);
        match mathml_to_ascii(&math_html, true) {
            Ok(ascii_math) => match mode {
                ContentCollectionMode::StructuredBlocks { .. } if ascii_math.contains('\n') => {
                    MathContent::Block(ascii_math)
                }
                _ => MathContent::Inline(ascii_math),
            },
            Err(e) => MathContent::Error(format!("Failed to parse math: {:?}", e)),
        }
    }

    /// Handle link elements consistently
    fn handle_link_element(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        context: ProcessingContext,
    ) -> Option<Inline> {
        if let Some(href) = self.get_attr_value(attrs, "href") {
            let mut link_text = Text::default();
            for child in node.children.borrow().iter() {
                self.collect_as_text(child, &mut link_text, context.clone());
            }
            let title = self.get_attr_value(attrs, "title");

            // Classify the link and extract target information
            let (link_type, target_chapter, target_anchor) =
                crate::markdown::classify_link_href(&href);

            Some(Inline::Link {
                text: link_text,
                url: href,
                title,
                link_type: Some(link_type),
                target_chapter,
                target_anchor,
            })
        } else {
            None
        }
    }

    /// Normalize text content with proper whitespace handling
    fn normalize_text_content(
        &self,
        content: &str,
        current_text: &Text,
        current_style: Option<Style>,
    ) -> Option<String> {
        // Special case: if content is only whitespace but we have existing text,
        // preserve a single space to maintain separation between elements
        if content.trim().is_empty() {
            if !current_text.is_empty() && content.contains(|c: char| c.is_whitespace()) {
                return Some(" ".to_string());
            }
            return None;
        }

        let mut normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");

        // Preserve leading space if original had it and we already have content
        if !current_text.is_empty() && content.chars().next().map_or(false, |c| c.is_whitespace()) {
            normalized = format!(" {}", normalized);
        }

        // Preserve trailing space if original had it
        if content.chars().last().map_or(false, |c| c.is_whitespace()) {
            normalized.push(' ');
        }

        if normalized.trim().is_empty() {
            return None;
        }

        // Add spacing around inline code elements
        let adjusted_content = if current_style == Some(Style::Code) {
            self.add_code_spacing(&normalized)
        } else {
            normalized
        };

        Some(adjusted_content)
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

        // Check for epub:type attribute first
        if let Some(epub_type) = self.get_epub_type_attr(attrs) {
            self.handle_epub_block(tag_name, epub_type, attrs, node, document);
            return;
        }

        match tag_name {
            "html" | "body" => {
                for child in node.children.borrow().iter() {
                    self.visit_node(child, document);
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.handle_heading(tag_name, attrs, node, document);
            }
            "div" | "section" | "article" => {
                // Check if this div/section has an ID and contains a heading
                let div_id = self.get_attr_value(attrs, "id");

                // Check if the first significant child is a heading
                let mut has_immediate_heading = false;
                for child in node.children.borrow().iter() {
                    if let NodeData::Element { ref name, .. } = child.data {
                        let child_tag = name.local.as_ref();
                        if matches!(child_tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                            has_immediate_heading = true;
                            break;
                        }
                        // Don't break on whitespace-only text nodes
                        if child_tag != "style" && child_tag != "script" {
                            // If we hit a non-heading element, stop looking
                            break;
                        }
                    } else if let NodeData::Text { ref contents } = child.data {
                        // Skip whitespace-only text nodes
                        if !contents.borrow().trim().is_empty() {
                            break;
                        }
                    }
                }

                if has_immediate_heading && div_id.is_some() {
                    // Process children but pass the div's ID to any heading
                    for child in node.children.borrow().iter() {
                        if let NodeData::Element {
                            ref name,
                            ref attrs,
                            ..
                        } = child.data
                        {
                            let child_tag = name.local.as_ref();
                            if matches!(child_tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                                // Handle the heading with the div's ID if the heading doesn't have its own
                                let heading_id = self.get_attr_value(attrs, "id");
                                if heading_id.is_none() {
                                    // Use the div's ID for this heading
                                    self.handle_heading_with_id(
                                        child_tag,
                                        child,
                                        document,
                                        div_id.clone(),
                                    );
                                } else {
                                    // Heading has its own ID, use that
                                    self.handle_heading(child_tag, attrs, child, document);
                                }
                            } else {
                                self.visit_node(child, document);
                            }
                        } else {
                            self.visit_node(child, document);
                        }
                    }
                } else {
                    // Normal div without special heading handling
                    for child in node.children.borrow().iter() {
                        self.visit_node(child, document);
                    }
                }
            }
            "p" => {
                self.handle_paragraph(attrs, node, document);
            }
            "img" => {
                self.handle_image(attrs, document);
            }
            "pre" => {
                self.handle_pre(attrs, node, document);
            }
            "math" => {
                self.handle_mathml(attrs, node, document);
            }
            "ul" | "ol" => {
                self.handle_list(tag_name, attrs, node, document);
            }
            "style" | "script" | "head" => {
                // Do nothing
            }
            // Handle inline formatting elements within other content
            "strong" | "b" | "em" | "i" | "code" | "a" | "br" | "del" | "s" | "strike" | "sub"
            | "sup" => {
                // These are handled within extract_formatted_content, skip at block level
                for child in node.children.borrow().iter() {
                    self.visit_node(child, document);
                }
            }
            "table" => {
                self.handle_table(attrs, node, document);
            }
            "dl" => {
                self.handle_definition_list(attrs, node, document);
            }
            "blockquote" => {
                self.handle_blockquote(attrs, node, document);
            }
            "hr" => {
                // Add a thematic break (horizontal rule)
                document.blocks.push(Node::new(Block::ThematicBreak, 0..0));
            }
            // Skip li, dt, dd at this level - they're handled within their containers
            "li" | "dt" | "dd" => {}
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
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
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

        // Extract the id attribute from the HTML element
        let id = self.get_attr_value(attrs, "id");

        let heading_block = Block::Heading { level, content };
        let heading_node = Node::new_with_id(heading_block, 0..0, id); // Store HTML id for anchor resolution
        document.blocks.push(heading_node);
    }

    fn handle_heading_with_id(
        &mut self,
        tag_name: &str,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
        provided_id: Option<String>,
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
        let heading_node = Node::new_with_id(heading_block, 0..0, provided_id); // Use the provided ID from parent div
        document.blocks.push(heading_node);
    }

    fn handle_paragraph(
        &mut self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let blocks = self.extract_formatted_content_as_blocks(node, false);

        // Extract the id attribute from the HTML paragraph element
        let paragraph_id = self.get_attr_value(attrs, "id");

        // Filter out empty paragraph blocks and apply ID to paragraph blocks
        for mut block_node in blocks {
            let should_add = match &block_node.block {
                Block::Paragraph { content } => !content.is_empty(),
                Block::CodeBlock { content, .. } => !content.trim().is_empty(),
                _ => true,
            };

            if should_add {
                // Apply the paragraph ID only to paragraph blocks
                if matches!(block_node.block, Block::Paragraph { .. }) {
                    block_node.id = paragraph_id.clone();
                }
                document.blocks.push(block_node);
            }
        }
    }

    fn handle_image(
        &mut self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        document: &mut Document,
    ) {
        if let Some(src) = self.get_attr_value(attrs, "src") {
            info!("Found image during HTML->Markdown conversion: src={}", src);
            let alt_text = self.get_attr_value(attrs, "alt").unwrap_or_default();
            let title = self.get_attr_value(attrs, "title");

            // Extract the id attribute from the HTML img element
            let id = self.get_attr_value(attrs, "id");

            // Create an Inline::Image
            let image_inline = Inline::Image {
                alt_text,
                url: src,
                title,
            };

            let mut content = Text::default();
            content.push_inline(image_inline);
            let paragraph_block = Block::Paragraph { content };
            let paragraph_node = Node::new_with_id(paragraph_block, 0..0, id);
            document.blocks.push(paragraph_node);
        }
    }

    fn handle_pre(
        &mut self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        // TODO: In the future, we should handle inline formatting like <sub> within <pre>
        let mut content = String::new();
        self.collect_text_from_node(node, &mut content);

        let id = self.get_attr_value(attrs, "id");

        let language = self.get_attr_value(attrs, "data-type");

        let code_block = Block::CodeBlock { language, content };
        let code_node = Node::new_with_id(code_block, 0..0, id);
        document.blocks.push(code_node);
    }

    fn collect_text_from_node(&self, node: &Rc<markup5ever_rcdom::Node>, output: &mut String) {
        match &node.data {
            NodeData::Text { contents } => {
                output.push_str(&contents.borrow());
            }
            _ => {
                for child in node.children.borrow().iter() {
                    self.collect_text_from_node(child, output);
                }
            }
        }
    }

    fn handle_mathml(
        &mut self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let mathml_html = self.serialize_node_to_html(node);

        // Extract the id attribute from the HTML mathml element
        let id = self.get_attr_value(attrs, "id");

        let content = match mathml_to_ascii(&mathml_html, true) {
            Ok(ascii_math) => {
                // Check if the math expression is single-line or multi-line
                if ascii_math.contains('\n') {
                    // Multi-line math: use CodeBlock with language "math"
                    let code_block = Block::CodeBlock {
                        language: Some("math".to_string()),
                        content: ascii_math,
                    };
                    Node::new_with_id(code_block, 0..0, id)
                } else {
                    // Single-line math: use as regular paragraph text
                    let content = Text::from(ascii_math);
                    let paragraph_block = Block::Paragraph { content };
                    Node::new_with_id(paragraph_block, 0..0, id)
                }
            }
            Err(e) => {
                let paragraph_block = Block::CodeBlock {
                    language: Some(format!("failed to extract mathml: {:?}", e)),
                    content: mathml_html,
                };
                Node::new_with_id(paragraph_block, 0..0, id)
            }
        };

        document.blocks.push(content);
    }

    fn handle_table(
        &mut self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let mut header: Option<crate::markdown::TableRow> = None;
        let mut rows: Vec<crate::markdown::TableRow> = Vec::new();
        let mut max_columns = 0;
        let mut rowspan_tracker: Vec<u32> = Vec::new(); // Track remaining rowspan for each column

        // Process table children to find thead and tbody
        for child in node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, .. } => {
                    let tag_name = name.local.as_ref();
                    match tag_name {
                        "thead" => {
                            // Process header rows
                            for thead_child in child.children.borrow().iter() {
                                if let NodeData::Element { name, .. } = &thead_child.data {
                                    if name.local.as_ref() == "tr" {
                                        let row = self.extract_table_row_with_rowspan(
                                            thead_child,
                                            &mut rowspan_tracker,
                                        );
                                        max_columns = max_columns.max(row.cells.len());
                                        header = Some(row);
                                        break; // Only take the first header row
                                    }
                                }
                            }
                        }
                        "tbody" => {
                            // Process body rows
                            for tbody_child in child.children.borrow().iter() {
                                if let NodeData::Element { name, .. } = &tbody_child.data {
                                    if name.local.as_ref() == "tr" {
                                        let row = self.extract_table_row_with_rowspan(
                                            tbody_child,
                                            &mut rowspan_tracker,
                                        );
                                        max_columns = max_columns.max(row.cells.len());
                                        rows.push(row);
                                    }
                                }
                            }
                        }
                        "tr" => {
                            // Direct tr children (no tbody/thead)
                            let row =
                                self.extract_table_row_with_rowspan(child, &mut rowspan_tracker);
                            max_columns = max_columns.max(row.cells.len());

                            // First row becomes header if we don't have one yet
                            if header.is_none() && rows.is_empty() {
                                header = Some(row);
                            } else {
                                rows.push(row);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Set default alignment for all columns
        let alignment = vec![crate::markdown::TableAlignment::None; max_columns];

        // Pad all rows to have max_columns cells
        if let Some(ref mut header_row) = header {
            while header_row.cells.len() < max_columns {
                header_row.cells.push(crate::markdown::TableCell::new(
                    crate::markdown::Text::default(),
                ));
            }
        }
        for row in &mut rows {
            while row.cells.len() < max_columns {
                row.cells.push(crate::markdown::TableCell::new(
                    crate::markdown::Text::default(),
                ));
            }
        }

        // Only create table if we have content
        if header.is_some() || !rows.is_empty() {
            // Extract the id attribute from the HTML table element
            let id = self.get_attr_value(attrs, "id");

            let table_block = Block::Table {
                header,
                rows,
                alignment,
            };
            let table_node = Node::new_with_id(table_block, 0..0, id);
            document.blocks.push(table_node);
        }
    }

    fn extract_table_as_block(&mut self, node: &Rc<markup5ever_rcdom::Node>) -> Option<Block> {
        let mut header: Option<crate::markdown::TableRow> = None;
        let mut rows: Vec<crate::markdown::TableRow> = Vec::new();
        let mut max_columns = 0;
        let mut rowspan_tracker: Vec<u32> = Vec::new(); // Track remaining rowspan for each column

        // Process table children to find thead and tbody
        for child in node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, .. } => {
                    let tag_name = name.local.as_ref();
                    match tag_name {
                        "thead" => {
                            // Process header rows
                            for thead_child in child.children.borrow().iter() {
                                if let NodeData::Element { name, .. } = &thead_child.data {
                                    if name.local.as_ref() == "tr" {
                                        let row = self.extract_table_row_with_rowspan(
                                            thead_child,
                                            &mut rowspan_tracker,
                                        );
                                        max_columns = max_columns.max(row.cells.len());
                                        header = Some(row);
                                        break; // Only take the first header row
                                    }
                                }
                            }
                        }
                        "tbody" => {
                            // Process body rows
                            for tbody_child in child.children.borrow().iter() {
                                if let NodeData::Element { name, .. } = &tbody_child.data {
                                    if name.local.as_ref() == "tr" {
                                        let row = self.extract_table_row_with_rowspan(
                                            tbody_child,
                                            &mut rowspan_tracker,
                                        );
                                        max_columns = max_columns.max(row.cells.len());
                                        rows.push(row);
                                    }
                                }
                            }
                        }
                        "tr" => {
                            // Direct tr children (no tbody/thead)
                            let row =
                                self.extract_table_row_with_rowspan(child, &mut rowspan_tracker);
                            max_columns = max_columns.max(row.cells.len());

                            // First row becomes header if we don't have one yet
                            if header.is_none() && rows.is_empty() {
                                header = Some(row);
                            } else {
                                rows.push(row);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Set default alignment for all columns
        let alignment = vec![crate::markdown::TableAlignment::None; max_columns];

        // Pad all rows to have max_columns cells
        if let Some(ref mut header_row) = header {
            while header_row.cells.len() < max_columns {
                header_row.cells.push(crate::markdown::TableCell::new(
                    crate::markdown::Text::default(),
                ));
            }
        }
        for row in &mut rows {
            while row.cells.len() < max_columns {
                row.cells.push(crate::markdown::TableCell::new(
                    crate::markdown::Text::default(),
                ));
            }
        }

        // Only create table if we have content
        if header.is_some() || !rows.is_empty() {
            Some(Block::Table {
                header,
                rows,
                alignment,
            })
        } else {
            None
        }
    }

    fn extract_table_row_with_rowspan(
        &mut self,
        tr_node: &Rc<markup5ever_rcdom::Node>,
        rowspan_tracker: &mut Vec<u32>,
    ) -> crate::markdown::TableRow {
        let mut cells = Vec::new();
        let mut column_index = 0;

        // Collect all actual td/th elements first
        let mut actual_cells = Vec::new();
        for child in tr_node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, attrs, .. } => {
                    let tag_name = name.local.as_ref();
                    if tag_name == "th" || tag_name == "td" {
                        let content = self.extract_formatted_content_with_context(child, true);
                        let rowspan = self
                            .get_attr_value(attrs, "rowspan")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(1);

                        let cell = if tag_name == "th" {
                            if rowspan > 1 {
                                crate::markdown::TableCell::new_header_with_rowspan(
                                    content, rowspan,
                                )
                            } else {
                                crate::markdown::TableCell::new_header(content)
                            }
                        } else {
                            if rowspan > 1 {
                                crate::markdown::TableCell::new_with_rowspan(content, rowspan)
                            } else {
                                crate::markdown::TableCell::new(content)
                            }
                        };
                        actual_cells.push((cell, rowspan));
                    }
                }
                _ => {}
            }
        }

        // Now build the row, skipping occupied columns
        let mut actual_cell_index = 0;
        while actual_cell_index < actual_cells.len() || column_index < rowspan_tracker.len() {
            // Extend rowspan_tracker if needed
            while rowspan_tracker.len() <= column_index {
                rowspan_tracker.push(0);
            }

            if rowspan_tracker[column_index] > 0 {
                // This column is occupied by a cell from previous row
                cells.push(crate::markdown::TableCell::new(
                    crate::markdown::Text::default(),
                ));
                rowspan_tracker[column_index] -= 1;
            } else if actual_cell_index < actual_cells.len() {
                // Place the next actual cell here
                let (cell, rowspan) = actual_cells[actual_cell_index].clone();
                cells.push(cell);

                // Set up rowspan tracking for this cell (subtract 1 for current row)
                if rowspan > 1 {
                    rowspan_tracker[column_index] = rowspan - 1;
                } else {
                    rowspan_tracker[column_index] = 0;
                }

                actual_cell_index += 1;
            } else {
                // No more actual cells, but we still need to decrement remaining rowspans
                rowspan_tracker[column_index] = 0;
            }

            column_index += 1;
        }

        crate::markdown::TableRow::new(cells)
    }

    fn handle_list(
        &mut self,
        tag_name: &str,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let kind = self.get_list_kind(tag_name, attrs);

        let items = self.extract_list_items(node);

        if !items.is_empty() {
            // Extract the id attribute from the HTML list element (ul/ol)
            let id = self.get_attr_value(attrs, "id");

            let list_block = Block::List { kind, items };
            let list_node = Node::new_with_id(list_block, 0..0, id); // Store HTML id for anchor resolution
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

    /// Extracts block content from a container element (li, dd, etc.)
    /// This handles nested lists, paragraphs, images, code blocks, etc.
    fn extract_container_blocks(
        &mut self,
        container_node: &Rc<markup5ever_rcdom::Node>,
    ) -> Vec<Node> {
        let mut content = Vec::new();
        let mut current_text = Text::default();

        for child in container_node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, attrs, .. } => {
                    let tag_name = name.local.as_ref();

                    match tag_name {
                        "ul" | "ol" => {
                            // Flush current text before block element
                            self.flush_text_as_paragraph(&mut current_text, &mut content);

                            let kind = self.get_list_kind(tag_name, attrs);
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
                            self.flush_text_as_paragraph(&mut current_text, &mut content);

                            let para_content = self.extract_formatted_content(child);
                            if !para_content.is_empty() {
                                let paragraph = Block::Paragraph {
                                    content: para_content,
                                };
                                content.push(Node::new(paragraph, 0..0));
                            }
                        }
                        "img" => {
                            // Handle images inline within the container
                            if let Some(src) = self.get_attr_value(attrs, "src") {
                                info!("Found inline image in container: src={}", src);
                                let alt_text =
                                    self.get_attr_value(attrs, "alt").unwrap_or_default();
                                let title = self.get_attr_value(attrs, "title");

                                let image_inline = Inline::Image {
                                    alt_text,
                                    url: src,
                                    title,
                                };
                                current_text.push_inline(image_inline);
                            }
                        }
                        "pre" => {
                            self.flush_text_as_paragraph(&mut current_text, &mut content);

                            // Extract code block content
                            let mut code_content = String::new();
                            self.collect_text_from_node(child, &mut code_content);
                            let language = self.get_attr_value(attrs, "data-type");

                            let code_block = Block::CodeBlock {
                                language,
                                content: code_content,
                            };
                            content.push(Node::new(code_block, 0..0));
                        }
                        "table" => {
                            // Flush current text before table
                            self.flush_text_as_paragraph(&mut current_text, &mut content);

                            // Extract table content using the existing table extraction logic
                            let table_block = self.extract_table_as_block(child);
                            if let Some(table) = table_block {
                                content.push(Node::new(table, 0..0));
                            }
                        }
                        "math" => {
                            // Handle MathML elements - check if multiline to determine block vs inline
                            let mathml_html = self.serialize_node_to_html(child);
                            match mathml_to_ascii(&mathml_html, true) {
                                Ok(ascii_math) => {
                                    if ascii_math.contains('\n') {
                                        // Multi-line math: flush text and create CodeBlock
                                        self.flush_text_as_paragraph(
                                            &mut current_text,
                                            &mut content,
                                        );

                                        let code_block = Block::CodeBlock {
                                            language: Some("math".to_string()),
                                            content: ascii_math,
                                        };
                                        content.push(Node::new(code_block, 0..0));
                                    } else {
                                        // Single-line math: add as inline text
                                        current_text.push_text(TextNode::new(ascii_math, None));
                                    }
                                }
                                Err(e) => {
                                    // Error case: add as text
                                    let error_text = format!("Failed to parse math: {:?}", e);
                                    current_text.push_text(TextNode::new(error_text, None));
                                }
                            }
                        }
                        _ => {
                            let context = ProcessingContext {
                                in_table: false,
                                current_style: None,
                                text_transform: None,
                            };
                            self.collect_as_text(child, &mut current_text, context);
                        }
                    }
                }
                NodeData::Text { .. } => {
                    // Process text nodes through the normal formatted content collector
                    let context = ProcessingContext {
                        in_table: false,
                        current_style: None,
                        text_transform: None,
                    };
                    self.collect_as_text(child, &mut current_text, context);
                }
                _ => {
                    // Process other node types
                    for grandchild in child.children.borrow().iter() {
                        let context = ProcessingContext {
                            in_table: false,
                            current_style: None,
                            text_transform: None,
                        };
                        self.collect_as_text(grandchild, &mut current_text, context);
                    }
                }
            }
        }

        // Flush any remaining text
        self.flush_text_as_paragraph(&mut current_text, &mut content);

        content
    }

    /// Helper method to flush current text as a paragraph if it has content
    fn flush_text_as_paragraph(&mut self, current_text: &mut Text, content: &mut Vec<Node>) {
        if !current_text.is_empty() {
            // Trim trailing whitespace before creating paragraph
            self.trim_text_trailing_whitespace(current_text);

            // Check if the text has any non-whitespace content
            let has_content = current_text.clone().into_iter().any(|item| match item {
                TextOrInline::Text(node) => !node.content.trim().is_empty(),
                TextOrInline::Inline(_) => true,
            });

            if has_content {
                let paragraph = Block::Paragraph {
                    content: current_text.clone(),
                };
                content.push(Node::new(paragraph, 0..0));
            }
            *current_text = Text::default();
        }
    }

    /// Extract list kind from tag name and attributes
    fn get_list_kind(
        &self,
        tag_name: &str,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
    ) -> crate::markdown::ListKind {
        if tag_name == "ol" {
            let start = self
                .get_attr_value(attrs, "start")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            crate::markdown::ListKind::Ordered { start }
        } else {
            crate::markdown::ListKind::Unordered
        }
    }

    fn extract_list_item(
        &mut self,
        li_node: &Rc<markup5ever_rcdom::Node>,
    ) -> crate::markdown::ListItem {
        let content = self.extract_container_blocks(li_node);

        crate::markdown::ListItem::new(content)
    }

    fn extract_definition_content(&mut self, dd_node: &Rc<markup5ever_rcdom::Node>) -> Vec<Node> {
        // Extract blocks from definition element
        // Note: Can return empty Vec - definition list items can have empty definitions
        self.extract_container_blocks(dd_node)
    }

    fn handle_definition_list(
        &mut self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let mut definition_items = Vec::new();
        let mut current_term: Option<Text> = None;
        let mut current_definitions: Vec<Vec<Node>> = Vec::new();

        for child in node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, .. } => {
                    let tag_name = name.local.as_ref();
                    match tag_name {
                        "dt" => {
                            // If we have a previous term with definitions, save it
                            if let Some(term) = current_term.take() {
                                if !current_definitions.is_empty() {
                                    definition_items
                                        .push(DefinitionListItem::new(term, current_definitions));
                                    current_definitions = Vec::new();
                                }
                            }

                            // Extract new term
                            let term_content = self.extract_formatted_content(child);
                            if !term_content.is_empty() {
                                current_term = Some(term_content);
                            }
                        }
                        "dd" => {
                            // Extract definition content as blocks (like list items)
                            let definition_blocks = self.extract_definition_content(child);
                            if !definition_blocks.is_empty() {
                                current_definitions.push(definition_blocks);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Don't forget the last term
        if let Some(term) = current_term {
            if !current_definitions.is_empty() {
                definition_items.push(DefinitionListItem::new(term, current_definitions));
            }
        }

        // Create the definition list block if we have items
        if !definition_items.is_empty() {
            // Extract the id attribute from the HTML dl element
            let id = self.get_attr_value(attrs, "id");

            let definition_list_block = Block::DefinitionList {
                items: definition_items,
            };
            let definition_list_node = Node::new_with_id(definition_list_block, 0..0, id);
            document.blocks.push(definition_list_node);
        }
    }

    fn handle_blockquote(
        &mut self,
        _attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        // Extract the content of the blockquote as blocks
        let content = self.extract_container_blocks(node);

        // Create a Quote block containing the extracted content
        if !content.is_empty() {
            let quote_block = Block::Quote { content };
            document.blocks.push(Node::new(quote_block, 0..0));
        }
    }

    fn handle_epub_block(
        &mut self,
        element_name: &str,
        epub_type: String,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        node: &Rc<markup5ever_rcdom::Node>,
        document: &mut Document,
    ) {
        let mut content = Vec::new();

        // Process children directly to properly handle block elements
        for child in node.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, attrs, .. } => {
                    let tag_name = name.local.as_ref();
                    match tag_name {
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            let level = match tag_name {
                                "h1" => HeadingLevel::H1,
                                "h2" => HeadingLevel::H2,
                                "h3" => HeadingLevel::H3,
                                "h4" => HeadingLevel::H4,
                                "h5" => HeadingLevel::H5,
                                "h6" => HeadingLevel::H6,
                                _ => HeadingLevel::H1,
                            };

                            let heading_content = self.extract_formatted_content(child);
                            let heading_block = Block::Heading {
                                level,
                                content: heading_content,
                            };
                            content.push(Node::new(heading_block, 0..0));
                        }
                        "p" => {
                            let para_content = self.extract_formatted_content(child);
                            if !para_content.is_empty() {
                                let paragraph_block = Block::Paragraph {
                                    content: para_content,
                                };
                                content.push(Node::new(paragraph_block, 0..0));
                            }
                        }
                        "ul" | "ol" => {
                            let kind = self.get_list_kind(tag_name, attrs);

                            let items = self.extract_list_items(child);
                            if !items.is_empty() {
                                let list_block = Block::List { kind, items };
                                content.push(Node::new(list_block, 0..0));
                            }
                        }
                        "div" | "section" | "article" => {
                            // For wrapper elements like div, recursively process their children
                            let mut temp_doc = Document::new();
                            for grandchild in child.children.borrow().iter() {
                                self.visit_node(grandchild, &mut temp_doc);
                            }
                            content.extend(temp_doc.blocks);
                        }
                        _ => {
                            // For other elements, process as blocks
                            let blocks = self.extract_formatted_content_as_blocks(child, false);
                            content.extend(blocks);
                        }
                    }
                }
                _ => {
                    // For text nodes and other non-element nodes, process as blocks
                    let blocks = self.extract_formatted_content_as_blocks(child, false);
                    content.extend(blocks);
                }
            }
        }

        // Note: EpubBlock can have empty content vector - no need to add default paragraph

        // Extract the id attribute from the HTML element with epub:type
        let id = self.get_attr_value(attrs, "id");

        let epub_block = Block::EpubBlock {
            epub_type,
            element_name: element_name.to_string(),
            content,
        };

        let epub_node = Node::new_with_id(epub_block, 0..0, id);
        document.blocks.push(epub_node);
    }

    fn extract_formatted_content(&self, node: &Rc<markup5ever_rcdom::Node>) -> Text {
        let mode = ContentCollectionMode::FlatText { in_table: false };
        let context = ProcessingContext {
            in_table: false,
            current_style: None,
            text_transform: None,
        };
        self.collect_content(node, mode, context).into_text()
    }

    fn extract_formatted_content_with_context(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        in_table: bool,
    ) -> Text {
        let mode = ContentCollectionMode::FlatText { in_table };
        let context = ProcessingContext {
            in_table,
            current_style: None,
            text_transform: None,
        };
        self.collect_content(node, mode, context).into_text()
    }

    fn extract_formatted_content_as_blocks(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        in_table: bool,
    ) -> Vec<Node> {
        let mode = ContentCollectionMode::StructuredBlocks { in_table };
        let context = ProcessingContext {
            in_table,
            current_style: None,
            text_transform: None,
        };
        self.collect_content(node, mode, context).into_blocks()
    }

    /// Collect content as text (unified implementation)
    fn collect_as_text(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        text: &mut Text,
        context: ProcessingContext,
    ) {
        match &node.data {
            NodeData::Text { contents } => {
                let content = contents.borrow().to_string();
                if let Some(normalized) =
                    self.normalize_text_content(&content, text, context.current_style.clone())
                {
                    // Apply text transformation if needed
                    let transformed_text =
                        match context.text_transform {
                            Some(TextTransform::Subscript) => {
                                MathMLParser::try_unicode_subscript(&normalized, true)
                                    .unwrap_or_else(|| {
                                        if normalized.len() == 1 {
                                            format!("_{}", normalized)
                                        } else {
                                            format!("_{{{}}}", normalized)
                                        }
                                    })
                            }
                            Some(TextTransform::Superscript) => {
                                MathMLParser::try_unicode_superscript(&normalized, true)
                                    .unwrap_or_else(|| {
                                        if normalized.len() == 1 {
                                            format!("^{}", normalized)
                                        } else {
                                            format!("^{{{}}}", normalized)
                                        }
                                    })
                            }
                            None => normalized,
                        };
                    let text_node = TextNode::new(transformed_text, context.current_style.clone());
                    text.push_text(text_node);
                }
            }
            NodeData::Element { name, attrs, .. } => {
                let tag_name = name.local.as_ref();
                self.handle_element_for_text(node, tag_name, attrs, text, context);
            }
            _ => {
                // For other node types, process children with inherited context
                for child in node.children.borrow().iter() {
                    self.collect_as_text(child, text, context.clone());
                }
            }
        }
    }

    /// Collect content as blocks (unified implementation)
    fn collect_as_blocks(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        blocks: &mut Vec<Node>,
        current_text: &mut Text,
        context: ProcessingContext,
    ) {
        match &node.data {
            NodeData::Text { contents } => {
                let content = contents.borrow().to_string();
                if let Some(normalized) = self.normalize_text_content(
                    &content,
                    current_text,
                    context.current_style.clone(),
                ) {
                    // Apply text transformation if needed
                    let transformed_text =
                        match context.text_transform {
                            Some(TextTransform::Subscript) => {
                                MathMLParser::try_unicode_subscript(&normalized, true)
                                    .unwrap_or_else(|| {
                                        if normalized.len() == 1 {
                                            format!("_{}", normalized)
                                        } else {
                                            format!("_{{{}}}", normalized)
                                        }
                                    })
                            }
                            Some(TextTransform::Superscript) => {
                                MathMLParser::try_unicode_superscript(&normalized, true)
                                    .unwrap_or_else(|| {
                                        if normalized.len() == 1 {
                                            format!("^{}", normalized)
                                        } else {
                                            format!("^{{{}}}", normalized)
                                        }
                                    })
                            }
                            None => normalized,
                        };
                    let text_node = TextNode::new(transformed_text, context.current_style.clone());
                    current_text.push_text(text_node);
                }
            }
            NodeData::Element { name, attrs, .. } => {
                let tag_name = name.local.as_ref();
                self.handle_element_for_blocks(
                    node,
                    tag_name,
                    attrs,
                    blocks,
                    current_text,
                    context,
                );
            }
            _ => {
                // For other node types, process children with inherited context
                for child in node.children.borrow().iter() {
                    self.collect_as_blocks(child, blocks, current_text, context.clone());
                }
            }
        }
    }

    /// Handle element processing for text collection
    fn handle_element_for_text(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        tag_name: &str,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        text: &mut Text,
        context: ProcessingContext,
    ) {
        let new_context = self.get_element_context(tag_name, context.clone());

        match tag_name {
            "a" => {
                // Check if this <a> element has an ID attribute - if so, create an anchor marker
                if let Some(id) = self.get_attr_value(attrs, "id") {
                    text.push_inline(Inline::Anchor { id });
                }

                // Handle as a link if it has an href
                if let Some(link) = self.handle_link_element(node, attrs, context) {
                    text.push_inline(link);
                } else {
                    // If it's not a link (no href), still process children
                    // This handles cases like <a id="anchor">text content</a>
                    for child in node.children.borrow().iter() {
                        self.collect_as_text(child, text, new_context.clone());
                    }
                }
            }
            "math" => {
                let mode = ContentCollectionMode::FlatText {
                    in_table: context.in_table,
                };
                match self.handle_math_element(node, &mode) {
                    MathContent::Inline(math_text) | MathContent::Block(math_text) => {
                        text.push_text(TextNode::new(math_text, None));
                    }
                    MathContent::Error(error_text) => {
                        text.push_text(TextNode::new(error_text, None));
                    }
                }
            }
            "br" => {
                if context.in_table {
                    text.push_text(TextNode::new("<br/>".to_string(), None));
                } else {
                    text.push_inline(Inline::LineBreak);
                }
            }
            "sub" => {
                // Process children with subscript transformation context
                let sub_context = ProcessingContext {
                    in_table: context.in_table,
                    current_style: context.current_style,
                    text_transform: Some(TextTransform::Subscript),
                };
                for child in node.children.borrow().iter() {
                    self.collect_as_text(child, text, sub_context.clone());
                }
            }
            "sup" => {
                // Process children with superscript transformation context
                let sup_context = ProcessingContext {
                    in_table: context.in_table,
                    current_style: context.current_style,
                    text_transform: Some(TextTransform::Superscript),
                };
                for child in node.children.borrow().iter() {
                    self.collect_as_text(child, text, sup_context.clone());
                }
            }
            "img" => {
                // Handle image elements
                if let Some(src) = self.get_attr_value(attrs, "src") {
                    info!("Found inline image in text context: src={}", src);
                    let alt_text = self.get_attr_value(attrs, "alt").unwrap_or_default();
                    let title = self.get_attr_value(attrs, "title");

                    let image_inline = Inline::Image {
                        alt_text,
                        url: src,
                        title,
                    };
                    text.push_inline(image_inline);
                }
            }
            _ => {
                // Check if this element has an ID attribute - if so, create an anchor marker
                if let Some(id) = self.get_attr_value(attrs, "id") {
                    text.push_inline(Inline::Anchor { id });
                }

                // Process children with the new context
                for child in node.children.borrow().iter() {
                    self.collect_as_text(child, text, new_context.clone());
                }
            }
        }
    }

    /// Handle element processing for block collection
    fn handle_element_for_blocks(
        &self,
        node: &Rc<markup5ever_rcdom::Node>,
        tag_name: &str,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
        blocks: &mut Vec<Node>,
        current_text: &mut Text,
        context: ProcessingContext,
    ) {
        let new_context = self.get_element_context(tag_name, context.clone());

        match tag_name {
            "a" => {
                // Check if this <a> element has an ID attribute - if so, create an anchor marker
                if let Some(id) = self.get_attr_value(attrs, "id") {
                    current_text.push_inline(Inline::Anchor { id });
                }

                // Handle as a link if it has an href
                if let Some(link) = self.handle_link_element(node, attrs, context) {
                    current_text.push_inline(link);
                } else {
                    // If it's not a link (no href), still process children
                    // This handles cases like <a id="anchor">text content</a>
                    for child in node.children.borrow().iter() {
                        self.collect_as_blocks(child, blocks, current_text, new_context.clone());
                    }
                }
            }
            "math" => {
                let mode = ContentCollectionMode::StructuredBlocks {
                    in_table: context.in_table,
                };
                match self.handle_math_element(node, &mode) {
                    MathContent::Block(math_text) => {
                        // Flush current text and create CodeBlock
                        if !current_text.is_empty() {
                            blocks.push(Node::new(
                                Block::Paragraph {
                                    content: current_text.clone(),
                                },
                                0..0,
                            ));
                            *current_text = Text::default();
                        }
                        blocks.push(Node::new(
                            Block::CodeBlock {
                                language: Some("math".to_string()),
                                content: math_text,
                            },
                            0..0,
                        ));
                    }
                    MathContent::Inline(math_text) => {
                        current_text.push_text(TextNode::new(math_text, None));
                    }
                    MathContent::Error(error_text) => {
                        current_text.push_text(TextNode::new(error_text, None));
                    }
                }
            }
            "br" => {
                if context.in_table {
                    current_text.push_text(TextNode::new("<br/>".to_string(), None));
                } else {
                    current_text.push_inline(Inline::LineBreak);
                }
            }
            "sub" => {
                // Process children with subscript transformation
                let sub_context = ProcessingContext {
                    in_table: context.in_table,
                    current_style: context.current_style,
                    text_transform: Some(TextTransform::Subscript),
                };
                for child in node.children.borrow().iter() {
                    self.collect_as_blocks(child, blocks, current_text, sub_context.clone());
                }
            }
            "sup" => {
                // Process children with superscript transformation
                let sup_context = ProcessingContext {
                    in_table: context.in_table,
                    current_style: context.current_style,
                    text_transform: Some(TextTransform::Superscript),
                };
                for child in node.children.borrow().iter() {
                    self.collect_as_blocks(child, blocks, current_text, sup_context.clone());
                }
            }
            "img" => {
                // Handle image elements inside paragraphs and other blocks
                if let Some(src) = self.get_attr_value(attrs, "src") {
                    info!("Found inline image in block context: src={}", src);
                    let alt_text = self.get_attr_value(attrs, "alt").unwrap_or_default();
                    let title = self.get_attr_value(attrs, "title");

                    let image_inline = Inline::Image {
                        alt_text,
                        url: src,
                        title,
                    };
                    current_text.push_inline(image_inline);
                }
            }
            _ => {
                // Check if this element has an ID attribute - if so, create an anchor marker
                if let Some(id) = self.get_attr_value(attrs, "id") {
                    current_text.push_inline(Inline::Anchor { id });
                }

                // Process children with the new context
                for child in node.children.borrow().iter() {
                    self.collect_as_blocks(child, blocks, current_text, new_context.clone());
                }
            }
        }
    }

    /// Get appropriate context for element based on tag name
    fn get_element_context(
        &self,
        tag_name: &str,
        mut context: ProcessingContext,
    ) -> ProcessingContext {
        context.current_style = match tag_name {
            "strong" | "b" => Some(Style::Strong),
            "em" | "i" => Some(Style::Emphasis),
            "code" => Some(Style::Code),
            "del" | "s" | "strike" => Some(Style::Strikethrough),
            _ => context.current_style,
        };
        context
    }

    /// Check if an element has an epub:type attribute and return its value
    /// Only returns Some for epub:type values that should be rendered as special blocks
    fn get_epub_type_attr(
        &self,
        attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>,
    ) -> Option<String> {
        let epub_type = attrs
            .borrow()
            .iter()
            .find(|attr| attr.name.local.as_ref() == "epub:type")
            .map(|attr| attr.value.to_string())?;

        // Only process specific epub:type values that represent content blocks
        // Exclude structural/navigational types that shouldn't be rendered specially
        match epub_type.as_str() {
            // Content blocks we want to highlight
            "footnote" | "endnote" | "note" | "sidebar" | "pullquote" | "tip" | "warning"
            | "caution" | "important" | "example" | "definition" | "glossary" | "bibliography"
            | "appendix" | "preface" | "foreword" | "introduction" | "conclusion" | "epigraph"
            | "dedication" => Some(epub_type),

            // Structural types we ignore (let them be processed normally)
            "chapter" | "part" | "section" | "subsection" | "titlepage" | "toc" | "bodymatter"
            | "frontmatter" | "backmatter" | "cover" | "acknowledgments" | "copyright-page" => None,

            // For unknown types, be conservative and render them specially
            _ => Some(epub_type),
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
        format!("{}", content)
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
        markdown::{Style, TextOrInline},
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
    fn test_subscript_unicode_conversion() {
        let mut converter = HtmlToMarkdownConverter::new();

        // Simple subscript that can be converted to Unicode
        let html = r#"<p>H<sub>2</sub>O</p>"#;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let text_content: String = content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();
            assert_eq!(
                text_content, "H₂O",
                "Should convert subscript 2 to Unicode ₂"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_superscript_unicode_conversion() {
        let mut converter = HtmlToMarkdownConverter::new();

        // Simple superscript that can be converted to Unicode
        let html = r#"<p>x<sup>2</sup> + y<sup>3</sup></p>"#;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let text_content: String = content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();
            assert_eq!(
                text_content, "x² + y³",
                "Should convert superscripts to Unicode"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_subscript_latex_fallback() {
        let mut converter = HtmlToMarkdownConverter::new();

        // Complex subscript that should use LaTeX notation
        let html = r#"<p>A<sub>xyz</sub></p>"#;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let text_content: String = content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();
            assert_eq!(
                text_content, "A_{xyz}",
                "Should use LaTeX notation for complex subscript"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_superscript_latex_fallback() {
        let mut converter = HtmlToMarkdownConverter::new();

        // Complex superscript that can't be fully converted to Unicode
        let html = r#"<p>2<sup>xy</sup></p>"#;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let text_content: String = content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();
            assert_eq!(
                text_content, "2ˣʸ",
                "Should convert available characters to Unicode"
            );
        } else {
            panic!("Expected paragraph block");
        }

        // Test with characters that really can't be converted
        let html2 = r#"<p>2<sup>αβ</sup></p>"#;
        let doc2 = converter.convert(html2);

        if let Block::Paragraph { content } = &doc2.blocks[0].block {
            let text_content: String = content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();
            assert_eq!(
                text_content, "2^{αβ}",
                "Should use LaTeX notation when Unicode not available"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_subscript_in_bold() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r#"<p><strong>H<sub>2</sub>SO<sub>4</sub></strong></p>"#;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            // Check that we have bold text with subscripts
            let has_bold_with_subscripts = content.clone().into_iter().any(|item| {
                if let TextOrInline::Text(node) = item {
                    node.style == Some(Style::Strong)
                        && (node.content.contains('₂') || node.content.contains('₄'))
                } else {
                    false
                }
            });
            assert!(
                has_bold_with_subscripts,
                "Should preserve bold formatting with subscripts"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_superscript_in_italic() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r#"<p><em>E=mc<sup>2</sup></em></p>"#;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            // Check that we have italic text with superscript
            let has_italic_with_superscript = content.clone().into_iter().any(|item| {
                if let TextOrInline::Text(node) = item {
                    node.style == Some(Style::Emphasis) && node.content.contains('²')
                } else {
                    false
                }
            });
            assert!(
                has_italic_with_superscript,
                "Should preserve italic formatting with superscript"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_subscript_in_link() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r##"<p><a href="#water"><sub>2</sub></a></p>"##;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            // Debug: print what we got
            let text_items: Vec<String> = content
                .clone()
                .into_iter()
                .map(|item| match item {
                    TextOrInline::Text(node) => format!("Text: '{}'", node.content),
                    TextOrInline::Inline(Inline::Link { text, url, .. }) => {
                        let link_text: String = text
                            .into_iter()
                            .map(|t| match t {
                                TextOrInline::Text(n) => n.content,
                                _ => String::new(),
                            })
                            .collect();
                        format!("Link: url='{}', text='{}'", url, link_text)
                    }
                    _ => format!("Other inline"),
                })
                .collect();

            // Check for link with subscript in text
            let has_link_with_subscript = content.clone().into_iter().any(|item| {
                if let TextOrInline::Inline(Inline::Link { text, url, .. }) = item {
                    let link_text: String = text
                        .into_iter()
                        .map(|t| match t {
                            TextOrInline::Text(n) => n.content,
                            _ => String::new(),
                        })
                        .collect();
                    url == "#water" && link_text.contains("₂")
                } else {
                    false
                }
            });

            assert!(
                has_link_with_subscript,
                "Should handle subscript within link text. Got items: {:?}",
                text_items
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_subscript_in_table() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r#"
        <table>
            <tr>
                <td>H<sub>2</sub>O</td>
                <td>CO<sub>2</sub></td>
            </tr>
        </table>
        "#;

        let doc = converter.convert(html);

        if let Block::Table { rows, .. } = &doc.blocks[0].block {
            assert!(!rows.is_empty(), "Should have table rows");
            let first_row = &rows[0];

            // Get text from first cell
            let first_cell_text: String = first_row.cells[0]
                .content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();

            assert_eq!(
                first_cell_text.trim(),
                "H₂O",
                "Should convert subscript in table cell"
            );

            // Get text from second cell
            let second_cell_text: String = first_row.cells[1]
                .content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();

            assert_eq!(
                second_cell_text.trim(),
                "CO₂",
                "Should convert subscript in second table cell"
            );
        } else {
            panic!("Expected table block");
        }
    }

    #[test]
    fn test_mixed_subscript_superscript() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r#"<p>Formula: x<sup>2</sup> + y<sub>1</sub> = z<sup>n</sup></p>"#;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let text_content: String = content
                .clone()
                .into_iter()
                .filter_map(|item| match item {
                    TextOrInline::Text(node) => Some(node.content),
                    _ => None,
                })
                .collect();
            assert!(text_content.contains("x²"), "Should have x superscript 2");
            assert!(text_content.contains("y₁"), "Should have y subscript 1");
            assert!(text_content.contains("zⁿ"), "Should have z superscript n");
        } else {
            panic!("Expected paragraph block");
        }
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

    #[test]
    fn test_simple_table_with_header() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"
            <table>
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Age</th>
                        <th>City</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>Alice</td>
                        <td>30</td>
                        <td>New York</td>
                    </tr>
                    <tr>
                        <td>Bob</td>
                        <td>25</td>
                        <td>San Francisco</td>
                    </tr>
                    <tr>
                        <td>Charlie</td>
                        <td>35</td>
                        <td>Los Angeles</td>
                    </tr>
                </tbody>
            </table>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"
[table width="3" height="3" header="true"]
| Name    | Age | City          |
| ------- | --- | ------------- |
| Alice   | 30  | New York      |
| Bob     | 25  | San Francisco |
| Charlie | 35  | Los Angeles   |

"#;

        assert_eq!(rendered, expected.trim_start());
    }

    #[test]
    fn test_table_with_mixed_header_cells() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"
            <table>
                <tr>
                    <th>Product</th>
                    <th>Q1</th>
                    <th>Q2</th>
                    <th>Q3</th>
                    <th>Q4</th>
                </tr>
                <tr>
                    <th>Widgets</th>
                    <td>100</td>
                    <td>150</td>
                    <td>200</td>
                    <td>175</td>
                </tr>
                <tr>
                    <th>Gadgets</th>
                    <td>50</td>
                    <td>75</td>
                    <td>90</td>
                    <td>85</td>
                </tr>
                <tr>
                    <th>Total</th>
                    <td>150</td>
                    <td>225</td>
                    <td>290</td>
                    <td>260</td>
                </tr>
            </table>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"
[table width="5" height="4" header="false"]
| ------- | --- | --- | --- | --- |
| **Product** | **Q1** | **Q2** | **Q3** | **Q4** |
| **Widgets** | 100 | 150 | 200 | 175 |
| **Gadgets** | 50  | 75  | 90  | 85  |
| **Total** | 150 | 225 | 290 | 260 |

"#;

        assert_eq!(rendered, expected.trim_start());
    }

    #[test]
    fn test_table_with_formatting_in_cells() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"
            <table>
                <thead>
                    <tr>
                        <th>Feature</th>
                        <th>Description</th>
                        <th>Status</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td><strong>Performance</strong></td>
                        <td>Improved <em>response time</em> by 50%</td>
                        <td><code>completed</code></td>
                    </tr>
                    <tr>
                        <td><strong>Security</strong></td>
                        <td>Added <a href="https://example.com">two-factor auth</a></td>
                        <td><code>in-progress</code></td>
                    </tr>
                </tbody>
            </table>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"
[table width="3" height="2" header="true"]
| Feature         | Description                                  | Status        |
| --------------- | -------------------------------------------- | ------------- |
| **Performance** | Improved _response time_ by 50%              | `completed`   |
| **Security**    | Added [two-factor auth](https://example.com) | `in-progress` |

"#
        .trim_start();

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_table_without_header() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"
            <table>
                <tr>
                    <td>Row 1 Col 1</td>
                    <td>Row 1 Col 2</td>
                    <td>Row 1 Col 3</td>
                </tr>
                <tr>
                    <td>Row 2 Col 1</td>
                    <td>Row 2 Col 2</td>
                    <td>Row 2 Col 3</td>
                </tr>
            </table>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"[table width="3" height="2" header="false"]
| ----------- | ----------- | ----------- |
| Row 1 Col 1 | Row 1 Col 2 | Row 1 Col 3 |
| Row 2 Col 1 | Row 2 Col 2 | Row 2 Col 3 |

"#;

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_wide_table() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"
            <table>
                <thead>
                    <tr>
                        <th>Category</th>
                        <th>Examples of consumer use cases</th>
                        <th>Examples of enterprise use cases</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>Coding</td>
                        <td>Coding</td>
                        <td>Coding</td>
                    </tr>
                    <tr>
                        <td>Image and video production</td>
                        <td>Photo and video editing<br/>Design</td>
                        <td>Presentation<br/>Ad generation</td>
                    </tr>
                    <tr>
                        <td>Writing</td>
                        <td>Email<br/>Social media and blog posts</td>
                        <td>Copywriting, search engine optimization (SEO)<br/>Reports, memos, design docs</td>
                    </tr>
                </tbody>
            </table>
        "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        // The key test here is that the table should be parseable
        // even though it's wider than typical terminal width
        assert!(rendered.contains("[table width=\"3\" height=\"3\" header=\"true\"]"));
    }

    #[test]
    fn test_very_wide_table_with_br() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"
            <table id="ch01_table_5_1730130814941611">
              <caption><span class="label">Table 1-5. </span>Different prompts can cause models to perform very differently, as seen in Gemini’s technical report (December 2023).</caption>
              <thead>
                <tr>
                  <th> </th>
                  <th>Gemini Ultra</th>
                  <th>Gemini Pro</th>
                  <th>GPT-4</th>
                  <th>GPT-3.5</th>
                  <th>PaLM <span class="keep-together">2-L</span></th>
                  <th>Claude 2</th>
                  <th>Inflection-2</th>
                  <th>Grok 1</th>
                  <th>Llama-2</th>
                </tr>
              </thead>
              <tr>
                <td rowspan="2">MMLU performance</td>
                <td>90.04%<br/> CoT@32</td>
                <td>79.13%<br/> CoT@8</td>
                <td>87.29%<br/> CoT@32<br/> (via API)</td>
                <td>70%<br/> 5-shot</td>
                <td>78.4%<br/> 5-shot</td>
                <td>78.5%<br/> 5-shot CoT</td>
                <td>79.6%<br/> 5-shot</td>
                <td>73.0%<br/> 5-shot</td>
                <td>68.0%</td>
              </tr>
              <tr>
                <td>83.7%<br/> 5-shot</td>
                <td>71.8%<br/> 5-shot</td>
                <td>86.4%<br/> 5-shot (reported)</td>
                <td> </td>
                <td> </td>
                <td> </td>
                <td> </td>
                <td> </td>
                <td> </td>
              </tr>
            </table>
            "#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);
        // The key test here is that the table should be parseable
        // even though it's wider than typical terminal width
        let expected = r#"[table width="10" height="2" header="true"]
|                  | Gemini Ultra       | Gemini Pro        | GPT-4                             | GPT-3.5         | PaLM 2-L          | Claude 2              | Inflection-2      | Grok 1            | Llama-2 |
| ---------------- | ------------------ | ----------------- | --------------------------------- | --------------- | ----------------- | --------------------- | ----------------- | ----------------- | ------- |
| MMLU performance | 90.04%<br/> CoT@32 | 79.13%<br/> CoT@8 | 87.29%<br/> CoT@32<br/> (via API) | 70%<br/> 5-shot | 78.4%<br/> 5-shot | 78.5%<br/> 5-shot CoT | 79.6%<br/> 5-shot | 73.0%<br/> 5-shot | 68.0%   |
|                  | 83.7%<br/> 5-shot  | 71.8%<br/> 5-shot | 86.4%<br/> 5-shot (reported)      |                 |                   |                       |                   |                   |         |

            "#;
        assert_eq!(rendered.trim_end(), expected.trim_end());
    }

    //     #[test]
    //     fn test_definition_list_with_complex_content() {
    //         let mut converter = HtmlToMarkdownConverter::new();
    //         let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

    //         let html = r#"
    //             <dl>
    //               <dt>Local factual consistency</dt>
    // <dd><p><a contenteditable="false" data-primary="local factual consistency" data-type="indexterm" id="id1002"></a>The output is evaluated against a context. The output is considered factually consistent if it's supported by the given context. For example, if the model outputs "the sky is blue" and the given context says that the sky is purple, this output is considered factually inconsistent. Conversely, given this context, if the model outputs "the sky is purple", this output is factually consistent.</p></dd>
    // <dd><p>Local factual consistency is important for tasks with limited scopes such as summarization (the summary should be consistent with the original document), customer support chatbots (the chatbot's responses should be consistent with the company's policies), and business analysis (the extracted insights should be consistent with the data).</p></dd>
    //               <dt>Global factual consistency </dt>
    // <dd><p><a contenteditable="false" data-primary="global factual consistency" data-type="indexterm" id="id1003"></a>The output is evaluated against open knowledge. If the model outputs "the sky is blue" and it's a commonly accepted fact that the sky is blue, this statement is considered factually correct. Global factual consistency is important for tasks with broad scopes such as general chatbots, fact-checking, market research, etc.</p></dd>
    //             </dl>
    //         "#;

    //         let doc = converter.convert(html);
    //         let rendered = renderer.render(&doc);

    //         let expected = r#"##### Local factual consistency
    // : The output is evaluated against a context. The output is considered factually consistent if it's supported by the given context. For example, if the model outputs "the sky is blue" and the given context says that the sky is purple, this output is considered factually inconsistent. Conversely, given this context, if the model outputs "the sky is purple", this output is factually consistent.
    // : Local factual consistency is important for tasks with limited scopes such as summarization (the summary should be consistent with the original document), customer support chatbots (the chatbot's responses should be consistent with the company's policies), and business analysis (the extracted insights should be consistent with the data).

    // ##### Global factual consistency
    // : The output is evaluated against open knowledge. If the model outputs "the sky is blue" and it's a commonly accepted fact that the sky is blue, this statement is considered factually correct. Global factual consistency is important for tasks with broad scopes such as general chatbots, fact-checking, market research, etc.
    // "#;

    //         assert_eq!(rendered.trim_end(), expected.trim_end());
    //     }

    #[test]
    fn test_paragraph_with_superscript_and_external_links() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"<p>Google's largest PaLM-2 model, for example, was trained using <code>10</code><sup>22</sup> FLOPs (<a href="https://arxiv.org/abs/2204.02311">Chowdhery et al., 2022</a>). GPT-3-175B was trained using <code>3.14 × 10</code><sup>23</sup> FLOPs (<a href="https://arxiv.org/abs/2005.14165">Brown et al., 2020</a>).</p>"#;

        let doc = converter.convert(html);
        let rendered = renderer.render(&doc);

        let expected = r#"Google's largest PaLM-2 model, for example, was trained using `10`²² FLOPs ([Chowdhery et al., 2022](https://arxiv.org/abs/2204.02311)). GPT-3-175B was trained using `3.14 × 10`²³ FLOPs ([Brown et al., 2020](https://arxiv.org/abs/2005.14165)).

"#;

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_inline_anchor_extraction_with_sup_and_a_tags() {
        let mut converter = HtmlToMarkdownConverter::new();

        let html = r#"<p><a contenteditable="false" data-primary="foundation models" data-secondary="modeling" data-type="indexterm" id="ch02.html10"></a><a contenteditable="false" data-primary="modeling" data-type="indexterm" id="ch02.html11"></a>Before training a model, developers need to decide what the model should look like. What architecture should it follow? How many parameters should it have? These decisions impact not only the model's capabilities but also its usability for <span class="keep-together">downstream</span> applications.<sup><a data-type="noteref" id="id715-marker" href="ch02.html#id715">5</a></sup> For example, a 7B-parameter model will be vastly easier to deploy than a 175B-parameter model. Similarly, optimizing a transformer model for latency is very different from optimizing another architecture. Let's explore the factors behind these decisions.</p>"#;

        let doc = converter.convert(html);

        assert_eq!(
            doc.blocks.len(),
            1,
            "Should have exactly one paragraph block"
        );

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            // Convert content to vector to inspect inline anchors
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            // Count anchor markers
            let anchor_count = items
                .iter()
                .filter(|item| matches!(item, TextOrInline::Inline(Inline::Anchor { .. })))
                .count();

            assert_eq!(
                anchor_count, 3,
                "Should have exactly 3 inline anchor markers"
            );

            // Check that specific anchor IDs are present
            let anchor_ids: Vec<String> = items
                .iter()
                .filter_map(|item| {
                    if let TextOrInline::Inline(Inline::Anchor { id }) = item {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();

            assert!(
                anchor_ids.contains(&"ch02.html10".to_string()),
                "Should contain anchor 'ch02.html10'"
            );
            assert!(
                anchor_ids.contains(&"ch02.html11".to_string()),
                "Should contain anchor 'ch02.html11'"
            );
            assert!(
                anchor_ids.contains(&"id715-marker".to_string()),
                "Should contain anchor 'id715-marker'"
            );

            // Verify the anchors are in the expected order
            let mut anchor_iter = items.iter().filter_map(|item| {
                if let TextOrInline::Inline(Inline::Anchor { id }) = item {
                    Some(id.as_str())
                } else {
                    None
                }
            });

            assert_eq!(
                anchor_iter.next(),
                Some("ch02.html10"),
                "First anchor should be 'ch02.html10'"
            );
            assert_eq!(
                anchor_iter.next(),
                Some("ch02.html11"),
                "Second anchor should be 'ch02.html11'"
            );
            assert_eq!(
                anchor_iter.next(),
                Some("id715-marker"),
                "Third anchor should be 'id715-marker'"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_paragraph_with_pagebreak_class_and_anchor() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        let html = r#"<p class="pagebreak-before"><a href="https://oreil.ly/LcBfx">NewsGuard</a> <sup><a data-type="noteref" id="id699-marker" href="ch02.html#id699">2</a></sup></p>"#;

        let doc = converter.convert(html);

        assert_eq!(
            doc.blocks.len(),
            1,
            "Should have exactly one paragraph block"
        );

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            // Convert content to vector to inspect inline elements
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            // Count anchor markers (should have one from the <a> with id="id699-marker")
            let anchor_count = items
                .iter()
                .filter(|item| matches!(item, TextOrInline::Inline(Inline::Anchor { .. })))
                .count();

            assert_eq!(
                anchor_count, 1,
                "Should have exactly 1 inline anchor marker"
            );

            // Count links (should have two: external NewsGuard link and internal ch02.html#id699 link)
            let link_count = items
                .iter()
                .filter(|item| matches!(item, TextOrInline::Inline(Inline::Link { .. })))
                .count();

            assert_eq!(link_count, 2, "Should have exactly 2 links");

            // Check that the specific anchor ID is present
            let anchor_ids: Vec<String> = items
                .iter()
                .filter_map(|item| {
                    if let TextOrInline::Inline(Inline::Anchor { id }) = item {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();

            assert!(
                anchor_ids.contains(&"id699-marker".to_string()),
                "Should contain anchor 'id699-marker'"
            );

            // Verify link navigation information
            let links: Vec<&Inline> = items
                .iter()
                .filter_map(|item| {
                    if let TextOrInline::Inline(link @ Inline::Link { .. }) = item {
                        Some(link)
                    } else {
                        None
                    }
                })
                .collect();

            // Check external NewsGuard link
            let external_link = links
                .iter()
                .find(|link| {
                    if let Inline::Link { url, .. } = link {
                        url == "https://oreil.ly/LcBfx"
                    } else {
                        false
                    }
                })
                .expect("Should have external NewsGuard link");

            if let Inline::Link {
                link_type,
                target_chapter,
                target_anchor,
                ..
            } = external_link
            {
                assert_eq!(
                    *link_type,
                    Some(crate::markdown::LinkType::External),
                    "NewsGuard link should be classified as External"
                );
                assert_eq!(
                    *target_chapter, None,
                    "External link should have no target chapter"
                );
                assert_eq!(
                    *target_anchor, None,
                    "External link should have no target anchor"
                );
            }

            // Check internal chapter link with anchor
            let internal_link = links
                .iter()
                .find(|link| {
                    if let Inline::Link { url, .. } = link {
                        url == "ch02.html#id699"
                    } else {
                        false
                    }
                })
                .expect("Should have internal chapter link");

            if let Inline::Link {
                link_type,
                target_chapter,
                target_anchor,
                ..
            } = internal_link
            {
                assert_eq!(
                    *link_type,
                    Some(crate::markdown::LinkType::InternalChapter),
                    "Chapter link should be classified as InternalChapter"
                );
                assert_eq!(
                    *target_chapter,
                    Some("ch02.html".to_string()),
                    "Chapter link should have correct target chapter"
                );
                assert_eq!(
                    *target_anchor,
                    Some("id699".to_string()),
                    "Chapter link should have correct target anchor"
                );
            }
        } else {
            panic!("Expected paragraph block");
        }

        // Also test the rendered output
        let rendered = renderer.render(&doc);
        let expected = r#"[NewsGuard](https://oreil.ly/LcBfx) [²](ch02.html#id699)

"#;

        assert_eq!(rendered, expected);
    }

    #[test]
    fn test_debug_newsguard_link_issue() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = crate::parsing::markdown_renderer::MarkdownRenderer::new();

        // This is the exact HTML causing the issue from the user's log
        let html = r#"<p class="pagebreak-before">Models can also have unexpected performance challenges in non-English languages. <a contenteditable="false" data-type="indexterm" data-primary="ChatGPT" data-secondary="and languages other than English" data-secondary-sortas="languages other" id="id698"></a>For example, <a href="https://oreil.ly/LcBfx">NewsGuard</a> found that ChatGPT is more willing to produce misinformation in Chinese than in English. In April 2023, NewsGuard asked ChatGPT-3.5 to produce misinformation articles about China in English, simplified Chinese, and traditional Chinese. For English, ChatGPT declined to produce false claims for six out of seven prompts. However, it produced false claims in simplified Chinese and traditional Chinese all seven times. It's unclear what causes this difference in behavior.<sup><a data-type="noteref" id="id699-marker" href="ch02.html#id699">2</a></sup></p>"#;

        let doc = converter.convert(html);

        assert_eq!(
            doc.blocks.len(),
            1,
            "Should have exactly one paragraph block"
        );

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            // Debug: Print all items to understand the structure
            println!("All items in paragraph:");
            for (i, item) in items.iter().enumerate() {
                match item {
                    TextOrInline::Inline(Inline::Link { url, text, .. }) => {
                        let text_str = renderer.render_text(text);
                        println!("  Item {}: Link: text='{}', url='{}'", i, text_str, url);
                    }
                    TextOrInline::Inline(Inline::Anchor { id }) => {
                        println!("  Item {}: Anchor: id='{}'", i, id);
                    }
                    TextOrInline::Text(text_node) => {
                        if text_node.content.len() > 20 {
                            println!("  Item {}: Text: '{}'...", i, &text_node.content[..20]);
                        } else {
                            println!("  Item {}: Text: '{}'", i, text_node.content);
                        }
                    }
                    _ => {
                        println!("  Item {}: Other inline element", i);
                    }
                }
            }

            // Find the problematic link
            let links: Vec<&Inline> = items
                .iter()
                .filter_map(|item| {
                    if let TextOrInline::Inline(link @ Inline::Link { .. }) = item {
                        Some(link)
                    } else {
                        None
                    }
                })
                .collect();

            println!("\nFound {} links", links.len());

            // Check the link that should go to ch02.html#id699
            let problematic_link = links
                .iter()
                .find(|link| {
                    if let Inline::Link { text, .. } = link {
                        let text_str = renderer.render_text(text);
                        text_str.trim() == "²" // Superscript 2
                    } else {
                        false
                    }
                })
                .expect("Should find the footnote link with text '²' (superscript)");

            if let Inline::Link {
                url,
                target_chapter,
                target_anchor,
                ..
            } = problematic_link
            {
                println!("\nProblematic link analysis:");
                println!("  URL: '{}'", url);
                println!("  Target chapter: {:?}", target_chapter);
                println!("  Target anchor: {:?}", target_anchor);

                // This should be ch02.html#id699, NOT ch02.html#id699-marker
                assert_eq!(url, "ch02.html#id699", "URL should be ch02.html#id699");
                assert_eq!(
                    target_chapter,
                    &Some("ch02.html".to_string()),
                    "Target chapter should be ch02.html"
                );
                assert_eq!(
                    target_anchor,
                    &Some("id699".to_string()),
                    "Target anchor should be id699"
                );
            }

            // Also check that we have the anchor marker
            let anchor_count = items
                .iter()
                .filter(|item| matches!(item, TextOrInline::Inline(Inline::Anchor { .. })))
                .count();

            assert_eq!(
                anchor_count, 2,
                "Should have 2 anchor markers (id698 and id699-marker)"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_check_for_malformed_url_bug() {
        let mut converter = HtmlToMarkdownConverter::new();

        // Test to see if there's any way the URL could get malformed to ch02.html#id699-marker
        let html = r#"<p><sup><a data-type="noteref" id="id699-marker" href="ch02.html#id699">2</a></sup></p>"#;

        let doc = converter.convert(html);

        assert!(
            !doc.blocks.is_empty(),
            "Document should have at least one block"
        );

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            println!("Malformed URL test items:");
            for (i, item) in items.iter().enumerate() {
                match item {
                    TextOrInline::Inline(Inline::Link { url, .. }) => {
                        println!("  Item {}: Link URL: '{}'", i, url);
                        // This should be ch02.html#id699, NOT ch02.html#id699-marker
                        assert_eq!(
                            url, "ch02.html#id699",
                            "URL should be exactly ch02.html#id699"
                        );
                    }
                    TextOrInline::Inline(Inline::Anchor { id }) => {
                        println!("  Item {}: Anchor ID: '{}'", i, id);
                        assert_eq!(id, "id699-marker", "Anchor should be id699-marker");
                    }
                    _ => {
                        println!("  Item {}: Other", i);
                    }
                }
            }
        }
    }

    #[test]
    fn test_footnote_with_internal_link() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = MarkdownRenderer::new();

        let html = r#"<p data-type="footnote" id="id699"><sup><a href="ch02.html#id699-marker">2</a></sup> It might be because of some biases in pre-training data or alignment data. Perhaps OpenAI just didn't include as much data in the Chinese language or China-centric narratives to train their models.</p>"#;

        let doc = converter.convert(html);

        assert!(
            !doc.blocks.is_empty(),
            "Document should have at least one block"
        );

        // Verify the paragraph has the correct ID
        assert_eq!(
            doc.blocks[0].id,
            Some("id699".to_string()),
            "Paragraph should have id='id699'"
        );

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            // Find the link
            let mut found_link = false;
            for item in &items {
                if let TextOrInline::Inline(Inline::Link { text, url, .. }) = item {
                    let text_str = renderer.render_text(text);
                    println!("Found link: text='{}', url='{}'", text_str, url);

                    // Validate the link text and URL (should be superscript since it's in <sup>)
                    assert_eq!(text_str, "²", "Link text should be '²' (superscript)");
                    assert_eq!(
                        url, "ch02.html#id699-marker",
                        "Link URL should be 'ch02.html#id699-marker'"
                    );
                    found_link = true;
                    break;
                }
            }

            assert!(
                found_link,
                "Should find internal link with href='ch02.html#id699-marker'"
            );
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn test_link_with_subscript_superscript() {
        let mut converter = HtmlToMarkdownConverter::new();
        let renderer = MarkdownRenderer::new();

        let html = r##"<p><a href="#water">H<sub>2</sub>O<sup>+</sup> molecule</a></p>"##;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            if let TextOrInline::Inline(Inline::Link { text, url, .. }) = &items[0] {
                let text_str = renderer.render_text(text);
                assert_eq!(
                    text_str, "H₂O⁺ molecule",
                    "Should have both sub and superscript"
                );
                assert_eq!(url, "#water", "URL should be preserved");
            }
        }

        let html =
            r##"<p data-type="footnote" id="water"><sup><a href="#water-marker">1</a></sup></p>"##;
        let doc = converter.convert(html);

        if let Block::Paragraph { content } = &doc.blocks[0].block {
            let items: Vec<TextOrInline> = content.clone().into_iter().collect();

            // Debug output to see what we actually got
            println!("Number of items: {}", items.len());
            for (i, item) in items.iter().enumerate() {
                match item {
                    TextOrInline::Text(node) => {
                        println!("Item {}: Text('{}')", i, node.content);
                    }
                    TextOrInline::Inline(inline) => match inline {
                        Inline::Link { text, url, .. } => {
                            let text_str = renderer.render_text(text);
                            println!("Item {}: Link(text='{}', url='{}')", i, text_str, url);
                        }
                        Inline::Anchor { id } => println!("Item {}: Anchor(id='{}')", i, id),
                        _ => println!("Item {}: Other inline type", i),
                    },
                }
            }

            // First item should be the anchor from id="water"
            if let TextOrInline::Inline(Inline::Anchor { id }) = &items[0] {
                assert_eq!(id, "water", "Should have anchor from paragraph id");
            } else {
                panic!("Expected anchor at items[0] but got something else");
            }

            // Second item should be the link with superscript
            if let TextOrInline::Inline(Inline::Link { text, url, .. }) = &items[1] {
                let text_str = renderer.render_text(text);
                assert_eq!(text_str, "¹", "Should have superscript text");
                assert_eq!(url, "#water-marker", "URL should be preserved");
            } else {
                panic!("Expected link at items[1] but got something else");
            }
        } else {
            panic!("oops 2");
        }
    }
}
