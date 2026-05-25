//! Image region widget for preserving Kitty graphics
//!
//! This widget sets the `skip` flag on buffer cells to tell ratatui
//! not to overwrite them, preserving existing Kitty graphics placements.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

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

/// A single Kitty Unicode-placeholder anchor cell for tmux-aware relative placements.
#[derive(Debug, Clone, Copy)]
pub struct KittyTmuxAnchorCell {
    pub image_id: u32,
    pub placement_id: u32,
}

impl Widget for KittyTmuxAnchorCell {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let buffer_area = buf.area();
        if area.x < buffer_area.left()
            || area.x >= buffer_area.right()
            || area.y < buffer_area.top()
            || area.y >= buffer_area.bottom()
        {
            return;
        }

        let cell = &mut buf[(area.x, area.y)];
        cell.set_skip(false);
        cell.set_symbol(&kitty_placeholder_symbol(self.image_id, self.placement_id));
    }
}

fn kitty_placeholder_symbol(image_id: u32, placement_id: u32) -> String {
    kitty_placeholder_cell_symbol(image_id, placement_id, 0, 0)
}

fn kitty_placeholder_cell_symbol(image_id: u32, placement_id: u32, row: u16, col: u16) -> String {
    debug_assert!(
        placement_id <= 0x00ff_ffff,
        "Kitty placeholder placement IDs are encoded in 24-bit underline color"
    );

    let [id_extra, id_r, id_g, id_b] = image_id.to_be_bytes();
    let [_placement_extra, placement_r, placement_g, placement_b] = placement_id.to_be_bytes();
    let row = crate::vendored::ratatui_image::protocol::kitty::diacritic(row);
    let col = crate::vendored::ratatui_image::protocol::kitty::diacritic(col);
    let id_extra = crate::vendored::ratatui_image::protocol::kitty::diacritic(u16::from(id_extra));

    if placement_id == 0 {
        return format!("\x1b[38;2;{id_r};{id_g};{id_b}m\u{10EEEE}{row}{col}{id_extra}\x1b[39m");
    }

    // Kitty Unicode placeholders encode the image id in foreground color and
    // the placement id in underline color, so the raw SGR sequence must live
    // inside the cell symbol rather than ratatui's normal style fields.
    format!(
        "\x1b[38;2;{id_r};{id_g};{id_b}m\x1b[58;2;{placement_r};{placement_g};{placement_b}m\u{10EEEE}{row}{col}{id_extra}\x1b[59m\x1b[39m"
    )
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

/// A widget that fills an area with a dark background, clearing skip flags
/// and writing actual content to overwrite terminal images (iTerm2 protocol).
#[derive(Debug, Clone, Copy, Default)]
pub struct DimOverlay;

impl Widget for DimOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let buffer_area = buf.area();
        let x_end = area.x.saturating_add(area.width).min(buffer_area.right());
        let y_end = area.y.saturating_add(area.height).min(buffer_area.bottom());
        let x_start = area.x.max(buffer_area.left());
        let y_start = area.y.max(buffer_area.top());

        let style = Style::default().bg(Color::Rgb(10, 10, 10));

        for y in y_start..y_end {
            for x in x_start..x_end {
                let cell = &mut buf[(x, y)];
                cell.set_skip(false);
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }
}
