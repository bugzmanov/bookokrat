//! Image region widget for preserving Kitty graphics
//!
//! This widget sets the `skip` flag on buffer cells to tell ratatui
//! not to overwrite them, preserving existing Kitty graphics placements.

use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

/// A widget that marks an area as containing an image.
///
/// When rendered, this sets `skip=true` on all cells in the area,
/// telling ratatui to preserve any existing content (like Kitty graphics).
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageRegion;

impl Widget for ImageRegion {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let buffer_area = buf.area();
        let x_end = area.x.saturating_add(area.width).min(buffer_area.right());
        let y_end = area.y.saturating_add(area.height).min(buffer_area.bottom());
        let x_start = area.x.max(buffer_area.left());
        let y_start = area.y.max(buffer_area.top());

        for y in y_start..y_end {
            for x in x_start..x_end {
                buf[(x, y)].set_skip(true);
            }
        }
    }
}

/// A widget that marks an area as containing text (not images).
///
/// This clears the `skip` flag, ensuring cells will be rendered normally.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextRegion;

impl Widget for TextRegion {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let buffer_area = buf.area();
        let x_end = area.x.saturating_add(area.width).min(buffer_area.right());
        let y_end = area.y.saturating_add(area.height).min(buffer_area.bottom());
        let x_start = area.x.max(buffer_area.left());
        let y_start = area.y.max(buffer_area.top());

        for y in y_start..y_end {
            for x in x_start..x_end {
                buf[(x, y)].set_skip(false);
            }
        }
    }
}
