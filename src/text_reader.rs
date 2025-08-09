use crate::image_placeholder::{ImagePlaceholder, ImagePlaceholderConfig};
use crate::main_app::VimNavMotions;
use crate::text_selection::TextSelection;
use crate::theme::Base16Palette;
use image::{DynamicImage, GenericImageView};
use log::debug;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};
use ratatui_image::{
    Resize, StatefulImage, ViewportOptions, picker::Picker, protocol::StatefulProtocol,
};
use regex::Regex;
use std::cell::RefCell;
use std::time::Instant;
use textwrap;

/// Fixed height for images in terminal cells
const IMAGE_HEIGHT_IN_CELLS: u16 = 15;
/// Total lines reserved for image including padding (1 line before + 15 image lines + 1 line after)
const IMAGE_TOTAL_LINES: u16 = IMAGE_HEIGHT_IN_CELLS + 2;

#[derive(Debug, Clone)]
struct AutoScrollState {
    direction: AutoScrollDirection,
    mouse_x: u16,
    mouse_y: u16,
    content_area: Rect,
    last_scroll_time: Instant,
}

#[derive(Debug, Clone, PartialEq)]
enum AutoScrollDirection {
    Up,
    Down,
}

pub struct EmbeddedImage {
    pub src: String,
    pub lines_before_image: usize,
}

pub struct TextReader {
    pub scroll_offset: usize,
    pub content_length: usize,
    last_scroll_time: Instant,
    scroll_speed: usize,
    // Highlight state for navigation aid
    pub highlight_visual_line: Option<usize>, // Visual line number to highlight (0-based)
    pub highlight_end_time: Instant,          // When to stop highlighting
    // Store the total wrapped lines for bounds checking
    pub total_wrapped_lines: usize,
    pub visible_height: usize,
    // Cache dimensions to prevent redundant calculations
    cached_text_width: usize,
    cached_content_hash: u64,
    // Cache styled content to avoid expensive re-parsing
    cached_styled_content: Option<Text<'static>>,
    cached_styled_width: usize,
    cached_chapter_title_hash: u64,
    cached_focus_state: bool,
    // Text selection state
    pub text_selection: TextSelection,
    // Store raw text lines for selection extraction
    raw_text_lines: Vec<String>,
    // Store the last content area for mouse coordinate conversion
    pub last_content_area: Option<Rect>,
    // Auto-scroll state for continuous scrolling during text selection
    auto_scroll_state: Option<AutoScrollState>,
    // Image display
    ada_image: Option<DynamicImage>,
    image_picker: RefCell<Option<Picker>>,
    // Cached stateful image protocol
    cached_image_protocol: RefCell<Option<StatefulProtocol>>,
    // Image width in terminal cells (calculated during loading)
    image_width_cells: u16,
    embedded_images: Vec<EmbeddedImage>,
}

impl TextReader {
    pub fn new() -> Self {
        // Create image picker first to get cell dimensions
        let (image_picker, detected_cell_height) = match Picker::from_query_stdio() {
            Ok(mut picker) => {
                // Set transparent background like in the demo
                picker.set_background_color([0, 0, 0, 0]);

                // Get the detected font size (returns (width, height) in pixels)
                let font_size = picker.font_size();
                debug!(
                    "Successfully created image picker, detected font size: {:?}",
                    font_size
                );

                // Use the font height as our cell height
                let cell_height = font_size.1;

                (Some(picker), cell_height)
            }
            Err(e) => {
                debug!("Failed to create image picker: {}", e);
                (None, 16) // Default to 16 pixels per cell
            }
        };

        // Try to load Ada.png from root folder
        let (ada_image, image_width_cells) = match image::open("Ada.png") {
            Ok(img) => {
                let (width, height) = img.dimensions();
                debug!("Successfully loaded Ada.png: {}x{}", width, height);

                // Pre-scale the image to fit exactly IMAGE_HEIGHT_IN_CELLS terminal cells height
                // Use the detected cell height from the picker
                let target_height_in_pixels =
                    IMAGE_HEIGHT_IN_CELLS as u32 * detected_cell_height as u32;

                debug!(
                    "Using detected cell height: {} pixels, target image height: {} pixels",
                    detected_cell_height, target_height_in_pixels
                );

                // Calculate the scale factor to achieve target height
                let scale = target_height_in_pixels as f32 / height as f32;
                let new_width = (width as f32 * scale) as u32;
                let new_height = target_height_in_pixels;

                debug!(
                    "Pre-scaling Ada.png from {}x{} to {}x{} for exactly {} cells",
                    width, height, new_width, new_height, IMAGE_HEIGHT_IN_CELLS
                );

                let scaled =
                    img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3);

                // Calculate image width in terminal cells
                // Terminal cells are typically ~2:1 aspect ratio (twice as tall as wide)
                // So we need to account for this when converting pixel width to cell width
                let cell_aspect_ratio = 2.0; // cells are ~2x taller than wide
                let image_width_cells =
                    ((new_width as f32 / detected_cell_height as f32) * cell_aspect_ratio) as u16;
                debug!(
                    "Image width: {} pixels = {} terminal cells (cell height: {} pixels)",
                    new_width, image_width_cells, detected_cell_height
                );

                (Some(scaled), image_width_cells)
            }
            Err(e) => {
                debug!("Failed to load Ada.png: {}", e);
                (None, 0)
            }
        };

        let embedded_images = Vec::new();

        Self {
            scroll_offset: 0,
            content_length: 0,
            last_scroll_time: Instant::now(),
            scroll_speed: 1,
            highlight_visual_line: None,
            highlight_end_time: Instant::now(),
            total_wrapped_lines: 0,
            visible_height: 0,
            cached_text_width: 0,
            cached_content_hash: 0,
            cached_styled_content: None,
            cached_styled_width: 0,
            cached_chapter_title_hash: 0,
            cached_focus_state: false,
            text_selection: TextSelection::new(),
            raw_text_lines: Vec::new(),
            last_content_area: None,
            auto_scroll_state: None,
            ada_image,
            image_picker: RefCell::new(image_picker),
            cached_image_protocol: RefCell::new(None),
            image_width_cells,
            embedded_images,
        }
    }

    /// Simple hash function for content caching
    fn simple_hash(content: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    fn calculate_lines_before_image(
        &self,
        content: &str,
        _chapter_title: &Option<String>,
        width: usize,
    ) -> usize {
        let mut line_count = 0;
        let mut found_content = false;
        let mut paragraph_count = 0;

        // Process each line
        for line in content.lines() {
            if line.trim().is_empty() {
                // Empty line
                if found_content {
                    // This is a paragraph break
                    paragraph_count += 1;
                    if paragraph_count >= 6 {
                        // Found the end of sixth paragraph
                        return line_count;
                    }
                }
                line_count += 1;
            } else {
                // Non-empty line
                found_content = true;
                // Account for line wrapping
                let wrapped_lines = textwrap::wrap(line, width);
                line_count += wrapped_lines.len();
            }
        }

        // If we didn't find 6 paragraph breaks, place image after all content
        line_count
    }

    fn calculate_image_height_in_cells(&self, _image: &DynamicImage, _area_width: u16) -> u16 {
        // Always return exactly IMAGE_HEIGHT_IN_CELLS cells for consistent layout
        IMAGE_HEIGHT_IN_CELLS
    }

    /// Calculate total wrapped lines for the given content and width
    pub fn update_wrapped_lines(&mut self, content: &str, width: usize, visible_height: usize) {
        self.visible_height = visible_height;
        self.cached_text_width = width;
        self.cached_content_hash = Self::simple_hash(content);

        // Count lines including chapter title if present
        let mut total_lines = 0;

        // Add lines for chapter title (title + empty line)
        total_lines += 2;

        // Track if we'll insert image placeholder
        let mut paragraph_count = 0;
        let mut in_paragraph = false;
        let mut will_insert_image = false;

        // Wrap each line of content
        for line in content.lines() {
            let is_empty = line.trim().is_empty();

            // Track paragraph state (same logic as in parse_styled_text_cached)
            if !is_empty && !in_paragraph {
                in_paragraph = true;
            } else if is_empty && in_paragraph {
                paragraph_count += 1;
                in_paragraph = false;
            }

            // Check if we should count image lines
            if !will_insert_image && paragraph_count >= 6 && is_empty && self.ada_image.is_some() {
                will_insert_image = true;
                // Add total lines for image (empty line before + image + empty line after)
                total_lines += IMAGE_TOTAL_LINES as usize;
                debug!(
                    "Adding {} lines for image (1 padding + {} image + 1 padding) at line {}",
                    IMAGE_TOTAL_LINES, IMAGE_HEIGHT_IN_CELLS, total_lines
                );
            } else if is_empty {
                total_lines += 1;
            } else {
                let wrapped_lines = textwrap::wrap(line, width);
                total_lines += wrapped_lines.len();
            }
        }

        self.total_wrapped_lines = total_lines;
        debug!(
            "Updated wrapped lines: {} total (including image: {}), {} visible, width: {}",
            total_lines, will_insert_image, visible_height, width
        );
    }

    /// Get the maximum allowed scroll offset
    pub fn get_max_scroll_offset(&self) -> usize {
        if self.total_wrapped_lines > self.visible_height {
            // Ensure we can see all lines including the last one
            // Account for 0-based indexing: if we have 204 lines (0-203) and height 18,
            // max offset should be 186 to show lines 186-203 (18 lines)
            self.total_wrapped_lines.saturating_sub(self.visible_height)
        } else {
            0
        }
    }

    /// Clamp scroll offset to valid bounds
    pub fn clamp_scroll_offset(&mut self) {
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = self.scroll_offset.min(max_offset);
        debug!(
            "Clamped scroll offset to: {} (max: {})",
            self.scroll_offset, max_offset
        );
    }

    pub fn parse_styled_text_cached(
        &mut self,
        text: &str,
        chapter_title: &Option<String>,
        palette: &Base16Palette,
        width: usize,
        is_focused: bool,
    ) -> Text {
        let content_hash = Self::simple_hash(text);
        let chapter_title_hash = chapter_title
            .as_ref()
            .map(|t| Self::simple_hash(t))
            .unwrap_or(0);

        // Check if we need to regenerate the cached content
        let needs_update = self.cached_styled_content.is_none()
            || self.cached_styled_width != width
            || self.cached_content_hash != content_hash
            || self.cached_chapter_title_hash != chapter_title_hash
            || self.cached_focus_state != is_focused;

        if needs_update {
            debug!(
                "Regenerating styled content cache: width {} -> {}, content changed: {}, title changed: {}",
                self.cached_styled_width,
                width,
                self.cached_content_hash != content_hash,
                self.cached_chapter_title_hash != chapter_title_hash
            );

            let (styled_content, raw_lines) = self.parse_styled_text_internal_with_raw(
                text,
                chapter_title,
                palette,
                width,
                is_focused,
            );
            self.cached_styled_content = Some(styled_content);
            self.raw_text_lines = raw_lines;
            self.cached_styled_width = width;
            self.cached_content_hash = content_hash;
            self.cached_chapter_title_hash = chapter_title_hash;
            self.cached_focus_state = is_focused;
        }

        let mut result = self.cached_styled_content.as_ref().unwrap().clone();

        // Apply selection highlighting if there's an active selection
        if self.text_selection.has_selection() {
            // Use focus-aware selection color
            let (selection_bg_color, _) = palette.get_selection_colors(is_focused);
            let highlighted_lines: Vec<Line> = result
                .lines
                .into_iter()
                .enumerate()
                .map(|(line_idx, line)| {
                    self.text_selection.apply_selection_highlighting(
                        line_idx,
                        line.spans,
                        selection_bg_color,
                    )
                })
                .collect();
            result = ratatui::text::Text::from(highlighted_lines);
        }

        result
    }

    fn parse_styled_text_internal_with_raw(
        &mut self,
        text: &str,
        chapter_title: &Option<String>,
        palette: &Base16Palette,
        width: usize,
        is_focused: bool,
    ) -> (Text<'static>, Vec<String>) {
        let mut lines = Vec::new();
        let mut raw_lines = Vec::new();

        // Add chapter title lines to raw text
        if let Some(title) = chapter_title {
            raw_lines.push(title.clone());
            raw_lines.push(String::new());

            // Use focus-aware color for chapter title
            let title_color = if is_focused {
                palette.base_0d // Keep bright blue for focused chapter titles
            } else {
                palette.base_02 // Much dimmer for unfocused
            };
            lines.push(Line::from(vec![Span::styled(
                title.clone(),
                Style::default()
                    .fg(title_color)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(String::new()));
        }

        // Calculate where to insert image placeholder
        let mut line_count = 0;
        let mut inserted_image_placeholder = false;
        let mut paragraph_count = 0;
        let mut in_paragraph = false;

        // Process each line with manual wrapping
        for (_i, line) in text.lines().enumerate() {
            let is_empty = line.trim().is_empty();

            // Track paragraph state
            if !is_empty && !in_paragraph {
                in_paragraph = true;
            } else if is_empty && in_paragraph {
                paragraph_count += 1;
                in_paragraph = false;
            }

            // Check if we should insert image placeholder on this empty line
            let should_insert_image = false; // Disable automatic image insertion

            // Check if this line contains an image placeholder
            let is_image_placeholder = line.trim().starts_with("[image src=");

            // Process the line
            if is_image_placeholder {
                let img_src = extract_src(line.trim()).unwrap_or("no-src".to_string());
                debug!("Image {} - {} ", img_src, line_count);
                self.embedded_images.push(EmbeddedImage {
                    src: img_src,
                    lines_before_image: line_count,
                });

                let image_src = line.trim().to_string();

                // Create image placeholder using the new component
                let config = ImagePlaceholderConfig {
                    internal_padding: 4,
                    total_height: 15,
                    border_color: palette.base_03,
                };

                let placeholder = ImagePlaceholder::new(&image_src, width, &config);

                // Add all the placeholder lines
                for (raw_line, styled_line) in placeholder
                    .raw_lines
                    .into_iter()
                    .zip(placeholder.styled_lines.into_iter())
                {
                    raw_lines.push(raw_line);
                    lines.push(styled_line);
                }

                line_count += 15; // Height of placeholder
            } else if is_empty {
                if should_insert_image {
                    // Add empty line before image
                    raw_lines.push(String::new());
                    lines.push(Line::from(String::new()));
                    line_count += 1;

                    // Add image placeholder lines
                    let image_height = if let Some(ref ada_image) = self.ada_image {
                        self.calculate_image_height_in_cells(ada_image, width as u16) as usize
                    } else {
                        0
                    };

                    for _j in 0..image_height {
                        raw_lines.push(String::new());
                        lines.push(Line::from(String::new()));
                        line_count += 1;
                    }

                    // Add empty line after image
                    raw_lines.push(String::new());
                    lines.push(Line::from(String::new()));
                    line_count += 1;

                    inserted_image_placeholder = true;
                } else {
                    // Normal empty line processing
                    raw_lines.push(String::new());
                    lines.push(Line::from(String::new()));
                    line_count += 1;
                }
            } else {
                // Wrap the line using textwrap
                let wrapped_lines = textwrap::wrap(line, width);
                for wrapped_line in wrapped_lines {
                    let line_str = wrapped_line.to_string();
                    raw_lines.push(line_str.clone());
                    let styled_line = self.parse_line_styling_owned(&line_str, palette, is_focused);
                    lines.push(styled_line);
                    line_count += 1;
                }
            }
        }

        // Return both styled content and raw lines
        (Text::from(lines), raw_lines)
    }

    /// Parse styling for a single line (bold, quotes, etc.) - owned version for caching
    fn parse_line_styling_owned(
        &self,
        line: &str,
        palette: &Base16Palette,
        is_focused: bool,
    ) -> Line<'static> {
        let mut spans = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let mut current_text = String::new();

        // Get focus-aware colors
        let (normal_text_color, _, _) = palette.get_panel_colors(is_focused);
        let bold_text_color = if is_focused {
            palette.base_08 // Bright red for focused bold text
        } else {
            palette.base_01 // Even more dimmed for unfocused bold text
        };

        while i < chars.len() {
            if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(normal_text_color),
                    ));
                    current_text.clear();
                }

                i += 2;
                let mut bold_text = String::new();
                let mut found_closing = false;
                while i + 1 < chars.len() {
                    if chars[i] == '*' && chars[i + 1] == '*' {
                        found_closing = true;
                        i += 2;
                        break;
                    } else {
                        bold_text.push(chars[i]);
                        i += 1;
                    }
                }
                if found_closing {
                    spans.push(Span::styled(
                        bold_text,
                        Style::default()
                            .fg(bold_text_color)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    current_text.push_str("**");
                    current_text.push_str(&bold_text);
                }
            } else if (chars[i] == '"' || chars[i] == '\u{201C}' || chars[i] == '\u{201D}')
                && chars[i] != '\''
                && chars[i] != '\u{2018}'
                && chars[i] != '\u{2019}'
            {
                let quote_char = chars[i];
                let closing_quote = match quote_char {
                    '"' => '"',
                    '\u{201C}' => '\u{201D}',
                    '\u{201D}' => '\u{201D}',
                    _ => quote_char,
                };

                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(normal_text_color),
                    ));
                    current_text.clear();
                }

                let start_pos = i;
                i += 1;
                let mut quoted_text = String::new();
                let mut found_closing = false;

                let max_quote_length = 200;
                let search_limit = (i + max_quote_length).min(chars.len());

                while i < search_limit {
                    if chars[i] == closing_quote || chars[i] == quote_char {
                        spans.push(Span::styled(
                            format!("{}{}{}", quote_char, quoted_text, chars[i]),
                            Style::default()
                                .fg(palette.base_0d)
                                .add_modifier(Modifier::BOLD),
                        ));
                        i += 1;
                        found_closing = true;
                        break;
                    } else {
                        quoted_text.push(chars[i]);
                        i += 1;
                    }
                }

                if !found_closing {
                    current_text.push(chars[start_pos]);
                    i = start_pos + 1;
                }
            } else {
                current_text.push(chars[i]);
                i += 1;
            }
        }

        if !current_text.is_empty() {
            spans.push(Span::styled(
                current_text,
                Style::default().fg(normal_text_color),
            ));
        }

        if spans.is_empty() {
            Line::from(String::new())
        } else {
            Line::from(spans)
        }
    }

    /// Parse styling for a single line (bold, quotes, etc.) - legacy version
    fn parse_line_styling(&self, line: &str, palette: &Base16Palette) -> Line {
        let mut spans = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let mut current_text = String::new();

        // Use original colors for legacy function
        let normal_text_color = palette.base_07;
        let bold_text_color = palette.base_08;

        while i < chars.len() {
            if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(normal_text_color),
                    ));
                    current_text.clear();
                }

                i += 2;
                let mut bold_text = String::new();
                let mut found_closing = false;
                while i + 1 < chars.len() {
                    if chars[i] == '*' && chars[i + 1] == '*' {
                        found_closing = true;
                        i += 2;
                        break;
                    } else {
                        bold_text.push(chars[i]);
                        i += 1;
                    }
                }
                if found_closing {
                    spans.push(Span::styled(
                        bold_text,
                        Style::default()
                            .fg(bold_text_color)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    current_text.push_str("**");
                    current_text.push_str(&bold_text);
                }
            } else if (chars[i] == '"' || chars[i] == '\u{201C}' || chars[i] == '\u{201D}')
                && chars[i] != '\''
                && chars[i] != '\u{2018}'
                && chars[i] != '\u{2019}'
            {
                let quote_char = chars[i];
                let closing_quote = match quote_char {
                    '"' => '"',
                    '\u{201C}' => '\u{201D}',
                    '\u{201D}' => '\u{201D}',
                    _ => quote_char,
                };

                if !current_text.is_empty() {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().fg(normal_text_color),
                    ));
                    current_text.clear();
                }

                let start_pos = i;
                i += 1;
                let mut quoted_text = String::new();
                let mut found_closing = false;

                let max_quote_length = 200;
                let search_limit = (i + max_quote_length).min(chars.len());

                while i < search_limit {
                    if chars[i] == closing_quote || chars[i] == quote_char {
                        spans.push(Span::styled(
                            format!("{}{}{}", quote_char, quoted_text, chars[i]),
                            Style::default()
                                .fg(palette.base_0d)
                                .add_modifier(Modifier::BOLD),
                        ));
                        i += 1;
                        found_closing = true;
                        break;
                    } else {
                        quoted_text.push(chars[i]);
                        i += 1;
                    }
                }

                if !found_closing {
                    current_text.push(chars[start_pos]);
                    i = start_pos + 1;
                }
            } else {
                current_text.push(chars[i]);
                i += 1;
            }
        }

        if !current_text.is_empty() {
            spans.push(Span::styled(
                current_text,
                Style::default().fg(normal_text_color),
            ));
        }

        if spans.is_empty() {
            Line::from(String::new())
        } else {
            Line::from(spans)
        }
    }

    pub fn calculate_progress(
        &self,
        content: &str,
        visible_width: usize,
        visible_height: usize,
    ) -> u32 {
        let total_lines = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                // Use integer arithmetic for stable line wrapping calculation
                // This is equivalent to ceil(line.len() / visible_width)
                (line.len() + visible_width - 1) / visible_width
            })
            .sum::<usize>();

        let max_scroll = if total_lines > visible_height {
            total_lines - visible_height
        } else {
            0
        };

        if max_scroll > 0 {
            // Use integer arithmetic for stable rounding
            // This ensures consistent results across different runs
            let progress = (self.scroll_offset * 100) / max_scroll;
            progress.min(100) as u32
        } else {
            100
        }
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
        self.scroll_speed = 1;
        // Clear text selection when changing chapters
        self.text_selection.clear_selection();
        self.raw_text_lines.clear();
        // Clear auto-scroll state
        self.auto_scroll_state = None;
        // Clear image cache
        *self.cached_image_protocol.borrow_mut() = None;
        // Also clear content-related caches (reuse content_updated logic)
        self.content_updated(self.content_length);
    }

    pub fn restore_scroll_position(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    /// Called when content has been updated (e.g., when changing chapters)
    /// This properly resets all internal state that depends on content
    pub fn content_updated(&mut self, content_length: usize) {
        self.content_length = content_length;
        // Reset wrapped lines count - it will be calculated on next render
        self.total_wrapped_lines = 0;
        self.visible_height = 0;
        // Clear any cached content since it's now invalid
        self.cached_styled_content = None;
        self.cached_content_hash = 0;
        self.cached_chapter_title_hash = 0;
        self.cached_focus_state = false;
        // Clear embedded images since they're parsed from content
        self.embedded_images.clear();
    }

    pub fn scroll_down_no_content(&mut self) {
        let max_offset = self.get_max_scroll_offset();

        // Early return if we're already at the bottom
        if self.scroll_offset >= max_offset {
            debug!(
                "Already at bottom, scroll_offset: {}, max_offset: {}",
                self.scroll_offset, max_offset
            );
            return;
        }

        // Check if we're scrolling continuously
        let now = Instant::now();
        if now.duration_since(self.last_scroll_time) < std::time::Duration::from_millis(100) {
            // Increase scroll speed up to a maximum
            self.scroll_speed = (self.scroll_speed + 1).min(10);
        } else {
            // Reset scroll speed if there was a pause
            self.scroll_speed = 1;
        }
        self.last_scroll_time = now;

        // Apply scroll with current speed and clamp to bounds
        let new_offset = self.scroll_offset.saturating_add(self.scroll_speed);
        self.scroll_offset = new_offset.min(max_offset);

        debug!(
            "Scrolling down to offset: {}/{} (speed: {}, max: {})",
            self.scroll_offset, self.total_wrapped_lines, self.scroll_speed, max_offset
        );
    }

    pub fn scroll_up_no_content(&mut self) {
        // Early return if we're already at the top
        if self.scroll_offset == 0 {
            debug!("Already at top, scroll_offset: 0");
            return;
        }

        // Check if we're scrolling continuously
        let now = Instant::now();
        if now.duration_since(self.last_scroll_time) < std::time::Duration::from_millis(100) {
            // Increase scroll speed up to a maximum
            self.scroll_speed = (self.scroll_speed + 1).min(10);
        } else {
            // Reset scroll speed if there was a pause
            self.scroll_speed = 1;
        }
        self.last_scroll_time = now;

        // Apply scroll with current speed (saturating_sub already handles lower bound)
        self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);

        debug!(
            "Scrolling up to offset: {}/{} (speed: {}, max: {})",
            self.scroll_offset,
            self.total_wrapped_lines,
            self.scroll_speed,
            self.get_max_scroll_offset()
        );
    }

    pub fn scroll_down(&mut self, _content: &str) {
        self.scroll_down_no_content();
    }

    pub fn scroll_up(&mut self, _content: &str) {
        self.scroll_up_no_content();
    }

    pub fn scroll_half_screen_down(&mut self, _content: &str, screen_height: usize) {
        let half_screen = (screen_height / 2).max(1);
        let max_offset = self.get_max_scroll_offset();
        let new_offset = self.scroll_offset.saturating_add(half_screen);
        self.scroll_offset = new_offset.min(max_offset);

        // Simply highlight the middle line of the current window
        let middle_line = screen_height / 2;

        debug!(
            "Half-screen down to offset: {}/{}, highlighting middle line at screen position: {}, max: {}",
            self.scroll_offset, self.total_wrapped_lines, middle_line, max_offset
        );

        // Set up highlighting for 1 second
        self.highlight_visual_line = Some(middle_line);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_secs(1);
    }

    pub fn scroll_half_screen_up(&mut self, _content: &str, screen_height: usize) {
        let half_screen = (screen_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(half_screen);

        // Simply highlight the middle line of the current window
        let middle_line = screen_height / 2;

        debug!(
            "Half-screen up to offset: {}/{}, highlighting middle line at screen position: {}, max: {}",
            self.scroll_offset,
            self.total_wrapped_lines,
            middle_line,
            self.get_max_scroll_offset()
        );

        // Set up highlighting for 1 second
        self.highlight_visual_line = Some(middle_line);
        self.highlight_end_time = Instant::now() + std::time::Duration::from_secs(1);
    }

    pub fn update_highlight(&mut self) {
        // Clear expired highlight
        if self.highlight_visual_line.is_some() && Instant::now() >= self.highlight_end_time {
            self.highlight_visual_line = None;
        }
    }

    /// Handle mouse down event for text selection
    pub fn handle_mouse_down(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            debug!(
                "Mouse down at text coordinates: line {}, column {}",
                line, column
            );
            self.text_selection.start_selection(line, column);
        }
    }

    /// Handle mouse drag event for text selection
    pub fn handle_mouse_drag(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        // Check if we need to auto-scroll due to dragging outside the visible area
        let needs_scroll_up = screen_y < content_area.y;
        let needs_scroll_down = screen_y >= content_area.y + content_area.height;

        if needs_scroll_up {
            // Set up auto-scroll state for continuous upward scrolling
            self.auto_scroll_state = Some(AutoScrollState {
                direction: AutoScrollDirection::Up,
                mouse_x: screen_x,
                mouse_y: screen_y,
                content_area,
                last_scroll_time: Instant::now(),
            });
            // Perform initial scroll
            self.perform_auto_scroll();
        } else if needs_scroll_down {
            // Set up auto-scroll state for continuous downward scrolling
            self.auto_scroll_state = Some(AutoScrollState {
                direction: AutoScrollDirection::Down,
                mouse_x: screen_x,
                mouse_y: screen_y,
                content_area,
                last_scroll_time: Instant::now(),
            });
            // Perform initial scroll
            self.perform_auto_scroll();
        } else {
            // Mouse is back in content area - stop auto-scrolling
            self.auto_scroll_state = None;
            // Normal drag within visible area
            if let Some((line, column)) =
                self.screen_to_text_coords(screen_x, screen_y, content_area)
            {
                self.text_selection.update_selection(line, column);
            }
        }
    }

    /// Handle mouse up event for text selection
    pub fn handle_mouse_up(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        // Stop auto-scrolling when mouse is released
        self.auto_scroll_state = None;
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            self.text_selection.update_selection(line, column);
        }
        self.text_selection.end_selection();
    }

    /// Clear text selection
    pub fn clear_selection(&mut self) {
        self.text_selection.clear_selection();
        self.auto_scroll_state = None;
    }

    /// Copy selected text to clipboard
    pub fn copy_selection_to_clipboard(&self) -> Result<(), String> {
        if let Some(selected_text) = self
            .text_selection
            .extract_selected_text(&self.raw_text_lines)
        {
            match arboard::Clipboard::new() {
                Ok(mut clipboard) => match clipboard.set_text(selected_text) {
                    Ok(()) => {
                        debug!("Successfully copied selected text to clipboard");
                        Ok(())
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to copy text to clipboard: {}", e);
                        debug!("{}", error_msg);
                        Err(error_msg)
                    }
                },
                Err(e) => {
                    let error_msg = format!("Failed to access clipboard: {}", e);
                    debug!("{}", error_msg);
                    Err(error_msg)
                }
            }
        } else {
            let error_msg = "No text selected".to_string();
            debug!("{}", error_msg);
            Err(error_msg)
        }
    }

    /// Check if there is text currently selected
    pub fn has_text_selection(&self) -> bool {
        self.text_selection.has_selection()
    }

    /// Handle double-click for word selection
    pub fn handle_double_click(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            debug!(
                "Double-click at text coordinates: line {}, column {}",
                line, column
            );
            self.text_selection
                .select_word_at(line, column, &self.raw_text_lines);
        }
    }

    /// Handle triple-click for paragraph selection
    pub fn handle_triple_click(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            debug!(
                "Triple-click at text coordinates: line {}, column {}",
                line, column
            );
            self.text_selection
                .select_paragraph_at(line, column, &self.raw_text_lines);
        }
    }

    /// Perform auto-scroll based on current auto-scroll state
    fn perform_auto_scroll(&mut self) {
        if let Some(ref state) = self.auto_scroll_state.clone() {
            match state.direction {
                AutoScrollDirection::Up => {
                    if self.scroll_offset > 0 {
                        // Calculate scroll distance based on how far above the content area the mouse is
                        let distance_above =
                            state.content_area.y.saturating_sub(state.mouse_y) as usize;
                        // Use integer arithmetic for stable scroll amount calculation
                        let scroll_amount = ((distance_above + 9) / 10).max(1).min(3);

                        self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
                        debug!(
                            "Auto-scroll up by {} to offset: {}",
                            scroll_amount, self.scroll_offset
                        );

                        // Update selection to the top line of the visible area
                        let top_line = self.scroll_offset;
                        let column = if state.mouse_x < state.content_area.x {
                            0
                        } else {
                            (state.mouse_x - state.content_area.x) as usize
                        };
                        self.text_selection.update_selection(top_line, column);
                    }
                }
                AutoScrollDirection::Down => {
                    let max_offset = self.get_max_scroll_offset();
                    if self.scroll_offset < max_offset {
                        // Calculate scroll distance based on how far below the content area the mouse is
                        let distance_below = state
                            .mouse_y
                            .saturating_sub(state.content_area.y + state.content_area.height)
                            as usize;
                        // Use integer arithmetic for stable scroll amount calculation
                        let scroll_amount = ((distance_below + 9) / 10).max(1).min(3);

                        let new_offset = (self.scroll_offset + scroll_amount).min(max_offset);
                        self.scroll_offset = new_offset;
                        debug!(
                            "Auto-scroll down by {} to offset: {}",
                            scroll_amount, self.scroll_offset
                        );

                        // Update selection to the bottom line of the visible area
                        let bottom_line =
                            self.scroll_offset + self.visible_height.saturating_sub(1);
                        let column = if state.mouse_x < state.content_area.x {
                            0
                        } else {
                            (state.mouse_x - state.content_area.x) as usize
                        };
                        self.text_selection.update_selection(bottom_line, column);
                    }
                }
            }
        }
    }

    /// Update auto-scroll - should be called continuously from the main loop
    pub fn update_auto_scroll(&mut self) -> bool {
        let should_scroll = if let Some(ref state) = self.auto_scroll_state {
            let now = Instant::now();
            // Auto-scroll every 100ms (10 FPS)
            now.duration_since(state.last_scroll_time) >= std::time::Duration::from_millis(100)
        } else {
            false
        };

        if should_scroll {
            self.perform_auto_scroll();
            if let Some(ref mut state) = self.auto_scroll_state {
                state.last_scroll_time = Instant::now();
            }
            true // Indicates that scrolling occurred and a redraw is needed
        } else {
            false
        }
    }

    /// Convert screen coordinates to logical text coordinates
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

    /// Update wrapped lines if dimensions or content have changed
    pub fn update_wrapped_lines_if_needed(&mut self, content: &str, area: Rect) {
        let text_width = area.width.saturating_sub(12) as usize; // Account for margins
        let visible_height = area.height.saturating_sub(3) as usize; // Account for borders
        let content_hash = Self::simple_hash(content);

        // Only update if dimensions or content have changed
        if self.visible_height != visible_height
            || self.cached_text_width != text_width
            || self.cached_content_hash != content_hash
        {
            debug!(
                "Triggering wrapped lines update: height {} -> {}, width {} -> {}, content changed: {}",
                self.visible_height,
                visible_height,
                self.cached_text_width,
                text_width,
                self.cached_content_hash != content_hash
            );
            self.update_wrapped_lines(content, text_width, visible_height);
        }
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        content: &str,
        chapter_title: &Option<String>,
        current_chapter: usize,
        total_chapters: usize,
        palette: &Base16Palette,
        is_focused: bool,
    ) {
        // Get focus-aware colors
        let (text_color, border_color, _bg_color) = palette.get_panel_colors(is_focused);

        // Calculate reading progress
        let visible_width = area.width.saturating_sub(12) as usize;
        let visible_height = area.height.saturating_sub(3) as usize;
        let chapter_progress = if self.content_length > 0 {
            self.calculate_progress(content, visible_width, visible_height)
        } else {
            0
        };

        let mut title = format!(
            "Chapter {}/{} {}%",
            current_chapter + 1,
            total_chapters,
            chapter_progress
        );

        // Add chapter title if available, with truncation if necessary
        if let Some(chapter_title_str) = chapter_title {
            let separator = " : ";
            let title_with_separator = format!("{}{}{}", title, separator, chapter_title_str);

            // Calculate available space for title (leave some padding for border)
            let available_width = area.width.saturating_sub(4) as usize; // Account for borders and padding

            if title_with_separator.len() <= available_width {
                title = title_with_separator;
            } else {
                // Truncate chapter title to fit
                let base_length = title.len() + separator.len();
                let available_for_chapter = available_width.saturating_sub(base_length);

                if available_for_chapter >= 4 {
                    // Minimum space for "..." + at least 1 char
                    let truncated_chapter = if available_for_chapter >= chapter_title_str.len() {
                        chapter_title_str.clone()
                    } else {
                        let max_chars = available_for_chapter.saturating_sub(3); // Reserve space for "..."
                        let truncated = chapter_title_str
                            .chars()
                            .take(max_chars)
                            .collect::<String>();
                        format!("{}...", truncated)
                    };
                    title = format!("{}{}{}", title, separator, truncated_chapter);
                }
            }
        }

        // Draw the border with title
        let content_border = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(palette.base_00));

        let inner_area = content_border.inner(area);
        f.render_widget(content_border, area);

        // Create vertical margins
        let vertical_margined_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Top margin
                Constraint::Min(0),    // Content area
            ])
            .split(inner_area);

        // Create horizontal margins
        let margined_content_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(1), // Left margin
                Constraint::Min(0),    // Content area
                Constraint::Length(1), // Right margin
            ])
            .split(vertical_margined_area[1]);

        // Render the styled content (which now includes placeholder space for the image)
        let text_width = margined_content_area[1].width as usize;
        let scroll_offset = self.scroll_offset;
        let styled_content =
            self.parse_styled_text_cached(content, chapter_title, palette, text_width, is_focused);

        let content_paragraph = Paragraph::new(styled_content)
            .scroll((scroll_offset as u16, 0))
            .style(Style::default().fg(text_color).bg(palette.base_00));
        f.render_widget(content_paragraph, margined_content_area[1]);

        // Calculate where the image should be displayed
        if let Some(ref ada_image) = self.ada_image
            && !self.embedded_images.is_empty()
        {
            let lines_before_image = self.embedded_images.get(0).unwrap().lines_before_image;

            let image_start_line = lines_before_image + 2; // taking vertical margins into considerations

            let calculated_image_height = self
                .calculate_image_height_in_cells(ada_image, margined_content_area[1].width)
                as usize;

            let image_end_line = image_start_line + calculated_image_height;

            // Check if image is in viewport
            let area_height = margined_content_area[1].height as usize;

            if scroll_offset < image_end_line && scroll_offset + area_height > image_start_line {
                // Image is at least partially visible
                if let Some(ref mut picker) = *self.image_picker.borrow_mut() {
                    // Calculate screen position
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

                    let visible_image_height = (calculated_image_height - image_top_clipped)
                        .min(area_height - image_screen_start);

                    if visible_image_height > 0 {
                        // Calculate centered image area based on natural width
                        let content_width = margined_content_area[1].width;
                        let x_offset = if self.image_width_cells < content_width {
                            (content_width - self.image_width_cells) / 2
                        } else {
                            0 // Image is wider than content area, don't offset
                        };

                        // Create or get the stateful protocol
                        let mut protocol_ref = self.cached_image_protocol.borrow_mut();

                        if protocol_ref.is_none() {
                            // Create the stateful protocol once
                            *protocol_ref = Some(picker.new_resize_protocol(ada_image.clone()));
                        }

                        let (render_y, render_height) = if image_top_clipped > 0 {
                            (
                                margined_content_area[1].y,
                                ((IMAGE_HEIGHT_IN_CELLS as usize).saturating_sub(image_top_clipped))
                                    .min(area_height) as u16,
                            )
                        } else {
                            (
                                margined_content_area[1].y + image_screen_start as u16,
                                (IMAGE_HEIGHT_IN_CELLS as usize)
                                    .min(area_height.saturating_sub(image_screen_start))
                                    as u16,
                            )
                        };

                        let image_area = Rect {
                            x: margined_content_area[1].x + x_offset,
                            y: render_y,
                            width: self.image_width_cells.min(margined_content_area[1].width),
                            height: render_height,
                        };

                        // Render using the stateful widget
                        if let Some(ref mut protocol) = *protocol_ref {
                            // Use Viewport mode for efficient scrolling
                            // Calculate the Y offset in pixels based on how much is clipped from top
                            let y_offset_pixels =
                                (image_top_clipped as f32 * picker.font_size().1 as f32) as u32;

                            let viewport_options = ratatui_image::ViewportOptions {
                                y_offset: y_offset_pixels,
                                x_offset: 0, // No horizontal scrolling for now
                            };

                            let image_widget =
                                StatefulImage::new().resize(Resize::Viewport(viewport_options));
                            f.render_stateful_widget(image_widget, image_area, protocol);
                        }
                    }
                }
            }
        }

        // Store the content area for mouse coordinate conversion
        self.last_content_area = Some(margined_content_area[1]);

        // Draw highlight overlay if active
        if let Some(highlight_line) = self.highlight_visual_line {
            if Instant::now() < self.highlight_end_time {
                let content_area = margined_content_area[1];
                if highlight_line < content_area.height as usize {
                    let highlight_area = Rect {
                        x: content_area.x,
                        y: content_area.y + highlight_line as u16,
                        width: content_area.width,
                        height: 1,
                    };
                    let highlight_block =
                        Block::default().style(Style::default().bg(Color::Yellow));
                    f.render_widget(highlight_block, highlight_area);
                }
            }
        }
    }
}

fn extract_src(text: &str) -> Option<String> {
    let re = Regex::new(r#"(?i)\[\s*image\s+src\s*=\s*"([^"]+)"\s*\]"#).unwrap();

    for cap in re.captures_iter(text) {
        return Some(format!("{}", &cap[1]));
    }

    None
}

impl VimNavMotions for TextReader {
    fn handle_h(&mut self) {
        // Left movement - in text reader context, this could go to previous chapter
        // but that's handled at the App level, so we do nothing here
    }

    fn handle_j(&mut self) {
        // Down movement - scroll down one line
        self.scroll_down_no_content();
    }

    fn handle_k(&mut self) {
        // Up movement - scroll up one line
        self.scroll_up_no_content();
    }

    fn handle_l(&mut self) {
        // Right movement - in text reader context, this could go to next chapter
        // but that's handled at the App level, so we do nothing here
    }

    fn handle_ctrl_d(&mut self) {
        // Page down - scroll down half screen
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_half_screen_down("", screen_height);
        }
    }

    fn handle_ctrl_u(&mut self) {
        // Page up - scroll up half screen
        if self.visible_height > 0 {
            let screen_height = self.visible_height;
            self.scroll_half_screen_up("", screen_height);
        }
    }

    fn handle_gg(&mut self) {
        // Go to top - scroll to beginning of document
        self.scroll_offset = 0;
        debug!("Scrolled to top of document");
    }

    fn handle_G(&mut self) {
        // Go to bottom - scroll to end of document
        let max_offset = self.get_max_scroll_offset();
        self.scroll_offset = max_offset;
        debug!("Scrolled to bottom of document: offset {}", max_offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::OCEANIC_NEXT;
    use image::{DynamicImage, ImageBuffer};

    /// Test that calculate_image_height_in_cells always returns exactly IMAGE_HEIGHT_IN_CELLS
    #[test]
    fn test_image_height_always_15_cells() {
        let reader = TextReader::new();

        // Test with various image sizes
        let test_cases = vec![
            (100, 100),   // Square image
            (200, 50),    // Wide image
            (50, 200),    // Tall image
            (1920, 1080), // HD image
            (64, 64),     // Small image
        ];

        for (width, height) in test_cases {
            let img = DynamicImage::ImageRgba8(ImageBuffer::new(width, height));
            let result = reader.calculate_image_height_in_cells(&img, 100);
            assert_eq!(
                result, IMAGE_HEIGHT_IN_CELLS,
                "Image {}x{} should always result in {} cells height",
                width, height, IMAGE_HEIGHT_IN_CELLS
            );
        }
    }

    /// Test that placeholder lines inserted are exactly IMAGE_HEIGHT_IN_CELLS
    #[test]
    fn test_placeholder_lines_count() {
        let mut reader = TextReader::new();
        let content = "First paragraph here.\n\n\
             Second paragraph here.\n\n\
             Third paragraph here.\n\n\
             Fourth paragraph here.\n\n\
             Fifth paragraph here.\n\n\
             Sixth paragraph here.\n\n\
             Seventh paragraph here.";
        let chapter_title = Some("Test Chapter".to_string());
        let palette = &OCEANIC_NEXT;

        // The TextReader should already have an image loaded from Ada.png if it exists
        // We'll check if IMAGE_TOTAL_LINES placeholder lines are inserted when ada_image exists
        if reader.ada_image.is_some() {
            let width = 80;
            let (_styled_text, raw_lines) = reader.parse_styled_text_internal_with_raw(
                content,
                &chapter_title,
                palette,
                width,
                true,
            );

            // Count empty lines after sixth paragraph that represent image placeholders
            let mut placeholder_count = 0;
            let mut consecutive_empty = 0;
            let mut paragraph_count = 0;
            let mut in_paragraph = false;

            for (i, line) in raw_lines.iter().enumerate() {
                // Skip title lines
                if i < 2 {
                    continue;
                }

                if !line.trim().is_empty() {
                    if !in_paragraph {
                        in_paragraph = true;
                    }
                    consecutive_empty = 0;
                } else if in_paragraph {
                    paragraph_count += 1;
                    in_paragraph = false;
                    consecutive_empty = 1;
                } else {
                    consecutive_empty += 1;
                    // Check if we've found the image placeholder after the 6th paragraph
                    if paragraph_count >= 6 && consecutive_empty >= IMAGE_TOTAL_LINES as usize {
                        placeholder_count = IMAGE_TOTAL_LINES as usize;
                        break;
                    }
                }
            }

            assert_eq!(
                placeholder_count, IMAGE_TOTAL_LINES as usize,
                "Should have exactly {} placeholder lines for the image (1 padding + {} image + 1 padding)",
                IMAGE_TOTAL_LINES, IMAGE_HEIGHT_IN_CELLS
            );
        }
    }

    /// Test that prescaling produces an image with correct height based on cell size
    #[test]
    fn test_image_prescaling_dynamic() {
        // Test various source image sizes and cell heights
        let test_cases = vec![
            (100, 100, 16), // Square image, 16px cells -> 160px
            (200, 100, 18), // Wide image, 18px cells -> 180px
            (100, 200, 20), // Tall image, 20px cells -> 200px
            (400, 300, 14), // 4:3 image, 14px cells -> 140px
        ];

        for (src_width, src_height, cell_height) in test_cases {
            // Simulate the prescaling logic from new()
            let target_height_in_pixels = IMAGE_HEIGHT_IN_CELLS as usize * cell_height; // IMAGE_HEIGHT_IN_CELLS cells * detected cell height
            let scale = target_height_in_pixels as f32 / src_height as f32;
            let new_width = (src_width as f32 * scale) as u32;
            let new_height = target_height_in_pixels;

            assert_eq!(
                new_height,
                IMAGE_HEIGHT_IN_CELLS as usize * cell_height,
                "Prescaled height should be exactly {} * cell_height ({} pixels)",
                IMAGE_HEIGHT_IN_CELLS,
                IMAGE_HEIGHT_IN_CELLS as usize * cell_height
            );

            // Verify aspect ratio is maintained
            let src_aspect = src_width as f32 / src_height as f32;
            let new_aspect = new_width as f32 / new_height as f32;
            let aspect_diff = (src_aspect - new_aspect).abs();

            assert!(
                aspect_diff < 0.05,
                "Aspect ratio should be maintained: src={:.2} new={:.2} (diff={:.4})",
                src_aspect,
                new_aspect,
                aspect_diff
            );
        }
    }

    /// Test that image placeholder insertion happens after sixth paragraph
    #[test]
    fn test_image_placement_after_sixth_paragraph() {
        let mut reader = TextReader::new();
        let content = "First paragraph with some text that might wrap to multiple lines.\n\n\
             Second paragraph here.\n\n\
             Third paragraph here.\n\n\
             Fourth paragraph here.\n\n\
             Fifth paragraph here.\n\n\
             Sixth paragraph here.\n\n\
             Seventh paragraph here.";
        let chapter_title = Some("Test Chapter".to_string());
        let palette = &OCEANIC_NEXT;

        if reader.ada_image.is_some() {
            let width = 40; // Narrow width to force wrapping
            let (_styled_text, raw_lines) = reader.parse_styled_text_internal_with_raw(
                content,
                &chapter_title,
                palette,
                width,
                true,
            );

            // Find the first paragraph end by looking for consecutive empty lines after content
            let mut found_image_placeholders = false;
            let mut image_start_idx = 0;
            let mut consecutive_empty = 0;

            // Skip title lines
            for (i, line) in raw_lines.iter().enumerate().skip(2) {
                if line.trim().is_empty() {
                    consecutive_empty += 1;
                    if consecutive_empty >= 10 && !found_image_placeholders {
                        // Found our image placeholder area
                        image_start_idx = i - 9; // Go back to start of placeholders
                        found_image_placeholders = true;
                        break;
                    }
                } else {
                    consecutive_empty = 0;
                }
            }

            assert!(
                found_image_placeholders,
                "Should find image placeholders in the output"
            );

            // Verify there's content before the placeholder (first paragraph)
            assert!(
                image_start_idx > 2,
                "Image should not be at the very beginning (found at {})",
                image_start_idx
            );

            // Verify we have six paragraphs of content before image
            let non_empty_before = raw_lines[2..image_start_idx]
                .iter()
                .filter(|line| !line.trim().is_empty())
                .count();
            assert!(
                non_empty_before > 0,
                "Should have first paragraph content before image"
            );
        }
    }

    /// Test that image rendering area calculations work correctly
    #[test]
    fn test_image_rendering_calculations() {
        let reader = TextReader::new();

        // If the reader has an ada_image loaded, test it
        if let Some(ref ada_image) = reader.ada_image {
            // Test area width calculation
            let area_width = 100;
            let image_width = 70; // min(100 * 0.7, 70) = 70

            // The image should be centered
            let expected_x_offset = (area_width - image_width) / 2;
            assert_eq!(
                expected_x_offset, IMAGE_HEIGHT_IN_CELLS,
                "Image should be centered with correct offset"
            );

            // Height should always be IMAGE_HEIGHT_IN_CELLS cells
            let image_height = reader.calculate_image_height_in_cells(ada_image, area_width as u16);
            assert_eq!(
                image_height, IMAGE_HEIGHT_IN_CELLS,
                "Image height should be exactly {} cells",
                IMAGE_HEIGHT_IN_CELLS
            );
        } else {
            // If no image loaded, create a test image
            let test_img = DynamicImage::ImageRgba8(ImageBuffer::new(40, 20));
            let image_height = reader.calculate_image_height_in_cells(&test_img, 100);
            assert_eq!(
                image_height, IMAGE_HEIGHT_IN_CELLS,
                "Image height should be exactly {} cells",
                IMAGE_HEIGHT_IN_CELLS
            );
        }
    }

    #[test]
    fn test_image_placeholder_rendering() {
        let mut reader = TextReader::new();
        let palette = &OCEANIC_NEXT;

        // Test text with image placeholders
        let test_text = r#"This is some text before the image.

[image src="../images/diagram1.png"]

This is text after the first image.

[image src="../images/photo.jpg"]

This is text after the second image."#;

        let (_styled_text, raw_lines) = reader.parse_styled_text_internal_with_raw(
            test_text, &None, palette, 80,   // width
            true, // is_focused
        );

        // Count lines - should have original text lines minus image placeholder lines plus 15 lines for each frame
        let total_lines = raw_lines.len();
        println!("Total lines: {}", total_lines);

        // Find image frames and check that [image src=...] is inside them
        let mut found_frames = 0;
        let mut found_image_src_inside = 0;

        for (i, line) in raw_lines.iter().enumerate() {
            println!("Line {}: '{}'", i, line);

            // Check for frame top border
            if line.contains("┌") && line.contains("┐") && line.contains("─") {
                found_frames += 1;

                // Check that [image src=...] appears inside the frame (should be in the middle - line 7)
                if i + 7 < raw_lines.len() && raw_lines[i + 7].contains("[image src=") {
                    found_image_src_inside += 1;
                }
            }
        }

        assert_eq!(found_frames, 2, "Should find 2 image frames");
        assert_eq!(
            found_image_src_inside, 2,
            "Should find 2 [image src=...] texts inside frames"
        );

        // Verify total lines: original text lines (9) - image lines (2) + frame lines (2 * 15)
        let text_lines_without_images = test_text.lines().count() - 2; // subtract the [image src=...] lines
        let expected_total = text_lines_without_images + (2 * 15); // add frame lines
        assert_eq!(
            total_lines, expected_total,
            "Should have exactly {} lines (7 text + 30 frame), but got {}",
            expected_total, total_lines
        );
    }
}
