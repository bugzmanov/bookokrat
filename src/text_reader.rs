use crate::images::background_image_loader::BackgroundImageLoader;
use crate::images::book_images::BookImages;
use crate::images::image_placeholder::{ImagePlaceholder, ImagePlaceholderConfig, LoadingStatus};
use crate::main_app::VimNavMotions;
use crate::text_selection::TextSelection;
use crate::theme::Base16Palette;
use image::{DynamicImage, GenericImageView};
use log::{debug, error, info, warn};
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
use std::sync::Arc;
use std::time::Instant;
use textwrap;

/// Height for regular images in terminal cells
const IMAGE_HEIGHT_REGULAR: u16 = 15;
/// Height for wide images (aspect ratio > 3:1) in terminal cells
const IMAGE_HEIGHT_WIDE: u16 = 7;
/// Aspect ratio threshold for wide images
const WIDE_IMAGE_ASPECT_RATIO: f32 = 3.0;

#[derive(Debug, Clone)]
pub struct LinkInfo {
    pub text: String,
    pub url: String,
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
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

pub enum ImageLoadState {
    NotLoaded,

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
    image_picker: Option<Picker>,
    embedded_images: RefCell<HashMap<String, EmbeddedImage>>,
    // Background image loader
    background_loader: BackgroundImageLoader,
    // Raw HTML viewing
    show_raw_html: bool,
    raw_html_content: Option<String>,
    // Store link information for click handling
    links: Vec<LinkInfo>,
}

impl TextReader {
    pub fn new() -> Self {
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
            image_picker,
            embedded_images: RefCell::new(HashMap::new()),
            background_loader: BackgroundImageLoader::new(),
            show_raw_html: false,
            raw_html_content: None,
            links: Vec::new(),
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
                // Check if this image was filtered out (too small)
                if let Some(image_src) = extract_src(line.trim()) {
                    // Only skip if we've attempted to load dimensions and the image was filtered
                    let embedded_images = self.embedded_images.borrow();
                    if !embedded_images.is_empty() && !embedded_images.contains_key(&image_src) {
                        // Image was filtered out - don't count it
                        continue;
                    }
                }

                // For now, assume regular height - actual height will be determined when image is loaded
                // We use regular height as default for line counting
                total_lines += IMAGE_HEIGHT_REGULAR as usize;
                image_count += 1;
            } else if is_empty {
                total_lines += 1;
            } else {
                // Process markdown links before wrapping (remove URL part for accurate line counting)
                let link_re = Regex::new(r"\[([^\]]+)\]\([^\)]+\)").unwrap();
                let line_for_wrapping = link_re.replace_all(line, "$1").to_string();

                let wrapped_lines = textwrap::wrap(&line_for_wrapping, width);
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

        // Clear links for new content
        self.links.clear();

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
        // Start at 2 if we have a title (title line + empty line)
        let mut line_count = if chapter_title.is_some() { 2 } else { 0 };

        // Process each line with manual wrapping
        for (_i, line) in text.lines().enumerate() {
            let is_image_placeholder = line.trim().starts_with("[image src=");

            if is_image_placeholder {
                if let Some(image_src) = extract_src(line.trim()) {
                    // Check if image exists in cache and if it was filtered out for being too small
                    // Only skip if we've attempted to load dimensions and the image was filtered
                    let embedded_images = self.embedded_images.borrow();
                    if !embedded_images.is_empty() && !embedded_images.contains_key(&image_src) {
                        // Image was filtered out (too small) during preload - skip it completely
                        continue;
                    }
                }

                // Add empty line before image
                raw_lines.push(String::new());
                lines.push(Line::from(String::new()));
                line_count += 1;

                if let Some(image_src) = extract_src(line.trim()) {
                    if let Some(image) = self.embedded_images.borrow_mut().get_mut(&image_src) {
                        //todo.. this +1 and +2 is very messy and needs to be consolidated
                        image.lines_before_image = line_count; // +2 <-- to account for empty line before and after the image
                    } else {
                        error!(
                            "Image '{}' not found in embedded_images cache. This suggests preload_image_dimensions was not called or failed. THIS SHOULD NOT HAPPEN",
                            image_src
                        );
                        self.embedded_images.borrow_mut().insert(
                            image_src.clone(),
                            EmbeddedImage {
                                src: image_src.clone(),
                                lines_before_image: line_count,
                                height_cells: IMAGE_HEIGHT_WIDE,
                                width: 200,
                                height: 200,
                                state: ImageLoadState::Failed {
                                    reason: "Not pre-loaded".to_string(),
                                },
                            },
                        );
                    }

                    let placeholder_height = self
                        .embedded_images
                        .borrow()
                        .get(&image_src)
                        .unwrap()
                        .height_cells;

                    let config = ImagePlaceholderConfig {
                        internal_padding: 4,
                        total_height: placeholder_height as usize,
                        border_color: palette.base_03,
                    };

                    // Check image loading state to determine visibility and status
                    let embedded_images = self.embedded_images.borrow();
                    let image_state = embedded_images.get(&image_src).map(|img| &img.state);

                    let (is_successfully_loaded, loading_status) = match image_state {
                        Some(ImageLoadState::Loaded { .. }) => (true, LoadingStatus::Loaded),
                        Some(ImageLoadState::Failed { .. }) => (false, LoadingStatus::Failed),
                        Some(ImageLoadState::NotLoaded) | None => (false, LoadingStatus::Loading),
                    };
                    drop(embedded_images);

                    let placeholder = ImagePlaceholder::new(
                        &image_src,
                        width,
                        &config,
                        !is_successfully_loaded,
                        loading_status,
                    );
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

                    line_count += placeholder_line_count;

                    // Add empty line after image
                    raw_lines.push(String::new());
                    lines.push(Line::from(String::new()));
                    line_count += 1;
                }
            } else if line.trim().is_empty() {
                // Normal empty line processing
                raw_lines.push(String::new());
                lines.push(Line::from(String::new()));
                line_count += 1;
            } else {
                // Check if this is a markdown heading
                let is_heading = line.starts_with('#');
                let heading_level = if is_heading {
                    line.chars().take_while(|&c| c == '#').count()
                } else {
                    0
                };

                // First, replace markdown links with just their text for wrapping purposes
                // but keep track of link positions
                let link_re = Regex::new(r"\[([^\]]+)\]\(([^\)]+)\)").unwrap();
                let mut links_in_line = Vec::new();

                // Collect all links in this line
                for cap in link_re.captures_iter(line) {
                    if let (Some(text), Some(url)) = (cap.get(1), cap.get(2)) {
                        links_in_line.push((text.as_str().to_string(), url.as_str().to_string()));
                    }
                }

                // Replace links with just text for wrapping
                let line_for_wrapping = link_re.replace_all(line, "$1").to_string();

                // Wrap the line
                let wrapped_lines = textwrap::wrap(&line_for_wrapping, width);
                for wrapped_line in wrapped_lines {
                    let line_str = wrapped_line.to_string();
                    raw_lines.push(line_str.clone());

                    // Parse styling for this wrapped line segment
                    // Note: Links are already processed, so we parse the text with link text visible
                    let styled_line = self.parse_line_styling_for_wrapped(
                        &line_str,
                        &links_in_line,
                        palette,
                        is_focused,
                        line_count,
                    );
                    lines.push(styled_line);
                    line_count += 1;
                }

                // Add decoration lines after headings
                if is_heading && heading_level >= 1 && heading_level <= 3 {
                    let decoration_line = match heading_level {
                        1 => {
                            // h1: thick line (using ━ character)
                            let thick_line = "▀".repeat(width);
                            raw_lines.push(thick_line.clone());
                            Line::from(vec![Span::styled(
                                thick_line,
                                Style::default().fg(if is_focused {
                                    palette.base_0a
                                } else {
                                    palette.base_03
                                }),
                            )])
                        }
                        2 => {
                            // h2: double thin lines
                            let thin_line = "═".repeat(width);
                            raw_lines.push(thin_line.clone());
                            let line1 = Line::from(vec![Span::styled(
                                thin_line.clone(),
                                Style::default().fg(if is_focused {
                                    palette.base_0a
                                } else {
                                    palette.base_03
                                }),
                            )]);
                            lines.push(line1);
                            line_count += 1;

                            raw_lines.push(thin_line.clone());
                            Line::from(vec![Span::styled(
                                thin_line,
                                Style::default().fg(if is_focused {
                                    palette.base_0a
                                } else {
                                    palette.base_03
                                }),
                            )])
                        }
                        3 => {
                            // h3: single thin line
                            let thin_line = "─".repeat(width);
                            raw_lines.push(thin_line.clone());
                            Line::from(vec![Span::styled(
                                thin_line,
                                Style::default().fg(if is_focused {
                                    palette.base_0a
                                } else {
                                    palette.base_03
                                }),
                            )])
                        }
                        _ => Line::from(String::new()),
                    };
                    lines.push(decoration_line);
                    line_count += 1;
                }
            }
        }

        // Return both styled content and raw lines
        (Text::from(lines), raw_lines)
    }

    /// Parse markdown heading and return styled line
    fn parse_markdown_heading(
        &self,
        line: &str,
        palette: &Base16Palette,
        is_focused: bool,
    ) -> Line<'static> {
        // Count the number of # symbols to determine heading level
        let level = line.chars().take_while(|&c| c == '#').count();

        // Extract the heading text (remove # symbols and trim)
        let text = line.trim_start_matches('#').trim();

        // Get focus-aware colors
        let heading_color = if is_focused {
            palette.base_0a // Yellow for all heading levels
        } else {
            palette.base_03 // Dimmed for unfocused
        };

        // Create the styled heading text
        // For h4, add underline modifier
        // For h5-h7, just bold
        let modifiers = if level == 4 {
            Modifier::BOLD | Modifier::UNDERLINED
        } else {
            Modifier::BOLD
        };

        Line::from(vec![Span::styled(
            text.to_string(),
            Style::default().fg(heading_color).add_modifier(modifiers),
        )])
    }

    /// Parse styling for a wrapped line that may contain link text (but not the full markdown syntax)
    fn parse_line_styling_for_wrapped(
        &mut self,
        line: &str,
        links_in_original: &[(String, String)],
        palette: &Base16Palette,
        is_focused: bool,
        line_num: usize,
    ) -> Line<'static> {
        // Check if this is a markdown heading
        if line.starts_with('#') {
            return self.parse_markdown_heading(line, palette, is_focused);
        }

        let mut spans = Vec::new();
        let mut current_pos = 0;

        // Get focus-aware colors
        let (normal_text_color, _, _) = palette.get_panel_colors(is_focused);
        let link_text_color = if is_focused {
            palette.base_0c // Cyan for links when focused
        } else {
            palette.base_03 // Dimmed for unfocused links
        };

        // Check if any link text appears in this wrapped line segment
        let mut found_links = Vec::new();
        for (link_text, url) in links_in_original {
            if let Some(pos) = line.find(link_text) {
                found_links.push((pos, link_text.clone(), url.clone()));
            }
        }

        // Sort by position to process in order
        found_links.sort_by_key(|k| k.0);

        // Process the line with link highlighting
        for (link_pos, link_text, url) in found_links {
            // Add text before the link
            if link_pos > current_pos {
                let before_text = &line[current_pos..link_pos];
                if !before_text.is_empty() {
                    spans.push(Span::styled(
                        before_text.to_string(),
                        Style::default().fg(normal_text_color),
                    ));
                }
            }

            // Store link information for click handling
            self.links.push(LinkInfo {
                text: link_text.clone(),
                url: url.clone(),
                line: line_num,
                start_col: link_pos,
                end_col: link_pos + link_text.len(),
            });

            // Add the link with styling
            spans.push(Span::styled(
                link_text.clone(),
                Style::default()
                    .fg(link_text_color)
                    .add_modifier(Modifier::UNDERLINED),
            ));

            current_pos = link_pos + link_text.len();
        }

        // Add any remaining text
        if current_pos < line.len() {
            let remaining = &line[current_pos..];
            if !remaining.is_empty() {
                spans.push(Span::styled(
                    remaining.to_string(),
                    Style::default().fg(normal_text_color),
                ));
            }
        }

        if spans.is_empty() {
            // No links found, process normally for other styling (bold, quotes)
            self.parse_line_styling_owned(line, palette, is_focused, line_num)
        } else {
            Line::from(spans)
        }
    }

    /// Parse styling for a single line (bold, quotes, links, etc.) - owned version for caching
    fn parse_line_styling_owned(
        &mut self,
        line: &str,
        palette: &Base16Palette,
        is_focused: bool,
        line_num: usize,
    ) -> Line<'static> {
        // Check if this is a markdown heading
        if line.starts_with('#') {
            return self.parse_markdown_heading(line, palette, is_focused);
        }

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
        let link_text_color = if is_focused {
            palette.base_0c // Cyan for links when focused
        } else {
            palette.base_03 // Dimmed for unfocused links
        };

        while i < chars.len() {
            // Check for markdown-style links [text](url)
            if chars[i] == '[' {
                // Look for the matching ]( pattern
                let mut link_text = String::new();
                let mut j = i + 1;
                let mut found_link = false;

                while j < chars.len() && chars[j] != ']' {
                    link_text.push(chars[j]);
                    j += 1;
                }

                if j + 1 < chars.len() && chars[j] == ']' && chars[j + 1] == '(' {
                    // Found link pattern, extract URL
                    j += 2; // Skip ](
                    let mut url = String::new();
                    while j < chars.len() && chars[j] != ')' {
                        url.push(chars[j]);
                        j += 1;
                    }

                    if j < chars.len() && chars[j] == ')' {
                        // Valid link found
                        found_link = true;

                        // Add any pending text
                        if !current_text.is_empty() {
                            spans.push(Span::styled(
                                current_text.clone(),
                                Style::default().fg(normal_text_color),
                            ));
                            current_text.clear();
                        }

                        // Store link information for click handling
                        // Calculate the actual position where the link text will appear in the rendered line
                        // This is the position BEFORE adding the link span
                        let mut rendered_pos = 0;
                        for span in &spans {
                            rendered_pos += span.content.chars().count();
                        }
                        rendered_pos += current_text.chars().count(); // Add any pending text

                        let start_col = rendered_pos;
                        let end_col = start_col + link_text.chars().count();

                        self.links.push(LinkInfo {
                            text: link_text.clone(),
                            url: url.clone(),
                            line: line_num,
                            start_col,
                            end_col,
                        });

                        // Add link with underline styling
                        spans.push(Span::styled(
                            link_text,
                            Style::default()
                                .fg(link_text_color)
                                .add_modifier(Modifier::UNDERLINED),
                        ));

                        i = j + 1; // Move past the closing )
                        continue;
                    }
                }

                if !found_link {
                    // Not a valid link, treat [ as normal character
                    current_text.push('[');
                    i += 1;
                }
            } else if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
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
        // Cancel any ongoing background image loading
        self.background_loader.cancel_loading();

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
        // Reset raw HTML view when changing chapters
        self.show_raw_html = false;
    }

    /// Toggle raw HTML viewing mode
    pub fn toggle_raw_html(&mut self) {
        self.show_raw_html = !self.show_raw_html;
        // Reset scroll position when toggling
        self.scroll_offset = 0;
        // Clear cached content to force re-render
        self.cached_styled_content = None;
        self.cached_content_hash = 0;
    }

    /// Check if a click at the given coordinates is on a link
    pub fn get_link_at_position(&self, line: usize, column: usize) -> Option<&LinkInfo> {
        debug!("Links - {:?}", &self.links[0..5]);
        debug!("mouse position - {}:{}", line, column);
        for link in &self.links {
            if link.line == line && column >= link.start_col && column < link.end_col {
                debug!("Found link match: '{}'", link.url);
                return Some(link);
            }
        }
        None
    }

    /// Set the raw HTML content for the current chapter
    pub fn set_raw_html(&mut self, html: String) {
        self.raw_html_content = Some(html);
    }

    /// Quickly read image sizes and start background loading
    pub fn preload_image_dimensions(&mut self, content: &str, book_images: &BookImages) {
        let quick_scan_start = std::time::Instant::now();
        let mut images_to_load: Vec<(String, u16)> = Vec::new();
        let mut images_processed = 0;

        // Reset any previous loading state
        self.background_loader.cancel_loading();

        for line in content.lines() {
            if line.trim().starts_with("[image src=") {
                if let Some(img_src) = extract_src(line.trim()) {
                    images_processed += 1;

                    if self.embedded_images.borrow().contains_key(&img_src) {
                        continue;
                    }

                    let dimensions_result = book_images.get_image_size(&img_src);

                    if let Some((img_width, img_height)) = dimensions_result {
                        // Filter out small images (less than 64x64)
                        if img_width < 64 || img_height < 64 {
                            warn!(
                                "Ignoring small image ({}x{}): {}",
                                img_width, img_height, img_src
                            );
                            continue;
                        }

                        let height_cells = EmbeddedImage::height_in_cells(img_width, img_height);
                        self.embedded_images.borrow_mut().insert(
                            img_src.clone(),
                            EmbeddedImage {
                                src: img_src.clone(),
                                lines_before_image: 0, // Will be set properly in parse_styled_text_internal_with_raw
                                height_cells,
                                width: img_width,
                                height: img_height,
                                state: ImageLoadState::NotLoaded,
                            },
                        );
                        images_to_load.push((img_src.clone(), height_cells));
                    } else {
                        warn!("Could not get dimensions for: {}", img_src);
                        self.embedded_images.borrow_mut().insert(
                            img_src.clone(),
                            EmbeddedImage::failed_img(&img_src, "Could not read image metadata"),
                        );
                    }
                }
            }
        }

        let quick_scan_time = quick_scan_start.elapsed();
        if images_processed > 0 {
            info!(
                "      - Quick image scan: {} images found, dimensions read in {:?}",
                images_processed, quick_scan_time
            );
        }

        //todo this is wrong or not sufficient at best
        self.cached_styled_content = None;
        self.cached_content_hash = 0;

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
                        img_state.state = ImageLoadState::Failed {
                            reason: "terminal doesnt' support images".to_string(),
                        }
                    }
                }
            }
        }
    }

    /// Check for loaded images from background thread and update caches
    /// Returns true if images were loaded and a redraw is needed
    pub fn check_for_loaded_images(&mut self) -> bool {
        if let Some(loaded_images) = self.background_loader.check_for_loaded_images() {
            debug!(
                "Received {} loaded images from background thread",
                loaded_images.len()
            );

            let picker = self.image_picker.as_ref().unwrap_or_else(|| {
                panic!("Picker is not available, this branch of code should never be executed!")
            });

            let mut embedded_images = self.embedded_images.borrow_mut();
            for (img_src, image) in loaded_images {
                if let Some(embedded_image) = embedded_images.get_mut(&img_src) {
                    let protocol = picker.new_resize_protocol(image.clone()); // todo: i don't like this clone
                    embedded_image.state = ImageLoadState::Loaded {
                        image: Arc::new(image),
                        protocol,
                    };
                } else {
                    // This can happen due to race condition when user switches chapters quickly
                    // The background thread loaded images for the previous chapter, but we've already
                    // switched to a new chapter and cleared embedded_images. This is expected behavior.
                    debug!(
                        "Received loaded image '{}' that is no longer in embedded_images (likely due to chapter switch). Ignoring.",
                        &img_src
                    );
                }
            }

            // Clear styled content cache to trigger re-render with actual images
            self.cached_styled_content = None;
            self.cached_content_hash = 0;

            return true;
        }
        false
    }

    pub fn scroll_down(&mut self) {
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

    pub fn scroll_up(&mut self) {
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

    pub fn update_highlight(&mut self) -> bool {
        // Clear expired highlight
        if self.highlight_visual_line.is_some() && Instant::now() >= self.highlight_end_time {
            self.highlight_visual_line = None;
            return true; // Highlight state changed, needs redraw
        }
        false // No change, no redraw needed
    }

    /// Handle mouse down event for text selection
    pub fn handle_mouse_down(&mut self, screen_x: u16, screen_y: u16, content_area: Rect) {
        // Check if click is on a link first
        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            if self.get_link_at_position(line, column).is_some() {
                // Don't start text selection if clicking on a link
                debug!("Mouse down on link, skipping text selection");
                return;
            }

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
    /// Returns Some(url) if a link was clicked without text selection
    pub fn handle_mouse_up(
        &mut self,
        screen_x: u16,
        screen_y: u16,
        content_area: Rect,
    ) -> Option<String> {
        // Stop auto-scrolling when mouse is released
        self.auto_scroll_state = None;

        let mut link_url = None;

        if let Some((line, column)) = self.screen_to_text_coords(screen_x, screen_y, content_area) {
            self.text_selection.update_selection(line, column);

            // Check if this is just a click (no meaningful selection)
            // Selection is considered meaningful if start and end are different
            let is_just_click = if let (Some(start), Some(end)) =
                (&self.text_selection.start, &self.text_selection.end)
            {
                start.line == end.line && (start.column as i32 - end.column as i32).abs() <= 1
            } else {
                true
            };

            // If it's just a click (no selection), check for link
            if is_just_click {
                if let Some(link) = self.get_link_at_position(line, column) {
                    link_url = Some(link.url.clone());
                    debug!("Link clicked: {}", link.url);
                }
            }
        }

        self.text_selection.end_selection();

        // Clear selection if it was just a click
        if link_url.is_some() {
            self.text_selection.clear_selection();
        }

        link_url
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

    /// Copy entire chapter content to clipboard
    pub fn copy_chapter_to_clipboard(&self) -> Result<(), String> {
        let content_to_copy = if self.show_raw_html {
            // If in raw HTML mode, copy the raw HTML content
            self.raw_html_content.clone().unwrap_or_default()
        } else {
            // In normal mode, copy the rendered text lines
            self.raw_text_lines.join("\n")
        };

        if content_to_copy.is_empty() {
            let error_msg = "No chapter content to copy".to_string();
            debug!("{}", error_msg);
            return Err(error_msg);
        }

        match arboard::Clipboard::new() {
            Ok(mut clipboard) => match clipboard.set_text(content_to_copy) {
                Ok(()) => {
                    debug!("Successfully copied chapter content to clipboard");
                    Ok(())
                }
                Err(e) => {
                    let error_msg = format!("Failed to copy chapter to clipboard: {}", e);
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

    /// Check if a click is on an image and return the image source if found
    pub fn check_image_click(
        &self,
        screen_x: u16,
        screen_y: u16,
        content_area: Rect,
    ) -> Option<String> {
        // Convert screen coordinates to logical line position
        let relative_y = screen_y.saturating_sub(content_area.y);
        let click_line = self.scroll_offset + relative_y as usize;

        debug!(
            "Checking image click at screen ({}, {}), relative_y: {}, click_line: {}",
            screen_x, screen_y, relative_y, click_line
        );

        // Check if click is within any image boundaries
        for (src, embedded_image) in self.embedded_images.borrow().iter() {
            let image_start_line = embedded_image.lines_before_image;
            let image_end_line = image_start_line + embedded_image.height_cells as usize;

            debug!(
                "Image '{}': lines {} to {} (height: {} cells)",
                src, image_start_line, image_end_line, embedded_image.height_cells
            );

            if click_line >= image_start_line && click_line < image_end_line {
                debug!("Image click detected on: {}", src);
                return Some(src.clone());
            }
        }

        debug!("No image click detected at line {}", click_line);
        None
    }

    /// Get a reference to the image picker for creating protocols
    pub fn get_image_picker(&self) -> Option<&Picker> {
        self.image_picker.as_ref()
    }

    /// Get an embedded image by source path
    pub fn get_embedded_image(&self, image_src: &str) -> Option<&EmbeddedImage> {
        // Since we can't return a reference from the borrow, we'll need a different approach
        // For now, let's just check if the image exists and is loaded
        None // Placeholder - we'll implement this differently
    }

    /// Check if an embedded image is loaded and return the image if it is
    pub fn get_loaded_image(&self, image_src: &str) -> Option<Arc<DynamicImage>> {
        if let Some(embedded_image) = self.embedded_images.borrow().get(image_src) {
            if let ImageLoadState::Loaded { image, .. } = &embedded_image.state {
                return Some(image.clone());
            }
        }
        None
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

        // Check if we should display raw HTML
        let styled_content = if self.show_raw_html {
            // Display raw HTML content
            if let Some(ref raw_html) = self.raw_html_content {
                // Escape potentially dangerous characters that could cause terminal glitches
                let escaped_html = raw_html
                    .replace('\x1b', "\\x1b") // Escape ESC character
                    .replace('\x00', "\\x00") // Escape NULL
                    .replace('\x07', "\\x07") // Escape BEL (bell)
                    .replace('\x08', "\\x08") // Escape BS (backspace)
                    .replace('\x0C', "\\x0C") // Escape FF (form feed)
                    .replace('\r', "\\r") // Escape CR (carriage return)
                    .chars()
                    .filter(|c| {
                        // Filter out other control characters except newline and tab
                        !c.is_control() || *c == '\n' || *c == '\t'
                    })
                    .collect::<String>();

                // Wrap the escaped HTML content and create simple styled text
                let wrapped_lines = textwrap::wrap(&escaped_html, text_width);
                let mut lines = Vec::new();
                for wrapped_line in wrapped_lines {
                    lines.push(Line::from(vec![Span::styled(
                        wrapped_line.to_string(),
                        Style::default().fg(text_color),
                    )]));
                }
                // Update total wrapped lines for scrolling
                self.total_wrapped_lines = lines.len();
                Text::from(lines)
            } else {
                Text::from("No raw HTML content available")
            }
        } else {
            // Normal styled content rendering
            self.parse_styled_text_cached(content, chapter_title, palette, text_width, is_focused)
        };

        let content_paragraph = Paragraph::new(styled_content)
            .scroll((scroll_offset as u16, 0))
            .style(Style::default().fg(text_color).bg(palette.base_00));
        f.render_widget(content_paragraph, margined_content_area[1]);

        // Display all embedded images from the chapter (only if not showing raw HTML)
        if !self.show_raw_html {
            self.check_for_loaded_images();
            if !self.embedded_images.borrow().is_empty() && self.image_picker.is_some() {
                let area_height = margined_content_area[1].height as usize;

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
                                        self.calculate_image_height_in_cells(scaled_image);

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

                                    // Calculate actual image width in terminal cells based on aspect ratio
                                    let (image_width_pixels, image_height_pixels) =
                                        scaled_image.dimensions();
                                    let font_size = picker.font_size();
                                    let image_width_cells =
                                        (image_width_pixels as f32 / font_size.0 as f32).ceil()
                                            as u16;

                                    // Center the image horizontally within the text area
                                    let text_area_width = margined_content_area[1].width;
                                    let image_display_width =
                                        image_width_cells.min(text_area_width);
                                    let x_offset =
                                        (text_area_width.saturating_sub(image_display_width)) / 2;

                                    let image_area = Rect {
                                        x: margined_content_area[1].x + x_offset,
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
                                    f.render_stateful_widget(image_widget, image_area, protocol);
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
                self.image_picker = Some(picker);

                debug!("Image picker updated for new font size");
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
        self.scroll_down();
    }

    fn handle_k(&mut self) {
        // Up movement - scroll up one line
        self.scroll_up();
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
            (100, 100, IMAGE_HEIGHT_WIDE), // Square image (aspect ratio 1.0) - but height < 150, so uses WIDE
            (200, 50, IMAGE_HEIGHT_WIDE),  // Wide image (aspect ratio 4.0)
            (50, 200, IMAGE_HEIGHT_REGULAR), // Tall image (aspect ratio 0.25)
            (1920, 1080, IMAGE_HEIGHT_REGULAR), // HD image (aspect ratio ~1.78)
            (64, 64, IMAGE_HEIGHT_WIDE),   // Small square image - height < 150, so uses WIDE
            (400, 100, IMAGE_HEIGHT_WIDE), // Wide image (aspect ratio 4.0)
            (300, 100, IMAGE_HEIGHT_WIDE), // Almost wide (aspect ratio 3.0, at threshold) - but height < 150
            (301, 100, IMAGE_HEIGHT_WIDE), // Just over threshold (aspect ratio 3.01)
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

        // Test with square image (height < 150, so uses WIDE)
        {
            let square_image = DynamicImage::ImageRgba8(ImageBuffer::new(100, 100));

            // Height should be WIDE for images with height < 150
            let image_height = reader.calculate_image_height_in_cells(&square_image);
            assert_eq!(
                image_height, IMAGE_HEIGHT_WIDE,
                "Square image with height < 150 should be exactly {} cells",
                IMAGE_HEIGHT_WIDE
            );
        }

        // Test with wide image (reduced height)
        {
            let wide_image = DynamicImage::ImageRgba8(ImageBuffer::new(400, 100));

            // Height should be reduced for wide images
            let image_height = reader.calculate_image_height_in_cells(&wide_image);
            assert_eq!(
                image_height, IMAGE_HEIGHT_WIDE,
                "Wide image height should be exactly {} cells",
                IMAGE_HEIGHT_WIDE
            );
        }
    }
}
