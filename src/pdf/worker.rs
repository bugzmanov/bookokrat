//! PDF/DjVu render worker - runs in separate thread(s)

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

enum DocumentBackend {
    Pdf(Document),
    Djvu(rdjvu::Document),
}

pub(crate) fn is_djvu_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        let lower = e.to_lowercase();
        lower == "djvu" || lower == "djv"
    })
}

const TITLE_RGB: (u8, u8, u8) = (0x66, 0x99, 0xCC);
const TITLE_LUMA_DARK_MAX: u8 = 90;
const TITLE_LUMA_LIGHT_MIN: u8 = 180;
const DJVU_INTERNAL_RENDER_SCALE: u32 = 2;

fn align_raster_to_cells(
    page_bounds: (f32, f32),
    mag: f32,
    cell_dims: (f32, f32),
) -> (f32, f32, f32) {
    let (page_width, page_height) = page_bounds;
    let (cell_width, cell_height) = cell_dims;

    // Snap the raster to whole terminal cells without changing the page aspect
    // ratio. Distorting the page at this stage makes text strokes look uneven.
    let width_cells = ((page_width * mag) / cell_width).floor().max(1.0);
    let height_cells = ((page_height * mag) / cell_height).floor().max(1.0);

    let width_mag = (width_cells * cell_width) / page_width;
    let height_mag = (height_cells * cell_height) / page_height;
    let aligned_mag = width_mag.min(height_mag);

    let output_width = (((page_width * aligned_mag) / cell_width).floor().max(1.0)) * cell_width;
    let output_height =
        (((page_height * aligned_mag) / cell_height).floor().max(1.0)) * cell_height;

    (aligned_mag, output_width, output_height)
}

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
        let base_mag = if page_width / page_height > view_width / view_height {
            view_height / page_height
        } else {
            view_width / page_width
        };

        let mut mag = base_mag * user_scale;
        let out_width = page_width * mag;
        let out_height = page_height * mag;

        let max_dim = out_width.max(out_height);
        if max_dim > KITTY_MAX_DIMENSION {
            let reduction = KITTY_MAX_DIMENSION / max_dim;
            mag *= reduction;
        }

        let (mag, output_width, output_height) = align_raster_to_cells(page_bounds, mag, cell_dims);

        Self {
            output_width,
            output_height,
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
    let backend = if is_djvu_path(doc_path) {
        match rdjvu::Document::open(doc_path) {
            Ok(d) => DocumentBackend::Djvu(d),
            Err(e) => {
                let _ = responses.send(RenderResponse::Error {
                    id: RequestId::new(0),
                    error: WorkerFault::generic(format!("DjVu: {e}")),
                });
                return;
            }
        }
    } else {
        match Document::open(doc_path.to_string_lossy().as_ref()) {
            Ok(d) => DocumentBackend::Pdf(d),
            Err(e) => {
                let _ = responses.send(RenderResponse::Error {
                    id: RequestId::new(0),
                    error: WorkerFault::Pdf(e),
                });
                return;
            }
        }
    };

    for request in requests {
        match request {
            RenderRequest::Page { id, page, params }
            | RenderRequest::Prefetch { id, page, params } => {
                handle_page_request_backend(&backend, id, page, &params, &cache, &responses);
            }

            RenderRequest::ExtractText { id, bounds, params } => {
                let text = match &backend {
                    DocumentBackend::Pdf(doc) => extract_text(doc, &bounds, &params),
                    DocumentBackend::Djvu(_) => String::new(),
                };
                let _ = responses.send(RenderResponse::ExtractedText { id, text });
            }

            RenderRequest::Cancel(id) => {
                let _ = responses.send(RenderResponse::Cancelled(id));
            }

            RenderRequest::Shutdown => break,
        }
    }
}

fn handle_page_request_backend(
    backend: &DocumentBackend,
    id: RequestId,
    page_num: usize,
    params: &RenderParams,
    cache: &Arc<Mutex<PageCache>>,
    responses: &Sender<RenderResponse>,
) {
    match backend {
        DocumentBackend::Pdf(doc) => {
            handle_page_request(doc, id, page_num, params, cache, responses);
        }
        DocumentBackend::Djvu(doc) => {
            handle_djvu_page_request(doc, id, page_num, params, cache, responses);
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
    let themed_rendering = params.black >= 0 && params.white >= 0;

    let image_regions = if themed_rendering && !params.invert_images {
        let rects = collect_image_rects(&page, spec.mag, pixmap.width(), pixmap.height());
        stash_image_regions(&pixmap, &rects)
    } else {
        Vec::new()
    };

    if themed_rendering {
        pixmap.tint(params.white, params.black)?;
    }

    if themed_rendering && !image_regions.is_empty() {
        restore_image_regions(&mut pixmap, &image_regions);
    }

    if themed_rendering {
        if let Ok(title_rects) = collect_title_rects(&page, spec.mag, page_bounds.0) {
            if !title_rects.is_empty() {
                apply_title_color(&mut pixmap, &title_rects, true);
            }
        }
    }

    let (base_dpi_x, base_dpi_y) = pixmap.resolution();
    pixmap.set_resolution(
        (base_dpi_x as f32 * spec.mag) as i32,
        (base_dpi_y as f32 * spec.mag) as i32,
    );

    let line_bounds = extract_line_bounds_merged(&page, spec.mag);
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
        requested_scale: params.scale,
        render_area_width_cells: params.area.width,
        render_area_height_cells: params.area.height,
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

/// Extract line bounds and merge lines that are on the same visual row.
/// This is useful for TOC extraction and text yanking where PDF text objects
/// are split across multiple lines but should be treated as a single row.
pub(crate) fn extract_line_bounds_merged(page: &Page, scale_factor: f32) -> Vec<LineBounds> {
    let raw_lines = extract_line_bounds(page, scale_factor);
    merge_same_row_lines(raw_lines)
}

/// Merge lines that have overlapping Y ranges into single visual rows.
fn merge_same_row_lines(lines: Vec<LineBounds>) -> Vec<LineBounds> {
    if lines.len() <= 1 {
        return lines;
    }

    // Sort by block_id first (reading order - left column before right),
    // then by Y position within each block
    let mut sorted = lines;
    sorted.sort_by(|a, b| {
        a.block_id
            .cmp(&b.block_id)
            .then_with(|| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut result = Vec::new();
    let mut current_group: Vec<LineBounds> = vec![sorted.remove(0)];

    for line in sorted {
        // Check if this line overlaps with the current group's Y range
        let group_y0 = current_group.iter().map(|l| l.y0).fold(f32::MAX, f32::min);
        let group_y1 = current_group.iter().map(|l| l.y1).fold(f32::MIN, f32::max);

        // Also check if line is from the same block (MuPDF uses blocks for columns)
        let same_block = current_group.iter().any(|l| l.block_id == line.block_id);

        if y_ranges_overlap(line.y0, line.y1, group_y0, group_y1) && same_block {
            current_group.push(line);
        } else {
            // Merge current group and start new one
            result.push(merge_line_group(current_group));
            current_group = vec![line];
        }
    }

    // Don't forget the last group
    if !current_group.is_empty() {
        result.push(merge_line_group(current_group));
    }

    result
}

/// Check if two Y ranges overlap significantly (at least 50% of smaller line's height).
/// This prevents merging consecutive lines that only barely touch.
fn y_ranges_overlap(y0_a: f32, y1_a: f32, y0_b: f32, y1_b: f32) -> bool {
    // Calculate overlap amount
    let overlap_start = y0_a.max(y0_b);
    let overlap_end = y1_a.min(y1_b);
    let overlap = (overlap_end - overlap_start).max(0.0);

    // Calculate heights
    let height_a = (y1_a - y0_a).max(0.1);
    let height_b = (y1_b - y0_b).max(0.1);
    let min_height = height_a.min(height_b);

    // Require at least 50% overlap relative to the smaller line
    overlap >= min_height * 0.5
}

/// Merge a group of lines on the same visual row into a single LineBounds.
fn merge_line_group(mut group: Vec<LineBounds>) -> LineBounds {
    if group.len() == 1 {
        return group.remove(0);
    }

    // Sort by X position (left to right)
    group.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal));

    // Compute merged bounding box
    let x0 = group.iter().map(|l| l.x0).fold(f32::MAX, f32::min);
    let y0 = group.iter().map(|l| l.y0).fold(f32::MAX, f32::min);
    let x1 = group.iter().map(|l| l.x1).fold(f32::MIN, f32::max);
    let y1 = group.iter().map(|l| l.y1).fold(f32::MIN, f32::max);

    // Merge characters, inserting spaces for gaps
    let mut chars = Vec::new();
    let mut last_x1: Option<f32> = None;

    for line in group {
        if let Some(prev_x1) = last_x1 {
            // Check gap between previous line's end and this line's start
            let gap = line.x0 - prev_x1;
            // If gap is significant, insert a space
            let avg_char_width = estimate_char_width(&line);
            if gap > avg_char_width * 0.5 {
                // Insert space at the gap position
                chars.push(CharInfo {
                    x: prev_x1 + gap / 2.0,
                    c: ' ',
                });
            }
        }
        chars.extend(line.chars);
        last_x1 = Some(line.x1);
    }

    LineBounds {
        x0,
        y0,
        x1,
        y1,
        chars,
        block_id: 0, // Merged lines don't have a single block_id
    }
}

/// Estimate average character width for a line
fn estimate_char_width(line: &LineBounds) -> f32 {
    if line.chars.len() < 2 {
        return 10.0; // Default fallback
    }
    (line.x1 - line.x0) / line.chars.len() as f32
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

// ============================================================
// DjVu rendering
// ============================================================

fn handle_djvu_page_request(
    doc: &rdjvu::Document,
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

    match render_djvu_page(doc, page_num, params) {
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

fn render_djvu_page(
    doc: &rdjvu::Document,
    page_num: usize,
    params: &RenderParams,
) -> Result<PageData, WorkerFault> {
    let page = doc
        .page(page_num)
        .map_err(|e| WorkerFault::generic(format!("DjVu page {page_num}: {e}")))?;

    let viewport_px = (
        f32::from(params.area.width) * f32::from(params.cell_size.width),
        f32::from(params.area.height) * f32::from(params.cell_size.height),
    );

    let page_width = page.display_width() as f32;
    let page_height = page.display_height() as f32;
    let page_bounds = (page_width, page_height);
    let cell_dims = (
        f32::from(params.cell_size.width),
        f32::from(params.cell_size.height),
    );

    let spec = DjvuRasterSpec::compute(page_bounds, viewport_px, params.scale, cell_dims);

    let output_w = spec.output_width as u32;
    let output_h = spec.output_height as u32;
    let target_w = output_w
        .saturating_mul(DJVU_INTERNAL_RENDER_SCALE)
        .min(page.display_width().max(output_w));
    let target_h = output_h
        .saturating_mul(DJVU_INTERNAL_RENDER_SCALE)
        .min(page.display_height().max(output_h));

    let themed_rendering = params.black >= 0 && params.white >= 0;

    let (mut pixels, width_px, height_px) =
        render_djvu_page_rgb(&page, page_num, target_w, target_h, themed_rendering)?;
    if themed_rendering {
        djvu_tint_rgb(&mut pixels, params.white, params.black);
    }

    Ok(PageData {
        img_data: ImageData {
            pixels,
            width_px,
            height_px,
            width_cell: (spec.output_width / f32::from(params.cell_size.width)) as u16,
            height_cell: (spec.output_height / f32::from(params.cell_size.height)) as u16,
        },
        page_num,
        scale_factor: spec.mag,
        requested_scale: params.scale,
        render_area_width_cells: params.area.width,
        render_area_height_cells: params.area.height,
        line_bounds: Vec::new(),
        link_rects: Vec::new(),
        page_height_px: height_px as f32,
    })
}

fn render_djvu_page_rgb(
    page: &rdjvu::Page<'_>,
    page_num: usize,
    target_w: u32,
    target_h: u32,
    themed: bool,
) -> Result<(Vec<u8>, u32, u32), WorkerFault> {
    let native_w = page.display_width();
    let native_h = page.display_height();

    // Normal (white bg): strong boldness to counteract perceptual thinning of
    // dark strokes on bright backgrounds.  Themed/inverted: no boost needed —
    // light-on-dark text already appears at correct weight.
    let boldness = if themed { 0.0 } else { 3.0 };

    let pixmap = if target_w >= native_w && target_h >= native_h {
        page.render()
    } else {
        page.render_aa(target_w, target_h, boldness)
    }
    .map_err(|e| WorkerFault::generic(format!("DjVu render page {page_num}: {e}")))?;

    Ok((djvu_pixmap_to_rgb(&pixmap), pixmap.width, pixmap.height))
}

/// Tint RGB pixel data, matching MuPDF `pixmap.tint(black, white)` semantics:
/// original black pixels → `black` color, original white pixels → `white` color.
/// Called as `tint(params.white, params.black)` — so foreground replaces black,
/// background replaces white.
fn djvu_tint_rgb(pixels: &mut [u8], black: i32, white: i32) {
    let br = ((black >> 16) & 0xFF) as i32;
    let bg = ((black >> 8) & 0xFF) as i32;
    let bb = (black & 0xFF) as i32;
    let wr = ((white >> 16) & 0xFF) as i32;
    let wg = ((white >> 8) & 0xFF) as i32;
    let wb = (white & 0xFF) as i32;

    for px in pixels.chunks_exact_mut(3) {
        let r = px[0] as i32;
        let g = px[1] as i32;
        let b = px[2] as i32;
        let luma = (r * 77 + g * 150 + b * 29) >> 8;

        px[0] = (br + (wr - br) * luma / 255) as u8;
        px[1] = (bg + (wg - bg) * luma / 255) as u8;
        px[2] = (bb + (wb - bb) * luma / 255) as u8;
    }
}

fn djvu_pixmap_to_rgb(pixmap: &rdjvu::Pixmap) -> Vec<u8> {
    let pixel_count = pixmap.width as usize * pixmap.height as usize;
    let mut out = Vec::with_capacity(pixel_count * 3);
    for i in 0..pixel_count {
        let base = i * 4;
        out.push(pixmap.data[base]);
        out.push(pixmap.data[base + 1]);
        out.push(pixmap.data[base + 2]);
    }
    out
}

struct DjvuRasterSpec {
    output_width: f32,
    output_height: f32,
    mag: f32,
}

impl DjvuRasterSpec {
    fn compute(
        page_bounds: (f32, f32),
        viewport_px: (f32, f32),
        user_scale: f32,
        cell_dims: (f32, f32),
    ) -> Self {
        let (page_width, page_height) = page_bounds;
        let (view_width, view_height) = viewport_px;
        let base_mag = if page_width / page_height > view_width / view_height {
            view_height / page_height
        } else {
            view_width / page_width
        };

        let mut mag = base_mag * user_scale;
        let out_width = page_width * mag;
        let out_height = page_height * mag;

        let max_dim = out_width.max(out_height);
        if max_dim > KITTY_MAX_DIMENSION {
            let reduction = KITTY_MAX_DIMENSION / max_dim;
            mag *= reduction;
        }

        let (mag, output_width, output_height) = align_raster_to_cells(page_bounds, mag, cell_dims);

        Self {
            output_width,
            output_height,
            mag,
        }
    }
}
