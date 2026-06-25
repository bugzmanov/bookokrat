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

/// Compute the padded, text-clipped visual box(es) for a single link — one per
/// text line it overlaps. This is the single source of truth for link geometry:
/// the converter strokes these boxes (the orange outline) and the reader
/// hit-tests clicks against them, so what you see is what you click.
///
/// Link annotation rects are authored loosely (they overhang into the margin and
/// are taller than the glyphs), and a wrapped link is one rect per visual line.
/// Each box is clamped horizontally to the link∩line span with a small padding,
/// and sized vertically to the glyph box plus padding. Padding and the overlap
/// threshold scale with line height. Links covering no text line (e.g. over a
/// figure) fall back to the raw clickbox. Returned tuples are `(x0, y0, x1, y1)`
/// in the page's pixel space.
pub(crate) fn link_visual_boxes(
    link: &LinkRect,
    lines: &[LineBounds],
) -> Vec<(u32, u32, u32, u32)> {
    let (lx0, ly0, lx1, ly1) = (
        link.x0 as f32,
        link.y0 as f32,
        link.x1 as f32,
        link.y1 as f32,
    );
    let mut out = Vec::new();
    for line in lines {
        if lx0 >= line.x1 || lx1 <= line.x0 {
            continue;
        }
        // Require substantial vertical overlap: link boxes are taller than the
        // glyphs (extra ascender slop), so a tiny overlap means the link belongs
        // to an adjacent line, not this one.
        let line_height = line.y1 - line.y0;
        let overlap = ly1.min(line.y1) - ly0.max(line.y0);
        if line_height <= 0.0 || overlap < 0.5 * line_height {
            continue;
        }
        let pad_x = (line_height * 0.10).round().max(1.0);
        let pad_y = (line_height * 0.04).round().max(1.0);
        let x0 = (lx0.max(line.x0) - pad_x).max(0.0).round() as u32;
        let x1 = (lx1.min(line.x1) + pad_x).round() as u32;
        let y0 = (line.y0 - pad_y).max(0.0).round() as u32;
        let y1 = (line.y1 + pad_y).round() as u32;
        out.push((x0, y0, x1, y1));
    }
    if out.is_empty() {
        out.push((link.x0, link.y0, link.x1, link.y1));
    }
    out
}

/// Raw rendered page image before protocol encoding.
///
/// Contains RGB pixel data and dimensions in both pixels and terminal cells.
/// This is the intermediate format between MuPDF rendering and terminal
/// protocol encoding (Kitty/Sixel/iTerm2).
#[derive(Clone)]
pub struct ImageData {
    /// Raw pixel data. Bytes per pixel is given by `channels` (3 = RGB, 4 = RGBA).
    pub pixels: Vec<u8>,
    /// Image width in pixels
    pub width_px: u32,
    /// Image height in pixels
    pub height_px: u32,
    /// Image width in terminal cells
    pub width_cell: u16,
    /// Image height in terminal cells
    pub height_cell: u16,
    /// Number of channels per pixel: 3 (RGB) or 4 (RGBA, transparent mode).
    pub channels: u8,
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
    /// Requested user zoom factor used for rendering
    pub requested_scale: f32,
    /// Viewport width (in terminal cells) used for rendering
    pub render_area_width_cells: u16,
    /// Viewport height (in terminal cells) used for rendering
    pub render_area_height_cells: u16,
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
            .field("requested_scale", &self.requested_scale)
            .field("render_area_width_cells", &self.render_area_width_cells)
            .field("render_area_height_cells", &self.render_area_height_cells)
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
    pub x_offset_cells: u32,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> LineBounds {
        LineBounds {
            x0,
            y0,
            x1,
            y1,
            chars: Vec::new(),
            block_id: 0,
        }
    }

    fn contains(boxes: &[(u32, u32, u32, u32)], x: f32, y: f32) -> bool {
        boxes.iter().any(|&(bx0, by0, bx1, by1)| {
            x >= bx0 as f32 && x <= bx1 as f32 && y >= by0 as f32 && y <= by1 as f32
        })
    }

    #[test]
    fn visual_box_clips_overhang_and_adds_tolerance() {
        // Wrapped-link second fragment (page 18 of the LLM book): its annotation
        // box overhangs into the left margin (x0=66 vs text x0=102).
        let link = LinkRect {
            x0: 66,
            y0: 297,
            x1: 285,
            y1: 310,
            target: LinkTarget::External { uri: "u".into() },
        };
        let lines = [line(102.0, 300.9, 474.1, 310.4)];
        let boxes = link_visual_boxes(&link, &lines);
        assert_eq!(boxes.len(), 1);

        let (bx0, by0, bx1, by1) = boxes[0];
        // Left margin overhang is clipped to the text start (≈102), not 66.
        assert!(bx0 >= 100 && bx0 < 105, "bx0={bx0}");
        // The padded box is slightly taller than the raw glyph box, so a click
        // just below the raw link rect (y=311, raw y1=310) now lands inside.
        assert!(by1 >= 311, "by1={by1}");
        assert!(contains(&boxes, 150.0, 311.0));
        // A point well outside still misses.
        assert!(!contains(&boxes, 150.0, 330.0));
        let _ = (bx1, by0);
    }

    #[test]
    fn visual_box_falls_back_to_raw_rect_without_text() {
        let link = LinkRect {
            x0: 10,
            y0: 10,
            x1: 50,
            y1: 30,
            target: LinkTarget::Internal { page: 3 },
        };
        let lines = [line(0.0, 100.0, 200.0, 110.0)];
        let boxes = link_visual_boxes(&link, &lines);
        assert_eq!(boxes, vec![(10, 10, 50, 30)]);
    }
}
