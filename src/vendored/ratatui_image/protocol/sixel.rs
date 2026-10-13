//! Sixel protocol implementations.
//! Uses [`sixel-bytes`] to draw image pixels, if the terminal [supports] the [Sixel] protocol.
//! Needs the `sixel` feature.
//!
//! [`sixel-bytes`]: https://github.com/benjajaja/sixel-bytes
//! [supports]: https://arewesixelyet.com
//! [Sixel]: https://en.wikipedia.org/wiki/Sixel
use std::cmp::min;

use icy_sixel::{EncodeOptions, QuantizeMethod, sixel_encode};
use image::DynamicImage;
use ratatui::{buffer::Buffer, layout::Rect};

use super::{ProtocolTrait, StatefulProtocolTrait};
use crate::vendored::ratatui_image::{Result, errors::Errors, picker::cap_parser::Parser};

#[derive(Clone, Copy, Debug)]
pub struct SixelOptions {
    /// Palette size (2-256).
    pub max_colors: u16,
    /// Floyd-Steinberg error diffusion strength (0.0 disables dithering).
    pub diffusion: f32,
}

impl SixelOptions {
    pub const DEFAULT: SixelOptions = SixelOptions {
        max_colors: 256,
        diffusion: 0.875,
    };

    pub fn low_quality() -> Self {
        Self {
            max_colors: 64,
            diffusion: 0.0,
        }
    }
}

impl Default for SixelOptions {
    fn default() -> Self {
        Self::DEFAULT
    }
}

// Fixed sixel protocol
#[derive(Clone, Default, Debug)]
pub struct Sixel {
    pub data: String,
    pub area: Rect,
    pub is_tmux: bool,
    pub options: SixelOptions,
}

impl Sixel {
    pub fn new(
        image: DynamicImage,
        area: Rect,
        is_tmux: bool,
        options: SixelOptions,
    ) -> Result<Self> {
        let data = encode(&image, is_tmux, options)?;
        Ok(Self {
            data,
            area,
            is_tmux,
            options,
        })
    }
}

fn encode(img: &DynamicImage, is_tmux: bool, options: SixelOptions) -> Result<String> {
    // No padding to a multiple of 6: the raster attributes carry the exact height, and a taller
    // image would spill into the next row (scrolling the screen when drawn on the last row).
    let img_rgba8 = DynamicImage::ImageRgb8(img.to_rgb8()).into_rgba8();
    let bytes = img_rgba8.as_raw();
    let w = img_rgba8.width();
    let h = img_rgba8.height();

    let encode_options = EncodeOptions {
        max_colors: options.max_colors,
        diffusion: options.diffusion,
        quantize_method: QuantizeMethod::Wu,
    };
    let mut data = sixel_encode(bytes, w as usize, h as usize, &encode_options)
        .map_err(|err| Errors::Sixel(err.to_string()))?;

    if is_tmux {
        let (start, escape, end) = Parser::escape_tmux(is_tmux);
        if data.strip_prefix('\x1b').is_none() {
            return Err(Errors::Tmux("sixel string did not start with escape"));
        }

        data.insert_str(0, escape);
        data.insert_str(0, start);
        data.push_str(end);
    }
    Ok(data)
}

impl ProtocolTrait for Sixel {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        render(self.area, &self.data, area, buf, false)
    }

    fn area(&self) -> Rect {
        self.area
    }
}

fn render(rect: Rect, data: &str, area: Rect, buf: &mut Buffer, overdraw: bool) {
    let render_area = match calc_render_area(rect, area, overdraw) {
        None => {
            // If we render out of area, then the buffer will attempt to write regular text (or
            // possibly other sixels) over the image.
            //
            // On some implementations (e.g. Xterm), this actually works but the image is
            // forever overwritten since we won't write out the same sixel data for the same
            // (col,row) position again (see buffer diffing).
            // Thus, when the area grows, the newly available cells will skip rendering and
            // leave artifacts instead of the image data.
            //
            // On some implementations (e.g. ???), only text with its foreground color is
            // overlayed on the image, also forever overwritten.
            //
            // On some implementations (e.g. patched Alactritty), image graphics are never
            // overwritten and simply draw over other UI elements.
            //
            // Note that [ResizeProtocol] forces to ignore this early return, since it will
            // always resize itself to the area.
            return;
        }
        Some(r) => r,
    };

    // WezTerm places a sixel before flushing printable text that immediately precedes it, so a
    // character emitted right before this cell would shift the image one cell left. An explicit
    // cursor position flushes that text and pins the image to its cell on every terminal.
    let positioned = format!(
        "\x1b[{};{}H{data}",
        u32::from(render_area.y) + 1,
        u32::from(render_area.x) + 1
    );
    buf[(render_area.x, render_area.y)].set_symbol(&positioned);
    let mut skip_first = false;

    // Skip entire area
    for y in render_area.top()..render_area.bottom() {
        for x in render_area.left()..render_area.right() {
            if !skip_first {
                skip_first = true;
                continue;
            }
            buf[(x, y)].set_skip(true);
        }
    }
}

fn calc_render_area(rect: Rect, area: Rect, overdraw: bool) -> Option<Rect> {
    if overdraw {
        return Some(Rect::new(
            area.x,
            area.y,
            min(rect.width, area.width),
            min(rect.height, area.height),
        ));
    }

    if rect.width > area.width || rect.height > area.height {
        return None;
    }
    Some(Rect::new(area.x, area.y, rect.width, rect.height))
}

impl StatefulProtocolTrait for Sixel {
    fn resize_encode(&mut self, img: DynamicImage, area: Rect) -> Result<()> {
        let data = encode(&img, self.is_tmux, self.options)?;
        *self = Sixel {
            data,
            area,
            ..*self
        };
        Ok(())
    }
}
