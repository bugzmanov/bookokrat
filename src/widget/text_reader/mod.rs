mod comments;
mod images;
mod navigation;
mod rendering;
mod search;
mod selection;
mod text_selection;
mod types;

pub use types::*;

use crate::comments::{BookComments, Comment};
use crate::images::background_image_loader::BackgroundImageLoader;
use crate::markdown::Document;
use crate::markdown_text_reader::text_selection::TextSelection;
use crate::ratatui_image::{Resize, StatefulImage, ViewportOptions, picker::Picker};
use crate::search::SearchState;
use crate::theme::Base16Palette;
use crate::types::LinkInfo;
use image::{DynamicImage, GenericImageView};
use log::{info, warn};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style as RatatuiStyle,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct MarkdownTextReader {
    markdown_document: Option<Arc<Document>>,
    rendered_content: RenderedContent,

    // Scrolling state
    scroll_offset: usize,
    last_scroll_time: Instant,
    scroll_speed: usize,

    // Visual highlighting
    highlight_visual_line: Option<usize>,
    highlight_end_time: Instant,

    // Content dimensions
    total_wrapped_lines: usize,
    visible_height: usize,

    // Caching
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
    current_chapter_comments: HashMap<usize, Vec<Comment>>,

    /// Comment input state
    comment_input: CommentInputState,

    chapter_title: Option<String>,
}

impl Default for MarkdownTextReader {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownTextReader {
    pub fn new() -> Self {
        let image_picker = match Picker::from_query_stdio() {
            Ok(mut picker) => {
                info!(
                    "Image picker initial protocol type: {:?}",
                    picker.protocol_type()
                );

                // Check capabilities to see what's actually supported
                use crate::ratatui_image::picker::{Capability, ProtocolType};
                let has_kitty = picker
                    .capabilities()
                    .iter()
                    .any(|c| matches!(c, Capability::Kitty));
                let has_sixel = picker
                    .capabilities()
                    .iter()
                    .any(|c| matches!(c, Capability::Sixel));

                // Prefer: Kitty > Sixel > iTerm > Halfblocks
                let chosen_protocol = if has_kitty {
                    info!("Kitty protocol detected, using Kitty");
                    ProtocolType::Kitty
                } else if has_sixel {
                    info!("Sixel protocol detected, using Sixel");
                    ProtocolType::Sixel
                } else {
                    // Keep whatever was detected (iTerm or Halfblocks)
                    picker.protocol_type()
                };

                picker.set_protocol_type(chosen_protocol);
                info!("Final protocol type: {:?}", picker.protocol_type());

                // Check if protocol requires true color but terminal doesn't support it
                let requires_true_color =
                    matches!(picker.protocol_type(), ProtocolType::Halfblocks);

                if requires_true_color && !crate::color_mode::supports_true_color() {
                    warn!(
                        "Image protocol {:?} requires true color, but terminal doesn't support it. Disabling image rendering.",
                        picker.protocol_type()
                    );
                    None
                } else {
                    picker.set_background_color([0, 0, 0, 0]);
                    Some(picker)
                }
            }
            Err(e) => {
                warn!(
                    "Failed to create image picker: {e}. The terminal would not support image rendering!"
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
            last_scroll_time: Instant::now(),
            scroll_speed: 1,
            highlight_visual_line: None,
            highlight_end_time: Instant::now(),
            total_wrapped_lines: 0,
            visible_height: 0,
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
            comment_input: CommentInputState::default(),
            chapter_title: None,
        }
    }

    fn calculate_progress(&self, _content: &str, _width: usize, _height: usize) -> u32 {
        if self.total_wrapped_lines == 0 {
            return 0;
        }

        let visible_end = (self.scroll_offset + self.visible_height).min(self.total_wrapped_lines);
        ((visible_end as f32 / self.total_wrapped_lines as f32) * 100.0) as u32
    }
}

impl MarkdownTextReader {
    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        current_chapter: usize,
        total_chapters: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Store content area for hit-testing and mouse interactions
        self.last_content_area = Some(area);
        // Trim borders plus footer space to get actual viewport height
        self.visible_height = area.height.saturating_sub(3) as usize;

        if self.show_raw_html {
            self.render_raw_html(frame, area, current_chapter, total_chapters, palette);
            return;
        }

        // Account for borders and side padding
        let width = area.width.saturating_sub(4) as usize;

        // Re-render when dimensions, focus, or cached content change
        if self.last_width != width
            || self.last_focus_state != is_focused
            || self.rendered_content.generation != self.cache_generation
        {
            if let Some(doc) = self.markdown_document.clone() {
                self.rendered_content =
                    self.render_document_to_lines(doc.as_ref(), width, palette, is_focused);
                self.total_wrapped_lines = self.rendered_content.total_height;
                self.last_width = width;
                self.last_focus_state = is_focused;

                if let Some(node_index) = self.pending_node_restore.take() {
                    self.perform_node_restore(node_index);
                }

                if let Some(anchor_id) = self.pending_anchor_scroll.take() {
                    if let Some(target_line) = self.get_anchor_position(&anchor_id) {
                        self.scroll_to_line(target_line);
                        self.highlight_line_temporarily(target_line, Duration::from_secs(2));
                    } else {
                        warn!("Pending anchor '{anchor_id}' not found after re-render");
                    }
                }
            }
        }
        let title_text = if let Some(ref title) = self.chapter_title {
            format!("[{current_chapter}/{total_chapters}] {title}")
        } else {
            format!("Chapter {current_chapter}/{total_chapters}")
        };

        let progress = self.calculate_progress("", width, self.visible_height);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .title_bottom(Line::from(format!(" {progress}% ")).right_aligned());

        // Remove borders so the text sits inside the frame cleanly
        let mut inner_area = block.inner(area);
        inner_area.y = inner_area.y.saturating_add(1);
        inner_area.height = inner_area.height.saturating_sub(1);
        inner_area.x = inner_area.x.saturating_add(1);

        // Remember the focused text area for mouse hover/selection logic
        self.last_inner_text_area = Some(inner_area);

        // First pass: render text lines (no images yet)
        let mut visible_lines = Vec::new();
        let end_offset =
            (self.scroll_offset + self.visible_height).min(self.rendered_content.lines.len());

        // Selection background depends on focus state
        let selection_bg = if is_focused {
            palette.base_02
        } else {
            palette.base_01
        };

        // Reserve empty lines where the comment textarea will be drawn
        let mut textarea_lines_to_insert = 0;
        let mut textarea_insert_position = None;

        if self.comment_input.is_active() {
            if let Some(target_line) = self.comment_input.target_line {
                if target_line >= self.scroll_offset && target_line < end_offset {
                    textarea_insert_position = Some(target_line);

                    let content_lines = if let Some(ref textarea) = self.comment_input.textarea {
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

                let skip_placeholder =
                    if let LineType::ImagePlaceholder { src } = &rendered_line.line_type {
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

                let mut line_spans = if self.highlight_visual_line == Some(visual_line_idx) {
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

                if self.text_selection.has_selection() {
                    let line_with_selection = self.text_selection.apply_selection_highlighting(
                        line_idx,
                        line_spans,
                        selection_bg,
                    );
                    line_spans = line_with_selection.spans;
                }

                line_spans = self.apply_search_highlighting(line_idx, line_spans, palette);

                visible_lines.push(Line::from(line_spans));
            }
        }

        let paragraph = Paragraph::new(vec![])
            .block(block.clone())
            .wrap(ratatui::widgets::Wrap { trim: false });

        frame.render_widget(paragraph, area);

        let inner_text_paragraph = Paragraph::new(visible_lines)
            .block(Block::default().borders(Borders::NONE))
            .wrap(ratatui::widgets::Wrap { trim: false });

        frame.render_widget(inner_text_paragraph, inner_area);

        // Second pass: draw inline images over the text block
        let scroll_offset = self.scroll_offset;

        if !self.show_raw_html {
            self.check_for_loaded_images();
            if !self.embedded_images.borrow().is_empty() && self.image_picker.is_some() {
                let area_height = inner_area.height as usize;

                for (_, embedded_image) in self.embedded_images.borrow_mut().iter_mut() {
                    let image_height_cells = embedded_image.height_cells as usize;
                    let image_start_line = embedded_image.lines_before_image;
                    let image_end_line = image_start_line + image_height_cells;

                    if scroll_offset < image_end_line
                        && scroll_offset + area_height > image_start_line
                    {
                        if let ImageLoadState::Loaded {
                            ref image,
                            ref mut protocol,
                        } = embedded_image.state
                        {
                            let scaled_image = image;

                            if let Some(ref picker) = self.image_picker {
                                let image_screen_start =
                                    image_start_line.saturating_sub(scroll_offset);

                                // Clip the top portion if the image starts above the viewport
                                let image_top_clipped =
                                    scroll_offset.saturating_sub(image_start_line);

                                let visible_image_height = (image_height_cells - image_top_clipped)
                                    .min(area_height - image_screen_start);

                                if visible_image_height > 0 {
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

                                    // Determine the terminal width required for the pixels
                                    let (image_width_pixels, _image_height_pixels) =
                                        scaled_image.dimensions();
                                    let font_size = picker.font_size();
                                    let image_width_cells =
                                        (image_width_pixels as f32 / font_size.0 as f32).ceil()
                                            as u16;

                                    // Center the image horizontally in the text area
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

                                    // Render using the ratatui_image viewport for scrolling
                                    let current_font_size = picker.font_size();
                                    let y_offset_pixels = (image_top_clipped as f32
                                        * current_font_size.1 as f32)
                                        as u32;

                                    let viewport_options = ViewportOptions {
                                        y_offset: y_offset_pixels,
                                        x_offset: 0, // No horizontal scrolling for now
                                    };

                                    let image_widget = StatefulImage::new()
                                        .resize(Resize::Viewport(viewport_options));
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

        if self.comment_input.is_active() {
            if let Some(ref mut textarea) = self.comment_input.textarea {
                if let Some(target_line) = self.comment_input.target_line {
                    if target_line >= self.scroll_offset
                        && target_line < self.scroll_offset + self.visible_height
                    {
                        let visual_position = target_line - self.scroll_offset;

                        let textarea_y = inner_area.y + visual_position as u16;

                        if textarea_y < inner_area.y + inner_area.height {
                            // Compute minimum height so borders never collapse
                            let content_lines = textarea.lines().len();
                            let min_lines = 3;
                            let actual_content_lines = content_lines.max(min_lines);
                            let desired_height = (actual_content_lines + 2) as u16;

                            // Constrain height to the remaining view
                            let textarea_height =
                                desired_height.min(inner_area.y + inner_area.height - textarea_y);

                            // Shift left to align with paragraph text (inner_area.x already has padding)
                            let left_adjust = 2;

                            let textarea_rect = Rect {
                                x: inner_area.x.saturating_sub(left_adjust),
                                y: textarea_y,
                                width: inner_area.width + left_adjust,
                                height: textarea_height,
                            };

                            let clear_block =
                                Block::default().style(RatatuiStyle::default().bg(palette.base_00));
                            frame.render_widget(clear_block, textarea_rect);

                            let padded_rect = Rect {
                                x: textarea_rect.x + 2,
                                y: textarea_y,
                                width: textarea_rect.width.saturating_sub(4),
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
                                .title(" Add Note ")
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

    pub fn render_raw_html(
        &mut self,
        frame: &mut ratatui::Frame,
        area: Rect,
        current_chapter: usize,
        total_chapters: usize,
        palette: &Base16Palette,
    ) {
        let title_text = if let Some(ref title) = self.chapter_title {
            format!("[{current_chapter}/{total_chapters}] {title} [RAW HTML]")
        } else {
            format!("Chapter {current_chapter}/{total_chapters} [RAW HTML]")
        };

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .title(title_text)
            .style(RatatuiStyle::default().fg(palette.base_09)); // Red border for raw mode

        let raw_content = if let Some(html) = &self.raw_html_content {
            html.clone()
        } else {
            "Raw HTML content not available".to_string()
        };

        let paragraph = ratatui::widgets::Paragraph::new(raw_content)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((self.scroll_offset as u16, 0));

        frame.render_widget(paragraph, area);
    }

    pub fn set_content_from_string(
        &mut self,
        content_raw_html: &str,
        chapter_title: Option<String>,
    ) {
        self.clear_content();

        use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
        let mut converter = HtmlToMarkdownConverter::new();
        let doc = Arc::new(converter.convert(content_raw_html));

        self.markdown_document = Some(doc);
        self.chapter_title = chapter_title;

        // Mark cached render as stale so next draw rebuilds it
        self.cache_generation += 1;
    }

    pub fn clear_content(&mut self) {
        self.scroll_offset = 0;
        self.text_selection.clear_selection();

        // IMPORTANT: Clear the markdown document so new content can be parsed
        self.markdown_document = None;

        self.cache_generation += 1;

        self.links.clear();
        self.embedded_tables.borrow_mut().clear();
        self.raw_text_lines.clear();
        self.rendered_content = RenderedContent {
            lines: Vec::new(),
            total_height: 0,
            generation: 0,
        };
        self.embedded_images.borrow_mut().clear();
    }

    pub fn set_raw_html(&mut self, html: String) {
        self.raw_html_content = Some(html);
    }

    pub fn toggle_raw_html(&mut self) {
        self.show_raw_html = !self.show_raw_html;
    }

    pub fn handle_terminal_resize(&mut self) {
        self.cache_generation += 1;
    }
}

fn calculate_image_height_in_cells(image: &DynamicImage) -> u16 {
    let (width, height) = image.dimensions();
    EmbeddedImage::height_in_cells(width, height)
}
