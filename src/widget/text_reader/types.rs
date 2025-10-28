use crate::ratatui_image::protocol::StatefulProtocol;
use image::DynamicImage;
use ratatui::text::Span;
use std::sync::Arc;
use tui_textarea::TextArea;

use crate::types::LinkInfo;

/// Height for regular images in terminal cells
pub const IMAGE_HEIGHT_REGULAR: u16 = 15;
/// Height for wide images (aspect ratio > 3:1) in terminal cells
pub const IMAGE_HEIGHT_WIDE: u16 = 7;
/// Aspect ratio threshold for wide images
pub const WIDE_IMAGE_ASPECT_RATIO: f32 = 3.0;

/// Pre-processed rendering structure
pub struct RenderedContent {
    pub lines: Vec<RenderedLine>,
    pub total_height: usize,
    pub generation: u64, // For cache validation
}

#[derive(Clone)]
pub struct RenderedLine {
    pub spans: Vec<Span<'static>>,
    pub raw_text: String, // For text selection
    pub line_type: LineType,
    pub link_nodes: Vec<LinkInfo>, // Links that are visible on this line
    pub node_anchor: Option<String>, // Anchor/id from the Node if present
    pub node_index: Option<usize>, // Index of the node in the document this line belongs to
}

impl RenderedLine {
    pub fn empty() -> Self {
        Self {
            spans: Vec::new(),
            raw_text: String::new(),
            line_type: LineType::Text,
            link_nodes: Vec::new(),
            node_anchor: None,
            node_index: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LineType {
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
pub enum RichSpan {
    Text(Span<'static>),
    Link { span: Span<'static>, info: LinkInfo },
}

impl RichSpan {
    /// Extract the underlying ratatui Span
    pub fn into_span(self) -> Span<'static> {
        match self {
            RichSpan::Text(span) => span,
            RichSpan::Link { span, .. } => span,
        }
    }

    /// Get link info if this is a link
    pub fn link_info(&self) -> Option<&LinkInfo> {
        match self {
            RichSpan::Text(_) => None,
            RichSpan::Link { info, .. } => Some(info),
        }
    }
}

#[allow(clippy::large_enum_variant)]
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
    Unsupported,
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

        if aspect_ratio > WIDE_IMAGE_ASPECT_RATIO || height < 150 {
            IMAGE_HEIGHT_WIDE
        } else {
            IMAGE_HEIGHT_REGULAR
        }
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

/// Represents the active section being read
#[derive(Clone, Debug)]
pub struct ActiveSection {
    pub chapter: usize,
    pub chapter_href: String,
    pub chapter_base_href: String,
    pub anchor: Option<String>,
}

impl ActiveSection {
    pub fn new(chapter: usize, chapter_href: String, anchor: Option<String>) -> Self {
        let chapter_base_href = Self::base_href(&chapter_href);
        let normalized_anchor = anchor.map(|a| Self::normalize_anchor(&a));
        Self {
            chapter,
            chapter_href,
            chapter_base_href,
            anchor: normalized_anchor,
        }
    }

    pub fn base_href(href: &str) -> String {
        href.split('#').next().unwrap_or(href).to_string()
    }

    pub fn normalize_anchor(anchor: &str) -> String {
        anchor
            .split('#')
            .next_back()
            .unwrap_or(anchor)
            .trim_start_matches('#')
            .to_string()
    }
}

#[derive(Clone, Debug)]
pub enum CommentEditMode {
    Creating,
    Editing {
        chapter_href: String,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    },
}

#[derive(Default)]
pub struct CommentInputState {
    pub textarea: Option<TextArea<'static>>,
    pub target_node_index: Option<usize>,
    pub target_line: Option<usize>,
    pub edit_mode: Option<CommentEditMode>,
}

impl CommentInputState {
    pub fn clear(&mut self) {
        self.textarea = None;
        self.target_node_index = None;
        self.target_line = None;
        self.edit_mode = None;
    }

    pub fn is_active(&self) -> bool {
        self.textarea.is_some()
    }
}
