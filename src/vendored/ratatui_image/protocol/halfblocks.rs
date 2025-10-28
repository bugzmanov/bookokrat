//! Halfblocks protocol implementations.
//! Uses the unicode character `▀` combined with foreground and background color. Assumes that the
//! font aspect ratio is roughly 1:2. Should work in all terminals.
use image::{DynamicImage, imageops::FilterType};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use super::Result;
use super::{ProtocolTrait, StatefulProtocolTrait};

// Fixed Halfblocks protocol
#[derive(Clone, Default)]
pub struct Halfblocks {
    data: Vec<HalfBlock>,
    area: Rect,
}

#[derive(Clone, Debug)]
struct HalfBlock {
    upper: Color,
    lower: Color,
}

impl Halfblocks {
    /// Create a FixedHalfblocks from an image.
    ///
    /// The "resolution" is determined by the font size of the terminal. Smaller fonts will result
    /// in more half-blocks for the same image size. To get a size independent of the font size,
    /// the image could be resized in relation to the font size beforehand.
    /// Also note that the font-size is probably just some arbitrary size with a 1:2 ratio when the
    /// protocol is Halfblocks, and not the actual font size of the terminal.
    pub fn new(image: DynamicImage, area: Rect) -> Result<Self> {
        let data = encode(&image, area);
        Ok(Self { data, area })
    }
}

fn encode(img: &DynamicImage, rect: Rect) -> Vec<HalfBlock> {
    let img = img.resize_exact(
        rect.width as u32,
        (rect.height * 2) as u32,
        FilterType::Triangle,
    );

    let mut data = vec![
        HalfBlock {
            upper: Color::Rgb(0, 0, 0),
            lower: Color::Rgb(0, 0, 0),
        };
        (rect.width * rect.height) as usize
    ];

    for (y, row) in img.to_rgb8().rows().enumerate() {
        for (x, pixel) in row.enumerate() {
            let position = x + (rect.width as usize) * (y / 2);
            if y % 2 == 0 {
                data[position].upper = Color::Rgb(pixel[0], pixel[1], pixel[2]);
            } else {
                data[position].lower = Color::Rgb(pixel[0], pixel[1], pixel[2]);
            }
        }
    }
    data
}

impl ProtocolTrait for Halfblocks {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Early return for empty areas
        if area.width == 0 || area.height == 0 || self.data.is_empty() {
            return;
        }

        // Direct access to buffer cells for better performance
        let buf_area = buf.area;
        let buf_width = buf_area.width as usize;
        let cells = &mut buf.content;

        // Pre-calculate bounds to avoid repeated checks
        let render_width = area.width.min(self.area.width) as usize;
        let render_height = area.height.min(self.area.height) as usize;

        // Calculate base offset in the buffer
        let base_offset =
            ((area.y - buf_area.y) as usize * buf_width) + (area.x - buf_area.x) as usize;

        // Batch process rows for better cache locality
        for y in 0..render_height {
            let row_offset = base_offset + (y * buf_width);
            let data_row_offset = y * self.area.width as usize;

            // Process entire row at once
            for x in 0..render_width {
                let buffer_idx = row_offset + x;
                let data_idx = data_row_offset + x;

                // Safety: We've pre-calculated bounds
                if buffer_idx < cells.len() && data_idx < self.data.len() {
                    let hb = &self.data[data_idx];
                    let cell = &mut cells[buffer_idx];
                    cell.set_fg(hb.upper).set_bg(hb.lower).set_char('▀');
                }
            }
        }
    }
    fn area(&self) -> Rect {
        self.area
    }
}

impl StatefulProtocolTrait for Halfblocks {
    fn resize_encode(&mut self, img: DynamicImage, area: Rect) -> Result<()> {
        let data = encode(&img, area);
        *self = Halfblocks { data, area };
        Ok(())
    }
}
