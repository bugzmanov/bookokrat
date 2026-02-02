//! Types for PDF rendering and display batching
//!
//! Contains types used for coordinating image rendering,
//! display batches, and page layout information.

use ratatui::layout::{Position, Rect};

use crate::pdf::kittyv2::{DisplayLocation, ImageState};
use crate::pdf::{CellSize, ConvertedImage};

/// Request to display a single image at a position
#[derive(Debug)]
pub struct ImageRequest<'a> {
    /// The image state to display
    pub image: &'a mut ImageState,
    /// Page index this image belongs to
    pub page: usize,
    /// Terminal position for display
    pub position: Position,
    /// Display location configuration for Kitty protocol
    pub location: DisplayLocation,
}

/// Batch of display operations to execute
#[derive(Debug)]
pub enum DisplayBatch<'a> {
    /// No change needed, keep existing display
    NoChange,
    /// Clear all existing images
    Clear,
    /// Display these images
    Display(Vec<ImageRequest<'a>>),
}

#[derive(Clone)]
pub struct PdfDisplayRequest {
    pub page: usize,
    pub position: Position,
    pub location: DisplayLocation,
}

pub enum PdfDisplayPlan {
    NoChange,
    Clear,
    Display(Vec<PdfDisplayRequest>),
}

/// Information about a visible page in the UI
#[derive(Debug, Clone)]
pub struct VisiblePageUiInfo {
    /// Page index
    pub page_idx: usize,
    /// Y position on screen where page starts
    pub screen_y_start: u16,
    /// Number of rows being displayed
    pub display_rows: u16,
    /// Display width in terminal cells
    pub dest_w: u16,
    /// Display height in terminal cells
    pub dest_h: u16,
    /// Offset in destination cells (for partial page display)
    pub offset_dest_cells: u16,
}

/// Information about the last render pass
#[derive(Default, Debug)]
pub struct LastRender {
    /// The frame rect from last render
    pub rect: Rect,
    /// Number of pages shown side by side
    pub pages_shown: usize,
    /// Unused width after centering content
    pub unused_width: u16,
    /// Image area height
    pub img_area_height: u16,
    /// Image area width
    pub img_area_width: u16,
    /// The image display area
    pub img_area: Rect,
}

/// Information about a pending scroll operation
#[derive(Clone, Copy, Debug)]
pub struct PendingScroll {
    /// Delta in cells to scroll (positive = down, negative = up)
    pub delta_cells: i16,
    /// The image area this scroll applies to
    pub img_area: Rect,
}

/// Rendered page information
#[derive(Default)]
pub struct RenderedInfo {
    /// The converted image ready for display
    pub img: Option<ConvertedImage>,
    /// Full size in terminal cells
    pub full_cell_size: Option<CellSize>,
    /// Width in pixels
    pub pixel_w: Option<u32>,
    /// Height in pixels
    pub pixel_h: Option<u32>,
    /// Scale factor applied
    pub scale_factor: Option<f32>,
    /// Line bounds for text selection
    pub line_bounds: Vec<crate::pdf::LineBounds>,
    /// Link rectangles
    pub link_rects: Vec<crate::pdf::LinkRect>,
    /// Page height in pixels
    pub page_px_height: Option<f32>,
}

/// Layout for rendering
#[derive(PartialEq, Debug)]
pub struct RenderLayout {
    /// Area for page content
    pub page_area: Rect,
}

/// Mode for page jump input
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PageJumpMode {
    /// Jump to content page number
    Content,
    /// Jump to PDF page number
    #[default]
    Pdf,
}

/// Previous frame content for scroll optimization
pub struct PrevFrame {
    /// Area of previous frame
    pub area: Rect,
    /// Cell content from previous frame
    pub content: Vec<ratatui::buffer::Cell>,
}
