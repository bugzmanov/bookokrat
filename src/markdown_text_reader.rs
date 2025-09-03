use crate::images::background_image_loader::BackgroundImageLoader;
use crate::images::book_images::BookImages;
use crate::images::image_placeholder::{ImagePlaceholder, ImagePlaceholderConfig, LoadingStatus};
use crate::main_app::VimNavMotions;
use crate::markdown::{
    Block as MarkdownBlock, Document, HeadingLevel, Inline, Node, Style, Text as MarkdownText,
    TextOrInline,
};
use crate::parsing::markdown_renderer::MarkdownRenderer;
use crate::table::{Table as CustomTable, TableConfig};
use crate::text_reader::{EmbeddedImage, EmbeddedTable, ImageLoadState, LinkInfo};
use crate::text_reader_trait::TextReaderTrait;
use crate::text_selection::TextSelection;
use crate::theme::Base16Palette;
use image::{DynamicImage, GenericImageView};
use log::{debug, warn};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style as RatatuiStyle},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use ratatui_image::{Resize, StatefulImage, picker::Picker};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Pre-processed rendering structure
struct RenderedContent {
    lines: Vec<RenderedLine>,
    total_height: usize,
    generation: u64, // For cache validation
}

#[derive(Clone)]
struct RenderedLine {
    spans: Vec<Span<'static>>,
    raw_text: String, // For text selection
    line_type: LineType,
    source_node: NodeReference, // Links back to AST
    visual_height: usize,       // 1 for text, IMAGE_HEIGHT for images, etc.
}

#[derive(Clone, Debug, PartialEq)]
enum LineType {
    Text,
    Heading {
        level: u8,
        needs_decoration: bool,
    },
    CodeBlock {
        language: Option<String>,
    },
    ListItem {
        kind: crate::markdown::ListKind,
        indent: usize,
    },
    TableRow {
        is_header: bool,
    },
    ImagePlaceholder {
        src: String,
    },
    HorizontalRule,
    Empty,
}

#[derive(Clone)]
struct NodeReference {
    node_index: usize,
    block_path: Vec<usize>, // Path through nested blocks
}

/// AST-based text reader that directly processes Markdown Document
pub struct MarkdownTextReader {
    markdown_document: Option<Document>,

    // Rendering cache - built from AST
    rendered_content: RenderedContent,

    // Scrolling state
    scroll_offset: usize,
    content_length: usize,
    last_scroll_time: Instant,
    scroll_speed: usize,

    // Visual highlighting
    highlight_visual_line: Option<usize>,
    highlight_end_time: Instant,

    // Content dimensions
    total_wrapped_lines: usize,
    visible_height: usize,

    // Caching
    cached_text_width: usize,
    cache_generation: u64,
    last_width: usize,
    last_focus_state: bool,

    // Text selection
    text_selection: TextSelection,
    raw_text_lines: Vec<String>, // Still needed for clipboard
    last_content_area: Option<Rect>,

    last_inner_text_area: Option<Rect>, // Track the actual text rendering area
    auto_scroll_active: bool,
    auto_scroll_speed: f32,

    // Image handling
    image_picker: Option<Picker>,
    embedded_images: RefCell<HashMap<String, EmbeddedImage>>,
    background_loader: BackgroundImageLoader,

    // Raw HTML mode
    show_raw_html: bool,
    raw_html_content: Option<String>,

    // Links extracted from AST
    links: Vec<LinkInfo>,

    // Tables extracted from AST
    embedded_tables: RefCell<Vec<EmbeddedTable>>,
}

impl MarkdownTextReader {
    fn render_raw_html(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        chapter_title: &Option<String>,
        current_chapter: usize,
        total_chapters: usize,
        palette: &Base16Palette,
    ) {
        let title_text = if let Some(title) = chapter_title {
            format!(
                "[{}/{}] {} [RAW HTML]",
                current_chapter, total_chapters, title
            )
        } else {
            format!("Chapter {}/{} [RAW HTML]", current_chapter, total_chapters)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .style(RatatuiStyle::default().fg(palette.base_09)); // Red border for raw mode

        let raw_content = if let Some(html) = &self.raw_html_content {
            html.clone()
        } else {
            "Raw HTML content not available".to_string()
        };

        let paragraph = Paragraph::new(raw_content)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((self.scroll_offset as u16, 0));

        frame.render_widget(paragraph, area);
    }

    fn extract_images_from_node(
        &mut self,
        node: &Node,
        book_images: &BookImages,
        images_processed: &mut usize,
    ) -> Vec<(String, u16)> {
        use MarkdownBlock::*;
        match &node.block {
            Paragraph { content } => {
                return self.extract_images_from_text(content, book_images, images_processed);
            }
            Quote {
                content: quote_content,
            } => {
                let mut vec = Vec::new();
                for inner_node in quote_content {
                    vec.append(&mut self.extract_images_from_node(
                        inner_node,
                        book_images,
                        images_processed,
                    ));
                }
                vec
            }
            List { items, .. } => {
                let mut vec = Vec::new();
                for item in items {
                    for inner_node in &item.content {
                        vec.append(&mut self.extract_images_from_node(
                            inner_node,
                            book_images,
                            images_processed,
                        ));
                    }
                }
                vec
            }
            _ => Vec::new(),
        }
    }

    fn extract_images_from_text(
        &mut self,
        text: &MarkdownText,
        book_images: &BookImages,
        images_processed: &mut usize,
    ) -> Vec<(String, u16)> {
        let mut images_to_load: Vec<(String, u16)> = Vec::new();

        for item in text.iter() {
            if let TextOrInline::Inline(Inline::Image { url, .. }) = item {
                *images_processed += 1;

                // Skip if already loaded
                if self.embedded_images.borrow().contains_key(url) {
                    continue;
                }

                // Try to get image dimensions from book images
                if let Some((img_width, img_height)) = book_images.get_image_size(url) {
                    // Skip very small images
                    if img_width < 64 || img_height < 64 {
                        warn!(
                            "Ignoring small image ({}x{}): {}",
                            img_width, img_height, url
                        );
                        continue;
                    }

                    let height_cells = EmbeddedImage::height_in_cells(img_width, img_height);

                    self.embedded_images.borrow_mut().insert(
                        url.clone(),
                        EmbeddedImage {
                            src: url.clone(),
                            lines_before_image: 0, // Will be set during rendering
                            height_cells,
                            width: img_width,
                            height: img_height,
                            state: crate::text_reader::ImageLoadState::NotLoaded,
                        },
                    );

                    images_to_load.push((url.clone(), height_cells));
                } else {
                    warn!("Could not get dimensions for: {}", url);
                    self.embedded_images.borrow_mut().insert(
                        url.clone(),
                        EmbeddedImage::failed_img(url, "Could not read image metadata"),
                    );
                }
            }
        }
        images_to_load
    }

    pub fn new() -> Self {
        // Initialize image picker exactly like in TextReader
        let image_picker = match Picker::from_query_stdio() {
            Ok(mut picker) => {
                picker.set_background_color([0, 0, 0, 0]);
                let font_size = picker.font_size();
                debug!(
                    "Successfully created image picker, detected font size: {:?}",
                    font_size
                );
                Some(picker)
            }
            Err(e) => {
                warn!(
                    "Failed to create image picker: {}. The terminal would not support image rendering!",
                    e
                );
                None
            }
        };

        Self {
            markdown_document: None,
            rendered_content: RenderedContent {
                lines: Vec::new(),
                total_height: 0,
                generation: 0,
            },
            scroll_offset: 0,
            content_length: 0,
            last_scroll_time: Instant::now(),
            scroll_speed: 1,
            highlight_visual_line: None,
            highlight_end_time: Instant::now(),
            total_wrapped_lines: 0,
            visible_height: 0,
            cached_text_width: 0,
            cache_generation: 0,
            last_width: 0,
            last_focus_state: false,
            text_selection: TextSelection::new(),
            raw_text_lines: Vec::new(),
            last_content_area: None,
            last_inner_text_area: None,
            auto_scroll_active: false,
            auto_scroll_speed: 1.0,
            image_picker,
            embedded_images: RefCell::new(HashMap::new()),
            background_loader: BackgroundImageLoader::new(),
            raw_html_content: None,
            show_raw_html: false,
            links: Vec::new(),
            embedded_tables: RefCell::new(Vec::new()),
        }
    }

    /// Main rendering entry point - replaces parse_styled_text_cached
    pub fn update_from_document(
        &mut self,
        doc: Document,
        chapter_title: Option<String>,
        palette: &Base16Palette,
        width: usize,
        is_focused: bool,
    ) {
        // Store the AST directly
        self.markdown_document = Some(doc);

        // Clear old state
        self.links.clear();
        self.embedded_tables.borrow_mut().clear();
        self.raw_text_lines.clear();

        // Build rendered content from AST
        if self.markdown_document.is_some() {
            // Clone the document to avoid borrow checker issues
            let doc = self.markdown_document.as_ref().unwrap().clone();
            self.rendered_content = self.render_document_to_lines(&doc, width, palette, is_focused);
        }

        // Update line counts
        self.total_wrapped_lines = self.rendered_content.total_height;

        // Mark cache as fresh
        self.cache_generation += 1;
        self.last_width = width;
        self.last_focus_state = is_focused;
    }

    fn render_document_to_lines(
        &mut self,
        doc: &Document,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) -> RenderedContent {
        let mut lines = Vec::new();
        let mut total_height = 0;

        // Iterate through all blocks in the document
        for (node_idx, node) in doc.blocks.iter().enumerate() {
            let node_ref = NodeReference {
                node_index: node_idx,
                block_path: vec![],
            };

            self.render_node(
                node,
                node_ref,
                &mut lines,
                &mut total_height,
                width,
                palette,
                is_focused,
                0, // indent level
            );
        }

        // Fix link coordinates after all text wrapping is complete
        self.fix_link_coordinates(&lines);

        RenderedContent {
            lines,
            total_height,
            generation: self.cache_generation,
        }
    }

    fn render_node(
        &mut self,
        node: &Node,
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
    ) {
        use MarkdownBlock::*;

        match &node.block {
            Heading { level, content } => {
                self.render_heading(
                    *level,
                    content,
                    node_ref,
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
                    node_ref,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    indent,
                );
            }

            CodeBlock { language, content } => {
                self.render_code_block(
                    language.as_deref(),
                    content,
                    node_ref,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                );
            }

            List { kind, items } => {
                self.render_list(
                    kind,
                    items,
                    node_ref,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    indent,
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
                    node_ref,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                );
            }

            Quote {
                content: quote_content,
            } => {
                self.render_quote(
                    quote_content,
                    node_ref,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                    indent,
                );
            }

            ThematicBreak => {
                self.render_thematic_break(
                    node_ref,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                );
            }

            DefinitionList { items: def_items } => {
                self.render_definition_list(
                    def_items,
                    node_ref,
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
                    node_ref,
                    lines,
                    total_height,
                    width,
                    palette,
                    is_focused,
                );
            }
        }
    }

    // Helper method to convert Text AST to plain string
    fn text_to_string(&self, text: &MarkdownText) -> String {
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
                        result.push_str(&self.text_to_string(link_text));
                    }
                    Inline::Image { alt_text, .. } => {
                        result.push_str(alt_text);
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

    fn render_heading(
        &mut self,
        level: HeadingLevel,
        content: &MarkdownText,
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Convert heading content to string for wrapping
        let heading_text = self.text_to_string(content);

        // Apply H1 uppercase transformation if needed
        let display_text = if level == HeadingLevel::H1 {
            heading_text.to_uppercase()
        } else {
            heading_text.clone()
        };

        // Wrap the heading text
        let wrapped = textwrap::wrap(&display_text, width);

        // Style for headings
        let heading_color = if is_focused {
            palette.base_0a // Yellow
        } else {
            palette.base_03 // Dimmed
        };

        let modifiers = match level {
            HeadingLevel::H4 => Modifier::BOLD | Modifier::UNDERLINED,
            _ => Modifier::BOLD,
        };

        // Add wrapped heading lines
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
                source_node: node_ref.clone(),
                visual_height: 1,
            });

            self.raw_text_lines.push(wrapped_line.to_string());
            *total_height += 1;
        }

        // Add decoration line for H1-H3
        if matches!(
            level,
            HeadingLevel::H1 | HeadingLevel::H2 | HeadingLevel::H3
        ) {
            let decoration = match level {
                HeadingLevel::H1 => "▀".repeat(width), // Thick line
                HeadingLevel::H2 => "═".repeat(width), // Double line
                HeadingLevel::H3 => "─".repeat(width), // Thin line
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
                source_node: node_ref.clone(),
                visual_height: 1,
            });

            self.raw_text_lines.push(decoration);
            *total_height += 1;
        }

        // Add empty line after heading
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_paragraph(
        &mut self,
        content: &MarkdownText,
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
    ) {
        // First, check if this paragraph contains any images and separate them
        let mut current_spans = Vec::new();
        let mut has_content = false;

        for item in content.iter() {
            match item {
                TextOrInline::Inline(Inline::Image { url, alt_text, .. }) => {
                    // If we have accumulated text before the image, render it first
                    if !current_spans.is_empty() {
                        self.render_text_spans(
                            &current_spans,
                            None, // no prefix
                            node_ref.clone(),
                            lines,
                            total_height,
                            width,
                            indent,
                            false, // don't add empty line after
                        );
                        current_spans.clear();
                        has_content = true;
                    }

                    // Render the image as a separate block
                    self.render_image_placeholder(
                        url,
                        alt_text,
                        node_ref.clone(),
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                    );
                    has_content = true;
                }
                _ => {
                    // Accumulate non-image content
                    let styled_spans =
                        self.render_text_or_inline(item, palette, is_focused, *total_height);
                    current_spans.extend(styled_spans);
                }
            }
        }

        // Render any remaining text spans
        if !current_spans.is_empty() {
            self.render_text_spans(
                &current_spans,
                None, // no prefix
                node_ref.clone(),
                lines,
                total_height,
                width,
                indent,
                true, // add empty line after
            );
        } else if !has_content {
            // Empty paragraph - just add an empty line
            lines.push(RenderedLine {
                spans: vec![Span::raw("")],
                raw_text: String::new(),
                line_type: LineType::Empty,
                source_node: node_ref,
                visual_height: 1,
            });
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }
    }

    fn render_text_or_inline(
        &mut self,
        item: &TextOrInline,
        palette: &Base16Palette,
        is_focused: bool,
        _line_num: usize, // Not used anymore - we'll fix links after wrapping
    ) -> Vec<Span<'static>> {
        let mut spans = Vec::new();

        match item {
            TextOrInline::Text(text_node) => {
                let styled_span = self.style_text_node(text_node, palette, is_focused);
                spans.push(styled_span);
            }

            TextOrInline::Inline(inline) => {
                match inline {
                    Inline::Link {
                        text: link_text,
                        url,
                        ..
                    } => {
                        // Store link info with temporary coordinates - we'll fix them after wrapping
                        let start_col = self.calculate_column_from_spans(&spans);
                        let link_text_str = self.text_to_string(link_text);

                        self.links.push(LinkInfo {
                            text: link_text_str.clone(),
                            url: url.clone(),
                            line: 0, // Temporary - will be updated after wrapping
                            start_col,
                            end_col: start_col + link_text_str.len(),
                        });

                        // Create underlined span
                        let link_color = if is_focused {
                            palette.base_0c
                        } else {
                            palette.base_03
                        };

                        spans.push(Span::styled(
                            link_text_str,
                            RatatuiStyle::default()
                                .fg(link_color)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                    }

                    Inline::Image { url, alt_text, .. } => {
                        // Images in inline context just show placeholder text
                        // They should be rendered as blocks in render_paragraph
                        spans.push(Span::raw(format!("[image: {}]", alt_text)));
                    }

                    Inline::LineBreak => {
                        // Force a line break
                        spans.push(Span::raw("\n"));
                    }

                    Inline::SoftBreak => {
                        // Space for soft break
                        spans.push(Span::raw(" "));
                    }
                }
            }
        }

        spans
    }

    fn style_text_node(
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

    fn calculate_column_from_spans(&self, spans: &[Span]) -> usize {
        spans.iter().map(|s| s.content.len()).sum()
    }

    fn render_code_block(
        &mut self,
        language: Option<&str>,
        content: &str,
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // TODO: Implement syntax highlighting if language is provided
        let code_lines: Vec<&str> = content.lines().collect();

        for code_line in code_lines {
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

            lines.push(RenderedLine {
                spans: vec![styled_span],
                raw_text: code_line.to_string(),
                line_type: LineType::CodeBlock {
                    language: language.map(String::from),
                },
                source_node: node_ref.clone(),
                visual_height: 1,
            });

            self.raw_text_lines.push(code_line.to_string());
            *total_height += 1;
        }

        // Add empty line after code block
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_list(
        &mut self,
        kind: &crate::markdown::ListKind,
        items: &[crate::markdown::ListItem],
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
    ) {
        use crate::markdown::ListKind;

        for (idx, item) in items.iter().enumerate() {
            // Determine bullet/number for this item
            let prefix = match kind {
                ListKind::Unordered => "• ".to_string(),
                ListKind::Ordered { start } => {
                    let num = start + idx as u32;
                    format!("{}. ", num)
                }
            };

            // Render the list item content
            // List items can contain multiple blocks (paragraphs, nested lists, etc.)
            for (block_idx, block_node) in item.content.iter().enumerate() {
                if block_idx == 0 {
                    // First block gets the bullet/number prefix
                    match &block_node.block {
                        MarkdownBlock::Paragraph { content } => {
                            // Render the rich text content with prefix and indentation
                            let mut content_spans = Vec::new();
                            for item in content.iter() {
                                content_spans.extend(self.render_text_or_inline(
                                    item,
                                    palette,
                                    is_focused,
                                    *total_height,
                                ));
                            }

                            // Store original line count to update line types after
                            let lines_before = lines.len();

                            self.render_text_spans(
                                &content_spans,
                                Some(&prefix), // add bullet/number prefix
                                node_ref.clone(),
                                lines,
                                total_height,
                                width,
                                indent, // proper indentation
                                false,  // don't add empty line after
                            );

                            // Update line types for all newly added lines
                            for line in &mut lines[lines_before..] {
                                line.line_type = LineType::ListItem {
                                    kind: kind.clone(),
                                    indent,
                                };
                            }
                        }
                        _ => {
                            // For other block types in list items, render them with increased indent
                            self.render_node(
                                block_node,
                                node_ref.clone(),
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                indent + 1,
                            );
                        }
                    }
                } else {
                    // Subsequent blocks are rendered with increased indent
                    self.render_node(
                        block_node,
                        node_ref.clone(),
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        indent + 1,
                    );
                }
            }
        }

        // Add empty line after list
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_table(
        &mut self,
        header: &Option<crate::markdown::TableRow>,
        rows: &[crate::markdown::TableRow],
        _alignment: &[crate::markdown::TableAlignment],
        node_ref: NodeReference,
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
                .map(|cell| self.text_to_string(&cell.content))
                .collect();
        }

        // Process data rows
        for row in rows {
            let row_data: Vec<String> = row
                .cells
                .iter()
                .map(|cell| self.text_to_string(&cell.content))
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
        let table_config = TableConfig {
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
        let mut custom_table = CustomTable::new(table_rows.clone())
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
                source_node: node_ref.clone(),
                visual_height: 1,
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
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    /// Calculate balanced column constraints for table rendering
    fn calculate_balanced_column_widths(
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
        let spacing_width = if num_cols > 1 { num_cols - 1 } else { 0 };
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
    fn calculate_display_width(&self, text: &str) -> usize {
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
    fn line_to_plain_text(&self, line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn render_quote(
        &mut self,
        content: &[Node],
        node_ref: NodeReference,
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
                    let mut content_spans = Vec::new();
                    for item in para_content.iter() {
                        content_spans.extend(self.render_text_or_inline(
                            item,
                            palette,
                            is_focused,
                            *total_height,
                        ));
                    }

                    // Apply quote styling to all spans
                    let quote_color = if is_focused {
                        palette.base_03
                    } else {
                        palette.base_02
                    };

                    let styled_spans: Vec<Span<'static>> = content_spans
                        .into_iter()
                        .map(|span| {
                            Span::styled(
                                span.content.clone(),
                                span.style.fg(quote_color).add_modifier(Modifier::ITALIC),
                            )
                        })
                        .collect();

                    self.render_text_spans(
                        &styled_spans,
                        Some("> "), // quote prefix
                        node_ref.clone(),
                        lines,
                        total_height,
                        width,
                        indent,
                        false, // don't add empty line after
                    );
                }
                _ => {
                    // Render other block types within quotes
                    self.render_node(
                        node,
                        node_ref.clone(),
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        indent + 1,
                    );
                }
            }
        }

        // Add empty line after quote
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_thematic_break(
        &mut self,
        node_ref: NodeReference,
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
            source_node: node_ref.clone(),
            visual_height: 1,
        });

        self.raw_text_lines.push(hr_line);
        *total_height += 1;

        // Add empty line after horizontal rule
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_definition_list(
        &mut self,
        items: &[crate::markdown::DefinitionListItem],
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Render each term-definition pair
        for item in items {
            // Render the term (dt) - bold and possibly colored
            let term_color = if is_focused {
                palette.base_0c // Yellow for focused
            } else {
                palette.base_03 // Dimmed when not focused
            };

            let mut term_spans = Vec::new();
            for term_item in item.term.iter() {
                term_spans.extend(self.render_text_or_inline(
                    term_item,
                    palette,
                    is_focused,
                    *total_height,
                ));
            }

            // Apply bold styling to all term spans
            let styled_term_spans: Vec<Span<'static>> = term_spans
                .into_iter()
                .map(|span| {
                    Span::styled(
                        span.content.clone(),
                        span.style.fg(term_color).add_modifier(Modifier::BOLD),
                    )
                })
                .collect();

            self.render_text_spans(
                &styled_term_spans,
                None, // no prefix for terms
                node_ref.clone(),
                lines,
                total_height,
                width,
                0,     // no indentation for terms
                false, // don't add empty line after
            );

            // Render each definition (dd) - indented
            for definition in &item.definitions {
                let mut def_spans = Vec::new();
                for def_item in definition.iter() {
                    def_spans.extend(self.render_text_or_inline(
                        def_item,
                        palette,
                        is_focused,
                        *total_height,
                    ));
                }

                self.render_text_spans(
                    &def_spans,
                    None, // no prefix for definitions
                    node_ref.clone(),
                    lines,
                    total_height,
                    width,
                    2,     // 2 levels of indentation (4 spaces)
                    false, // don't add empty line after
                );
            }

            // Add a small spacing between definition items (but not after the last one)
            if item != items.last().unwrap() {
                lines.push(RenderedLine {
                    spans: vec![Span::raw("")],
                    raw_text: String::new(),
                    line_type: LineType::Empty,
                    source_node: node_ref.clone(),
                    visual_height: 1,
                });
                self.raw_text_lines.push(String::new());
                *total_height += 1;
            }
        }

        // Add empty line after the entire definition list
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_epub_block(
        &mut self,
        _epub_type: &str,
        _element_name: &str,
        content: &[crate::markdown::Node],
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Add line separator before the block
        let separator_line = "─".repeat(width);
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
            source_node: node_ref.clone(),
            visual_height: 1,
        });
        self.raw_text_lines.push(separator_line);
        *total_height += 1;
        //
        // Add empty line before the block
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref.clone(),
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;

        debug!("EpubBlock: {:?}", content);
        // Render the content blocks with controlled spacing
        for (i, content_node) in content.iter().enumerate() {
            // Render the content node normally
            match &content_node.block {
                MarkdownBlock::Heading { level, content } => {
                    self.render_heading(
                        HeadingLevel::H5, // remap to always same time of heading to avoid visual hierarchy issues
                        content,
                        node_ref.clone(),
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
                        node_ref.clone(),
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        0, // no indentation
                    );
                }
            }
        }

        // Add line separator after the block
        let separator_line = "─".repeat(width);
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
            source_node: node_ref.clone(),
            visual_height: 1,
        });
        self.raw_text_lines.push(separator_line);
        *total_height += 1;

        // Add empty line after the block
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_text_spans(
        &mut self,
        spans: &[Span<'static>],
        prefix: Option<&str>,
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        indent: usize,
        add_empty_line_after: bool,
    ) {
        // Build the complete spans with prefix
        let mut complete_spans = Vec::new();
        if let Some(prefix_str) = prefix {
            complete_spans.push(Span::raw(prefix_str.to_string()));
        }
        complete_spans.extend_from_slice(spans);

        // Convert complete spans to plain text for wrapping
        let plain_text = complete_spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();

        // Calculate available width after accounting for indentation
        let indent_str = "  ".repeat(indent);
        let available_width = width.saturating_sub(indent_str.len());

        // Wrap the text
        let wrapped = textwrap::wrap(&plain_text, available_width);

        // Create lines from wrapped text
        for (line_idx, wrapped_line) in wrapped.iter().enumerate() {
            // For the first line, use the styled spans if possible
            // For subsequent lines, use plain text
            let mut line_spans = if line_idx == 0 && wrapped.len() == 1 {
                // Single line - use the styled spans
                complete_spans.clone()
            } else {
                // Multi-line content: map each wrapped line back to styled spans
                self.map_wrapped_line_to_styled_spans(wrapped_line, &complete_spans)
            };

            // Apply indentation by prepending indent span
            if indent > 0 {
                line_spans.insert(0, Span::raw(indent_str.clone()));
            }

            // Build the final raw text with indentation
            let final_raw_text = if indent > 0 {
                format!("{}{}", indent_str, wrapped_line)
            } else {
                wrapped_line.to_string()
            };

            lines.push(RenderedLine {
                spans: line_spans,
                raw_text: final_raw_text.clone(),
                line_type: LineType::Text,
                source_node: node_ref.clone(),
                visual_height: 1,
            });

            self.raw_text_lines.push(final_raw_text);
            *total_height += 1;
        }

        // Add empty line after if requested
        if add_empty_line_after {
            lines.push(RenderedLine {
                spans: vec![Span::raw("")],
                raw_text: String::new(),
                line_type: LineType::Empty,
                source_node: node_ref,
                visual_height: 1,
            });
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }
    }

    /// Fix link coordinates to match the final rendered line positions after text wrapping
    fn fix_link_coordinates(&mut self, rendered_lines: &[RenderedLine]) {
        // Build a map of link text to URLs for efficient lookup
        let mut link_map = std::collections::HashMap::new();
        for link in &self.links {
            link_map.insert(link.text.clone(), link.url.clone());
        }

        // Clear existing links and rebuild them with correct coordinates
        self.links.clear();

        // Scan through all rendered lines to find links
        for (line_idx, rendered_line) in rendered_lines.iter().enumerate() {
            let line_text = &rendered_line.raw_text;

            // Look for link text in each line
            for (link_text, url) in &link_map {
                if let Some(start_pos) = line_text.find(link_text) {
                    let end_pos = start_pos + link_text.len();

                    // Create new link info with correct coordinates
                    self.links.push(LinkInfo {
                        text: link_text.clone(),
                        url: url.clone(),
                        line: line_idx,
                        start_col: start_pos,
                        end_col: end_pos,
                    });
                }
            }
        }

        debug!("Fixed {} link coordinates", self.links.len());
    }

    /// Convert screen coordinates to logical text coordinates (like TextReader does)
    fn screen_to_text_coords(
        &self,
        screen_x: u16,
        screen_y: u16,
        content_area: Rect,
    ) -> Option<(usize, usize)> {
        self.text_selection.screen_to_text_coords(
            screen_x,
            screen_y,
            self.scroll_offset,
            content_area.x,
            content_area.y,
        )
    }

    /// Map a wrapped line back to its styled spans, preserving formatting like links
    fn map_wrapped_line_to_styled_spans(
        &self,
        wrapped_line: &str,
        original_spans: &[Span<'static>],
    ) -> Vec<Span<'static>> {
        // Build a flattened representation of the original content with style info
        struct CharWithStyle {
            ch: char,
            style: RatatuiStyle,
        }

        let mut chars_with_style = Vec::new();
        for span in original_spans {
            for ch in span.content.chars() {
                chars_with_style.push(CharWithStyle {
                    ch,
                    style: span.style,
                });
            }
        }

        // Find where this wrapped line starts in the original content
        let wrapped_chars: Vec<char> = wrapped_line.chars().collect();
        if wrapped_chars.is_empty() {
            return vec![Span::raw("")];
        }

        // Find the starting position of wrapped_line in the original content
        let mut start_pos = None;
        for i in 0..=chars_with_style.len().saturating_sub(wrapped_chars.len()) {
            let mut matches = true;
            for (j, &wrapped_ch) in wrapped_chars.iter().enumerate() {
                if i + j >= chars_with_style.len() || chars_with_style[i + j].ch != wrapped_ch {
                    matches = false;
                    break;
                }
            }
            if matches {
                start_pos = Some(i);
                break;
            }
        }

        // If we found the position, reconstruct the spans with proper styling
        if let Some(pos) = start_pos {
            let mut result_spans = Vec::new();
            let mut current_style = None;
            let mut current_text = String::new();

            for i in pos..pos + wrapped_chars.len() {
                if i >= chars_with_style.len() {
                    break;
                }

                let char_style = &chars_with_style[i];

                if current_style.as_ref() != Some(&char_style.style) {
                    // Style changed, push the accumulated span
                    if !current_text.is_empty() {
                        if let Some(style) = current_style {
                            result_spans.push(Span::styled(current_text.clone(), style));
                        } else {
                            result_spans.push(Span::raw(current_text.clone()));
                        }
                        current_text.clear();
                    }
                    current_style = Some(char_style.style);
                }

                current_text.push(char_style.ch);
            }

            // Push the final accumulated span
            if !current_text.is_empty() {
                if let Some(style) = current_style {
                    result_spans.push(Span::styled(current_text, style));
                } else {
                    result_spans.push(Span::raw(current_text));
                }
            }

            if result_spans.is_empty() {
                vec![Span::raw(wrapped_line.to_string())]
            } else {
                result_spans
            }
        } else {
            // Fallback: return as unstyled if we can't find the position
            vec![Span::raw(wrapped_line.to_string())]
        }
    }

    fn render_image_placeholder(
        &mut self,
        url: &str,
        alt_text: &str,
        node_ref: NodeReference,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Constants for image display
        const IMAGE_HEIGHT_WIDE: u16 = 20;
        const IMAGE_HEIGHT_TALL: u16 = 30;
        const TALL_ASPECT_RATIO: f32 = 1.5;

        // Add empty line before image
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref.clone(),
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;

        // Store the position where the image will be rendered
        let lines_before_image = *total_height;

        // Check if we have image dimensions already loaded
        let (image_height, loading_status) =
            if let Some(embedded_image) = self.embedded_images.borrow().get(url) {
                let height = embedded_image.height_cells;
                let status = match &embedded_image.state {
                    crate::text_reader::ImageLoadState::Loaded { .. } => LoadingStatus::Loaded,
                    crate::text_reader::ImageLoadState::Failed { .. } => LoadingStatus::Failed,
                    crate::text_reader::ImageLoadState::NotLoaded => LoadingStatus::Loading,
                };
                (height, status)
            } else {
                // Image not preloaded yet - use default height
                (IMAGE_HEIGHT_WIDE, LoadingStatus::Loading)
            };

        // Update or insert the embedded image info
        self.embedded_images
            .borrow_mut()
            .entry(url.to_string())
            .or_insert_with(|| {
                EmbeddedImage {
                    src: url.to_string(),
                    lines_before_image,
                    height_cells: image_height,
                    width: 200,  // Default width, will be updated when loaded
                    height: 200, // Default height, will be updated when loaded
                    state: crate::text_reader::ImageLoadState::NotLoaded,
                }
            })
            .lines_before_image = lines_before_image;

        // Create image placeholder configuration
        let config = ImagePlaceholderConfig {
            internal_padding: 4,
            total_height: image_height as usize,
            border_color: palette.base_03,
        };

        // Create the placeholder
        let placeholder = ImagePlaceholder::new(
            url,
            width,
            &config,
            loading_status != LoadingStatus::Loaded,
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
                source_node: node_ref.clone(),
                visual_height: 1,
            });

            self.raw_text_lines.push(String::new()); // Keep raw_text_lines in sync
            *total_height += 1;
        }

        // Add empty line after image
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Empty,
            source_node: node_ref,
            visual_height: 1,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    /// Perform immediate auto-scroll when dragging starts outside content area
    pub fn perform_auto_scroll(&mut self) {
        if self.auto_scroll_active {
            let scroll_amount = self.auto_scroll_speed.abs() as usize;

            if self.auto_scroll_speed < 0.0 && self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
            } else if self.auto_scroll_speed > 0.0 {
                let max_offset = self.get_max_scroll_offset();
                if self.scroll_offset < max_offset {
                    self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
                }
            }
        }
    }
}

impl VimNavMotions for MarkdownTextReader {
    fn handle_h(&mut self) {
        // Left movement - handled at App level
    }

    fn handle_j(&mut self) {
        self.scroll_down();
    }

    fn handle_k(&mut self) {
        self.scroll_up();
    }

    fn handle_l(&mut self) {
        // Right movement - handled at App level
    }

    fn handle_ctrl_d(&mut self) {
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_half_screen_down("", screen_height);
        }
    }

    fn handle_ctrl_u(&mut self) {
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_half_screen_up("", screen_height);
        }
    }

    fn handle_gg(&mut self) {
        self.scroll_offset = 0;
        debug!("Scrolled to top of document");
    }

    fn handle_G(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = max_offset;
        debug!("Scrolled to bottom of document: offset {}", max_offset);
    }
}

impl TextReaderTrait for MarkdownTextReader {
    fn set_content_from_string(&mut self, content: &str, _chapter_title: Option<String>) {
        // Parse HTML string to Markdown AST
        use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;

        debug!(
            "set_content_from_string called with {} bytes of content",
            content.len()
        );
        debug!(
            "First 500 chars of content: {}",
            &content.chars().take(500).collect::<String>()
        );

        let mut converter = HtmlToMarkdownConverter::new();
        let doc = converter.convert(content);

        debug!("Converted to AST with {} blocks", doc.blocks.len());
        for (i, block) in doc.blocks.iter().take(3).enumerate() {
            debug!("Block {}: {:?}", i, block);
        }

        // Store the document - rendering will happen in the render() method
        self.markdown_document = Some(doc);

        // Clear old state
        self.links.clear();
        self.embedded_tables.borrow_mut().clear();
        self.raw_text_lines.clear();
        self.scroll_offset = 0;
        self.text_selection.clear_selection();

        // Mark cache as invalid to force re-rendering
        self.cache_generation += 1;

        debug!("Parsed HTML to Markdown AST in set_content_from_string");
    }

    fn set_content_from_ast(&mut self, doc: Document, _chapter_title: Option<String>) {
        // This is the main entry point for the AST-based reader
        // Store the document - rendering will happen in the render() method
        // when we have access to palette and width
        self.markdown_document = Some(doc);

        // Clear old state
        self.links.clear();
        self.embedded_tables.borrow_mut().clear();
        self.raw_text_lines.clear();
        self.scroll_offset = 0;
        self.text_selection.clear_selection();

        // Mark cache as invalid to force re-rendering
        self.cache_generation += 1;
    }

    fn content_updated(&mut self, content_length: usize) {
        self.content_length = content_length;
        self.scroll_offset = 0;
        self.text_selection.clear_selection();

        // IMPORTANT: Clear the markdown document so new content can be parsed
        self.markdown_document = None;

        // Clear caches
        self.cache_generation += 1;

        // Clear other state
        self.links.clear();
        self.embedded_tables.borrow_mut().clear();
        self.raw_text_lines.clear();
        self.rendered_content = RenderedContent {
            lines: Vec::new(),
            total_height: 0,
            generation: 0,
        };
        self.embedded_images.borrow_mut().clear();

        debug!("content_updated: cleared markdown_document and all state");
    }

    fn update_wrapped_lines_if_needed(&mut self, content: &str, area: Rect) {
        let width = area.width as usize;
        let height = area.height as usize;

        if self.cached_text_width != width || self.visible_height != height {
            self.cached_text_width = width;
            self.visible_height = height;

            // Parse HTML content to Markdown AST if we don't have it yet
            if self.markdown_document.is_none() && !content.is_empty() {
                use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;

                debug!(
                    "update_wrapped_lines_if_needed: parsing {} bytes of content",
                    content.len()
                );
                debug!(
                    "First 500 chars: {}",
                    &content.chars().take(500).collect::<String>()
                );

                let mut converter = HtmlToMarkdownConverter::new();
                let doc = converter.convert(content);

                debug!("Converted to {} blocks", doc.blocks.len());
                for (i, block) in doc.blocks.iter().take(3).enumerate() {
                    debug!("Block {}: {:?}", i, block);
                }

                self.markdown_document = Some(doc);

                // Mark cache as invalid to force re-rendering
                self.cache_generation += 1;

                debug!("Parsed HTML to Markdown AST in update_wrapped_lines_if_needed");
            }
        }
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);
            self.last_scroll_time = Instant::now();
        }
    }

    fn scroll_down(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        if self.scroll_offset < max_offset {
            self.scroll_offset = (self.scroll_offset + self.scroll_speed).min(max_offset);
            self.last_scroll_time = Instant::now();
        }
    }

    fn scroll_half_screen_up(&mut self, _content: &str, screen_height: usize) {
        let scroll_amount = screen_height / 2;
        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
        self.highlight_visual_line = Some(0);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
    }

    fn scroll_half_screen_down(&mut self, _content: &str, screen_height: usize) {
        let scroll_amount = screen_height / 2;
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
        self.highlight_visual_line = Some(screen_height - 1);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
    }

    fn get_scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    fn restore_scroll_position(&mut self, offset: usize) {
        self.scroll_offset = offset.min(self.get_max_scroll_offset());
    }

    fn get_max_scroll_offset(&self) -> usize {
        self.total_wrapped_lines.saturating_sub(self.visible_height)
    }

    fn handle_mouse_down(&mut self, x: u16, y: u16, area: Rect) {
        // Use the inner text area if available, otherwise fall back to the provided area
        let text_area = self.last_inner_text_area.unwrap_or(area);

        // Use proper coordinate conversion like TextReader does
        if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
            // Check if click is on a link first
            if self.get_link_at_position(line, column).is_some() {
                // Don't start text selection if clicking on a link
                debug!("Mouse down on link, skipping text selection");
                return;
            }

            self.text_selection.start_selection(line, column);
            debug!("Started text selection at line {}, column {}", line, column);
        }
    }

    fn handle_mouse_drag(&mut self, x: u16, y: u16, area: Rect) {
        if self.text_selection.is_selecting {
            // Use the inner text area if available, otherwise fall back to the provided area
            let text_area = self.last_inner_text_area.unwrap_or(area);

            // Always try to update text selection first, regardless of auto-scroll
            if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
                self.text_selection.update_selection(line, column);
            }

            // Check if we need to auto-scroll due to dragging outside the visible area
            const SCROLL_MARGIN: u16 = 3;
            let needs_scroll_up = y <= text_area.y + SCROLL_MARGIN && self.scroll_offset > 0;
            let needs_scroll_down = y >= text_area.y + text_area.height - SCROLL_MARGIN;

            if needs_scroll_up {
                self.auto_scroll_active = true;
                self.auto_scroll_speed = -1.0;
                // Perform immediate scroll like text_reader.rs does
                self.perform_auto_scroll();
            } else if needs_scroll_down {
                self.auto_scroll_active = true;
                self.auto_scroll_speed = 1.0;
                // Perform immediate scroll like text_reader.rs does
                self.perform_auto_scroll();
            } else {
                self.auto_scroll_active = false;
            }
        }
    }

    fn handle_mouse_up(&mut self, x: u16, y: u16, area: Rect) -> Option<String> {
        self.auto_scroll_active = false;

        // Use the inner text area if available, otherwise fall back to the provided area
        let text_area = self.last_inner_text_area.unwrap_or(area);

        // Always check for link clicks first, regardless of selection state
        if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
            debug!("Mouse up at: {}x{} : {:?}", line, column, self.links);

            // Check if click is on a link
            if let Some(link) = self.get_link_at_position(line, column) {
                let url = link.url.clone();
                debug!("Link clicked: {}", url);
                // Clear any selection and return the link
                self.text_selection.clear_selection();
                return Some(url);
            }
        }

        // Handle text selection if we were selecting
        if self.text_selection.is_selecting {
            self.text_selection.end_selection();
            if self.text_selection.has_selection() {
                debug!("Text selection completed");
            }
        }

        // Check for image click (pass the original area as it uses inner text area internally)
        self.check_image_click(x, y, area)
    }

    fn handle_double_click(&mut self, x: u16, y: u16, area: Rect) {
        // Use the inner text area if available, otherwise fall back to the provided area
        let text_area = self.last_inner_text_area.unwrap_or(area);

        // Use proper coordinate conversion like TextReader does
        if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
            // Select word at position
            if line < self.raw_text_lines.len() {
                self.text_selection
                    .select_word_at(line, column, &self.raw_text_lines);
                debug!("Selected word at line {}, column {}", line, column);
            }
        }
    }

    fn handle_triple_click(&mut self, x: u16, y: u16, area: Rect) {
        // Use the inner text area if available, otherwise fall back to the provided area
        let text_area = self.last_inner_text_area.unwrap_or(area);

        // Use proper coordinate conversion like TextReader does
        if let Some((line, column)) = self.screen_to_text_coords(x, y, text_area) {
            // Select entire paragraph/line
            if line < self.raw_text_lines.len() {
                // For paragraph selection, we use the calculated column
                self.text_selection
                    .select_paragraph_at(line, column, &self.raw_text_lines);
                debug!("Selected paragraph at line {}", line);
            }
        }
    }

    fn clear_selection(&mut self) {
        self.text_selection.clear_selection();
    }

    fn copy_selection_to_clipboard(&self) -> Result<(), String> {
        if let Some(selected_text) = self
            .text_selection
            .extract_selected_text(&self.raw_text_lines)
        {
            use arboard::Clipboard;
            let mut clipboard =
                Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
            clipboard
                .set_text(selected_text)
                .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
            debug!("Copied selection to clipboard");
            Ok(())
        } else {
            Err("No text selected".to_string())
        }
    }

    fn copy_chapter_to_clipboard(&self) -> Result<(), String> {
        if self.show_raw_html {
            use arboard::Clipboard;
            let mut clipboard =
                Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
            clipboard
                .set_text(
                    self.raw_html_content
                        .as_ref()
                        .unwrap_or(&"<failed to get raw html>".to_string()),
                )
                .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
            debug!("Copied entire chapter to clipboard");
            Ok(())
        } else if !self.raw_text_lines.is_empty() {
            let chapter_text = self.raw_text_lines.join("\n");
            use arboard::Clipboard;
            let mut clipboard =
                Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
            clipboard
                .set_text(chapter_text)
                .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
            debug!("Copied entire chapter to clipboard");
            Ok(())
        } else {
            Err("No content to copy".to_string())
        }
    }

    fn has_text_selection(&self) -> bool {
        self.text_selection.has_selection()
    }

    fn preload_image_dimensions(&mut self, _content: &str, book_images: &BookImages) {
        // Extract images from the AST and preload their dimensions
        if let Some(doc) = self.markdown_document.clone() {
            self.background_loader.cancel_loading();
            let mut images_processed = 0;

            let mut images_to_load = vec![];

            for node in &doc.blocks {
                images_to_load.append(&mut self.extract_images_from_node(
                    node,
                    book_images,
                    &mut images_processed,
                ));
            }
            //
            // Start background loading if we have images and a picker
            if !images_to_load.is_empty() {
                if let Some(ref picker) = self.image_picker {
                    let font_size = picker.font_size();
                    let (cell_width, cell_height) = (font_size.0, font_size.1);
                    self.background_loader.start_loading(
                        images_to_load,
                        book_images,
                        cell_width,
                        cell_height,
                    );
                } else {
                    for (img, _) in images_to_load.iter() {
                        if let Some(img_state) = self.embedded_images.borrow_mut().get_mut(img) {
                            img_state.state = crate::text_reader::ImageLoadState::Failed {
                                reason: "terminal doesn't support images".to_string(),
                            };
                        }
                    }
                }
                debug!("Preloaded dimensions for {} images", images_processed);
            }
        }
    }

    fn check_for_loaded_images(&mut self) -> bool {
        let mut any_loaded = false;

        // Check for background-loaded images
        if let Some(loaded_images) = self.background_loader.check_for_loaded_images() {
            for (img_src, image) in loaded_images {
                let mut embedded_images = self.embedded_images.borrow_mut();
                if let Some(embedded_image) = embedded_images.get_mut(&img_src) {
                    // Store the loaded image with protocol
                    embedded_image.state = if let Some(ref picker) = self.image_picker {
                        crate::text_reader::ImageLoadState::Loaded {
                            image: Arc::new(image.clone()),
                            protocol: picker.new_resize_protocol(image),
                        }
                    } else {
                        crate::text_reader::ImageLoadState::Failed {
                            reason: "Image picker not initialized".to_string(),
                        }
                    };
                    any_loaded = true;
                    debug!("Image '{}' loaded successfully", img_src);
                } else {
                    warn!(
                        "Received loaded image '{}' that is no longer in embedded_images (likely due to chapter switch)",
                        img_src
                    );
                }
            }
        }

        any_loaded
    }

    fn check_image_click(&self, x: u16, y: u16, _area: Rect) -> Option<String> {
        // Use the inner text area if available
        let text_area = self.last_inner_text_area?;

        // Check if click is within the text area
        if x < text_area.x
            || x >= text_area.x + text_area.width
            || y < text_area.y
            || y >= text_area.y + text_area.height
        {
            return None;
        }

        // Calculate the line number that was clicked within the text area
        let clicked_line = self.scroll_offset + (y - text_area.y) as usize;

        // Check each embedded image to see if the click is within its bounds
        for (src, embedded_image) in self.embedded_images.borrow().iter() {
            let image_start = embedded_image.lines_before_image;
            let image_end = image_start + embedded_image.height_cells as usize;

            if clicked_line >= image_start && clicked_line < image_end {
                debug!("Clicked on image: {}", src);
                return Some(src.clone());
            }
        }

        None
    }

    fn get_image_picker(&self) -> Option<&Picker> {
        self.image_picker.as_ref()
    }

    fn get_loaded_image(&self, image_src: &str) -> Option<Arc<DynamicImage>> {
        self.embedded_images
            .borrow()
            .get(image_src)
            .and_then(|img| match &img.state {
                crate::text_reader::ImageLoadState::Loaded { image, .. } => Some(image.clone()),
                _ => None,
            })
    }

    fn get_link_at_position(&self, line: usize, column: usize) -> Option<&LinkInfo> {
        // Find link at the given position
        for link in &self.links {
            if link.line == line && column >= link.start_col && column <= link.end_col {
                return Some(link);
            }
        }
        None
    }

    fn update_highlight(&mut self) -> bool {
        if self.highlight_visual_line.is_some() && Instant::now() > self.highlight_end_time {
            self.highlight_visual_line = None;
            return true;
        }
        false
    }

    fn update_auto_scroll(&mut self) -> bool {
        if self.auto_scroll_active {
            let scroll_amount = self.auto_scroll_speed.abs() as usize;

            if self.auto_scroll_speed < 0.0 && self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
                return true;
            } else if self.auto_scroll_speed > 0.0 {
                let max_offset = self.get_max_scroll_offset();
                if self.scroll_offset < max_offset {
                    self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
                    return true;
                }
            }
        }
        false
    }

    fn toggle_raw_html(&mut self) {
        self.show_raw_html = !self.show_raw_html;
        debug!("Toggled raw HTML mode: {}", self.show_raw_html);
    }

    fn set_raw_html(&mut self, html: String) {
        self.raw_html_content = Some(html);
    }

    fn calculate_progress(&self, _content: &str, _width: usize, _height: usize) -> u32 {
        if self.total_wrapped_lines == 0 {
            return 0;
        }

        let visible_end = (self.scroll_offset + self.visible_height).min(self.total_wrapped_lines);
        ((visible_end as f32 / self.total_wrapped_lines as f32) * 100.0) as u32
    }

    fn handle_terminal_resize(&mut self) {
        // Clear caches on resize
        self.cache_generation += 1;
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _content: &str,
        chapter_title: &Option<String>,
        current_chapter: usize,
        total_chapters: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Store the content area for mouse events
        self.last_content_area = Some(area);
        self.visible_height = area.height.saturating_sub(3) as usize; // Account for borders and margin

        if self.show_raw_html {
            self.render_raw_html(
                frame,
                area,
                chapter_title,
                current_chapter,
                total_chapters,
                palette,
            );
            return;
        }

        // Re-render if width changed, focus changed, or content changed (cache invalidated)
        let width = area.width.saturating_sub(4) as usize; // Account for borders and margins

        if self.last_width != width
            || self.last_focus_state != is_focused
            || self.rendered_content.generation != self.cache_generation
        {
            if self.markdown_document.is_some() {
                // Clone the document to avoid borrow checker issues
                let doc = self.markdown_document.as_ref().unwrap().clone();
                self.rendered_content =
                    self.render_document_to_lines(&doc, width, palette, is_focused);
                self.total_wrapped_lines = self.rendered_content.total_height;
                self.last_width = width;
                self.last_focus_state = is_focused;
            } else {
                debug!("No markdown_document to render!");
            }
        }
        // Build the title
        let title_text = if let Some(title) = chapter_title {
            format!("[{}/{}] {}", current_chapter, total_chapters, title)
        } else {
            format!("Chapter {}/{}", current_chapter, total_chapters)
        };

        // Calculate progress percentage
        let progress = self.calculate_progress("", width, self.visible_height);

        // Create the border block
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .title_bottom(Line::from(format!(" {}% ", progress)).right_aligned());

        // Calculate the inner area after borders
        let mut inner_area = block.inner(area);
        inner_area.y = inner_area.y.saturating_add(1);
        inner_area.height = inner_area.height.saturating_sub(1);
        inner_area.x = inner_area.x.saturating_add(1);

        // Store the inner text area for mouse event handling
        self.last_inner_text_area = Some(inner_area);
        //
        // First pass: Collect non-image lines and render text
        let mut visible_lines = Vec::new();
        let end_offset =
            (self.scroll_offset + self.visible_height).min(self.rendered_content.lines.len());

        // Determine selection colors
        let selection_bg = if is_focused {
            palette.base_02
        } else {
            palette.base_01
        };

        for line_idx in self.scroll_offset..end_offset {
            if let Some(rendered_line) = self.rendered_content.lines.get(line_idx) {
                let visual_line_idx = line_idx - self.scroll_offset;

                // Check if this is an image placeholder with a loaded image
                let skip_placeholder =
                    if let LineType::ImagePlaceholder { src } = &rendered_line.line_type {
                        // Check if image is loaded
                        if let Some(embedded_image) = self.embedded_images.borrow().get(src) {
                            matches!(
                                embedded_image.state,
                                crate::text_reader::ImageLoadState::Loaded { .. }
                            )
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                // Skip rendering placeholder lines if the actual image is loaded
                if skip_placeholder {
                    // Add empty line instead of placeholder
                    visible_lines.push(Line::from(""));
                    continue;
                }

                // Apply highlight if needed
                let mut line_spans = if self.highlight_visual_line == Some(visual_line_idx) {
                    // Apply highlight
                    rendered_line
                        .spans
                        .iter()
                        .map(|span| {
                            Span::styled(span.content.clone(), span.style.bg(palette.base_02))
                        })
                        .collect()
                } else {
                    rendered_line.spans.clone()
                };

                // Apply text selection highlighting if needed
                if self.text_selection.has_selection() {
                    let line_with_selection = self.text_selection.apply_selection_highlighting(
                        line_idx,
                        line_spans,
                        selection_bg,
                    );
                    line_spans = line_with_selection.spans;
                }

                visible_lines.push(Line::from(line_spans));
            }
        }

        // Create the paragraph widget
        let paragraph = Paragraph::new(vec![])
            .block(block.clone())
            .wrap(ratatui::widgets::Wrap { trim: false });

        frame.render_widget(paragraph, area);

        let inner_text_paragraph = Paragraph::new(visible_lines)
            .block(Block::default().borders(Borders::NONE))
            .wrap(ratatui::widgets::Wrap { trim: false });

        frame.render_widget(inner_text_paragraph, inner_area);

        // Second pass: Render images on top
        // Create vertical margins

        let scroll_offset = self.scroll_offset;
        // Display all embedded images from the chapter (only if not showing raw HTML)
        if !self.show_raw_html {
            self.check_for_loaded_images();
            if !self.embedded_images.borrow().is_empty() && self.image_picker.is_some() {
                let area_height = inner_area.height as usize;

                // Iterate through all embedded images
                for (_, embedded_image) in self.embedded_images.borrow_mut().iter_mut() {
                    let image_height_cells = embedded_image.height_cells as usize;
                    let image_start_line = embedded_image.lines_before_image;
                    let image_end_line = image_start_line + image_height_cells;

                    // Check if image is in viewport
                    if scroll_offset < image_end_line
                        && scroll_offset + area_height > image_start_line
                    {
                        // Check if image is loaded
                        if let ImageLoadState::Loaded {
                            ref image,
                            ref mut protocol,
                        } = embedded_image.state
                        {
                            let scaled_image = image;

                            if let Some(ref picker) = self.image_picker {
                                let image_screen_start = if scroll_offset > image_start_line {
                                    0
                                } else {
                                    image_start_line - scroll_offset
                                };

                                // Calculate visible portion
                                let image_top_clipped = if scroll_offset > image_start_line {
                                    scroll_offset - image_start_line
                                } else {
                                    0
                                };

                                let visible_image_height = (image_height_cells - image_top_clipped)
                                    .min(area_height - image_screen_start);

                                if visible_image_height > 0 {
                                    debug!(
                                        "here3 - visible_image_height: {}, image_screen_start: {}, image_top_clipped: {}",
                                        visible_image_height, image_screen_start, image_top_clipped
                                    );
                                    // Don't center the image - use full width like the placeholder

                                    // Get the actual image height for this specific image
                                    let image_height_cells =
                                        calculate_image_height_in_cells(scaled_image);

                                    let (render_y, render_height) = if image_top_clipped > 0 {
                                        (
                                            inner_area.y,
                                            ((image_height_cells as usize)
                                                .saturating_sub(image_top_clipped))
                                            .min(area_height)
                                                as u16,
                                        )
                                    } else {
                                        (
                                            inner_area.y + image_screen_start as u16,
                                            (image_height_cells as usize)
                                                .min(area_height.saturating_sub(image_screen_start))
                                                as u16,
                                        )
                                    };

                                    // Calculate actual image width in terminal cells based on aspect ratio
                                    let (image_width_pixels, image_height_pixels) =
                                        scaled_image.dimensions();
                                    let font_size = picker.font_size();
                                    let image_width_cells =
                                        (image_width_pixels as f32 / font_size.0 as f32).ceil()
                                            as u16;

                                    // Center the image horizontally within the text area
                                    let text_area_width = inner_area.width;
                                    let image_display_width =
                                        image_width_cells.min(text_area_width);
                                    let x_offset =
                                        (text_area_width.saturating_sub(image_display_width)) / 2;

                                    let image_area = Rect {
                                        x: inner_area.x + x_offset,
                                        y: render_y,
                                        width: image_display_width,
                                        height: render_height,
                                    };

                                    // Render using the stateful widget
                                    // Use Viewport mode for efficient scrolling
                                    let current_font_size = picker.font_size();
                                    let y_offset_pixels = (image_top_clipped as f32
                                        * current_font_size.1 as f32)
                                        as u32;

                                    let viewport_options = ratatui_image::ViewportOptions {
                                        y_offset: y_offset_pixels,
                                        x_offset: 0, // No horizontal scrolling for now
                                    };

                                    // Use protocol directly for rendering
                                    let image_widget = StatefulImage::new()
                                        .resize(Resize::Viewport(viewport_options));
                                    debug!(
                                        "Rendering image at area: {:?}, scroll_offset: {}, image_start_line: {}",
                                        image_area, scroll_offset, image_start_line
                                    );
                                    frame.render_stateful_widget(
                                        image_widget,
                                        image_area,
                                        protocol,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Text selection is already handled via apply_selection_highlighting in the visible_lines loop
    }

    fn get_total_wrapped_lines(&self) -> usize {
        self.total_wrapped_lines
    }

    fn get_visible_height(&self) -> usize {
        self.visible_height
    }

    fn get_last_content_area(&self) -> Option<Rect> {
        self.last_content_area
    }
}

fn calculate_image_height_in_cells(image: &DynamicImage) -> u16 {
    let (width, height) = image.dimensions();
    EmbeddedImage::height_in_cells(width, height)
}
