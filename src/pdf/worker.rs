//! PDF render worker - runs in separate thread(s)

use std::path::Path;
use std::sync::{Arc, Mutex};

use flume::{Receiver, Sender};
use mupdf::text_page::TextBlockType;
use mupdf::{Colorspace, Document, Matrix, Page, Pixmap, TextPageFlags};
use rayon::prelude::*;

use super::KITTY_MAX_DIMENSION;
use super::cache::{CacheKey, PageCache};
use super::request::{
    PageSelectionBounds, RenderParams, RenderRequest, RenderResponse, RequestId, WorkerFault,
};
use super::types::{CharInfo, ImageData, LineBounds, LinkRect, LinkTarget, PageData};

const TITLE_RGB: (u8, u8, u8) = (0x66, 0x99, 0xCC);
const TITLE_LUMA_DARK_MAX: u8 = 90;
const TITLE_LUMA_LIGHT_MIN: u8 = 180;

/// Pre-computed rasterization parameters for a page
struct RasterSpec {
    output_width: f32,
    output_height: f32,
    transform: Matrix,
    mag: f32,
}

impl RasterSpec {
    fn compute(
        page_bounds: (f32, f32),
        viewport_px: (f32, f32),
        user_scale: f32,
        cell_dims: (f32, f32),
    ) -> Self {
        let (page_width, page_height) = page_bounds;
        let (view_width, view_height) = viewport_px;
        let (cell_width, cell_height) = cell_dims;

        let base_mag = if page_width / page_height > view_width / view_height {
            view_height / page_height
        } else {
            view_width / page_width
        };

        let mut mag = base_mag * user_scale;
        let mut out_width = page_width * mag;
        let mut out_height = page_height * mag;

        let max_dim = out_width.max(out_height);
        if max_dim > KITTY_MAX_DIMENSION {
            let reduction = KITTY_MAX_DIMENSION / max_dim;
            mag *= reduction;
            out_width *= reduction;
            out_height *= reduction;
        }

        // Align output dimensions to cell boundaries for Resize::None optimization
        // This ensures tiles don't need resizing, significantly improving scroll performance
        let aligned_width = (out_width / cell_width).floor() * cell_width;
        let aligned_height = (out_height / cell_height).ceil() * cell_height;

        // Adjust magnification to match the aligned dimensions
        let align_scale_width = aligned_width / out_width;
        let align_scale_height = aligned_height / out_height;
        let align_scale = align_scale_width.min(align_scale_height);
        mag *= align_scale;

        Self {
            output_width: aligned_width,
            output_height: aligned_height,
            transform: Matrix::new_scale(mag, mag),
            mag,
        }
    }
}

mod simd_luma {
    use wide::u16x8;

    const LUMA_R: u16 = 54;
    const LUMA_G: u16 = 183;
    const LUMA_B: u16 = 19;

    #[inline]
    pub fn apply_title_rgba(
        row: &mut [u8],
        title_rgb: (u8, u8, u8),
        threshold: u16,
        inverted: bool,
    ) {
        let chunks = row.len() / 16;
        let simd_end = chunks * 16;
        let (simd_part, remainder) = row.split_at_mut(simd_end);

        for chunk in simd_part.chunks_exact_mut(16) {
            let r = u16x8::new([
                u16::from(chunk[0]),
                u16::from(chunk[4]),
                u16::from(chunk[8]),
                u16::from(chunk[12]),
                0,
                0,
                0,
                0,
            ]);
            let g = u16x8::new([
                u16::from(chunk[1]),
                u16::from(chunk[5]),
                u16::from(chunk[9]),
                u16::from(chunk[13]),
                0,
                0,
                0,
                0,
            ]);
            let b = u16x8::new([
                u16::from(chunk[2]),
                u16::from(chunk[6]),
                u16::from(chunk[10]),
                u16::from(chunk[14]),
                0,
                0,
                0,
                0,
            ]);

            let luma: u16x8 =
                (r * u16x8::splat(LUMA_R) + g * u16x8::splat(LUMA_G) + b * u16x8::splat(LUMA_B))
                    >> 8;
            let luma_arr = luma.to_array();

            for i in 0..4 {
                let should_recolor = if inverted {
                    luma_arr[i] >= threshold
                } else {
                    luma_arr[i] <= threshold
                };

                if should_recolor {
                    chunk[i * 4] = title_rgb.0;
                    chunk[i * 4 + 1] = title_rgb.1;
                    chunk[i * 4 + 2] = title_rgb.2;
                }
            }
        }

        for px in remainder.chunks_exact_mut(4) {
            let r = px[0];
            let g = px[1];
            let b = px[2];
            let luma = (u16::from(r) * LUMA_R + u16::from(g) * LUMA_G + u16::from(b) * LUMA_B) >> 8;

            let should_recolor = if inverted {
                luma >= threshold
            } else {
                luma <= threshold
            };

            if should_recolor {
                px[0] = title_rgb.0;
                px[1] = title_rgb.1;
                px[2] = title_rgb.2;
            }
        }
    }

    #[inline]
    pub fn apply_title_rgb(
        row: &mut [u8],
        title_rgb: (u8, u8, u8),
        threshold: u16,
        inverted: bool,
    ) {
        let chunks = row.len() / 48;
        let simd_end = chunks * 48;
        let (simd_part, remainder) = row.split_at_mut(simd_end);

        for chunk in simd_part.chunks_exact_mut(48) {
            process_8_rgb_pixels(&mut chunk[0..24], title_rgb, threshold, inverted);
            process_8_rgb_pixels(&mut chunk[24..48], title_rgb, threshold, inverted);
        }

        for px in remainder.chunks_exact_mut(3) {
            let r = px[0];
            let g = px[1];
            let b = px[2];
            let luma = (u16::from(r) * LUMA_R + u16::from(g) * LUMA_G + u16::from(b) * LUMA_B) >> 8;

            let should_recolor = if inverted {
                luma >= threshold
            } else {
                luma <= threshold
            };

            if should_recolor {
                px[0] = title_rgb.0;
                px[1] = title_rgb.1;
                px[2] = title_rgb.2;
            }
        }
    }

    #[inline]
    fn process_8_rgb_pixels(
        chunk: &mut [u8],
        title_rgb: (u8, u8, u8),
        threshold: u16,
        inverted: bool,
    ) {
        debug_assert!(chunk.len() == 24, "Expected 24 bytes for 8 RGB pixels");

        let r = u16x8::new([
            u16::from(chunk[0]),
            u16::from(chunk[3]),
            u16::from(chunk[6]),
            u16::from(chunk[9]),
            u16::from(chunk[12]),
            u16::from(chunk[15]),
            u16::from(chunk[18]),
            u16::from(chunk[21]),
        ]);
        let g = u16x8::new([
            u16::from(chunk[1]),
            u16::from(chunk[4]),
            u16::from(chunk[7]),
            u16::from(chunk[10]),
            u16::from(chunk[13]),
            u16::from(chunk[16]),
            u16::from(chunk[19]),
            u16::from(chunk[22]),
        ]);
        let b = u16x8::new([
            u16::from(chunk[2]),
            u16::from(chunk[5]),
            u16::from(chunk[8]),
            u16::from(chunk[11]),
            u16::from(chunk[14]),
            u16::from(chunk[17]),
            u16::from(chunk[20]),
            u16::from(chunk[23]),
        ]);

        let luma: u16x8 =
            (r * u16x8::splat(LUMA_R) + g * u16x8::splat(LUMA_G) + b * u16x8::splat(LUMA_B)) >> 8;
        let luma_arr = luma.to_array();

        for (i, &l) in luma_arr.iter().enumerate() {
            let should_recolor = if inverted {
                l >= threshold
            } else {
                l <= threshold
            };

            if should_recolor {
                chunk[i * 3] = title_rgb.0;
                chunk[i * 3 + 1] = title_rgb.1;
                chunk[i * 3 + 2] = title_rgb.2;
            }
        }
    }
}

/// Main worker function - runs in a dedicated thread
#[expect(
    clippy::needless_pass_by_value,
    reason = "Values moved into thread, need ownership"
)]
pub fn render_worker(
    doc_path: &Path,
    requests: Receiver<RenderRequest>,
    responses: Sender<RenderResponse>,
    cache: Arc<Mutex<PageCache>>,
) {
    let doc = match Document::open(doc_path.to_string_lossy().as_ref()) {
        Ok(d) => d,
        Err(e) => {
            let _ = responses.send(RenderResponse::Error {
                id: RequestId::new(0),
                error: WorkerFault::Pdf(e),
            });
            return;
        }
    };

    for request in requests {
        match request {
            RenderRequest::Page { id, page, params }
            | RenderRequest::Prefetch { id, page, params } => {
                handle_page_request(&doc, id, page, &params, &cache, &responses);
            }

            RenderRequest::ExtractText { id, bounds, params } => {
                let text = extract_text(&doc, &bounds, &params);
                let _ = responses.send(RenderResponse::ExtractedText { id, text });
            }

            RenderRequest::Cancel(id) => {
                let _ = responses.send(RenderResponse::Cancelled(id));
            }

            RenderRequest::Shutdown => break,
        }
    }
}

fn handle_page_request(
    doc: &Document,
    id: RequestId,
    page_num: usize,
    params: &RenderParams,
    cache: &Arc<Mutex<PageCache>>,
    responses: &Sender<RenderResponse>,
) {
    let key = CacheKey::from_params(page_num, params);

    let cached = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key);
    if let Some(cached) = cached {
        let _ = responses.send(RenderResponse::Page {
            id,
            page: page_num,
            data: Arc::clone(&cached),
        });
        return;
    }

    match render_page(doc, page_num, params) {
        Ok(data) => {
            let cached = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(key, data);
            let _ = responses.send(RenderResponse::Page {
                id,
                page: page_num,
                data: Arc::clone(&cached),
            });
        }
        Err(e) => {
            let _ = responses.send(RenderResponse::Error { id, error: e });
        }
    }
}

/// Render a single page
pub fn render_page(
    doc: &Document,
    page_num: usize,
    params: &RenderParams,
) -> Result<PageData, WorkerFault> {
    let page = doc.load_page(page_num as i32)?;

    let viewport_px = (
        f32::from(params.area.width) * f32::from(params.cell_size.width),
        f32::from(params.area.height) * f32::from(params.cell_size.height),
    );

    let bounds = page.bounds()?;
    let page_bounds = (bounds.x1 - bounds.x0, bounds.y1 - bounds.y0);
    let cell_dims = (
        f32::from(params.cell_size.width),
        f32::from(params.cell_size.height),
    );

    let spec = RasterSpec::compute(page_bounds, viewport_px, params.scale, cell_dims);

    let rgb = Colorspace::device_rgb();
    let mut pixmap = page.to_pixmap(&spec.transform, &rgb, false, false)?;

    let image_regions = if !params.invert_images {
        let rects = collect_image_rects(&page, spec.mag, pixmap.width(), pixmap.height());
        stash_image_regions(&pixmap, &rects)
    } else {
        Vec::new()
    };

    pixmap.tint(params.white, params.black)?;

    if !image_regions.is_empty() {
        restore_image_regions(&mut pixmap, &image_regions);
    }

    if let Ok(title_rects) = collect_title_rects(&page, spec.mag, page_bounds.0) {
        if !title_rects.is_empty() {
            apply_title_color(&mut pixmap, &title_rects, true);
        }
    }

    let (base_dpi_x, base_dpi_y) = pixmap.resolution();
    pixmap.set_resolution(
        (base_dpi_x as f32 * spec.mag) as i32,
        (base_dpi_y as f32 * spec.mag) as i32,
    );

    let line_bounds = extract_line_bounds(&page, spec.mag);
    let link_rects = extract_link_rects(&page, spec.mag);

    let pixels = pixmap_to_rgb(&pixmap)?;

    Ok(PageData {
        img_data: ImageData {
            pixels,
            width_px: pixmap.width(),
            height_px: pixmap.height(),
            width_cell: (spec.output_width / f32::from(params.cell_size.width)) as u16,
            height_cell: (spec.output_height / f32::from(params.cell_size.height)) as u16,
        },
        page_num,
        scale_factor: spec.mag,
        line_bounds,
        link_rects,
        page_height_px: spec.output_height,
    })
}

#[derive(Clone, Copy, Debug)]
struct ImageRect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

#[derive(Debug)]
struct ImageRegion {
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
    data: Vec<u8>,
}

fn collect_image_rects(page: &Page, scale_factor: f32, width: u32, height: u32) -> Vec<ImageRect> {
    let flags = TextPageFlags::PRESERVE_IMAGES | TextPageFlags::ACCURATE_BBOXES;
    let Ok(text_page) = page.to_text_page(flags) else {
        return Vec::new();
    };
    let max_x = width as f32;
    let max_y = height as f32;

    text_page
        .blocks()
        .filter(|block| block.r#type() == TextBlockType::Image)
        .filter_map(|block| {
            let bbox = block.bounds();
            let mut x0 = (bbox.x0 * scale_factor).floor();
            let mut y0 = (bbox.y0 * scale_factor).floor();
            let mut x1 = (bbox.x1 * scale_factor).ceil();
            let mut y1 = (bbox.y1 * scale_factor).ceil();

            if x1 <= 0.0 || y1 <= 0.0 || x0 >= max_x || y0 >= max_y {
                return None;
            }

            x0 = x0.max(0.0);
            y0 = y0.max(0.0);
            x1 = x1.min(max_x);
            y1 = y1.min(max_y);

            let x0 = x0 as usize;
            let y0 = y0 as usize;
            let x1 = x1 as usize;
            let y1 = y1 as usize;

            if x0 >= x1 || y0 >= y1 {
                None
            } else {
                Some(ImageRect { x0, y0, x1, y1 })
            }
        })
        .collect()
}

fn stash_image_regions(pixmap: &Pixmap, rects: &[ImageRect]) -> Vec<ImageRegion> {
    if rects.is_empty() {
        return Vec::new();
    }

    let n = pixmap.n() as usize;
    let width = pixmap.width() as usize;
    let samples = pixmap.samples();
    let mut regions = Vec::with_capacity(rects.len());

    for rect in rects {
        let rect_width = rect.x1.saturating_sub(rect.x0);
        let rect_height = rect.y1.saturating_sub(rect.y0);
        if rect_width == 0 || rect_height == 0 {
            continue;
        }

        let row_bytes = rect_width * n;
        let mut data = Vec::with_capacity(rect_width * rect_height * n);
        for y in rect.y0..rect.y1 {
            let row_start = (y * width + rect.x0) * n;
            let row_end = row_start + row_bytes;
            data.extend_from_slice(&samples[row_start..row_end]);
        }

        regions.push(ImageRegion {
            x0: rect.x0,
            y0: rect.y0,
            width: rect_width,
            height: rect_height,
            data,
        });
    }

    regions
}

fn restore_image_regions(pixmap: &mut Pixmap, regions: &[ImageRegion]) {
    if regions.is_empty() {
        return;
    }

    let n = pixmap.n() as usize;
    let width = pixmap.width() as usize;
    let samples = pixmap.samples_mut();

    for region in regions {
        let row_bytes = region.width * n;
        let mut offset = 0;
        for y in region.y0..(region.y0 + region.height) {
            let row_start = (y * width + region.x0) * n;
            let row_end = row_start + row_bytes;
            let data_end = offset + row_bytes;
            samples[row_start..row_end].copy_from_slice(&region.data[offset..data_end]);
            offset = data_end;
        }
    }
}

fn pixmap_to_rgb(pixmap: &Pixmap) -> Result<Vec<u8>, WorkerFault> {
    let n = pixmap.n() as usize;
    if n < 3 {
        return Err(WorkerFault::generic(format!(
            "Unsupported pixmap format: {n} channels"
        )));
    }

    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let stride = pixmap.stride() as usize;
    let samples = pixmap.samples();
    let row_bytes = width * n;
    let expected_min = stride.saturating_mul(height);
    if samples.len() < expected_min || row_bytes > stride {
        return Err(WorkerFault::generic("Pixmap buffer size mismatch"));
    }

    let mut out = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        let row_start = y * stride;
        let row = &samples[row_start..row_start + row_bytes];
        if n == 3 {
            out.extend_from_slice(row);
        } else {
            for px in row.chunks_exact(n) {
                out.extend_from_slice(&px[..3]);
            }
        }
    }

    Ok(out)
}

pub(crate) fn extract_line_bounds(page: &Page, scale_factor: f32) -> Vec<LineBounds> {
    page.to_text_page(TextPageFlags::empty())
        .map(|text_page| {
            let mut bounds = Vec::new();
            let mut block_id = 0usize;

            for block in text_page.blocks() {
                if block.r#type() == TextBlockType::Text {
                    for line in block.lines() {
                        let bbox = line.bounds();
                        let chars: Vec<CharInfo> = line
                            .chars()
                            .filter_map(|ch| {
                                ch.char().map(|c| CharInfo {
                                    x: ch.origin().x * scale_factor,
                                    c,
                                })
                            })
                            .collect();

                        bounds.push(LineBounds {
                            x0: bbox.x0 * scale_factor,
                            y0: bbox.y0 * scale_factor,
                            x1: bbox.x1 * scale_factor,
                            y1: bbox.y1 * scale_factor,
                            chars,
                            block_id,
                        });
                    }
                    block_id += 1;
                }
            }
            bounds
        })
        .unwrap_or_default()
}

pub(crate) fn extract_link_rects(page: &Page, scale_factor: f32) -> Vec<LinkRect> {
    let Ok(links) = page.links() else {
        return Vec::new();
    };

    links
        .filter_map(|link| {
            let target = if let Some(dest) = link.dest {
                Some(LinkTarget::Internal {
                    page: dest.loc.page_number as usize,
                })
            } else if !link.uri.is_empty() {
                Some(LinkTarget::External {
                    uri: link.uri.clone(),
                })
            } else {
                None
            }?;

            let rect = link.bounds;
            if rect.is_empty() {
                return None;
            }

            let x0 = (rect.x0.min(rect.x1) * scale_factor).max(0.0);
            let y0 = (rect.y0.min(rect.y1) * scale_factor).max(0.0);
            let x1 = (rect.x0.max(rect.x1) * scale_factor).max(0.0);
            let y1 = (rect.y0.max(rect.y1) * scale_factor).max(0.0);

            Some(LinkRect {
                x0: x0 as u32,
                y0: y0 as u32,
                x1: x1 as u32,
                y1: y1 as u32,
                target,
            })
        })
        .collect()
}

struct TitleRect {
    topleft_x: u32,
    topleft_y: u32,
    bottomright_x: u32,
    bottomright_y: u32,
}

struct LineInfo {
    bbox: mupdf::Rect,
    max_size: f32,
    height: f32,
}

fn collect_title_rects(
    page: &Page,
    scale_factor: f32,
    page_width: f32,
) -> Result<Vec<TitleRect>, mupdf::error::Error> {
    let text_page = page.to_text_page(TextPageFlags::COLLECT_STYLES)?;
    let mut sizes = Vec::new();
    let mut heights = Vec::new();
    let mut lines = Vec::new();

    for block in text_page.blocks() {
        if block.r#type() != TextBlockType::Text {
            continue;
        }

        for line in block.lines() {
            let mut max_size: f32 = 0.0;
            let mut has_char = false;

            for ch in line.chars() {
                let size = ch.size();
                if size.is_finite() && size > 0.0 {
                    sizes.push(size);
                    max_size = max_size.max(size);
                    has_char = true;
                }
            }

            if has_char {
                let bbox = line.bounds();
                let height = (bbox.y1 - bbox.y0).abs();
                if height.is_finite() && height > 0.0 {
                    heights.push(height);
                }
                lines.push(LineInfo {
                    bbox,
                    max_size,
                    height,
                });
            }
        }
    }

    if sizes.is_empty() || lines.is_empty() || heights.is_empty() {
        return Ok(Vec::new());
    }

    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let median_size = sizes[sizes.len() / 2];
    let size_p90 = percentile(&sizes, 0.90).unwrap_or(median_size);
    let median_height = heights[heights.len() / 2];

    let min_size = (median_size * 1.8).max(size_p90);
    let min_size_looser = median_size * 1.5;

    Ok(title_rects_for_threshold(
        &lines,
        min_size,
        min_size_looser,
        median_height,
        page_width.max(1.0),
        scale_factor,
    ))
}

fn title_rects_for_threshold(
    lines: &[LineInfo],
    threshold: f32,
    threshold_loose: f32,
    median_height: f32,
    page_width: f32,
    scale_factor: f32,
) -> Vec<TitleRect> {
    lines
        .iter()
        .filter(|line| {
            let line_width = (line.bbox.x1 - line.bbox.x0).abs();
            let height_ok = line.height >= median_height * 1.1;
            let width_ok = line_width <= page_width * 0.7;

            line.max_size >= threshold
                || (line.max_size >= threshold_loose && height_ok && width_ok)
        })
        .map(|line| {
            let x0 = line.bbox.x0.min(line.bbox.x1);
            let x1 = line.bbox.x0.max(line.bbox.x1);
            let y0 = line.bbox.y0.min(line.bbox.y1);
            let y1 = line.bbox.y0.max(line.bbox.y1);

            TitleRect {
                topleft_x: (x0 * scale_factor).max(0.0) as u32,
                bottomright_x: (x1 * scale_factor).max(0.0) as u32,
                topleft_y: (y0 * scale_factor).max(0.0) as u32,
                bottomright_y: (y1 * scale_factor).max(0.0) as u32,
            }
        })
        .collect()
}

fn percentile(values: &[f32], pct: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let pct = pct.clamp(0.0, 1.0);
    let idx = ((values.len() - 1) as f32 * pct).round() as usize;
    values.get(idx).copied()
}

fn apply_title_color(pixmap: &mut Pixmap, rects: &[TitleRect], inverted: bool) {
    let n = pixmap.n() as usize;
    if n < 3 {
        return;
    }

    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let stride = pixmap.stride() as usize;
    let samples = pixmap.samples_mut();

    let mut clamped = Vec::new();
    let mut total_pixels: u64 = 0;
    for rect in rects {
        let x0 = rect.topleft_x.min(width as u32) as usize;
        let x1 = rect.bottomright_x.min(width as u32) as usize;
        let y0 = rect.topleft_y.min(height as u32) as usize;
        let y1 = rect.bottomright_y.min(height as u32) as usize;

        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        let rect_pixels = u64::from((x1 - x0) as u32) * u64::from((y1 - y0) as u32);
        total_pixels = total_pixels.saturating_add(rect_pixels);
        clamped.push((x0, x1, y0, y1));
    }

    if clamped.is_empty() {
        return;
    }

    let threshold = if inverted {
        u16::from(TITLE_LUMA_LIGHT_MIN)
    } else {
        u16::from(TITLE_LUMA_DARK_MAX)
    };

    let use_parallel = total_pixels >= 200_000 && height >= 4;
    if !use_parallel {
        for (x0, x1, y0, y1) in clamped {
            for y in y0..y1 {
                let row_start = y * stride;
                let start = row_start + x0 * n;
                let end = row_start + x1 * n;
                let row = &mut samples[start..end];

                match n {
                    3 => simd_luma::apply_title_rgb(row, TITLE_RGB, threshold, inverted),
                    4 => simd_luma::apply_title_rgba(row, TITLE_RGB, threshold, inverted),
                    _ => apply_title_row_scalar(row, n, TITLE_RGB, threshold, inverted),
                }
            }
        }
        return;
    }

    samples
        .par_chunks_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x0, x1, y0, y1) in &clamped {
                if y < *y0 || y >= *y1 {
                    continue;
                }
                let start = x0 * n;
                let end = x1 * n;
                let row = &mut row[start..end];

                match n {
                    3 => simd_luma::apply_title_rgb(row, TITLE_RGB, threshold, inverted),
                    4 => simd_luma::apply_title_rgba(row, TITLE_RGB, threshold, inverted),
                    _ => apply_title_row_scalar(row, n, TITLE_RGB, threshold, inverted),
                }
            }
        });
}

#[inline]
fn apply_title_row_scalar(
    row: &mut [u8],
    n: usize,
    title_rgb: (u8, u8, u8),
    threshold: u16,
    inverted: bool,
) {
    for px in row.chunks_exact_mut(n) {
        let r = px[0];
        let g = px[1];
        let b = px[2];
        let luma = (u16::from(r) * 54 + u16::from(g) * 183 + u16::from(b) * 19) / 256;

        let should_recolor = if inverted {
            luma >= threshold
        } else {
            luma <= threshold
        };

        if should_recolor {
            px[0] = title_rgb.0;
            px[1] = title_rgb.1;
            px[2] = title_rgb.2;
        }
    }
}

fn extract_text(
    doc: &Document,
    bounds_list: &[PageSelectionBounds],
    params: &RenderParams,
) -> String {
    let viewport_px = (
        f32::from(params.area.width) * f32::from(params.cell_size.width),
        f32::from(params.area.height) * f32::from(params.cell_size.height),
    );

    let mut text = String::new();

    for bounds in bounds_list {
        let Ok(page) = doc.load_page(bounds.page as i32) else {
            continue;
        };

        let Ok(pb) = page.bounds() else {
            continue;
        };
        let page_bounds = (pb.x1 - pb.x0, pb.y1 - pb.y0);
        let cell_dims = (
            f32::from(params.cell_size.width),
            f32::from(params.cell_size.height),
        );

        let spec = RasterSpec::compute(page_bounds, viewport_px, 1.0, cell_dims);

        let pdf_start_x = bounds.start_x / spec.mag;
        let pdf_end_x = bounds.end_x / spec.mag;
        let pdf_min_y = bounds.min_y / spec.mag;
        let pdf_max_y = bounds.max_y / spec.mag;

        if let Ok(text_page) = page.to_text_page(TextPageFlags::empty()) {
            let mut selected_lines: Vec<(f32, String, bool, bool)> = Vec::new();

            for block in text_page.blocks() {
                if block.r#type() == TextBlockType::Text {
                    for line in block.lines() {
                        let line_chars: Vec<_> = line.chars().collect();
                        if line_chars.is_empty() {
                            continue;
                        }

                        let first_y = line_chars[0].origin().y;
                        let line_bbox = line.bounds();
                        let line_top = line_bbox.y0;
                        let line_bottom = line_bbox.y1;

                        if line_bottom >= pdf_min_y && line_top <= pdf_max_y {
                            let is_first_line = line_top <= pdf_min_y && line_bottom >= pdf_min_y;
                            let is_last_line = line_top <= pdf_max_y && line_bottom >= pdf_max_y;

                            let mut line_text = String::new();
                            for ch in &line_chars {
                                let origin = ch.origin();
                                let include = if is_first_line && is_last_line {
                                    origin.x >= pdf_start_x && origin.x <= pdf_end_x
                                } else if is_first_line {
                                    origin.x >= pdf_start_x
                                } else if is_last_line {
                                    origin.x <= pdf_end_x
                                } else {
                                    true
                                };

                                if include {
                                    if let Some(c) = ch.char() {
                                        line_text.push(c);
                                    }
                                }
                            }

                            if !line_text.is_empty() {
                                selected_lines.push((
                                    first_y,
                                    line_text,
                                    is_first_line,
                                    is_last_line,
                                ));
                            }
                        }
                    }
                }
            }

            selected_lines
                .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            for (_, line_text, _, _) in selected_lines {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&line_text);
            }
        }

        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
    }

    text.trim().to_string()
}
