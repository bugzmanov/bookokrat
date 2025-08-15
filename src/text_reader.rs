use crate::image_placeholder::{ImagePlaceholder, ImagePlaceholderConfig};
use crate::main_app::VimNavMotions;
use crate::text_selection::TextSelection;
use crate::theme::Base16Palette;
use fast_image_resize as fr;
use image::{DynamicImage, GenericImageView, ImageBuffer};
use log::{debug, info, warn};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};
use ratatui_image::{Resize, StatefulImage, picker::Picker, protocol::StatefulProtocol};
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Instant;
use textwrap;

/// Height for regular images in terminal cells
const IMAGE_HEIGHT_REGULAR: u16 = 15;
/// Height for wide images (aspect ratio > 3:1) in terminal cells
const IMAGE_HEIGHT_WIDE: u16 = 7;
/// Aspect ratio threshold for wide images
const WIDE_IMAGE_ASPECT_RATIO: f32 = 3.0;

/// Fast image resize using fast_image_resize crate
fn fast_resize_image(
    src_image: &DynamicImage,
    new_width: u32,
    new_height: u32,
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let resize_start = std::time::Instant::now();

    // Convert to RGBA8 for processing
    let src_rgba = src_image.to_rgba8();
    let (src_width, src_height) = src_rgba.dimensions();

    // Create source image view
    let src_image_view = fr::Image::from_vec_u8(
        std::num::NonZeroU32::new(src_width).ok_or("Invalid width")?,
        std::num::NonZeroU32::new(src_height).ok_or("Invalid height")?,
        src_rgba.into_raw(),
        fr::PixelType::U8x4,
    )?;

    // Create destination image
    let dst_width = std::num::NonZeroU32::new(new_width).ok_or("Invalid target width")?;
    let dst_height = std::num::NonZeroU32::new(new_height).ok_or("Invalid target height")?;
    let mut dst_image = fr::Image::new(dst_width, dst_height, fr::PixelType::U8x4);

    // Create resizer with Lanczos3 algorithm for quality
    let mut resizer = fr::Resizer::new(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3));

    // Perform resize
    resizer.resize(&src_image_view.view(), &mut dst_image.view_mut())?;

    // Convert back to DynamicImage
    let buffer = ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
        new_width,
        new_height,
        dst_image.into_vec(),
    )
    .ok_or("Failed to create image buffer")?;

    let resize_time = resize_start.elapsed();
    debug!(
        "        Fast resize: {}x{} -> {}x{} in {:?}",
        src_width, src_height, new_width, new_height, resize_time
    );

    Ok(DynamicImage::ImageRgba8(buffer))
}

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
    pub height_cells: u16,
    pub width: u32,
    pub height: u32,
}

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
}

/// Status of image loading for background processing
#[derive(Clone, Debug)]
pub enum ImageLoadStatus {
    Placeholder {
        width: u32,
        height: u32,
        height_cells: u16,
    },
    Loaded {
        image: Arc<DynamicImage>,
        width_cells: u16,
        height_cells: u16,
    },
    Failed,
}

/// Message sent from background thread when images are loaded
pub struct ImagesLoadedMessage {
    pub images: HashMap<String, (DynamicImage, u16, u16)>, // src -> (image, width_cells, height_cells)
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
    image_picker: RefCell<Option<Picker>>,
    // Cached stateful image protocol
    cached_image_protocol: RefCell<Option<StatefulProtocol>>,
    embedded_images: RefCell<HashMap<String, EmbeddedImage>>,
    // Cache for loaded images to avoid reloading on every frame
    cached_images: RefCell<HashMap<String, (DynamicImage, u16, u16)>>, // Map image_src -> (loaded_image, image_width_cells, image_height_cells)
    cached_protocols: RefCell<HashMap<String, StatefulProtocol>>,      // Map image_src -> protocol
    // Image loading status for fast placeholder display
    image_load_status: RefCell<HashMap<String, ImageLoadStatus>>,
    // Channel for receiving loaded images from background thread
    image_receiver: Option<Receiver<ImagesLoadedMessage>>,
    // Track if background loading is in progress
    loading_in_progress: RefCell<bool>,
}

impl TextReader {
    pub fn new() -> Self {
        // Create image picker first to get cell dimensions
        let (image_picker, _detected_cell_height) = match Picker::from_query_stdio() {
            Ok(mut picker) => {
                picker.set_background_color([0, 0, 0, 0]);

                let font_size = picker.font_size();
                debug!(
                    "Successfully created image picker, detected font size: {:?}",
                    font_size
                );

                let cell_height = font_size.1;

                (Some(picker), cell_height)
            }
            Err(e) => {
                debug!("Failed to create image picker: {}", e);
                (None, 16)
            }
        };

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
            image_picker: RefCell::new(image_picker),
            cached_image_protocol: RefCell::new(None),
            embedded_images: RefCell::new(HashMap::new()),
            cached_images: RefCell::new(HashMap::new()),
            cached_protocols: RefCell::new(HashMap::new()),
            image_load_status: RefCell::new(HashMap::new()),
            image_receiver: None,
            loading_in_progress: RefCell::new(false),
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

    fn calculate_image_height_in_cells(&self, image: &DynamicImage) -> u16 {
        let (width, height) = image.dimensions();
        EmbeddedImage::height_in_cells(width, height)
    }

    /// Calculate total wrapped lines for the given content and width
    pub fn update_wrapped_lines(&mut self, content: &str, width: usize, visible_height: usize) {
        self.visible_height = visible_height;
        self.cached_text_width = width;
        self.cached_content_hash = Self::simple_hash(content);

        let mut total_lines = 0;
        let mut image_count = 0;

        // Add lines for chapter title (title + empty line)
        total_lines += 2;

        // Wrap each line of content
        for line in content.lines() {
            let is_empty = line.trim().is_empty();

            // Check if this line is an image placeholder
            let is_image_placeholder = line.trim().starts_with("[image src=");

            if is_image_placeholder {
                // For now, assume regular height - actual height will be determined when image is loaded
                // We use regular height as default for line counting
                total_lines += IMAGE_HEIGHT_REGULAR as usize;
                image_count += 1;
            } else if is_empty {
                total_lines += 1;
            } else {
                let wrapped_lines = textwrap::wrap(line, width);
                total_lines += wrapped_lines.len();
            }
        }

        self.total_wrapped_lines = total_lines;
        debug!(
            "Updated wrapped lines: {} total (including {} images), {} visible, width: {}",
            total_lines, image_count, visible_height, width
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

        // Calculate line count for image placement tracking
        let mut line_count = 0;

        // Process each line with manual wrapping
        for (_i, line) in text.lines().enumerate() {
            let testline_is_empty = line.trim().is_empty();
            let is_image_placeholder = line.trim().starts_with("[image src=");

            if is_image_placeholder {
                if let Some(image_src) = extract_src(line.trim()) {
                    if let Some(image) = self.embedded_images.borrow_mut().get_mut(&image_src) {
                        // just resizing
                        image.lines_before_image = line_count;
                        line_count += image.height_cells as usize + 2;
                    } else {
                        if let Some((width, height)) = TextReader::get_image_dimensions(self.) {
                            let height_cells = EmbeddedImage::height_in_cells(width, height);
                            self.embedded_images.borrow_mut().insert(
                                image_src.clone(),
                                EmbeddedImage {
                                    src: image_src.clone(),
                                    lines_before_image: line_count,
                                    height_cells,
                                    width,
                                    height
                                },
                            );
                        }
                    }

                    let placeholder_height = self.embedded_images.borrow().get(&image_src).unwrap().height_cells;

                    let config = ImagePlaceholderConfig {
                        internal_padding: 4,
                        total_height: placeholder_height as usize,
                        border_color: palette.base_03,
                    };

                    let placeholder = ImagePlaceholder::new(&image_src, width, &config);
                    let placeholder_line_count = placeholder.raw_lines.len();

                    // Add all the placeholder lines
                    for (raw_line, styled_line) in placeholder
                        .raw_lines
                        .into_iter()
                        .zip(placeholder.styled_lines.into_iter())
                    {
                        raw_lines.push(raw_line);
                        lines.push(styled_line);
                    }

                    line_count += placeholder_line_count; // Use actual placeholder height
                } else if testline_is_empty {
                    // Normal empty line processing
                    raw_lines.push(String::new());
                    lines.push(Line::from(String::new()));
                    line_count += 1;
                } else {
                    // Wrap the line using textwrap
                    let wrapped_lines = textwrap::wrap(line, width);
                    for wrapped_line in wrapped_lines {
                        let line_str = wrapped_line.to_string();
                        raw_lines.push(line_str.clone());
                        let styled_line =
                            self.parse_line_styling_owned(&line_str, palette, is_focused);
                        lines.push(styled_line);
                        line_count += 1;
                    }
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
            .map(|line| (line.len() + visible_width - 1) / visible_width)
            .sum::<usize>();

        let max_scroll = if total_lines > visible_height {
            total_lines - visible_height
        } else {
            0
        };

        if max_scroll > 0 {
            let progress = (self.scroll_offset * 100) / max_scroll;
            progress.min(100) as u32
        } else {
            100
        }
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
        self.embedded_images.borrow_mut().clear();
        // Clear image caches for multiple images
        self.cached_images.borrow_mut().clear();
        self.cached_protocols.borrow_mut().clear();
        // Clear old single-image caches
        *self.cached_image_protocol.borrow_mut() = None;
    }

    pub fn get_image_dimensions(
        image_storage: &crate::image_storage::ImageStorage,
        epub_path: &std::path::Path,
        img_src: &str,
    ) -> Option<(u32, u32)> {
        // width, height
        if let Some(resolved_image_path) = image_storage.resolve_image_path(epub_path, &img_src) {
            match imagesize::size(&resolved_image_path) {
                Ok(size) => Some((size.width as u32, size.height as u32)),
                Err(e) => {
                    warn!(
                        "Failed to read image size for {}: {}",
                        resolved_image_path.display(),
                        e
                    );
                    None
                }
            }
        } else {
            warn!(
                "Image {}/{} wasn't resolved by ImageStorage. And would not be loaded",
                epub_path.display(),
                img_src
            );
            None
        }
    }

    /// Quickly read image sizes and start background loading
    pub fn preload_image_dimensions(
        &mut self,
        content: &str,
        image_storage: &crate::image_storage::ImageStorage,
        current_file_path: &str,
    ) {
        let quick_scan_start = std::time::Instant::now();
        let epub_path = std::path::Path::new(current_file_path);
        let mut image_load_status = self.image_load_status.borrow_mut();
        let mut images_to_load = Vec::new();
        let mut images_processed = 0;

        // Clear previous load status and stop any pending background loading
        image_load_status.clear();
        *self.loading_in_progress.borrow_mut() = false;

        // First pass: quickly read image dimensions using imagesize
        for line in content.lines() {
            if line.trim().starts_with("[image src=") {
                if let Some(img_src) = extract_src(line.trim()) {
                    images_processed += 1;

                    // Skip if already fully loaded
                    if self.cached_images.borrow().contains_key(&img_src) {
                        debug!("Image '{}' already cached, skipping", img_src);
                        continue;
                    }

                    // Try to resolve image path
                    if let Some(resolved_image_path) =
                        image_storage.resolve_image_path(epub_path, &img_src)
                    {
                        // Use imagesize to quickly get dimensions without loading full image
                        match imagesize::size(&resolved_image_path) {
                            Ok(size) => {
                                let img_width = size.width as u32;
                                let img_height = size.height as u32;
                                let aspect_ratio = img_width as f32 / img_height as f32;

                                debug!(
                                    "Quick scan: Image {} dimensions: {}x{} pixels, aspect ratio: {:.2}",
                                    img_src, img_width, img_height, aspect_ratio
                                );

                                // Determine placeholder height based on aspect ratio and size
                                let height_cells = if aspect_ratio > WIDE_IMAGE_ASPECT_RATIO {
                                    IMAGE_HEIGHT_WIDE
                                } else if img_height < 150 {
                                    IMAGE_HEIGHT_WIDE
                                } else {
                                    IMAGE_HEIGHT_REGULAR
                                };

                                // Store placeholder status with dimensions
                                image_load_status.insert(
                                    img_src.clone(),
                                    ImageLoadStatus::Placeholder {
                                        width: img_width,
                                        height: img_height,
                                        height_cells,
                                    },
                                );

                                // Add to list for background loading
                                images_to_load.push((
                                    img_src.clone(),
                                    resolved_image_path.clone(),
                                    height_cells,
                                ));
                            }
                            Err(e) => {
                                warn!("Failed to read image size for {}: {}", img_src, e);
                                // Mark as failed so we don't try again
                                image_load_status.insert(img_src.clone(), ImageLoadStatus::Failed);
                            }
                        }
                    } else {
                        warn!("Could not resolve image path for: {}", img_src);
                        image_load_status.insert(img_src.clone(), ImageLoadStatus::Failed);
                    }
                }
            }
        }

        // Report quick scan time
        let quick_scan_time = quick_scan_start.elapsed();
        if images_processed > 0 {
            info!(
                "      - Quick image scan: {} images found, dimensions read in {:?}",
                images_processed, quick_scan_time
            );
        }

        // Clear the styled content cache so placeholders are rendered with correct heights
        self.cached_styled_content = None;
        self.cached_content_hash = 0;

        // Must drop the borrow before calling start_background_image_loading
        drop(image_load_status);

        // Start background loading if we have images to load
        if !images_to_load.is_empty() {
            self.start_background_image_loading(images_to_load);
        }
    }

    /// Start loading and scaling images in a background thread
    fn start_background_image_loading(&mut self, images_to_load: Vec<(String, PathBuf, u16)>) {
        *self.loading_in_progress.borrow_mut() = true;

        // Create channel for receiving loaded images
        let (sender, receiver) = channel();
        self.image_receiver = Some(receiver);

        // Get cell dimensions from picker for scaling
        let (cell_width, cell_height) = if let Some(ref picker) = *self.image_picker.borrow() {
            let font_size = picker.font_size();
            (font_size.0, font_size.1)
        } else {
            (8, 16) // Default fallback
        };

        // Spawn background thread for loading and scaling
        thread::spawn(move || {
            let mut loaded_images = HashMap::new();
            let load_start = std::time::Instant::now();

            for (img_src, path, height_cells) in images_to_load {
                debug!("Background loading: {}", img_src);

                match image::open(&path) {
                    Ok(loaded_image) => {
                        let (img_width, img_height) = loaded_image.dimensions();

                        // Calculate target dimensions for scaling
                        let target_height_in_pixels = height_cells as u32 * cell_height as u32;
                        let scale = target_height_in_pixels as f32 / img_height as f32;
                        let new_width = (img_width as f32 * scale) as u32;
                        let new_height = target_height_in_pixels;

                        debug!(
                            "Background scaling {} from {}x{} to {}x{}",
                            img_src, img_width, img_height, new_width, new_height
                        );

                        // Scale the image
                        let scaled_image = fast_resize_image(&loaded_image, new_width, new_height)
                            .unwrap_or_else(|e| {
                                debug!("Fast resize failed, using fallback: {}", e);
                                loaded_image.resize_exact(
                                    new_width,
                                    new_height,
                                    image::imageops::FilterType::Lanczos3,
                                )
                            });

                        // Calculate width in cells
                        let width_cells = (new_width as f32 / cell_width as f32).ceil() as u16;

                        loaded_images
                            .insert(img_src.clone(), (scaled_image, width_cells, height_cells));
                    }
                    Err(e) => {
                        warn!("Failed to load image {}: {}", img_src, e);
                    }
                }
            }

            let load_time = load_start.elapsed();
            info!(
                "Background image loading complete: {} images loaded and scaled in {:?}",
                loaded_images.len(),
                load_time
            );

            // Send the loaded images back to the main thread
            let _ = sender.send(ImagesLoadedMessage {
                images: loaded_images,
            });
        });
    }

    /// Check for loaded images from background thread and update caches
    /// Returns true if images were loaded and a redraw is needed
    pub fn check_for_loaded_images(&mut self) -> bool {
        if let Some(ref receiver) = self.image_receiver {
            // Try to receive without blocking
            if let Ok(message) = receiver.try_recv() {
                debug!(
                    "Received {} loaded images from background thread",
                    message.images.len()
                );

                // Update our caches with the loaded images
                let mut cached_images = self.cached_images.borrow_mut();
                let mut image_load_status = self.image_load_status.borrow_mut();

                for (img_src, (image, width_cells, height_cells)) in message.images {
                    // Update cache
                    cached_images
                        .insert(img_src.clone(), (image.clone(), width_cells, height_cells));

                    // Update status
                    image_load_status.insert(
                        img_src,
                        ImageLoadStatus::Loaded {
                            image: Arc::new(image),
                            width_cells,
                            height_cells,
                        },
                    );
                }

                // Clear styled content cache to trigger re-render with actual images
                self.cached_styled_content = None;
                self.cached_content_hash = 0;

                // Mark loading as complete
                *self.loading_in_progress.borrow_mut() = false;
                self.image_receiver = None;

                // Return true to indicate images were loaded and redraw is needed
                return true;
            }
        }
        false
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
        image_storage: Option<&crate::image_storage::ImageStorage>,
        current_file_path: Option<&str>,
    ) {
        // Check for loaded images from background thread
        self.check_for_loaded_images();
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

        // Display all embedded images from the book
        if !self.embedded_images.borrow().is_empty()
            && image_storage.is_some()
            && current_file_path.is_some()
            && self.image_picker.borrow().is_some()
        {
            let _image_storage = image_storage.unwrap();
            let _current_file_path = current_file_path.unwrap();
            let _epub_path = std::path::Path::new(_current_file_path);
            let area_height = margined_content_area[1].height as usize;

            // Iterate through all embedded images
            for (_, embedded_image) in self.embedded_images.borrow().iter() {
                let lines_before_image = embedded_image.lines_before_image;
                let image_src = &embedded_image.src;
                // The image should render at the exact position where placeholder starts
                // lines_before_image is where the placeholder begins in the text
                let image_start_line = lines_before_image;

                // We need to determine the image height - default to regular for now
                // The actual height will be determined when the image is loaded
                let mut calculated_image_height = IMAGE_HEIGHT_REGULAR as usize;

                // Check if image is already cached to get its actual height
                if let Some((_, _, height_cells)) = self.cached_images.borrow().get(image_src) {
                    calculated_image_height = *height_cells as usize;
                }

                let image_end_line = image_start_line + calculated_image_height;

                // Check if image is in viewport
                if scroll_offset < image_end_line && scroll_offset + area_height > image_start_line
                {
                    // Image is at least partially visible
                    let cached_images_ref = self.cached_images.borrow_mut();
                    let mut cached_protocols_ref = self.cached_protocols.borrow_mut();

                    // Check if we have the image cached (either from preload or background loading)
                    if !cached_images_ref.contains_key(image_src) {
                        // Image not loaded yet - skip rendering it (placeholder is shown in text)
                        // debug!("Image {} not loaded yet, showing placeholder", image_src);
                        continue;
                    }

                    // Get the cached image
                    if let Some((scaled_image, _image_width_cells, _height_cells)) =
                        cached_images_ref.get(image_src)
                    {
                        debug!("here");
                        // Image is at least partially visible
                        if let Some(ref mut picker) = *self.image_picker.borrow_mut() {
                            // Calculate screen position
                            debug!("here2");
                            // Calculate where the image should appear on screen
                            // The placeholder is at line 'lines_before_image' in the text
                            // After scrolling, it appears at (lines_before_image - scroll_offset) on screen
                            let image_screen_start = if scroll_offset > lines_before_image {
                                0
                            } else {
                                lines_before_image - scroll_offset
                            };

                            // Calculate visible portion
                            let image_top_clipped = if scroll_offset > lines_before_image {
                                scroll_offset - lines_before_image
                            } else {
                                0
                            };

                            let visible_image_height = (calculated_image_height
                                - image_top_clipped)
                                .min(area_height - image_screen_start);

                            if visible_image_height > 0 {
                                debug!(
                                    "here3 - visible_image_height: {}, image_screen_start: {}, image_top_clipped: {}",
                                    visible_image_height, image_screen_start, image_top_clipped
                                );
                                // Don't center the image - use full width like the placeholder

                                // Create the stateful protocol only if not cached
                                if !cached_protocols_ref.contains_key(image_src) {
                                    debug!(
                                        "Creating new protocol for {} with picker font_size: {:?}",
                                        image_src,
                                        picker.font_size()
                                    );
                                    let protocol = picker.new_resize_protocol(scaled_image.clone());
                                    cached_protocols_ref.insert(image_src.to_string(), protocol);
                                    debug!("Created new image protocol for {}", image_src);
                                }

                                // Get the actual image height for this specific image
                                let image_height_cells = self.calculate_image_height_in_cells(
                                    scaled_image
                                );

                                let (render_y, render_height) = if image_top_clipped > 0 {
                                    (
                                        margined_content_area[1].y,
                                        ((image_height_cells as usize)
                                            .saturating_sub(image_top_clipped))
                                        .min(area_height)
                                            as u16,
                                    )
                                } else {
                                    (
                                        margined_content_area[1].y + image_screen_start as u16,
                                        (image_height_cells as usize)
                                            .min(area_height.saturating_sub(image_screen_start))
                                            as u16,
                                    )
                                };

                                let image_area = Rect {
                                    x: margined_content_area[1].x,
                                    y: render_y,
                                    width: margined_content_area[1].width, // Use full width like placeholder
                                    height: render_height,
                                };

                                // Render using the stateful widget
                                // Use Viewport mode for efficient scrolling
                                let current_font_size = picker.font_size();
                                let y_offset_pixels =
                                    (image_top_clipped as f32 * current_font_size.1 as f32) as u32;

                                let viewport_options = ratatui_image::ViewportOptions {
                                    y_offset: y_offset_pixels,
                                    x_offset: 0, // No horizontal scrolling for now
                                };

                                // Use the cached protocol
                                if let Some(protocol) = cached_protocols_ref.get_mut(image_src) {
                                    let image_widget = StatefulImage::new()
                                        .resize(Resize::Viewport(viewport_options));
                                    debug!(
                                        "Rendering image at area: {:?}, scroll_offset: {}, image_start_line: {}, lines_before_image: {}",
                                        image_area,
                                        scroll_offset,
                                        image_start_line,
                                        lines_before_image
                                    );
                                    f.render_stateful_widget(image_widget, image_area, protocol);
                                    // match protocol.protocol_type() {
                                    //     StatefulProtocolType::Kitty(kitty) => {
                                    //         debug!(
                                    //             "here4 {:?}",
                                    //             kitty.tiled_data.clone().unwrap().tiles.values()
                                    //         )
                                    //         // Access StatefulKitty here
                                    //         // kitty is &StatefulKitty
                                    //     }
                                    //     _ => {
                                    //         // Handle other protocols
                                    //     }
                                    // }
                                }
                            }
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

impl TextReader {
    pub fn handle_terminal_resize(&mut self) {
        debug!("Handling terminal resize in TextReader");

        // Clear embedded images cache since text wrapping will change
        self.embedded_images.borrow_mut().clear();
        debug!("Cleared embedded_images cache due to resize");

        // Clear image caches for multiple images
        self.cached_images.borrow_mut().clear();
        self.cached_protocols.borrow_mut().clear();
        // Clear old single-image caches
        *self.cached_image_protocol.borrow_mut() = None;
        debug!("Cleared all image caches due to resize");

        // Clear the cached styled content to force re-wrapping
        self.cached_styled_content = None;
        self.cached_content_hash = 0;
        self.cached_styled_width = 0;
        debug!("Cleared text content cache to force re-wrapping");

        // Recreate the image picker to get updated font size
        match Picker::from_query_stdio() {
            Ok(mut picker) => {
                // Set transparent background like in the constructor
                picker.set_background_color([0, 0, 0, 0]);

                let font_size = picker.font_size();
                debug!("Detected new font size after resize: {:?}", font_size);

                // Update the picker
                *self.image_picker.borrow_mut() = Some(picker);

                // Clear cached image protocol since it depends on old font size
                *self.cached_image_protocol.borrow_mut() = None;

                // Clear cached image to force re-scaling with new font size
                // This field doesn't exist anymore, we use cached_images instead

                debug!("Image picker and caches updated for new font size");
            }
            Err(e) => {
                debug!("Failed to recreate image picker on resize: {}", e);
                // Keep the existing picker if we can't create a new one
            }
        }
    }
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

    /// Test that calculate_image_height_in_cells returns correct height based on aspect ratio
    #[test]
    fn test_image_height_based_on_aspect_ratio() {
        let reader = TextReader::new();

        // Test with various image sizes and their expected heights
        let test_cases = vec![
            (100, 100, IMAGE_HEIGHT_REGULAR), // Square image (aspect ratio 1.0)
            (200, 50, IMAGE_HEIGHT_WIDE),     // Wide image (aspect ratio 4.0)
            (50, 200, IMAGE_HEIGHT_REGULAR),  // Tall image (aspect ratio 0.25)
            (1920, 1080, IMAGE_HEIGHT_REGULAR), // HD image (aspect ratio ~1.78)
            (64, 64, IMAGE_HEIGHT_REGULAR),   // Small square image
            (400, 100, IMAGE_HEIGHT_WIDE),    // Wide image (aspect ratio 4.0)
            (300, 100, IMAGE_HEIGHT_REGULAR), // Almost wide (aspect ratio 3.0, at threshold)
            (301, 100, IMAGE_HEIGHT_WIDE),    // Just over threshold (aspect ratio 3.01)
        ];

        for (width, height, expected_height) in test_cases {
            let img = DynamicImage::ImageRgba8(ImageBuffer::new(width, height));
            let result = reader.calculate_image_height_in_cells(&img);
            let aspect_ratio = width as f32 / height as f32;
            assert_eq!(
                result, expected_height,
                "Image {}x{} (aspect ratio {:.2}) should result in {} cells height",
                width, height, aspect_ratio, expected_height
            );
        }
    }

    /// Test that placeholder lines are inserted with correct height
    #[test]
    fn test_placeholder_lines_count() {
        // This test is kept for reference but the functionality for automatic
        // placeholder insertion after 6th paragraph is disabled since we show actual images from books
    }

    /// Test that prescaling produces an image with correct height based on cell size and aspect ratio
    #[test]
    fn test_image_prescaling_dynamic() {
        // Test various source image sizes and cell heights
        let test_cases = vec![
            (100, 100, 16, IMAGE_HEIGHT_REGULAR), // Square image, 16px cells
            (400, 100, 18, IMAGE_HEIGHT_WIDE),    // Wide image (4:1), 18px cells
            (100, 200, 20, IMAGE_HEIGHT_REGULAR), // Tall image, 20px cells
            (400, 300, 14, IMAGE_HEIGHT_REGULAR), // 4:3 image, 14px cells
        ];

        for (src_width, src_height, cell_height, expected_height_cells) in test_cases {
            let aspect_ratio = src_width as f32 / src_height as f32;

            // Simulate the prescaling logic
            let target_height_in_pixels = expected_height_cells as usize * cell_height;
            let scale = target_height_in_pixels as f32 / src_height as f32;
            let new_width = (src_width as f32 * scale) as u32;
            let new_height = target_height_in_pixels;

            assert_eq!(
                new_height,
                expected_height_cells as usize * cell_height,
                "Image {}x{} (aspect {:.2}) prescaled height should be exactly {} * cell_height ({} pixels)",
                src_width,
                src_height,
                aspect_ratio,
                expected_height_cells,
                expected_height_cells as usize * cell_height
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

    /// Test that image rendering area calculations work correctly
    #[test]
    fn test_image_rendering_calculations() {
        let reader = TextReader::new();

        // Test with square image (regular height)
        {
            let square_image = DynamicImage::ImageRgba8(ImageBuffer::new(100, 100));
            let area_width = 100;

            // Height should be regular for square images
            let image_height =
                reader.calculate_image_height_in_cells(&square_image);
            assert_eq!(
                image_height, IMAGE_HEIGHT_REGULAR,
                "Square image height should be exactly {} cells",
                IMAGE_HEIGHT_REGULAR
            );
        }

        // Test with wide image (reduced height)
        {
            let wide_image = DynamicImage::ImageRgba8(ImageBuffer::new(400, 100));
            let area_width = 100;

            // Height should be reduced for wide images
            let image_height =
                reader.calculate_image_height_in_cells(&wide_image);
            assert_eq!(
                image_height, IMAGE_HEIGHT_WIDE,
                "Wide image height should be exactly {} cells",
                IMAGE_HEIGHT_WIDE
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
