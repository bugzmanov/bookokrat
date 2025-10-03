use crate::comments::{BookComments, Comment};
use crate::images::background_image_loader::BackgroundImageLoader;
use crate::images::book_images::BookImages;
use crate::images::image_placeholder::{ImagePlaceholder, ImagePlaceholderConfig, LoadingStatus};
use crate::main_app::VimNavMotions;
use crate::markdown::{
    Block as MarkdownBlock, Document, HeadingLevel, Inline, Node, Style, Text as MarkdownText,
    TextOrInline,
};
use crate::search::{SearchMode, SearchState, SearchablePanel, find_matches_in_text};
use crate::table::{Table as CustomTable, TableConfig};
use crate::text_reader_trait::{LinkInfo, TextReaderTrait};
use crate::text_selection::TextSelection;
use crate::theme::Base16Palette;
use image::{DynamicImage, GenericImageView};
use log::{debug, info, warn};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style as RatatuiStyle},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage, picker::Picker};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tui_textarea::{Input, Key, TextArea};

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
    link_nodes: Vec<LinkInfo>,   // Links that are visible on this line
    node_anchor: Option<String>, // Anchor/id from the Node if present
    node_index: Option<usize>,   // Index of the node in the document this line belongs to
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
    Comment {
        chapter_href: String,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    },
}

/// Span that may contain link information
#[derive(Clone)]
enum RichSpan {
    Text(Span<'static>),
    Link { span: Span<'static>, info: LinkInfo },
}

impl RichSpan {
    /// Extract the underlying ratatui Span
    fn into_span(self) -> Span<'static> {
        match self {
            RichSpan::Text(span) => span,
            RichSpan::Link { span, .. } => span,
        }
    }

    /// Get link info if this is a link
    fn link_info(&self) -> Option<&LinkInfo> {
        match self {
            RichSpan::Text(_) => None,
            RichSpan::Link { info, .. } => Some(info),
        }
    }
}

pub enum ImageLoadState {
    NotLoaded,
    Loading,
    Loaded {
        image: Arc<DynamicImage>,
        protocol: StatefulProtocol,
    },
    Failed {
        reason: String,
    },
}

pub struct EmbeddedImage {
    pub src: String,
    pub lines_before_image: usize,
    pub height_cells: u16,
    pub width: u32,
    pub height: u32,
    pub state: ImageLoadState,
}

/// Height for regular images in terminal cells
const IMAGE_HEIGHT_REGULAR: u16 = 15;
/// Height for wide images (aspect ratio > 3:1) in terminal cells
const IMAGE_HEIGHT_WIDE: u16 = 7;
/// Aspect ratio threshold for wide images
const WIDE_IMAGE_ASPECT_RATIO: f32 = 3.0;

impl EmbeddedImage {
    pub fn height_in_cells(width: u32, height: u32) -> u16 {
        let aspect_ratio = width as f32 / height as f32;

        let height_cells = if aspect_ratio > WIDE_IMAGE_ASPECT_RATIO {
            IMAGE_HEIGHT_WIDE
        } else if height < 150 {
            IMAGE_HEIGHT_WIDE
        } else {
            IMAGE_HEIGHT_REGULAR
        };
        height_cells
    }

    pub fn failed_img(img_src: &str, error_msg: &str) -> EmbeddedImage {
        let height_cells = EmbeddedImage::height_in_cells(200, 200);
        EmbeddedImage {
            src: img_src.into(),
            lines_before_image: 0, // Will be set properly in parse_styled_text_internal_with_raw
            height_cells,
            width: 200,
            height: 200,
            state: ImageLoadState::Failed {
                reason: error_msg.into(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddedTable {
    pub lines_before_table: usize, // Line position where table starts
    pub num_rows: usize,
    pub num_cols: usize,
    pub has_header: bool,
    pub header_row: Option<Vec<String>>, // Header cells if present
    pub data_rows: Vec<Vec<String>>,     // Data cells
    pub height_cells: usize,             // Total height in terminal cells
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

    // Deferred node index to restore after rendering
    pending_node_restore: Option<usize>,

    // Raw HTML mode
    show_raw_html: bool,
    raw_html_content: Option<String>,

    // Links extracted from AST
    links: Vec<LinkInfo>,

    // Tables extracted from AST
    embedded_tables: RefCell<Vec<EmbeddedTable>>,

    /// Map of anchor IDs to their line positions in rendered content
    anchor_positions: HashMap<String, usize>,

    /// Current chapter filename (for resolving relative links)
    current_chapter_file: Option<String>,

    /// Search state for vim-like search
    search_state: SearchState,

    /// Pending anchor scroll after chapter navigation
    pending_anchor_scroll: Option<String>,

    /// Last active anchor for maintaining continuous highlighting
    last_active_anchor: Option<String>,

    /// Book comments to display alongside paragraphs
    book_comments: Option<Arc<Mutex<BookComments>>>,

    /// Pre-built comment lookup for current chapter (paragraph_index -> Vec<Comment>)
    /// Built once when chapter is loaded to avoid repeated lookups during rendering
    current_chapter_comments: HashMap<usize, Vec<Comment>>,

    /// Comment input state
    comment_input_active: bool,
    comment_textarea: Option<TextArea<'static>>,
    comment_target_node_index: Option<usize>, // The node index where comment will be attached
    comment_target_line: Option<usize>,       // The visual line where textarea should appear
}

/// Represents the active section being read
#[derive(Clone, Debug)]
pub enum ActiveSection {
    Anchor(String), // Specific section via anchor
    Chapter(usize), // Fallback to chapter index
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
            EpubBlock { content, .. } => {
                let mut vec = Vec::new();
                for inner_node in content {
                    vec.append(&mut self.extract_images_from_node(
                        inner_node,
                        book_images,
                        images_processed,
                    ));
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
                info!("Processing image #{}: {}", images_processed, url);

                // Skip if already loaded or currently loading
                if let Some(embedded_img) = self.embedded_images.borrow().get(url) {
                    match embedded_img.state {
                        ImageLoadState::Loaded { .. } | ImageLoadState::Loading => {
                            debug!("Skipping image {} - already loaded or loading", url);
                            continue;
                        }
                        ImageLoadState::NotLoaded | ImageLoadState::Failed { .. } => {
                            // Need to (re)load this image
                        }
                    }
                }

                // Try to get image dimensions from book images with chapter context
                let chapter_path = self.current_chapter_file.as_deref();
                if let Some((img_width, img_height)) =
                    book_images.get_image_size_with_context(url, chapter_path)
                {
                    info!(
                        "Got image dimensions for {}: {}x{}",
                        url, img_width, img_height
                    );
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
                            state: ImageLoadState::NotLoaded,
                        },
                    );

                    images_to_load.push((url.clone(), height_cells));
                    info!("Added image to load queue: {}", url);
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

    pub fn set_book_comments(&mut self, comments: Arc<Mutex<BookComments>>) {
        self.book_comments = Some(comments);
        self.rebuild_chapter_comments();
    }

    /// Rebuild the comment lookup for the current chapter
    /// Called when chapter changes or comments are updated
    fn rebuild_chapter_comments(&mut self) {
        self.current_chapter_comments.clear();

        if let Some(chapter_file) = &self.current_chapter_file {
            if let Some(comments_arc) = &self.book_comments {
                if let Ok(comments) = comments_arc.lock() {
                    // Get all comments for this chapter and group by paragraph index
                    for comment in comments.get_chapter_comments(chapter_file) {
                        self.current_chapter_comments
                            .entry(comment.paragraph_index)
                            .or_insert_with(Vec::new)
                            .push(comment.clone());
                    }
                }
            }
        }
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
            pending_node_restore: None,
            raw_html_content: None,
            show_raw_html: false,
            links: Vec::new(),
            embedded_tables: RefCell::new(Vec::new()),
            anchor_positions: HashMap::new(),
            current_chapter_file: None,
            search_state: SearchState::new(),
            pending_anchor_scroll: None,
            last_active_anchor: None,
            book_comments: None,
            current_chapter_comments: HashMap::new(),
            comment_input_active: false,
            comment_textarea: None,
            comment_target_node_index: None,
            comment_target_line: None,
        }
    }

    pub fn start_comment_input(&mut self) -> bool {
        if !self.has_text_selection() {
            return false;
        }

        if let Some((start, _end)) = self.text_selection.get_selection_range() {
            let visual_line = start.line;

            let mut node_index = None;
            for (idx, line) in self.rendered_content.lines.iter().enumerate() {
                if idx == visual_line {
                    node_index = line.node_index;
                    break;
                }
            }

            if let Some(node_idx) = node_index {
                let mut last_line_of_node = visual_line;
                for (idx, line) in self
                    .rendered_content
                    .lines
                    .iter()
                    .enumerate()
                    .skip(visual_line)
                {
                    if let Some(line_node_idx) = line.node_index {
                        if line_node_idx != node_idx {
                            break;
                        }
                    }
                    last_line_of_node = idx;
                }

                let mut textarea = TextArea::default();
                textarea.set_placeholder_text("Type your comment here...");

                self.comment_input_active = true;
                self.comment_textarea = Some(textarea);
                self.comment_target_node_index = Some(node_idx);
                self.comment_target_line = Some(last_line_of_node + 1);

                self.text_selection.clear_selection();

                return true;
            }
        }

        false
    }

    /// Handle input events when in comment mode
    pub fn handle_comment_input(&mut self, input: Input) -> bool {
        if !self.comment_input_active {
            return false;
        }

        if let Some(textarea) = &mut self.comment_textarea {
            match input {
                Input { key: Key::Esc, .. } => {
                    // Save the comment and exit comment mode
                    self.save_comment();
                    return true;
                }
                _ => {
                    // Pass the input to the textarea
                    textarea.input(input);
                    return true;
                }
            }
        }
        false
    }

    fn save_comment(&mut self) {
        if let Some(textarea) = &self.comment_textarea {
            let comment_text = textarea.lines().join("\n");

            if !comment_text.trim().is_empty() {
                if let Some(node_idx) = self.comment_target_node_index {
                    if let Some(chapter_file) = &self.current_chapter_file {
                        if let Some(comments_arc) = &self.book_comments {
                            if let Ok(mut comments) = comments_arc.lock() {
                                use chrono::Utc;

                                let comment = Comment {
                                    chapter_href: chapter_file.clone(),
                                    paragraph_index: node_idx,
                                    word_range: None, // For now, comment on whole paragraph
                                    content: comment_text.clone(),
                                    updated_at: Utc::now(),
                                };

                                if let Err(e) = comments.add_comment(comment) {
                                    warn!("Failed to add comment: {}", e);
                                } else {
                                    debug!("Saved comment for node {}: {}", node_idx, comment_text);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.rebuild_chapter_comments();

        // Clear comment input state AFTER rebuilding so the re-render doesn't try to show textarea
        self.comment_input_active = false;
        self.comment_textarea = None;
        self.comment_target_node_index = None;
        self.comment_target_line = None;

        self.cache_generation += 1;
        debug!("Invalidated render cache");
    }

    /// Check if we're currently in comment input mode
    pub fn is_comment_input_active(&self) -> bool {
        self.comment_input_active
    }

    /// Get comment ID from current text selection
    /// Returns the comment ID if any line in the selection is a comment line
    pub fn get_comment_at_cursor(&self) -> Option<(String, usize, Option<(usize, usize)>)> {
        if let Some((start, end)) = self.text_selection.get_selection_range() {
            // Check all lines in the selection range
            for line_idx in start.line..=end.line {
                if let Some(line) = self.rendered_content.lines.get(line_idx) {
                    if let LineType::Comment {
                        chapter_href,
                        paragraph_index,
                        word_range,
                    } = &line.line_type
                    {
                        return Some((chapter_href.clone(), *paragraph_index, *word_range));
                    }
                }
            }
        }

        None
    }

    /// Delete comment at current selection
    /// Returns true if a comment was deleted
    pub fn delete_comment_at_cursor(&mut self) -> anyhow::Result<bool> {
        if let Some((chapter_href, paragraph_index, word_range)) = self.get_comment_at_cursor() {
            if let Some(comments_arc) = &self.book_comments {
                let mut comments = comments_arc.lock().unwrap();
                comments.delete_comment(&chapter_href, paragraph_index, word_range)?;

                drop(comments);
                self.rebuild_chapter_comments();

                self.cache_generation += 1;

                self.text_selection.clear_selection();

                return Ok(true);
            }
        }

        Ok(false)
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

        // Debug assertion: raw_text_lines should be empty or we're accumulating garbage
        #[cfg(debug_assertions)]
        {
            let old_count = self.raw_text_lines.len();
            if old_count > 0 {
                // This would have caused the bug - accumulating lines on re-render
                debug_assert!(
                    false,
                    "BUG: raw_text_lines not cleared before render! Had {} old lines",
                    old_count
                );
            }
        }

        // Clear previous state before re-rendering
        self.raw_text_lines.clear();
        self.anchor_positions.clear();

        // Iterate through all blocks in the document
        for (node_idx, node) in doc.blocks.iter().enumerate() {
            // Track anchors before rendering each block
            self.extract_and_track_anchors_from_node(node, total_height);

            self.render_node(
                node,
                &mut lines,
                &mut total_height,
                width,
                palette,
                is_focused,
                0,              // indent level
                Some(node_idx), // Pass the node index
            );
        }

        // Collect all links from rendered lines
        self.links.clear();
        for rendered_line in &lines {
            self.links.extend(rendered_line.link_nodes.clone());
        }

        // Debug assertions to verify state consistency
        #[cfg(debug_assertions)]
        {
            // Assert that raw_text_lines count matches non-empty lines in rendered content
            let non_empty_lines = lines
                .iter()
                .filter(|l| !matches!(l.line_type, LineType::Empty))
                .count();

            // raw_text_lines should have roughly the same count as non-empty rendered lines
            // (some types like HorizontalRule might not add to raw_text_lines)
            let diff = (self.raw_text_lines.len() as i32 - non_empty_lines as i32).abs();
            debug_assert!(
                diff < 100, // Allow some difference for special line types
                "raw_text_lines has {} lines but rendered content has {} non-empty lines - mismatch!",
                self.raw_text_lines.len(),
                non_empty_lines
            );

            // Assert lines and raw_text_lines are not empty if document has content
            if !doc.blocks.is_empty() {
                debug_assert!(
                    !lines.is_empty(),
                    "Rendered lines is empty despite document having {} blocks",
                    doc.blocks.len()
                );
            }
        }

        RenderedContent {
            lines,
            total_height,
            generation: self.cache_generation,
        }
    }

    fn extract_and_track_anchors_from_node(&mut self, node: &Node, current_line: usize) {
        if let Some(html_id) = &node.id {
            self.anchor_positions.insert(html_id.clone(), current_line);
        }

        match &node.block {
            MarkdownBlock::Heading { content, .. } => {
                if node.id.is_none() {
                    let heading_text = self.text_to_string(content);
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
    fn generate_heading_anchor(&self, heading_text: &str) -> String {
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
    fn extract_inline_anchors_from_text(&mut self, text: &MarkdownText, current_line: usize) {
        for item in text.iter() {
            match item {
                TextOrInline::Inline(Inline::Anchor { id }) => {
                    self.anchor_positions.insert(id.clone(), current_line);
                }
                _ => {}
            }
        }
    }

    fn render_node(
        &mut self,
        node: &Node,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        node_index: Option<usize>,
    ) {
        use MarkdownBlock::*;

        // Store the current node's anchor to add to the first line rendered for this node
        let current_node_anchor = node.id.clone();
        let initial_line_count = lines.len();

        // Remember the starting line count to assign node_index to first line only
        let start_lines_count = lines.len();

        match &node.block {
            Heading { level, content } => {
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

            Quote {
                content: quote_content,
            } => {
                self.render_quote(
                    quote_content,
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

        // If this node has an anchor ID and we created new lines, attach it to the first line
        if let Some(anchor) = current_node_anchor {
            if initial_line_count < lines.len() {
                if let Some(line) = lines.get_mut(initial_line_count) {
                    line.node_anchor = Some(anchor);
                }
            }
        }

        // Set the node_index on the first line created for this node (if any)
        if let Some(idx) = node_index {
            if start_lines_count < lines.len() {
                if let Some(line) = lines.get_mut(start_lines_count) {
                    line.node_index = Some(idx);
                    debug!(
                        "Assigned node_index {} to line {} (line type: {:?})",
                        idx, start_lines_count, line.line_type
                    );
                }
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

    fn render_heading(
        &mut self,
        level: HeadingLevel,
        content: &MarkdownText,
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
            HeadingLevel::H3 => Modifier::BOLD | Modifier::UNDERLINED,
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
                HeadingLevel::H1 => "═".repeat(width), // Thick line
                HeadingLevel::H2 => "─".repeat(width), // Double line
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

    fn render_comment_as_quote(
        &mut self,
        comment: &Comment,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        _is_focused: bool,
        indent: usize,
    ) {
        debug!("rendering comments!");
        let comment_header = format!("Note // {}", comment.updated_at.format("%m-%d-%y %H:%M"));

        lines.push(RenderedLine {
            spans: vec![Span::styled(
                comment_header.clone(),
                RatatuiStyle::default().fg(palette.base_0e), // Purple text color
            )],
            raw_text: String::new(),
            line_type: LineType::Comment {
                chapter_href: comment.chapter_href.clone(),
                paragraph_index: comment.paragraph_index,
                word_range: comment.word_range,
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
        });
        self.raw_text_lines.push(comment_header);
        *total_height += 1;

        let quote_prefix = "> ";
        let effective_width = width.saturating_sub(indent + quote_prefix.len());

        let wrapped_lines = textwrap::wrap(&comment.content, effective_width);

        for line in wrapped_lines {
            let quoted_line = format!("{}{}{}", " ".repeat(indent), quote_prefix, line);
            lines.push(RenderedLine {
                spans: vec![Span::styled(
                    quoted_line.clone(),
                    RatatuiStyle::default().fg(palette.base_0e), // Purple text color
                )],
                raw_text: line.to_string(),
                line_type: LineType::Comment {
                    chapter_href: comment.chapter_href.clone(),
                    paragraph_index: comment.paragraph_index,
                    word_range: comment.word_range,
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
            });
            self.raw_text_lines.push(quoted_line);
            *total_height += 1;
        }

        // Add empty line after comment
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Comment {
                chapter_href: comment.chapter_href.clone(),
                paragraph_index: comment.paragraph_index,
                word_range: comment.word_range,
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn render_paragraph(
        &mut self,
        content: &MarkdownText,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
        node_index: Option<usize>,
    ) {
        // First, check if this paragraph contains any images and separate them
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
            self.render_text_spans(
                &current_rich_spans,
                None, // no prefix
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

    fn render_text_or_inline(
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
                        let link_text_str = self.text_to_string(link_text);

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
                                Some(crate::markdown::LinkType::External) => {
                                    (palette.base_0c, Modifier::UNDERLINED) // Cyan + underlined
                                }
                                Some(crate::markdown::LinkType::InternalChapter) => {
                                    (palette.base_0b, Modifier::UNDERLINED | Modifier::BOLD) // Green + bold underlined
                                }
                                Some(crate::markdown::LinkType::InternalAnchor) => {
                                    (palette.base_0a, Modifier::UNDERLINED | Modifier::ITALIC) // Yellow + italic underlined
                                }
                                None => {
                                    (palette.base_0c, Modifier::UNDERLINED) // Default to external style
                                }
                            }
                        } else {
                            // Unfocused state - use muted colors but maintain differentiation
                            match link_type {
                                Some(crate::markdown::LinkType::External) => {
                                    (palette.base_03, Modifier::UNDERLINED)
                                }
                                Some(crate::markdown::LinkType::InternalChapter) => {
                                    (palette.base_03, Modifier::UNDERLINED | Modifier::BOLD)
                                }
                                Some(crate::markdown::LinkType::InternalAnchor) => {
                                    (palette.base_03, Modifier::UNDERLINED | Modifier::ITALIC)
                                }
                                None => (palette.base_03, Modifier::UNDERLINED),
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
                        rich_spans
                            .push(RichSpan::Text(Span::raw(format!("[image: {}]", alt_text))));
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

    fn render_code_block(
        &mut self,
        language: Option<&str>,
        content: &str,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        _width: usize, //todo: not supported yet
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
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
            });

            self.raw_text_lines.push(code_line.to_string());
            *total_height += 1;
        }

        // Add empty line after code block
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

    fn render_list(
        &mut self,
        kind: &crate::markdown::ListKind,
        items: &[crate::markdown::ListItem],
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
                            let mut content_rich_spans = Vec::new();
                            for item in content.iter() {
                                content_rich_spans
                                    .extend(self.render_text_or_inline(item, palette, is_focused));
                            }

                            // Store original line count to update line types after
                            let lines_before = lines.len();

                            self.render_text_spans(
                                &content_rich_spans,
                                Some(&prefix), // add bullet/number prefix
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
                                lines,
                                total_height,
                                width,
                                palette,
                                is_focused,
                                indent + 1,
                                None, // No separate node index for nested blocks
                            );
                        }
                    }
                } else {
                    // Subsequent blocks are rendered with increased indent
                    self.render_node(
                        block_node,
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        indent + 1,
                        None, // No separate node index for nested blocks
                    );
                }
            }
        }

        // Add empty line after list
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

    fn render_table(
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
                    // Render other block types within quotes
                    self.render_node(
                        node,
                        lines,
                        total_height,
                        width,
                        palette,
                        is_focused,
                        indent + 1,
                        None, // No separate node index for nested blocks
                    );
                }
            }
        }

        // Add empty line after quote
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

    fn render_thematic_break(
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

    fn render_definition_list(
        &mut self,
        items: &[crate::markdown::DefinitionListItem],
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
                // Each definition is now a Vec<Node> (blocks), not just Text
                for block_node in definition_blocks {
                    // Render each block with 2 levels of indentation
                    self.render_node(
                        block_node,
                        lines,
                        total_height,
                        width.saturating_sub(4), // Reduce width for indentation
                        palette,
                        is_focused,
                        2,    // 2 levels of indentation (4 spaces)
                        None, // No separate node index for nested blocks
                    );
                }
            }

            // Add a small spacing between definition items (but not after the last one)
            if item != items.last().unwrap() {
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
        }

        // Add empty line after the entire definition list
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

    fn render_epub_block(
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
        // Add empty line before the block
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

        // Render the content blocks with controlled spacing
        for content_node in content.iter() {
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
                        0,    // no indentation
                        None, // No separate node index for footnote content
                    );
                }
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

    fn render_text_spans(
        &mut self,
        rich_spans: &[RichSpan],
        prefix: Option<&str>,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        indent: usize,
        add_empty_line_after: bool,
    ) {
        // Build complete rich spans with prefix
        let mut complete_rich_spans = Vec::new();
        if let Some(prefix_str) = prefix {
            complete_rich_spans.push(RichSpan::Text(Span::raw(prefix_str.to_string())));
        }
        complete_rich_spans.extend_from_slice(rich_spans);

        // Convert rich spans to plain text for wrapping
        let plain_text = complete_rich_spans
            .iter()
            .map(|rs| match rs {
                RichSpan::Text(span) => span.content.as_ref(),
                RichSpan::Link { span, .. } => span.content.as_ref(),
            })
            .collect::<String>();

        // Calculate available width after accounting for indentation
        let indent_str = "  ".repeat(indent);
        let available_width = width.saturating_sub(indent_str.len());

        // Wrap the text
        let wrapped = textwrap::wrap(&plain_text, available_width);

        // Create lines from wrapped text
        for (line_idx, wrapped_line) in wrapped.iter().enumerate() {
            let mut line_spans = Vec::new();
            let mut line_links = Vec::new();

            // Map wrapped line back to rich spans
            let rich_spans_for_line = if line_idx == 0 && wrapped.len() == 1 {
                // Single line - use all rich spans
                complete_rich_spans.clone()
            } else {
                // Multi-line content: map wrapped line back to rich spans
                self.map_wrapped_line_to_rich_spans(wrapped_line, &complete_rich_spans)
            };

            // Extract spans and links, calculating positions
            let mut current_col = 0;
            for rich_span in rich_spans_for_line {
                match rich_span {
                    RichSpan::Text(span) => {
                        let len = span.content.len();
                        line_spans.push(span);
                        current_col += len;
                    }
                    RichSpan::Link { span, mut info } => {
                        let len = span.content.len();
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
                    link.start_col += indent_str.len();
                    link.end_col += indent_str.len();
                }
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
                link_nodes: line_links, // Captured links!
                node_anchor: None,
                node_index: None,
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
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
            });
            self.raw_text_lines.push(String::new());
            *total_height += 1;
        }
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

    /// Map a wrapped line back to its rich spans, preserving links
    fn map_wrapped_line_to_rich_spans(
        &self,
        wrapped_line: &str,
        original_rich_spans: &[RichSpan],
    ) -> Vec<RichSpan> {
        // Build a flattened representation with rich span info
        #[derive(Clone)]
        struct CharWithRichSpan {
            ch: char,
            rich_span_idx: usize,    // Index into original_rich_spans
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
        // Constants for image display
        const IMAGE_HEIGHT_WIDE: u16 = 20;

        // Add empty line before image
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

        // Store the position where the image will be rendered
        let lines_before_image = *total_height;

        // Check if we have image dimensions already loaded
        let (image_height, loading_status) =
            if let Some(embedded_image) = self.embedded_images.borrow().get(url) {
                let height = embedded_image.height_cells;
                let status = match &embedded_image.state {
                    ImageLoadState::Loaded { .. } => LoadingStatus::Loaded,
                    ImageLoadState::Failed { .. } => LoadingStatus::Failed,
                    ImageLoadState::NotLoaded | ImageLoadState::Loading => LoadingStatus::Loading,
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
                    state: ImageLoadState::NotLoaded,
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
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
            });

            self.raw_text_lines.push(String::new()); // Keep raw_text_lines in sync
            *total_height += 1;
        }

        // Add empty line after image
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

    /// Clear the last active anchor when changing chapters
    pub fn clear_active_anchor(&mut self) {
        self.last_active_anchor = None;
    }

    /// Set the active anchor directly (used when clicking on TOC items)
    pub fn set_active_anchor(&mut self, anchor: Option<String>) {
        self.last_active_anchor = anchor;
    }

    /// Get the currently active section based on viewport position
    pub fn get_active_section(&mut self, current_chapter: usize) -> ActiveSection {
        // Calculate middle of viewport
        let viewport_middle = self.scroll_offset + (self.visible_height / 2);

        // Scan backwards from middle to find the most recent anchor
        let lines_to_check = viewport_middle.min(self.rendered_content.lines.len());

        for line_idx in (0..=lines_to_check).rev() {
            if let Some(line) = self.rendered_content.lines.get(line_idx) {
                if let Some(ref anchor) = line.node_anchor {
                    // Found an anchor - update and return it
                    self.last_active_anchor = Some(anchor.clone());
                    return ActiveSection::Anchor(anchor.clone());
                }
            }
        }

        // No anchor found before middle - check if we have a stored one
        if let Some(ref anchor) = self.last_active_anchor {
            // We still use the last known anchor if we're in the same chapter
            // This maintains highlighting when scrolling past the last anchor
            return ActiveSection::Anchor(anchor.clone());
        }

        // Fall back to chapter-level highlighting
        ActiveSection::Chapter(current_chapter)
    }

    /// Apply search highlighting to a line's spans
    fn apply_search_highlighting(
        &self,
        line_idx: usize,
        line_spans: Vec<Span<'static>>,
        _palette: &Base16Palette,
    ) -> Vec<Span<'static>> {
        if !self.search_state.active || self.search_state.matches.is_empty() {
            return line_spans;
        }

        // Check if this line has any search matches
        let line_matches: Vec<_> = self
            .search_state
            .matches
            .iter()
            .filter(|m| m.index == line_idx)
            .collect();

        if line_matches.is_empty() {
            return line_spans;
        }

        // Get the raw text for this line to calculate positions
        let _raw_text = self
            .rendered_content
            .lines
            .get(line_idx)
            .map(|l| l.raw_text.clone())
            .unwrap_or_default();

        let mut result_spans = Vec::new();
        let mut char_offset = 0;

        for span in line_spans {
            let span_text = span.content.to_string();
            let span_len = span_text.len();
            let span_end = char_offset + span_len;

            // Check if any highlights overlap with this span
            let mut segments = vec![];
            let mut last_pos = 0;

            for match_item in &line_matches {
                for (highlight_start, highlight_end) in &match_item.highlight_ranges {
                    // Check if this highlight overlaps with the current span
                    if *highlight_end > char_offset && *highlight_start < span_end {
                        // Calculate relative positions within the span
                        let rel_start = highlight_start.saturating_sub(char_offset).min(span_len);
                        let rel_end = highlight_end.saturating_sub(char_offset).min(span_len);

                        if rel_start > last_pos {
                            // Add non-highlighted segment before this match
                            segments.push((last_pos, rel_start, false));
                        }

                        // Add highlighted segment
                        segments.push((rel_start, rel_end, true));
                        last_pos = rel_end;
                    }
                }
            }

            // Add any remaining non-highlighted text
            if last_pos < span_len {
                segments.push((last_pos, span_len, false));
            }

            // Create new spans based on segments
            if segments.is_empty() {
                result_spans.push(span);
            } else {
                for (start, end, is_highlighted) in segments {
                    if start >= end {
                        continue;
                    }

                    let text_segment = span_text[start..end].to_string();
                    let style = if is_highlighted {
                        let is_current = self.search_state.is_current_match(line_idx);
                        if is_current {
                            // Current match: bright yellow background with black text
                            RatatuiStyle::default().bg(Color::Yellow).fg(Color::Black)
                        } else {
                            // Other matches: dim yellow background, preserve original fg
                            span.style.bg(Color::Rgb(100, 100, 0))
                        }
                    } else {
                        span.style
                    };

                    result_spans.push(Span::styled(text_segment, style));
                }
            }

            char_offset = span_end;
        }

        result_spans
    }

    /// Get searchable content (visible lines as text)
    fn get_visible_text(&self) -> Vec<String> {
        self.rendered_content
            .lines
            .iter()
            .map(|line| line.raw_text.clone())
            .collect()
    }

    /// Jump to a specific line in the content
    fn jump_to_line(&mut self, line_idx: usize) {
        if line_idx < self.rendered_content.lines.len() {
            // Center the line in the viewport if possible
            let half_height = self.visible_height / 2;
            self.scroll_offset = line_idx.saturating_sub(half_height);

            // Ensure we don't scroll past the end
            let max_scroll = self
                .rendered_content
                .total_height
                .saturating_sub(self.visible_height);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
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
            self.scroll_half_screen_down(screen_height);
        }
    }

    fn handle_ctrl_u(&mut self) {
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_half_screen_up(screen_height);
        }
    }

    fn handle_gg(&mut self) {
        self.scroll_offset = 0;
        debug!("Scrolled to top of document");
    }

    fn handle_upper_g(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = max_offset;
        debug!("Scrolled to bottom of document: offset {}", max_offset);
    }
}

impl MarkdownTextReader {
    /// Debug method to copy raw_text_lines with line numbers to clipboard
    pub fn copy_raw_text_lines_to_clipboard(&self) -> Result<(), String> {
        if self.raw_text_lines.is_empty() {
            return Err("No content to copy".to_string());
        }

        // Create a debug output with line numbers
        let mut debug_output = String::new();
        debug_output.push_str(&format!(
            "=== raw_text_lines debug (total {} lines) ===\n",
            self.raw_text_lines.len()
        ));

        for (idx, line) in self.raw_text_lines.iter().enumerate() {
            debug_output.push_str(&format!("{:4}: {}\n", idx, line));
        }

        use arboard::Clipboard;
        let mut clipboard =
            Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
        clipboard
            .set_text(debug_output)
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;

        debug!(
            "Copied raw_text_lines debug info to clipboard ({} lines)",
            self.raw_text_lines.len()
        );
        Ok(())
    }

    /// Actually perform the node restoration (called after rendering)
    fn perform_node_restore(&mut self, node_index: usize) {
        info!(
            "perform_node_restore called with node_index={}, total lines={}",
            node_index,
            self.rendered_content.lines.len()
        );

        // Count how many lines have node indices
        let lines_with_nodes = self
            .rendered_content
            .lines
            .iter()
            .filter(|line| line.node_index.is_some())
            .count();
        info!("Lines with node indices: {}", lines_with_nodes);

        // Find the first line that belongs to this node
        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if let Some(node_idx) = line.node_index {
                if node_idx >= node_index {
                    // Found the node or a later one, scroll to this position
                    self.scroll_offset = line_idx.min(self.get_max_scroll_offset());
                    info!("Restored to node {} at line {}", node_index, line_idx);
                    return;
                }
            }
        }

        // Node not found, stay at current position
        info!(
            "Could not find node index {} in rendered content, staying at current position",
            node_index
        );
    }
}

impl TextReaderTrait for MarkdownTextReader {
    fn set_content_from_string(&mut self, content: &str, _chapter_title: Option<String>) {
        // Parse HTML string to Markdown AST
        use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
        let mut converter = HtmlToMarkdownConverter::new();
        let doc = converter.convert(content);

        self.markdown_document = Some(doc);

        self.links.clear();
        self.embedded_tables.borrow_mut().clear();
        self.raw_text_lines.clear();
        self.scroll_offset = 0;
        self.text_selection.clear_selection();

        // Mark cache as invalid to force re-rendering
        self.cache_generation += 1;
        debug!("Parsed HTML to Markdown AST in set_content_from_string");
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

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);
            self.last_scroll_time = Instant::now();
            // Clear current match when manually scrolling so next 'n' finds from new position
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    fn scroll_down(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        if self.scroll_offset < max_offset {
            self.scroll_offset = (self.scroll_offset + self.scroll_speed).min(max_offset);
            self.last_scroll_time = Instant::now();
            // Clear current match when manually scrolling so next 'n' finds from new position
            if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
                self.search_state.current_match_index = None;
            }
        }
    }

    fn scroll_half_screen_up(&mut self, screen_height: usize) {
        let scroll_amount = screen_height / 2;
        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
        self.highlight_visual_line = Some(0);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        // Clear current match when manually scrolling so next 'n' finds from new position
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    fn scroll_half_screen_down(&mut self, screen_height: usize) {
        let scroll_amount = screen_height / 2;
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = (self.scroll_offset + scroll_amount).min(max_offset);
        self.highlight_visual_line = Some(screen_height - 1);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_millis(150);
        // Clear current match when manually scrolling so next 'n' finds from new position
        if self.search_state.active && self.search_state.mode == SearchMode::NavigationMode {
            self.search_state.current_match_index = None;
        }
    }

    fn get_scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    fn restore_scroll_position(&mut self, offset: usize) {
        self.scroll_offset = offset.min(self.get_max_scroll_offset());
    }

    /// Get the index of the first visible node in the viewport
    fn get_current_node_index(&self) -> usize {
        // Find the first visible line in the viewport
        let visible_start = self.scroll_offset;

        // Look through rendered lines to find the node index
        for (line_idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line_idx >= visible_start {
                if let Some(node_idx) = line.node_index {
                    return node_idx;
                }
            }
        }

        0 // Default to first node
    }

    /// Restore scroll position to show a specific node
    fn restore_to_node_index(&mut self, node_index: usize) {
        info!(
            "restore_to_node_index called with node_index={}, deferring until after render",
            node_index
        );

        // Defer the restoration until after content is rendered
        self.pending_node_restore = Some(node_index);
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
            // Check if click is on a link
            if let Some(link) = self.get_link_at_position(line, column) {
                let url = link.url.clone();
                debug!("Link clicked: {}", url);
                // Clear any selection and return the link
                self.text_selection.clear_selection();
                return Some(url);
            } else {
                debug!("links not found");
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

    fn preload_image_dimensions(&mut self, book_images: &BookImages) {
        // Extract images from the AST and preload their dimensions
        if let Some(doc) = self.markdown_document.clone() {
            info!(
                "Starting image dimension preload for document with {} blocks",
                doc.blocks.len()
            );
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

            info!("Found {} images to load in document", images_to_load.len());
            //
            // Start background loading if we have images and a picker
            if !images_to_load.is_empty() {
                if let Some(ref picker) = self.image_picker {
                    let font_size = picker.font_size();
                    let (cell_width, cell_height) = (font_size.0, font_size.1);
                    self.background_loader.start_loading(
                        images_to_load.clone(),
                        book_images,
                        cell_width,
                        cell_height,
                    );
                    // Mark all images as loading
                    for (img_src, _) in images_to_load.iter() {
                        if let Some(img_state) = self.embedded_images.borrow_mut().get_mut(img_src)
                        {
                            img_state.state = ImageLoadState::Loading;
                        }
                    }
                } else {
                    for (img, _) in images_to_load.iter() {
                        if let Some(img_state) = self.embedded_images.borrow_mut().get_mut(img) {
                            img_state.state = ImageLoadState::Failed {
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
                        ImageLoadState::Loaded {
                            image: Arc::new(image.clone()),
                            protocol: picker.new_resize_protocol(image),
                        }
                    } else {
                        ImageLoadState::Failed {
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
                ImageLoadState::Loaded { image, .. } => Some(image.clone()),
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

                // Check if we have a pending node restore after re-render
                if let Some(node_index) = self.pending_node_restore.take() {
                    self.perform_node_restore(node_index);
                    debug!("Performed pending node restore for node {}", node_index);
                }

                // Check if we have a pending anchor scroll after re-render
                if let Some(anchor_id) = self.pending_anchor_scroll.take() {
                    if let Some(target_line) = self.get_anchor_position(&anchor_id) {
                        self.scroll_to_line(target_line);
                        self.highlight_line_temporarily(target_line, Duration::from_secs(2));
                        debug!(
                            "Scrolled to pending anchor '{}' at line {} after re-render",
                            anchor_id, target_line
                        );
                    } else {
                        debug!("Pending anchor '{}' not found after re-render", anchor_id);
                    }
                }
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

        // Check if we need to insert space for comment textarea
        let mut textarea_lines_to_insert = 0;
        let mut textarea_insert_position = None;

        if self.comment_input_active {
            if let Some(target_line) = self.comment_target_line {
                if target_line >= self.scroll_offset && target_line < end_offset {
                    textarea_insert_position = Some(target_line);

                    let content_lines = if let Some(ref textarea) = self.comment_textarea {
                        textarea.lines().len()
                    } else {
                        0
                    };

                    let min_lines = 3;
                    let actual_content_lines = content_lines.max(min_lines);
                    textarea_lines_to_insert = actual_content_lines + 2;
                }
            }
        }

        for line_idx in self.scroll_offset..end_offset {
            if let Some(insert_pos) = textarea_insert_position {
                if line_idx == insert_pos {
                    for _ in 0..textarea_lines_to_insert {
                        visible_lines.push(Line::from(""));
                    }
                }
            }

            if let Some(rendered_line) = self.rendered_content.lines.get(line_idx) {
                let visual_line_idx = line_idx - self.scroll_offset;

                // Check if this is an image placeholder with a loaded image
                let skip_placeholder =
                    if let LineType::ImagePlaceholder { src } = &rendered_line.line_type {
                        // Check if image is loaded
                        if let Some(embedded_image) = self.embedded_images.borrow().get(src) {
                            matches!(embedded_image.state, ImageLoadState::Loaded { .. })
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                if skip_placeholder {
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

                // Apply search highlighting after selection highlighting
                line_spans = self.apply_search_highlighting(line_idx, line_spans, palette);

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
                                    let (image_width_pixels, _image_height_pixels) =
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

        if self.comment_input_active {
            if let Some(ref mut textarea) = self.comment_textarea {
                if let Some(target_line) = self.comment_target_line {
                    if target_line >= self.scroll_offset
                        && target_line < self.scroll_offset + self.visible_height
                    {
                        let visual_position = target_line - self.scroll_offset;

                        let textarea_y = inner_area.y + visual_position as u16;

                        if textarea_y < inner_area.y + inner_area.height {
                            // Calculate dynamic height based on content (same as above)
                            let content_lines = textarea.lines().len();
                            let min_lines = 3;
                            let actual_content_lines = content_lines.max(min_lines);
                            // Add 2 for borders
                            let desired_height = (actual_content_lines + 2) as u16;

                            // Make sure it fits in the available space
                            let textarea_height =
                                desired_height.min(inner_area.y + inner_area.height - textarea_y);

                            let textarea_rect = Rect {
                                x: inner_area.x, // Use full width
                                y: textarea_y,
                                width: inner_area.width, // Full width
                                height: textarea_height,
                            };

                            let clear_block =
                                Block::default().style(RatatuiStyle::default().bg(palette.base_00));
                            frame.render_widget(clear_block, textarea_rect);

                            let padded_rect = Rect {
                                x: inner_area.x + 2,
                                y: textarea_y,
                                width: inner_area.width.saturating_sub(4),
                                height: textarea_height,
                            };

                            textarea.set_style(
                                RatatuiStyle::default()
                                    .fg(palette.base_05)
                                    .bg(palette.base_00),
                            );
                            textarea.set_cursor_style(
                                RatatuiStyle::default()
                                    .fg(palette.base_00)
                                    .bg(palette.base_05),
                            );

                            let block = Block::default()
                                .borders(Borders::ALL)
                                .title(" Add Comment ")
                                .style(
                                    RatatuiStyle::default()
                                        .fg(palette.base_04)
                                        .bg(palette.base_00),
                                );
                            textarea.set_block(block);

                            frame.render_widget(&*textarea, padded_rect);
                        }
                    }
                }
            }
        }
    }

    fn get_last_content_area(&self) -> Option<Rect> {
        self.last_content_area
    }

    // Internal link navigation methods (from trait)
    fn get_anchor_position(&self, anchor_id: &str) -> Option<usize> {
        self.anchor_positions.get(anchor_id).copied()
    }

    fn scroll_to_line(&mut self, target_line: usize) {
        // Center target line in viewport if possible
        let desired_offset = if target_line > self.visible_height / 2 {
            target_line // - self.visible_height  / 2
        } else {
            0
        };

        self.scroll_offset = desired_offset.min(self.get_max_scroll_offset());
    }

    fn highlight_line_temporarily(&mut self, line: usize, duration: std::time::Duration) {
        if line >= self.scroll_offset && line < self.scroll_offset + self.visible_height {
            let visible_line = line - self.scroll_offset;
            self.highlight_visual_line = Some(visible_line);
            self.highlight_end_time = Instant::now() + duration;
        }
    }

    fn set_current_chapter_file(&mut self, chapter_file: Option<String>) {
        self.current_chapter_file = chapter_file;
        // Rebuild comment lookup when chapter changes
        self.rebuild_chapter_comments();
    }

    fn get_current_chapter_file(&self) -> &Option<String> {
        &self.current_chapter_file
    }

    fn handle_pending_anchor_scroll(&mut self, pending_anchor: Option<String>) {
        // Store the pending anchor to be processed after anchors are collected
        self.pending_anchor_scroll = pending_anchor;
        if let Some(ref anchor_id) = self.pending_anchor_scroll {
            debug!("Stored pending anchor scroll for '{}'", anchor_id);
        }
    }
}

impl SearchablePanel for MarkdownTextReader {
    fn start_search(&mut self) {
        self.search_state.start_search(self.scroll_offset);
    }

    fn cancel_search(&mut self) {
        let original_position = self.search_state.cancel_search();
        self.scroll_offset = original_position;
    }

    fn confirm_search(&mut self) {
        self.search_state.confirm_search();
        // If search was cancelled (empty query), restore position
        if !self.search_state.active {
            let original_position = self.search_state.original_position;
            self.scroll_offset = original_position;
        }
    }

    fn exit_search(&mut self) {
        self.search_state.exit_search();
        // Keep current position
    }

    fn update_search_query(&mut self, query: &str) {
        self.search_state.update_query(query.to_string());

        // Find matches in visible text
        let searchable = self.get_searchable_content();
        let matches = find_matches_in_text(query, &searchable);
        self.search_state.set_matches(matches);

        // Jump to match if found
        if let Some(match_index) = self.search_state.get_current_match() {
            self.jump_to_match(match_index);
        }
    }

    fn next_match(&mut self) {
        if self.search_state.matches.is_empty() {
            return;
        }

        // If we have a current match index, go to the next one
        if let Some(current_idx) = self.search_state.current_match_index {
            // Move to next match
            let next_idx = (current_idx + 1) % self.search_state.matches.len();
            self.search_state.current_match_index = Some(next_idx);

            if let Some(search_match) = self.search_state.matches.get(next_idx) {
                self.jump_to_match(search_match.index);
            }
        } else {
            // No current match, find the first match after current viewport position
            let current_position = self.scroll_offset;

            // Find the first match that's after the current position
            let mut next_match_idx = None;
            for (idx, search_match) in self.search_state.matches.iter().enumerate() {
                if search_match.index > current_position {
                    next_match_idx = Some(idx);
                    break;
                }
            }

            // If no match found after current position, wrap to beginning
            let target_idx = next_match_idx.unwrap_or(0);
            self.search_state.current_match_index = Some(target_idx);

            if let Some(search_match) = self.search_state.matches.get(target_idx) {
                self.jump_to_match(search_match.index);
            }
        }
    }

    fn previous_match(&mut self) {
        if self.search_state.matches.is_empty() {
            return;
        }

        // If we have a current match index, go to the previous one
        if let Some(current_idx) = self.search_state.current_match_index {
            // Move to previous match
            let prev_idx = if current_idx == 0 {
                self.search_state.matches.len() - 1
            } else {
                current_idx - 1
            };
            self.search_state.current_match_index = Some(prev_idx);

            if let Some(search_match) = self.search_state.matches.get(prev_idx) {
                self.jump_to_match(search_match.index);
            }
        } else {
            // No current match, find the last match before current viewport position
            let current_position = self.scroll_offset;

            // Find the last match that's before the current position
            let mut prev_match_idx = None;
            for (idx, search_match) in self.search_state.matches.iter().enumerate().rev() {
                if search_match.index < current_position {
                    prev_match_idx = Some(idx);
                    break;
                }
            }

            // If no match found before current position, wrap to end
            let target_idx = prev_match_idx.unwrap_or(self.search_state.matches.len() - 1);
            self.search_state.current_match_index = Some(target_idx);

            if let Some(search_match) = self.search_state.matches.get(target_idx) {
                self.jump_to_match(search_match.index);
            }
        }
    }

    fn get_search_state(&self) -> &SearchState {
        &self.search_state
    }

    fn is_searching(&self) -> bool {
        self.search_state.active
    }

    fn has_matches(&self) -> bool {
        !self.search_state.matches.is_empty()
    }

    fn jump_to_match(&mut self, match_index: usize) {
        self.jump_to_line(match_index);
    }

    fn get_searchable_content(&self) -> Vec<String> {
        self.get_visible_text()
    }
}

fn calculate_image_height_in_cells(image: &DynamicImage) -> u16 {
    let (width, height) = image.dimensions();
    EmbeddedImage::height_in_cells(width, height)
}
