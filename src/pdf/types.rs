//! Core types for PDF rendering

/// Character position information for text extraction
#[derive(Clone, Debug)]
pub struct CharInfo {
    /// X coordinate in scaled page coordinates
    pub x: f32,
    /// The character
    pub c: char,
}

/// Line bounding box with character information
#[derive(Clone, Debug)]
pub struct LineBounds {
    /// Left edge X coordinate
    pub x0: f32,
    /// Top edge Y coordinate
    pub y0: f32,
    /// Right edge X coordinate
    pub x1: f32,
    /// Bottom edge Y coordinate
    pub y1: f32,
    /// Characters in this line with their positions
    pub chars: Vec<CharInfo>,
    /// Block ID this line belongs to
    pub block_id: usize,
}

/// Link target type
#[derive(Clone, Debug)]
pub enum LinkTarget {
    Internal { page: usize },
    External { uri: String },
}

/// Link rectangle in pixel coordinates
#[derive(Clone, Debug)]
pub struct LinkRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub target: LinkTarget,
}

/// Raw rendered page image before protocol encoding.
///
/// Contains RGB pixel data and dimensions in both pixels and terminal cells.
/// This is the intermediate format between MuPDF rendering and terminal
/// protocol encoding (Kitty/Sixel/iTerm2).
#[derive(Clone)]
pub struct ImageData {
    /// Raw RGB pixel data (3 bytes per pixel: R, G, B)
    pub pixels: Vec<u8>,
    /// Image width in pixels
    pub width_px: u32,
    /// Image height in pixels
    pub height_px: u32,
    /// Image width in terminal cells
    pub width_cell: u16,
    /// Image height in terminal cells
    pub height_cell: u16,
}

/// Complete rendered page data
#[derive(Clone)]
pub struct PageData {
    /// Rendered image data
    pub img_data: ImageData,
    /// Page number (0-indexed)
    pub page_num: usize,
    /// Scale factor used for rendering
    pub scale_factor: f32,
    /// Text line bounds for selection/search
    pub line_bounds: Vec<LineBounds>,
    /// Clickable link areas
    pub link_rects: Vec<LinkRect>,
    /// Page height in pixels
    pub page_height_px: f32,
}

impl std::fmt::Debug for PageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageData")
            .field("page_num", &self.page_num)
            .field("img_data.cell_width", &self.img_data.width_cell)
            .field("img_data.cell_height", &self.img_data.height_cell)
            .field("scale_factor", &self.scale_factor)
            .field("page_height_px", &self.page_height_px)
            .field("line_bounds_count", &self.line_bounds.len())
            .field("link_rects_count", &self.link_rects.len())
            .finish_non_exhaustive()
    }
}

/// Viewport update message for scrolling
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ViewportUpdate {
    pub page: usize,
    pub y_offset_cells: u32,
    pub viewport_height_cells: u16,
    pub viewport_width_cells: u16,
}

/// Extension trait for Vec operations
pub trait VecExt<T> {
    /// Reset vector to a given length, clearing existing items
    fn reset_to_len(&mut self, len: usize)
    where
        T: Default;
}

impl<T> VecExt<T> for Vec<T> {
    #[inline]
    fn reset_to_len(&mut self, len: usize)
    where
        T: Default,
    {
        self.clear();
        self.resize_with(len, T::default);
    }
}

// =============================================================================
// TEMPORARY TYPES - Will be moved to their final locations in later commits
// =============================================================================

/// Terminal cell dimensions
/// TEMPORARY: Moves to converter.rs in commit 13
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSize {
    pub width: u16,
    pub height: u16,
}

impl CellSize {
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub fn from_rect(r: ratatui::layout::Rect) -> Self {
        Self::new(r.width, r.height)
    }

    #[must_use]
    pub const fn as_tuple(self) -> (u16, u16) {
        (self.width, self.height)
    }
}

/// Target of a TOC entry
/// TEMPORARY: Moves to parsing/toc.rs in commit 12
#[derive(Clone, Debug)]
pub enum TocTarget {
    /// Internal page (0-indexed)
    InternalPage(usize),
    /// External URI
    External(String),
    /// Printed page number (for display)
    PrintedPage(usize),
}

/// A single entry in the table of contents
/// TEMPORARY: Moves to parsing/toc.rs in commit 12
#[derive(Clone, Debug)]
pub struct TocEntry {
    /// Display title
    pub title: String,
    /// Nesting level (0 = top level)
    pub level: usize,
    /// Navigation target
    pub target: TocTarget,
}
