//! Sixel protocol implementations.
//! Uses [`sixel-bytes`] to draw image pixels, if the terminal [supports] the [Sixel] protocol.
//! Needs the `sixel` feature.
//!
//! [`sixel-bytes`]: https://github.com/benjajaja/sixel-bytes
//! [supports]: https://arewesixelyet.com
//! [Sixel]: https://en.wikipedia.org/wiki/Sixel
use std::cmp::min;

use icy_sixel::{
    DiffusionMethod, MethodForLargest, MethodForRep, PixelFormat, Quality, sixel_string,
};
use image::DynamicImage;
use ratatui::{buffer::Buffer, layout::Rect};

use super::{ProtocolTrait, StatefulProtocolTrait};
use crate::vendored::ratatui_image::{Result, errors::Errors, picker::cap_parser::Parser};

#[derive(Clone, Copy)]
pub struct SixelOptions {
    pub diffusion: DiffusionMethod,
    pub largest: MethodForLargest,
    pub rep: MethodForRep,
    pub quality: Quality,
}

impl std::fmt::Debug for SixelOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SixelOptions")
            .field("diffusion", &format_diffusion(self.diffusion))
            .field("largest", &format_largest(self.largest))
            .field("rep", &format_rep(self.rep))
            .field("quality", &format_quality(self.quality))
            .finish()
    }
}

fn format_diffusion(value: DiffusionMethod) -> &'static str {
    match value {
        DiffusionMethod::Auto => "Auto",
        DiffusionMethod::None => "None",
        DiffusionMethod::Atkinson => "Atkinson",
        DiffusionMethod::FS => "FS",
        DiffusionMethod::JaJuNi => "JaJuNi",
        DiffusionMethod::Stucki => "Stucki",
        DiffusionMethod::Burkes => "Burkes",
        DiffusionMethod::ADither => "ADither",
        DiffusionMethod::XDither => "XDither",
    }
}

fn format_largest(value: MethodForLargest) -> &'static str {
    match value {
        MethodForLargest::Auto => "Auto",
        MethodForLargest::Norm => "Norm",
        MethodForLargest::Lum => "Lum",
    }
}

fn format_rep(value: MethodForRep) -> &'static str {
    match value {
        MethodForRep::Auto => "Auto",
        MethodForRep::CenterBox => "CenterBox",
        MethodForRep::AverageColors => "AverageColors",
        MethodForRep::Pixels => "Pixels",
    }
}

fn format_quality(value: Quality) -> &'static str {
    match value {
        Quality::AUTO => "AUTO",
        Quality::HIGH => "HIGH",
        Quality::LOW => "LOW",
        Quality::FULL => "FULL",
        Quality::HIGHCOLOR => "HIGHCOLOR",
    }
}

impl SixelOptions {
    pub const DEFAULT: SixelOptions = SixelOptions {
        diffusion: DiffusionMethod::Stucki,
        largest: MethodForLargest::Auto,
        rep: MethodForRep::Auto,
        quality: Quality::HIGH,
    };

    pub fn low_quality() -> Self {
        Self {
            diffusion: DiffusionMethod::None,
            largest: MethodForLargest::Auto,
            rep: MethodForRep::Auto,
            quality: Quality::LOW,
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
    let mut img = img.to_rgb8();
    let w = img.width();
    let h = img.height();
    let rounded_h = h.div_ceil(6) * 6;
    if rounded_h != h {
        let bg = *img.get_pixel(0, 0);
        let mut padded = image::ImageBuffer::from_pixel(w, rounded_h, bg);
        image::imageops::overlay(&mut padded, &img, 0, 0);
        img = padded;
    }
    let img_rgb8 = img;
    let bytes = img_rgb8.as_raw();
    let w = img_rgb8.width();
    let h = img_rgb8.height();

    let mut data = sixel_string(
        bytes,
        w as i32,
        h as i32,
        PixelFormat::RGB888,
        options.diffusion,
        options.largest,
        options.rep,
        options.quality,
    )
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

    buf[(render_area.x, render_area.y)].set_symbol(data);
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
