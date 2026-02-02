//! ITerm2 protocol implementation.
use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    fmt::Write,
    io::Cursor,
};

use image::{DynamicImage, ImageBuffer, Rgb};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::vendored::ratatui_image::{Result, picker::cap_parser::Parser};

use super::{ProtocolTrait, StatefulProtocolTrait};

#[derive(Clone, Default, Debug)]
pub struct Iterm2 {
    pub data: String,
    pub area: Rect,
    pub is_tmux: bool,
}

impl Iterm2 {
    pub fn new(image: DynamicImage, area: Rect, is_tmux: bool) -> Result<Self> {
        let data = encode(&image, area, is_tmux)?;
        Ok(Self {
            data,
            area,
            is_tmux,
        })
    }
}

/// Encode as indexed PNG if ≤256 colors, otherwise RGB PNG
fn encode_optimized_png(img: &DynamicImage) -> Vec<u8> {
    let rgb = img.to_rgb8();

    // Collect unique colors (stop if >256)
    let mut unique_colors: HashSet<[u8; 3]> = HashSet::new();
    for pixel in rgb.pixels() {
        unique_colors.insert(pixel.0);
        if unique_colors.len() > 256 {
            // Too many colors - use RGB PNG directly (skip expensive quantization)
            let mut png_data: Vec<u8> = Vec::new();
            img.write_to(&mut Cursor::new(&mut png_data), image::ImageFormat::Png)
                .ok();
            return png_data;
        }
    }

    // ≤256 colors - use indexed PNG (much smaller)
    if let Some(indexed_png) = encode_indexed_png(&rgb, &unique_colors) {
        return indexed_png;
    }

    // Fallback: standard RGB PNG
    let mut png_data: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_data), image::ImageFormat::Png)
        .ok();
    png_data
}

/// Encode image as indexed PNG using the png crate directly
fn encode_indexed_png(
    rgb: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    unique_colors: &HashSet<[u8; 3]>,
) -> Option<Vec<u8>> {
    let (width, height) = rgb.dimensions();

    // Build palette
    let palette: Vec<[u8; 3]> = unique_colors.iter().copied().collect();
    let color_to_index: std::collections::HashMap<[u8; 3], u8> = palette
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, i as u8))
        .collect();

    // Build indexed pixel data
    let mut indexed_pixels: Vec<u8> = Vec::with_capacity((width * height) as usize);
    for pixel in rgb.pixels() {
        let idx = color_to_index.get(&pixel.0)?;
        indexed_pixels.push(*idx);
    }

    // Encode using png crate
    let mut png_data: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_data, width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);

        // Flatten palette to [R, G, B, R, G, B, ...]
        let flat_palette: Vec<u8> = palette.iter().flat_map(|c| c.iter().copied()).collect();
        encoder.set_palette(flat_palette);

        encoder.set_compression(png::Compression::Fast);

        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&indexed_pixels).ok()?;
    }

    Some(png_data)
}

/// Quantize to 256 colors using median cut algorithm and encode as indexed PNG
#[allow(dead_code)]
fn encode_quantized_indexed_png(rgb: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Option<Vec<u8>> {
    let (width, height) = rgb.dimensions();

    // Count color frequencies
    let total_pixels = u64::from(width) * u64::from(height);
    let target_samples = 100_000u64;
    let sample_every = (total_pixels / target_samples).max(1);
    let mut color_counts: HashMap<[u8; 3], u32> = HashMap::new();
    let mut idx = 0u64;
    for pixel in rgb.pixels() {
        if sample_every > 1 && idx % sample_every != 0 {
            idx += 1;
            continue;
        }
        idx += 1;
        *color_counts.entry(pixel.0).or_insert(0) += 1;
    }

    // Build palette using median cut
    let palette = median_cut_palette(&color_counts, 256);

    // Build color lookup for fast mapping
    let color_to_index: HashMap<[u8; 3], u8> = palette
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, i as u8))
        .collect();

    // Map each pixel to nearest palette color
    let mut indexed_pixels: Vec<u8> = Vec::with_capacity((width * height) as usize);
    for pixel in rgb.pixels() {
        let idx = color_to_index
            .get(&pixel.0)
            .copied()
            .unwrap_or_else(|| find_nearest_color(&pixel.0, &palette));
        indexed_pixels.push(idx);
    }

    // Encode using png crate
    let mut png_data: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_data, width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);

        let flat_palette: Vec<u8> = palette.iter().flat_map(|c| c.iter().copied()).collect();
        encoder.set_palette(flat_palette);
        encoder.set_compression(png::Compression::Fast);

        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&indexed_pixels).ok()?;
    }

    Some(png_data)
}

/// Median cut algorithm to generate optimal palette
#[allow(dead_code)]
fn median_cut_palette(color_counts: &HashMap<[u8; 3], u32>, max_colors: usize) -> Vec<[u8; 3]> {
    if color_counts.len() <= max_colors {
        return color_counts.keys().copied().collect();
    }

    // Start with all colors in one box
    let mut boxes: Vec<Vec<([u8; 3], u32)>> =
        vec![color_counts.iter().map(|(&c, &n)| (c, n)).collect()];

    // Split boxes until we have enough
    while boxes.len() < max_colors {
        // Find the box with the largest range to split
        let (box_idx, channel) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| {
                let (r_range, g_range, b_range) = color_range(b);
                let max_range = r_range.max(g_range).max(b_range);
                let channel = if max_range == r_range {
                    0
                } else if max_range == g_range {
                    1
                } else {
                    2
                };
                (i, channel, max_range)
            })
            .max_by_key(|&(_, _, range)| range)
            .map(|(i, c, _)| (i, c))
            .unwrap_or((0, 0));

        if boxes[box_idx].len() <= 1 {
            break;
        }

        // Sort by the channel with largest range and split at median
        let mut box_to_split = boxes.swap_remove(box_idx);
        box_to_split.sort_by_key(|(c, _)| c[channel]);

        let mid = box_to_split.len() / 2;
        let (left, right) = box_to_split.split_at(mid);

        boxes.push(left.to_vec());
        boxes.push(right.to_vec());
    }

    // Average each box to get palette color
    boxes
        .iter()
        .map(|b| {
            let total_weight: u64 = b.iter().map(|(_, n)| *n as u64).sum();
            if total_weight == 0 {
                return [0, 0, 0];
            }
            let r: u64 = b.iter().map(|(c, n)| c[0] as u64 * *n as u64).sum();
            let g: u64 = b.iter().map(|(c, n)| c[1] as u64 * *n as u64).sum();
            let b_sum: u64 = b.iter().map(|(c, n)| c[2] as u64 * *n as u64).sum();
            [
                (r / total_weight) as u8,
                (g / total_weight) as u8,
                (b_sum / total_weight) as u8,
            ]
        })
        .collect()
}

#[allow(dead_code)]
fn color_range(colors: &[([u8; 3], u32)]) -> (u8, u8, u8) {
    let mut r_min = 255u8;
    let mut r_max = 0u8;
    let mut g_min = 255u8;
    let mut g_max = 0u8;
    let mut b_min = 255u8;
    let mut b_max = 0u8;

    for (c, _) in colors {
        r_min = r_min.min(c[0]);
        r_max = r_max.max(c[0]);
        g_min = g_min.min(c[1]);
        g_max = g_max.max(c[1]);
        b_min = b_min.min(c[2]);
        b_max = b_max.max(c[2]);
    }

    (r_max - r_min, g_max - g_min, b_max - b_min)
}

#[allow(dead_code)]
fn find_nearest_color(color: &[u8; 3], palette: &[[u8; 3]]) -> u8 {
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| {
            let dr = color[0] as i32 - p[0] as i32;
            let dg = color[1] as i32 - p[1] as i32;
            let db = color[2] as i32 - p[2] as i32;
            dr * dr + dg * dg + db * db
        })
        .map(|(i, _)| i as u8)
        .unwrap_or(0)
}

fn encode(img: &DynamicImage, render_area: Rect, is_tmux: bool) -> Result<String> {
    let png = encode_optimized_png(img);

    let data = base64_simd::STANDARD.encode_to_string(&png);

    let (start, escape, end) = Parser::escape_tmux(is_tmux);

    let width = render_area.width;
    let height = render_area.height;
    let mut seq = String::from(start);

    // Erase cells before rendering to handle transparency
    write!(seq, "{escape}[s")?; // Save cursor position
    for _ in 0..height {
        write!(seq, "{escape}[{width}X{escape}[1B")?;
    }
    write!(seq, "{escape}[u")?; // Restore cursor position

    // Use cell units for width/height
    write!(
        seq,
        "{escape}]1337;File=inline=1;size={};width={};height={};preserveAspectRatio=0;doNotMoveCursor=1:{}\x07{end}",
        png.len(),
        render_area.width,
        render_area.height,
        data,
    )?;

    Ok(seq)
}

impl ProtocolTrait for Iterm2 {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        render(self.area, &self.data, area, buf, true)
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
            // Note that [StatefulProtocol] forces to ignore this early return, since it will
            // always resize itself to the area.
            return;
        }
        Some(r) => r,
    };

    let buf_area = buf.area();
    if render_area.x >= buf_area.right() || render_area.y >= buf_area.bottom() {
        return;
    }

    let right = render_area.right().min(buf_area.right());
    let bottom = render_area.bottom().min(buf_area.bottom());
    if right <= render_area.x || bottom <= render_area.y {
        return;
    }

    let render_area = Rect::new(
        render_area.x,
        render_area.y,
        right - render_area.x,
        bottom - render_area.y,
    );

    let nonce = crate::terminal::overlay_resend_nonce();
    if nonce == 0 {
        buf[(render_area.x, render_area.y)].set_symbol(data);
    } else {
        let marker = if nonce & 1 == 0 { "\x1b[0m" } else { "\x1b[m" };
        let payload = format!("{marker}{data}");
        buf[(render_area.x, render_area.y)].set_symbol(&payload);
    }

    for x in (render_area.left() + 1)..render_area.right() {
        buf[(x, render_area.top())].set_skip(true);
    }

    // Skip entire area
    for y in (render_area.top() + 1)..render_area.bottom() {
        for x in render_area.left()..render_area.right() {
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

impl StatefulProtocolTrait for Iterm2 {
    fn resize_encode(&mut self, img: DynamicImage, area: Rect) -> Result<()> {
        let data = encode(&img, area, self.is_tmux)?;
        *self = Iterm2 {
            data,
            area,
            ..*self
        };
        Ok(())
    }
}
