//! Image conversion pipeline for PDF rendering
//!
//! Converts rendered page data to terminal-displayable formats using
//! various protocols (Kitty, iTerm2, Sixel).

#![cfg(feature = "pdf")]

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::ops::Range;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use fast_image_resize as fir;
use flume::{Receiver, SendError, Sender};
use image::{DynamicImage, RgbImage, RgbaImage};
use ratatui::layout::Rect;
use rayon::prelude::*;

use crate::vendored::ratatui_image::{
    Resize,
    picker::{Picker, ProtocolType},
    protocol::{Protocol, iterm2::Iterm2},
};

use super::PageSource;
use super::kittyv2::ImageId;
use super::normal_mode::{CursorRect, VisualRect};
use super::selection::{HighlightOverlay, SelectionRect};
use super::types::{
    LineBounds, LinkRect, PageData, VecExt as _, ViewportUpdate, link_visual_boxes,
};

type PipelineError = super::request::WorkerFault;

/// Neighbour pages pre-encoded in Sixel mode. Matches the radius at which
/// `clear_distant_pixels` evicts page data, so nothing outside it can be encoded anyway.
const SIXEL_PREENCODE_RADIUS: usize = 5;

fn pipeline_error(msg: impl Into<String>) -> PipelineError {
    PipelineError::generic(msg)
}

/// Generate a unique image ID for a given page number.
fn page_image_id(page: usize) -> ImageId {
    ImageId::new(NonZeroU32::MIN.saturating_add(page as u32))
}

/// (current +- i) iterator
struct FocusPlusMinusOneIterator {
    start: usize,
    max_range: Range<usize>,
    step: usize,
}

impl FocusPlusMinusOneIterator {
    fn new(start: usize, range: Range<usize>) -> Self {
        debug_assert!(range.contains(&start), "start must be within range");
        Self {
            start,
            max_range: range,
            step: 0,
        }
    }
}

impl Iterator for FocusPlusMinusOneIterator {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        loop {
            if self.step >= self.max_range.len() * 2 {
                return None;
            }

            let offset = match self.step {
                0 => 0,
                s => {
                    let radius = s.div_ceil(2);
                    if s % 2 == 1 {
                        radius as isize
                    } else {
                        -(radius as isize)
                    }
                }
            };
            self.step += 1;

            if let Some(idx) = self.start.checked_add_signed(offset)
                && self.max_range.contains(&idx)
            {
                return Some(idx);
            }
        }
    }
}

pub type ImageState = super::kittyv2::ImageState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSize {
    pub width: u16,
    pub height: u16,
}

impl CellSize {
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub fn from_rect(r: Rect) -> Self {
        Self::new(r.width, r.height)
    }

    #[must_use]
    pub const fn as_tuple(self) -> (u16, u16) {
        (self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelDensity {
    x: u32,
    y: u32,
}

fn pixel_density_for_dims(
    width_px: u32,
    height_px: u32,
    cell_size: CellSize,
    picker: &Picker,
) -> PixelDensity {
    let (char_width, char_height) = picker.font_size();
    let fallback_x = u32::from(char_width).max(1);
    let fallback_y = u32::from(char_height).max(1);

    let x = if cell_size.width > 0 {
        (width_px / u32::from(cell_size.width)).max(1)
    } else {
        fallback_x
    };
    let y = if cell_size.height > 0 {
        (height_px / u32::from(cell_size.height)).max(1)
    } else {
        fallback_y
    };

    PixelDensity { x, y }
}

fn pixel_density_for_image(
    img: &DynamicImage,
    cell_size: CellSize,
    picker: &Picker,
) -> PixelDensity {
    pixel_density_for_dims(img.width(), img.height(), cell_size, picker)
}

pub enum ConvertedImage {
    Generic(Protocol),
    Tiled {
        tiles: Vec<TiledProtocol>,
        cell_size: CellSize,
    },
    Kitty {
        img: ImageState,
        cell_size: CellSize,
    },
    TileUpdate {
        tiles: Vec<TiledProtocol>,
        cell_size: CellSize,
    },
}

pub struct TiledProtocol {
    pub protocol: Arc<Protocol>,
    pub y_offset_cells: u16,
    pub height_cells: u16,
}

impl ConvertedImage {
    #[must_use]
    pub fn cell_dimensions(&self) -> CellSize {
        match self {
            Self::Generic(prot) => CellSize::from_rect(prot.area()),
            Self::Tiled { cell_size, .. }
            | Self::TileUpdate { cell_size, .. }
            | Self::Kitty { cell_size, .. } => *cell_size,
        }
    }

    /// Merge a TileUpdate into this image. Only works if self is Tiled.
    /// Returns true if merge was successful.
    pub fn merge_tile_update(&mut self, update: ConvertedImage) -> bool {
        let Self::Tiled { tiles, .. } = self else {
            return false;
        };
        let Self::TileUpdate {
            tiles: update_tiles,
            ..
        } = update
        else {
            return false;
        };

        for update_tile in update_tiles {
            if let Some(existing) = tiles
                .iter_mut()
                .find(|t| t.y_offset_cells == update_tile.y_offset_cells)
            {
                *existing = update_tile;
            } else {
                log::warn!(
                    "merge_tile_update: no tile at y_offset={} (existing: {:?})",
                    update_tile.y_offset_cells,
                    tiles.iter().map(|t| t.y_offset_cells).collect::<Vec<_>>()
                );
            }
        }
        true
    }
}

pub struct RenderedFrame {
    pub index: usize,
    pub requested_scale: f32,
    /// User zoom factor actually achieved by the worker (post max-dimension clamp).
    pub achieved_scale: f32,
    pub image: ConvertedImage,
}

#[derive(Clone, Copy, Debug)]
struct PixelRect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl PixelRect {
    fn new(x0: u32, y0: u32, x1: u32, y1: u32) -> Option<Self> {
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        Some(Self { x0, y0, x1, y1 })
    }

    fn clamp_to(&self, w: u32, h: u32) -> Option<Self> {
        let x0 = self.x0.min(w);
        let y0 = self.y0.min(h);
        let x1 = self.x1.min(w);
        let y1 = self.y1.min(h);
        Self::new(x0, y0, x1, y1)
    }

    fn intersects_y(&self, y0: u32, y1: u32) -> bool {
        !(self.y1 <= y0 || self.y0 >= y1)
    }

    fn offset_y(&self, offset: u32) -> Self {
        Self {
            x0: self.x0,
            y0: self.y0.saturating_sub(offset),
            x1: self.x1,
            y1: self.y1.saturating_sub(offset),
        }
    }
}

#[derive(Default, Clone)]
struct OverlaySet {
    highlights: Vec<HighlightPixelRect>,
    comments: Vec<PixelRect>,
    /// When true, `comments` contains pre-computed underline coordinates (for tile rendering).
    /// When false, `comments` contains selection rects and underline position is calculated.
    comments_are_underlines: bool,
    /// Solid stroke rects forming outline boxes around link clickboxes. These
    /// are final geometry (the four border strips of each box), drawn directly.
    link_strokes: Vec<PixelRect>,
    selection: Vec<PixelRect>,
    visual: Vec<PixelRect>,
    cursor: Option<PixelRect>,
}

impl OverlaySet {
    fn is_empty(&self) -> bool {
        self.comments.is_empty()
            && self.link_strokes.is_empty()
            && self.highlights.is_empty()
            && self.selection.is_empty()
            && self.visual.is_empty()
            && self.cursor.is_none()
    }

    fn for_tile(&self, tile_x: u32, tile_width: u32, tile_y: u32, tile_height: u32) -> Self {
        let tile_x_end = tile_x.saturating_add(tile_width);
        let tile_end = tile_y.saturating_add(tile_height);
        let clip = |rects: &[PixelRect]| -> Vec<PixelRect> {
            rects
                .iter()
                .filter(|rect| !(rect.x1 <= tile_x || rect.x0 >= tile_x_end))
                .filter(|rect| rect.intersects_y(tile_y, tile_end))
                .filter_map(|rect| {
                    let local_x0 = rect.x0.saturating_sub(tile_x);
                    let local_x1 = rect.x1.saturating_sub(tile_x).min(tile_width);
                    let local = rect.offset_y(tile_y);
                    PixelRect::new(local_x0, local.y0, local_x1, local.y1.min(tile_height))
                })
                .collect()
        };

        // Comments need special handling: underlines are drawn BELOW the rect
        // (at y1 + UNDERLINE_OFFSET with UNDERLINE_THICKNESS pixels).
        // We convert to underline coordinates here, then filter by tile intersection.
        // The stored rect IS the underline position (for tile rendering only).
        const UNDERLINE_OFFSET: u32 = 2;
        const UNDERLINE_THICKNESS: u32 = 3;
        let clip_comments = |rects: &[PixelRect]| -> Vec<PixelRect> {
            rects
                .iter()
                .filter_map(|rect| {
                    if rect.x1 <= tile_x || rect.x0 >= tile_x_end {
                        return None;
                    }
                    // Convert to underline coordinates (page-space)
                    let underline_y0 = rect.y1.saturating_add(UNDERLINE_OFFSET);
                    let underline_y1 = underline_y0.saturating_add(UNDERLINE_THICKNESS);
                    // Check if underline intersects this tile
                    if underline_y1 <= tile_y || underline_y0 >= tile_end {
                        return None;
                    }
                    // Convert to tile-local coordinates
                    let local_x0 = rect.x0.saturating_sub(tile_x);
                    let local_x1 = rect.x1.saturating_sub(tile_x).min(tile_width);
                    let local_y0 = underline_y0.saturating_sub(tile_y);
                    let local_y1 = underline_y1.saturating_sub(tile_y).min(tile_height);
                    PixelRect::new(local_x0, local_y0, local_x1, local_y1)
                })
                .collect()
        };

        let clip_highlights = |rects: &[HighlightPixelRect]| -> Vec<HighlightPixelRect> {
            rects
                .iter()
                .filter(|highlight| {
                    !(highlight.rect.x1 <= tile_x || highlight.rect.x0 >= tile_x_end)
                })
                .filter(|highlight| highlight.rect.intersects_y(tile_y, tile_end))
                .filter_map(|highlight| {
                    let local_x0 = highlight.rect.x0.saturating_sub(tile_x);
                    let local_x1 = highlight.rect.x1.saturating_sub(tile_x).min(tile_width);
                    let local = highlight.rect.offset_y(tile_y);
                    PixelRect::new(local_x0, local.y0, local_x1, local.y1.min(tile_height)).map(
                        |rect| HighlightPixelRect {
                            rect,
                            rgb: highlight.rgb,
                            alpha: highlight.alpha,
                        },
                    )
                })
                .collect()
        };

        let cursor = self.cursor.and_then(|rect| {
            if rect.intersects_y(tile_y, tile_end) && !(rect.x1 <= tile_x || rect.x0 >= tile_x_end)
            {
                PixelRect::new(
                    rect.x0.saturating_sub(tile_x),
                    rect.y0.saturating_sub(tile_y),
                    rect.x1.saturating_sub(tile_x).min(tile_width),
                    rect.y1.saturating_sub(tile_y).min(tile_height),
                )
            } else {
                None
            }
        });

        Self {
            highlights: clip_highlights(&self.highlights),
            comments: clip_comments(&self.comments),
            comments_are_underlines: true, // Tile rendering pre-computes underline positions
            // Link strokes are already final geometry, so clip them as plain rects.
            link_strokes: clip(&self.link_strokes),
            selection: clip(&self.selection),
            visual: clip(&self.visual),
            cursor,
        }
    }
}

#[derive(Clone)]
struct HighlightPixelRect {
    rect: PixelRect,
    rgb: crate::annotations::RgbColor,
    alpha: u8,
}

struct CachedPage {
    data: Arc<PageData>,
    tile_cache: HashMap<u32, Arc<Protocol>>,
}

impl CachedPage {
    fn has_source(&self) -> bool {
        !matches!(&self.data.img_data.source, PageSource::Raster(bytes) if bytes.is_empty())
    }

    fn density(&self, picker: &Picker) -> PixelDensity {
        let img = &self.data.img_data;
        pixel_density_for_dims(img.width_px, img.height_px, cached_cell_size(self), picker)
    }
}

pub enum ConversionCommand {
    SetPageCount(usize),
    NavigateTo(usize),
    EnqueuePage(Arc<PageData>),
    UpdateViewport(ViewportUpdate),
    UpdateDualViewport(Vec<ViewportUpdate>),
    UpdateSelection(Vec<SelectionRect>),
    UpdateComments(Vec<SelectionRect>),
    UpdateHighlights(Vec<HighlightOverlay>),
    UpdateCursor(Option<CursorRect>),
    UpdateVisual(Vec<VisualRect>),
    /// Toggle drawing of link clickbox underlines over all pages.
    SetShowLinkUnderlines(bool),
    InvalidatePageCache,
    /// Notify that display failed for these pages, allowing retry.
    DisplayFailed(Vec<usize>),
    /// Dump converter state for debugging.
    DumpState,
}

struct ConverterEngine {
    picker: Picker,
    prerender: usize,
    kitty_shm_support: bool,
    pid: u32,
    page: usize,
    images: Vec<Option<Arc<PageData>>>,
    page_cache: Vec<Option<CachedPage>>,
    selection_rects: Vec<SelectionRect>,
    comment_rects: Vec<SelectionRect>,
    highlight_overlays: Vec<HighlightOverlay>,
    comment_cache: HashMap<usize, CommentCacheEntry>,
    visual_rects: Vec<VisualRect>,
    cursor_rect: Option<CursorRect>,
    show_link_underlines: bool,
    viewport: Option<ViewportUpdate>,
    last_viewport_by_page: HashMap<usize, ViewportUpdate>,
    tiled_pages: HashSet<usize>,
    sent_for_viewport: HashSet<usize>,
    /// Pages that need cursor re-rendering once they arrive in cache.
    pending_cursor_pages: HashSet<usize>,
    /// True while the main thread drives the converter with dual-page viewports.
    dual_viewports: bool,
}

#[derive(Clone)]
struct CommentCacheEntry {
    rects: Vec<PixelRect>,
}

impl ConverterEngine {
    /// An iTerm2 page can only be encoded for a viewport currently on screen.
    /// Drop both the remembered viewport and its encoded strips on a page flip.
    fn retain_iterm_viewports(&mut self, visible: &[ViewportUpdate]) {
        if self.picker.protocol_type() != ProtocolType::Iterm2 {
            return;
        }
        self.last_viewport_by_page.retain(|page, _| {
            if visible.iter().any(|vp| vp.page == *page) {
                return true;
            }
            if let Some(Some(cached)) = self.page_cache.get_mut(*page) {
                cached.tile_cache.clear();
            }
            self.sent_for_viewport.remove(page);
            self.tiled_pages.remove(page);
            false
        });
        self.viewport = None;
    }

    fn handle_single_viewport_update(
        &mut self,
        new_viewport: ViewportUpdate,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        self.viewport = Some(new_viewport);
        let old_page_viewport = self.last_viewport_by_page.get(&new_viewport.page).copied();
        // Record the latest desired viewport per page even if we cannot
        // render immediately (e.g., page not cached yet). This allows
        // later EnqueuePage processing to render against the correct
        // viewport in dual-page non-Kitty mode.
        self.last_viewport_by_page
            .insert(new_viewport.page, new_viewport);
        // Drop distant pixel buffers when viewport moves, even if render is skipped.
        self.clear_distant_pixels(new_viewport.page, 5);

        // Invalidate tile cache when horizontal offset or width changes (tiles are keyed by Y only)
        if old_page_viewport.is_some_and(|old| {
            old.x_offset_cells != new_viewport.x_offset_cells
                || old.viewport_width_cells != new_viewport.viewport_width_cells
        }) && let Some(Some(cached)) = self.page_cache.get_mut(new_viewport.page)
        {
            cached.tile_cache.clear();
        }

        if self.picker.protocol_type() != ProtocolType::Kitty {
            let viewport_changed = old_page_viewport != Some(new_viewport);
            let already_sent = self.sent_for_viewport.contains(&new_viewport.page);

            if viewport_changed || !already_sent {
                let overlays = self.build_overlay_set(new_viewport.page);
                if let Some(Some(cached)) = self.page_cache.get_mut(new_viewport.page) {
                    match render_viewport_tiles(cached, &new_viewport, &overlays, &self.picker) {
                        Ok(img) => {
                            self.tiled_pages.insert(new_viewport.page);
                            self.sent_for_viewport.insert(new_viewport.page);
                            sender.send(Ok(RenderedFrame {
                                index: new_viewport.page,
                                requested_scale: cached.data.requested_scale,
                                achieved_scale: cached.data.achieved_scale,
                                image: img,
                            }))?;
                        }
                        Err(e) => {
                            sender.send(Err(e))?;
                        }
                    }
                }
            }
            return Ok(());
        }

        // For Kitty: skip reconversion if page already has an uploaded image.
        // The display layer will re-display the cached image at the new position.
        // Only reconvert if overlays changed or page not yet converted.
        let already_sent = self.sent_for_viewport.contains(&new_viewport.page);
        if already_sent {
            // Just update viewport tracking, don't reconvert
            return Ok(());
        }

        let overlays = self.build_overlay_set(new_viewport.page);
        if let Some(Some(cached)) = self.page_cache.get_mut(new_viewport.page) {
            let result = render_page_with_viewport(
                cached,
                new_viewport.page,
                &overlays,
                &self.picker,
                self.viewport.as_ref(),
                self.pid,
                self.kitty_shm_support,
            );

            match result {
                Ok(img) => {
                    self.tiled_pages.remove(&new_viewport.page);
                    self.sent_for_viewport.insert(new_viewport.page);

                    sender.send(Ok(RenderedFrame {
                        index: new_viewport.page,
                        requested_scale: cached.data.requested_scale,
                        achieved_scale: cached.data.achieved_scale,
                        image: img,
                    }))?;
                }
                Err(e) => {
                    sender.send(Err(e))?;
                }
            }
        }

        Ok(())
    }

    fn new(
        picker: Picker,
        prerender: usize,
        kitty_shm_support: bool,
        show_link_underlines: bool,
    ) -> Self {
        Self {
            picker,
            prerender,
            kitty_shm_support,
            pid: std::process::id(),
            page: 0,
            images: Vec::new(),
            page_cache: Vec::new(),
            selection_rects: Vec::new(),
            comment_rects: Vec::new(),
            highlight_overlays: Vec::new(),
            comment_cache: HashMap::new(),
            visual_rects: Vec::new(),
            cursor_rect: None,
            show_link_underlines,
            viewport: None,
            last_viewport_by_page: HashMap::new(),
            tiled_pages: HashSet::new(),
            sent_for_viewport: HashSet::new(),
            pending_cursor_pages: HashSet::new(),
            dual_viewports: false,
        }
    }

    fn next_page(&mut self, iteration: &mut usize) -> Result<Option<RenderedFrame>, PipelineError> {
        if self.images.is_empty() {
            return Ok(None);
        }

        let idx_start = self.page.saturating_sub(self.prerender / 2);
        let idx_end = idx_start
            .saturating_add(self.prerender)
            .min(self.images.len());

        if idx_end <= idx_start {
            return Ok(None);
        }

        let focus_page = self.page.clamp(idx_start, idx_end - 1);

        loop {
            if *iteration >= self.prerender {
                return Ok(None);
            }

            let Some((page_info, new_iter, page_num)) =
                self.pick_candidate(focus_page, idx_start..idx_end)
            else {
                return Ok(None);
            };

            self.update_cache_for_page(page_num, &page_info);

            if let Some(new_iter) = self.should_skip_render(page_num, new_iter) {
                // Page data is already in page_cache from update_cache_for_page above,
                // so UpdateViewport can render from cache later. Don't re-enqueue in
                // images — that would block pick_candidate from reaching other pages.
                *iteration = new_iter;
                if let Some(frame) = self.preencode_sixel_page(page_num)? {
                    return Ok(Some(frame));
                }
                continue;
            }

            let overlays = self.build_overlay_set(page_num);
            let img = self.render_page_from_cache(page_num, &overlays)?;

            *iteration = new_iter;

            self.sent_for_viewport.insert(page_info.page_num);

            // Clear pixel cache for distant pages to limit memory usage.
            // Keep pixels for nearby pages so overlays can be re-rendered.
            // Use self.page (navigation position) not the rendered page, so pages
            // near the user's view are preserved regardless of render order.
            self.clear_distant_pixels(self.page, 5);

            return Ok(Some(RenderedFrame {
                index: page_info.page_num,
                requested_scale: page_info.requested_scale,
                achieved_scale: page_info.achieved_scale,
                image: img,
            }));
        }
    }

    /// Sixel encoding is too slow to run on the keypress, so neighbour pages are encoded
    /// ahead for the viewport a page flip lands on (top of page, same pan and size) and
    /// sent to the main thread. If the flip's actual viewport matches, the frame is shown
    /// as-is; otherwise the regular viewport update re-renders it.
    fn preencode_sixel_page(
        &mut self,
        page_num: usize,
    ) -> Result<Option<RenderedFrame>, PipelineError> {
        if self.picker.protocol_type() != ProtocolType::Sixel
            || self.dual_viewports
            || page_num == self.page
            || page_num.abs_diff(self.page) > SIXEL_PREENCODE_RADIUS
            || self.sent_for_viewport.contains(&page_num)
            || self.last_viewport_by_page.contains_key(&page_num)
        {
            return Ok(None);
        }
        let Some(current) = self.viewport else {
            return Ok(None);
        };
        let predicted = ViewportUpdate {
            page: page_num,
            y_offset_cells: 0,
            ..current
        };
        let overlays = self.build_overlay_set(page_num);
        let Some(Some(cached)) = self.page_cache.get_mut(page_num) else {
            return Ok(None);
        };
        let image = render_viewport_tiles(cached, &predicted, &overlays, &self.picker)?;
        let frame = RenderedFrame {
            index: page_num,
            requested_scale: cached.data.requested_scale,
            achieved_scale: cached.data.achieved_scale,
            image,
        };
        self.last_viewport_by_page.insert(page_num, predicted);
        self.tiled_pages.insert(page_num);
        self.sent_for_viewport.insert(page_num);
        Ok(Some(frame))
    }

    fn pick_candidate(
        &mut self,
        focus_page: usize,
        range: std::ops::Range<usize>,
    ) -> Option<(Arc<PageData>, usize, usize)> {
        FocusPlusMinusOneIterator::new(focus_page, range)
            .enumerate()
            .take(self.prerender)
            .find_map(|(i_idx, p_idx)| self.images[p_idx].take().map(|p| (p, i_idx, p_idx)))
    }

    fn should_skip_render(&self, page_num: usize, new_iter: usize) -> Option<usize> {
        let needs_viewport = self.picker.protocol_type() != ProtocolType::Kitty;
        (self.sent_for_viewport.contains(&page_num)
            || (needs_viewport
                && !self.last_viewport_by_page.contains_key(&page_num)
                && !self.viewport.is_some_and(|vp| vp.page == page_num)))
        .then_some(new_iter)
    }

    fn update_cache_for_page(&mut self, page_num: usize, page_info: &Arc<PageData>) {
        if page_num >= self.page_cache.len() {
            return;
        }

        // Equal dimensions do not identify a render generation: a tint, original
        // image setting, or new document can change every pixel at the same size.
        if self.page_cache[page_num]
            .as_ref()
            .is_some_and(|cached| Arc::ptr_eq(&cached.data, page_info))
        {
            return;
        }
        self.sent_for_viewport.remove(&page_num);
        self.tiled_pages.remove(&page_num);
        self.page_cache[page_num] = Some(CachedPage {
            data: Arc::clone(page_info),
            tile_cache: HashMap::new(),
        });
        self.update_comment_cache_for_page(page_num, page_info.scale_factor);
    }

    fn handle_msg(
        &mut self,
        msg: ConversionCommand,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        match msg {
            ConversionCommand::EnqueuePage(img) => {
                let page_num = img.page_num;

                // For Kitty: skip enqueuing pages that were already sent.
                // The image is in Kitty's memory and can be re-displayed.
                if self.picker.protocol_type() == ProtocolType::Kitty
                    && self.sent_for_viewport.contains(&page_num)
                {
                    log::trace!(
                        "Converter: skipping EnqueuePage for page {page_num} (already sent to Kitty)"
                    );
                    return Ok(());
                }

                log::trace!(
                    "Converter: EnqueuePage for page {}, images.len={}",
                    page_num,
                    self.images.len()
                );
                if page_num < self.images.len() {
                    self.images[page_num] = Some(img);
                } else {
                    log::warn!(
                        "Converter: EnqueuePage index {} out of bounds (len={})",
                        page_num,
                        self.images.len()
                    );
                }
            }
            ConversionCommand::SetPageCount(n_pages) => {
                log::trace!("Converter: SetPageCount({n_pages})");
                self.images.reset_to_len(n_pages);
                self.page_cache.reset_to_len(n_pages);
                self.page = self.page.min(n_pages.saturating_sub(1));
                self.sent_for_viewport.clear();
                self.last_viewport_by_page.clear();
                self.viewport = None;
                self.tiled_pages.clear();
            }
            ConversionCommand::NavigateTo(new_page) => {
                log::trace!("Converter: NavigateTo({new_page})");
                self.page = new_page;
                // Update SHM protection to cover pages near current
                super::kittyv2::set_viewport_position(new_page as i64);
                // Also drop distant pixel buffers to cap memory even if no render happens.
                self.clear_distant_pixels(new_page, 5);
            }
            ConversionCommand::UpdateViewport(new_viewport) => {
                if std::env::var("BOOKOKRAT_DEBUG_NONKITTY_DUAL")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false)
                {
                    log::debug!(
                        "converter-viewport single page={} y={} x={} h={} w={}",
                        new_viewport.page,
                        new_viewport.y_offset_cells,
                        new_viewport.x_offset_cells,
                        new_viewport.viewport_height_cells,
                        new_viewport.viewport_width_cells
                    );
                }
                self.dual_viewports = false;
                self.retain_iterm_viewports(std::slice::from_ref(&new_viewport));
                self.handle_single_viewport_update(new_viewport, sender)?;
            }
            ConversionCommand::UpdateDualViewport(viewports) => {
                if std::env::var("BOOKOKRAT_DEBUG_NONKITTY_DUAL")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false)
                {
                    log::debug!(
                        "converter-viewport dual {:?}",
                        viewports
                            .iter()
                            .map(|v| {
                                (
                                    v.page,
                                    v.y_offset_cells,
                                    v.x_offset_cells,
                                    v.viewport_height_cells,
                                    v.viewport_width_cells,
                                )
                            })
                            .collect::<Vec<_>>()
                    );
                }
                self.dual_viewports = true;
                self.retain_iterm_viewports(&viewports);
                for viewport in viewports {
                    self.handle_single_viewport_update(viewport, sender)?;
                }
            }
            ConversionCommand::UpdateSelection(new_rects) => {
                let old_rects = std::mem::take(&mut self.selection_rects);
                let affected = Self::collect_affected_pages(&old_rects, &new_rects);
                self.selection_rects = new_rects;
                self.invalidate_tiles_for_pages(&affected);
                self.reconvert_pages(&affected, sender)?;
            }
            ConversionCommand::UpdateComments(new_rects) => {
                log::trace!("UpdateComments received: {} rects", new_rects.len());
                let old_rects = std::mem::take(&mut self.comment_rects);
                let affected = Self::collect_affected_pages(&old_rects, &new_rects);
                self.comment_rects = new_rects;
                self.comment_cache = self.build_comment_cache(&self.comment_rects);
                self.invalidate_tiles_for_pages(&affected);
                self.reconvert_pages(&affected, sender)?;
            }
            ConversionCommand::UpdateHighlights(new_overlays) => {
                let old = std::mem::take(&mut self.highlight_overlays);
                let affected = Self::collect_affected_pages(&old, &new_overlays);
                self.highlight_overlays = new_overlays;
                self.invalidate_tiles_for_pages(&affected);
                self.reconvert_pages(&affected, sender)?;
            }
            ConversionCommand::UpdateCursor(new_cursor) => {
                log::trace!(
                    "Converter: UpdateCursor page={:?}",
                    new_cursor.as_ref().map(|c| c.page)
                );
                let old_cursor = std::mem::replace(&mut self.cursor_rect, new_cursor.clone());
                self.reconvert_cursor_change(old_cursor.as_ref(), new_cursor.as_ref(), sender)?;
            }
            ConversionCommand::UpdateVisual(new_visual) => {
                let old_visual = std::mem::replace(&mut self.visual_rects, new_visual);
                let affected = Self::collect_affected_pages(&old_visual, &self.visual_rects);
                if self.picker.protocol_type() == ProtocolType::Iterm2 {
                    for page in affected {
                        let ranges = old_visual
                            .iter()
                            .chain(&self.visual_rects)
                            .filter(|rect| rect.page == page)
                            .map(|rect| rect.y..rect.y.saturating_add(rect.height));
                        Self::invalidate_row_ranges(
                            &mut self.page_cache,
                            page,
                            &self.picker,
                            ranges,
                        );
                        self.send_dirty_tiles(page, sender)?;
                    }
                } else {
                    self.invalidate_tiles_for_pages(&affected);
                    self.reconvert_pages(&affected, sender)?;
                }
            }
            ConversionCommand::SetShowLinkUnderlines(show) => {
                if self.show_link_underlines != show {
                    self.show_link_underlines = show;
                    // Re-render every cached page that has link clickboxes so the
                    // overlay change is reflected without re-rendering the pixmap.
                    let affected: HashSet<usize> = self
                        .page_cache
                        .iter()
                        .filter_map(|cached| cached.as_ref())
                        .filter(|cached| !cached.data.link_rects.is_empty())
                        .map(|cached| cached.data.page_num)
                        .collect();
                    self.invalidate_tiles_for_pages(&affected);
                    self.reconvert_pages(&affected, sender)?;
                }
            }
            ConversionCommand::InvalidatePageCache => {
                for img in &mut self.images {
                    *img = None;
                }
                for cached in &mut self.page_cache {
                    *cached = None;
                }
                self.tiled_pages.clear();
                self.comment_cache.clear();
                self.sent_for_viewport.clear();
                self.last_viewport_by_page.clear();
                self.viewport = None;
                // Clear overlay state to prevent stale rendering after cache invalidation
                self.cursor_rect = None;
                self.visual_rects.clear();
                self.selection_rects.clear();
            }
            ConversionCommand::DisplayFailed(pages) => {
                // Clear these pages from sent_for_viewport so they can be re-sent.
                // This handles the case where Kitty transmission failed (e.g., SHM
                // was unlinked before Kitty read it).
                if !pages.is_empty() {
                    log::debug!(
                        "Display failed for {} pages, clearing for retry",
                        pages.len()
                    );
                }
                for page in pages {
                    self.sent_for_viewport.remove(&page);
                    if self.picker.protocol_type() != ProtocolType::Iterm2 {
                        self.last_viewport_by_page.remove(&page);
                    }
                }
            }
            ConversionCommand::DumpState => {
                self.dump_debug_state();
            }
        }

        Ok(())
    }

    fn collect_affected_pages<T: PageScoped>(old: &[T], new: &[T]) -> HashSet<usize> {
        let mut affected: HashSet<usize> = HashSet::new();
        for rect in old {
            affected.insert(rect.page());
        }
        for rect in new {
            affected.insert(rect.page());
        }
        affected
    }

    fn invalidate_tiles_for_pages(&mut self, affected: &HashSet<usize>) {
        if !affected.is_empty() {
            log::debug!("invalidate_tiles_for_pages: clearing tiles for pages {affected:?}");
        }
        for page_num in affected {
            self.tiled_pages.remove(page_num);
            if let Some(Some(cached)) = self.page_cache.get_mut(*page_num) {
                cached.tile_cache.clear();
            }
        }
    }

    fn reconvert_pages(
        &mut self,
        affected: &HashSet<usize>,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        for page_num in affected {
            // Remove from sent set so the page will be re-rendered with new overlays
            self.sent_for_viewport.remove(page_num);

            if !self
                .page_cache
                .get(*page_num)
                .and_then(Option::as_ref)
                .is_some_and(CachedPage::has_source)
            {
                continue;
            }

            let overlays = self.build_overlay_set(*page_num);
            self.rerender_page(*page_num, &overlays, sender)?;
        }
        Ok(())
    }

    /// Re-renders `page_num` with `overlays` and sends the frame. Non-Kitty protocols render
    /// tiles for the page's own viewport: in dual layout each page has its own, and the last
    /// viewport handled belongs to the other page. A page without a viewport is not displayed
    /// and gets rendered with its overlays once its viewport arrives.
    fn rerender_page(
        &mut self,
        page_num: usize,
        overlays: &OverlaySet,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let use_tiles = self.picker.protocol_type() != ProtocolType::Kitty;
        let page_viewport = self
            .last_viewport_by_page
            .get(&page_num)
            .copied()
            .or_else(|| self.viewport.filter(|vp| vp.page == page_num));
        if use_tiles && page_viewport.is_none() {
            return Ok(());
        }
        let Some(Some(cached)) = self.page_cache.get_mut(page_num) else {
            return Ok(());
        };
        let result = match page_viewport {
            Some(viewport) if use_tiles => {
                render_viewport_tiles(cached, &viewport, overlays, &self.picker)
            }
            _ => render_page_with_viewport(
                cached,
                page_num,
                overlays,
                &self.picker,
                self.viewport.as_ref(),
                self.pid,
                self.kitty_shm_support,
            ),
        };
        match result {
            Ok(image) => {
                if use_tiles {
                    self.tiled_pages.insert(page_num);
                }
                self.sent_for_viewport.insert(page_num);
                sender.send(Ok(RenderedFrame {
                    index: page_num,
                    requested_scale: cached.data.requested_scale,
                    achieved_scale: cached.data.achieved_scale,
                    image,
                }))
            }
            Err(e) => sender.send(Err(e)),
        }
    }

    /// Convert page-pixel ranges to absolute strip indices, independent of
    /// either page's pan/scroll in a two-page spread.
    fn invalidate_row_ranges(
        pages: &mut [Option<CachedPage>],
        page: usize,
        picker: &Picker,
        ranges: impl Iterator<Item = Range<u32>>,
    ) {
        let Some(Some(cached)) = pages.get_mut(page) else {
            return;
        };
        let row_height = cached.density(picker).y;
        for range in ranges {
            let end = range.end.min(cached.data.img_data.height_px);
            if range.start < end {
                for row in range.start / row_height..end.div_ceil(row_height) {
                    cached.tile_cache.remove(&row);
                }
            }
        }
    }

    fn send_dirty_tiles(
        &mut self,
        page: usize,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let Some(viewport) = self.last_viewport_by_page.get(&page).copied() else {
            return Ok(());
        };
        let overlays = self.build_overlay_set(page);
        if !self.tiled_pages.contains(&page) {
            return self.rerender_page(page, &overlays, sender);
        }
        let Some(Some(cached)) = self.page_cache.get_mut(page) else {
            return Ok(());
        };
        match render_tiles(cached, &viewport, &overlays, &self.picker, true) {
            Ok((tiles, cell_size)) if !tiles.is_empty() => sender.send(Ok(RenderedFrame {
                index: page,
                requested_scale: cached.data.requested_scale,
                achieved_scale: cached.data.achieved_scale,
                image: ConvertedImage::TileUpdate { tiles, cell_size },
            })),
            Ok(_) => Ok(()),
            Err(e) => sender.send(Err(e)),
        }
    }

    fn reconvert_cursor_change(
        &mut self,
        old_cursor: Option<&CursorRect>,
        new_cursor: Option<&CursorRect>,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let iterm = self.picker.protocol_type() == ProtocolType::Iterm2;
        let cursors = [
            old_cursor.map(|c| expand_cursor_rect(c, &self.picker)),
            new_cursor.map(|c| expand_cursor_rect(c, &self.picker)),
        ];
        for (i, cursor) in cursors.iter().enumerate() {
            let Some(cursor) = cursor else { continue };
            if i == 1
                && cursors[0]
                    .as_ref()
                    .is_some_and(|old| old.page == cursor.page)
            {
                continue;
            }
            let ranges = cursors
                .iter()
                .flatten()
                .filter(|c| c.page == cursor.page)
                .map(|c| c.y..c.y.saturating_add(c.height));
            Self::invalidate_row_ranges(&mut self.page_cache, cursor.page, &self.picker, ranges);
            if iterm {
                self.send_dirty_tiles(cursor.page, sender)?;
            }
        }
        if iterm {
            Ok(())
        } else {
            self.render_cursor_full_pages(old_cursor, new_cursor, sender)
        }
    }

    fn render_cursor_full_pages(
        &mut self,
        old_cursor: Option<&CursorRect>,
        new_cursor: Option<&CursorRect>,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let mut affected: HashSet<usize> = HashSet::new();
        if let Some(cursor) = old_cursor {
            affected.insert(cursor.page);
        }
        if let Some(cursor) = new_cursor {
            affected.insert(cursor.page);
        }

        for page_num in affected {
            // Remove from sent set so the page will be re-rendered with new cursor
            self.sent_for_viewport.remove(&page_num);

            let Some(Some(cached)) = self.page_cache.get(page_num) else {
                // Page not in cache yet - mark as pending for when it arrives
                if new_cursor.is_some_and(|c| c.page == page_num) {
                    log::trace!(
                        "render_cursor_full_pages: page {page_num} not in cache, marking as pending"
                    );
                    self.pending_cursor_pages.insert(page_num);
                }
                continue;
            };

            // Skip if no pixel data (distant page was cleared)
            if !cached.has_source() {
                // Mark as pending - pixels may arrive later
                if new_cursor.is_some_and(|c| c.page == page_num) {
                    log::trace!(
                        "render_cursor_full_pages: page {page_num} has empty pixels, marking as pending"
                    );
                    self.pending_cursor_pages.insert(page_num);
                }
                continue;
            }

            // Successfully rendering - remove from pending
            self.pending_cursor_pages.remove(&page_num);
            let overlays = self.build_overlay_set_with_cursor(page_num, new_cursor);
            self.rerender_page(page_num, &overlays, sender)?;
        }

        Ok(())
    }

    fn render_page_from_cache(
        &mut self,
        page_num: usize,
        overlays: &OverlaySet,
    ) -> Result<ConvertedImage, PipelineError> {
        let Some(Some(cached)) = self.page_cache.get_mut(page_num) else {
            return Err(pipeline_error("Missing cached page"));
        };

        let page_viewport = self
            .last_viewport_by_page
            .get(&page_num)
            .copied()
            .or_else(|| self.viewport.filter(|vp| vp.page == page_num));

        // Use tile rendering for all non-Kitty protocols (matches reconvert_pages logic)
        if self.picker.protocol_type() != ProtocolType::Kitty {
            let viewport = page_viewport
                .as_ref()
                .ok_or_else(|| pipeline_error("Cannot encode strips without a viewport"))?;
            let image = render_viewport_tiles(cached, viewport, overlays, &self.picker)?;
            self.tiled_pages.insert(page_num);
            return Ok(image);
        }

        render_page_with_viewport(
            cached,
            page_num,
            overlays,
            &self.picker,
            page_viewport.as_ref(),
            self.pid,
            self.kitty_shm_support,
        )
    }

    fn build_overlay_set(&self, page_num: usize) -> OverlaySet {
        self.build_overlay_set_with_cursor(page_num, self.cursor_rect.as_ref())
    }

    fn build_overlay_set_with_cursor(
        &self,
        page_num: usize,
        cursor: Option<&CursorRect>,
    ) -> OverlaySet {
        let mut overlays = OverlaySet::default();

        if let Some(Some(cached)) = self.page_cache.get(page_num) {
            if let Some(cached_comments) = self.comment_cache.get(&page_num) {
                log::debug!(
                    "get_page_overlays: page={} comment_rects={}",
                    page_num,
                    cached_comments.rects.len()
                );
                overlays.comments.clone_from(&cached_comments.rects);
            }
            // Link clickboxes and text lines are both stored in pixel space at the
            // page's render scale, so they map directly onto the cached pixmap.
            if self.show_link_underlines {
                overlays.link_strokes =
                    link_box_stroke_rects(&cached.data.link_rects, &cached.data.line_bounds);
            }
        } else {
            log::debug!("get_page_overlays: page={page_num} - no page cache or no comments");
        }

        for highlight in &self.highlight_overlays {
            if highlight.rect.page != page_num {
                continue;
            }
            let scale = self
                .page_cache
                .get(page_num)
                .and_then(|cached| cached.as_ref())
                .map(|cached| f64::from(cached.data.scale_factor))
                .unwrap_or(1.0);
            if let Some(rect) = PixelRect::new(
                (f64::from(highlight.rect.topleft_x) * scale).round() as u32,
                (f64::from(highlight.rect.topleft_y) * scale).round() as u32,
                (f64::from(highlight.rect.bottomright_x) * scale).round() as u32,
                (f64::from(highlight.rect.bottomright_y) * scale).round() as u32,
            ) {
                overlays.highlights.push(HighlightPixelRect {
                    rect,
                    rgb: highlight.rgb,
                    alpha: highlight.alpha,
                });
            }
        }

        for sel in &self.selection_rects {
            if sel.page != page_num {
                continue;
            }
            if let Some(rect) = PixelRect::new(
                sel.topleft_x,
                sel.topleft_y,
                sel.bottomright_x,
                sel.bottomright_y,
            ) {
                overlays.selection.push(rect);
            }
        }

        for vis in &self.visual_rects {
            if vis.page != page_num {
                continue;
            }
            let x1 = vis.x.saturating_add(vis.width);
            let y1 = vis.y.saturating_add(vis.height);
            if let Some(rect) = PixelRect::new(vis.x, vis.y, x1, y1) {
                overlays.visual.push(rect);
            }
        }

        if let Some(cursor) = cursor
            && cursor.page == page_num
        {
            let expanded = expand_cursor_rect(cursor, &self.picker);
            let x1 = expanded.x.saturating_add(expanded.width);
            let y1 = expanded.y.saturating_add(expanded.height);
            if let Some(rect) = PixelRect::new(expanded.x, expanded.y, x1, y1) {
                overlays.cursor = Some(rect);
            }
        }

        overlays
    }

    fn build_comment_cache(&self, rects: &[SelectionRect]) -> HashMap<usize, CommentCacheEntry> {
        let mut cache: HashMap<usize, CommentCacheEntry> = HashMap::new();
        for rect in rects {
            let Some(Some(cached)) = self.page_cache.get(rect.page) else {
                continue;
            };
            let rects_px = comment_rects_for_page(rects, rect.page, cached.data.scale_factor);
            if rects_px.is_empty() {
                continue;
            }
            cache.insert(rect.page, CommentCacheEntry { rects: rects_px });
        }
        cache
    }

    fn update_comment_cache_for_page(&mut self, page_num: usize, scale_factor: f32) {
        let rects_px = comment_rects_for_page(&self.comment_rects, page_num, scale_factor);
        if rects_px.is_empty() {
            self.comment_cache.remove(&page_num);
            return;
        }
        self.comment_cache
            .insert(page_num, CommentCacheEntry { rects: rects_px });
    }

    /// Clear decoded/tiles for pages far from current to limit memory usage.
    /// Keeps nearby pages so overlays can still be re-rendered quickly.
    /// Also protects the cursor page and pages with pending cursor updates.
    fn clear_distant_pixels(&mut self, current_page: usize, radius: usize) {
        let start = current_page.saturating_sub(radius);
        let end = current_page.saturating_add(radius);

        // Protect the cursor page from clearing
        let cursor_page = self.cursor_rect.as_ref().map(|c| c.page);

        // First, clear images Vec for distant pages (even if not in page_cache yet)
        for (i, img) in self.images.iter_mut().enumerate() {
            if img.is_some()
                && (i < start || i > end)
                && cursor_page != Some(i)
                && !self.pending_cursor_pages.contains(&i)
            {
                *img = None;
            }
        }

        // Then clear page_cache for distant pages
        for (i, cached) in self.page_cache.iter_mut().enumerate() {
            if let Some(page) = cached {
                // Skip pages within radius
                if i >= start && i <= end {
                    continue;
                }
                // Skip cursor page - we need its pixels to render cursor overlay
                if cursor_page == Some(i) {
                    continue;
                }
                // Skip pages with pending cursor updates
                if self.pending_cursor_pages.contains(&i) {
                    continue;
                }
                if !page.tile_cache.is_empty() {
                    log::trace!("Clearing tile cache for distant page {i}");
                }
                // Drop cached page data entirely to free pixel buffers.
                // Also remove from sent_for_viewport so it can be re-rendered if needed.
                *cached = None;
                self.sent_for_viewport.remove(&i);
            }
        }
    }

    /// Log memory usage statistics for debugging.
    fn log_memory_stats(&self) {
        let mut cached_pages = 0usize;
        let mut pixel_bytes = 0usize;
        let mut tile_cache_count = 0usize;

        for cached in self.page_cache.iter().flatten() {
            cached_pages += 1;
            if let PageSource::Raster(bytes) = &cached.data.img_data.source {
                pixel_bytes += bytes.len();
            }
            tile_cache_count += cached.tile_cache.len();
        }

        let pixel_mb = pixel_bytes as f64 / (1024.0 * 1024.0);
        log::info!(
            "Converter memory: {} cached pages ({:.1} MB raster pixels), {} tiles, {} sent",
            cached_pages,
            pixel_mb,
            tile_cache_count,
            self.sent_for_viewport.len()
        );
    }

    /// Dump full converter state for debugging.
    fn dump_debug_state(&self) {
        log::info!("=== CONVERTER DEBUG DUMP ===");
        log::info!("  current_page={}", self.page);
        log::info!("  images.len={}", self.images.len());
        log::info!("  page_cache.len={}", self.page_cache.len());
        log::info!(
            "  sent_for_viewport ({} pages): {:?}",
            self.sent_for_viewport.len(),
            self.sent_for_viewport
        );
        log::info!("  tiled_pages: {:?}", self.tiled_pages);

        // Log which pages have images queued
        let queued_pages: Vec<usize> = self
            .images
            .iter()
            .enumerate()
            .filter_map(|(i, img)| img.as_ref().map(|_| i))
            .collect();
        log::info!("  queued_images (pages with pending data): {queued_pages:?}");

        // Log page cache status for pages around current
        let start = self.page.saturating_sub(5);
        let end = (self.page + 5).min(self.page_cache.len());
        log::info!("  page_cache status (pages {start}..{end}):");
        for i in start..end {
            if let Some(Some(cached)) = self.page_cache.get(i) {
                let has_source = cached.has_source();
                let in_sent = self.sent_for_viewport.contains(&i);
                log::info!("    page {i}: source={has_source}, sent={in_sent}");
            } else {
                log::info!("    page {i}: (no cache)");
            }
        }

        // Also dump SHM state
        super::kittyv2::dump_shm_state();

        log::info!("=== END CONVERTER DUMP ===");
    }
}

trait PageScoped {
    fn page(&self) -> usize;
}

impl PageScoped for SelectionRect {
    fn page(&self) -> usize {
        self.page
    }
}

impl PageScoped for HighlightOverlay {
    fn page(&self) -> usize {
        self.rect.page
    }
}

impl PageScoped for VisualRect {
    fn page(&self) -> usize {
        self.page
    }
}

fn expand_cursor_rect(cursor: &CursorRect, picker: &Picker) -> CursorRect {
    let (char_width, char_height) = picker.font_size();
    CursorRect {
        page: cursor.page,
        x: cursor.x,
        y: cursor.y,
        width: cursor.width.max(u32::from(char_width)),
        height: cursor.height.max(u32::from(char_height)),
    }
}

fn decode_pixels(
    pixels: &[u8],
    width: u32,
    height: u32,
    channels: u8,
) -> Result<DynamicImage, PipelineError> {
    copy_pixel_region(
        pixels,
        width,
        height,
        channels,
        PixelRect {
            x0: 0,
            y0: 0,
            x1: width,
            y1: height,
        },
    )
}

/// Make just the owned strip the protocol encoder needs. For a compiled
/// band this is a single contiguous copy, not a per-pixel crop.
fn copy_pixel_region(
    pixels: &[u8],
    width: u32,
    height: u32,
    channels: u8,
    rect: PixelRect,
) -> Result<DynamicImage, PipelineError> {
    let bpp = if channels == 4 { 4 } else { 3 };
    let expected = width
        .checked_mul(height)
        .and_then(|v| v.checked_mul(bpp))
        .ok_or_else(|| pipeline_error("pixel size overflow"))? as usize;
    if expected != pixels.len() {
        return Err(pipeline_error(format!(
            "pixel buffer size mismatch: expected {expected}, got {}",
            pixels.len()
        )));
    }
    let crop_w = rect.x1 - rect.x0;
    let crop_h = rect.y1 - rect.y0;
    let stride = width as usize * bpp as usize;
    let row_bytes = crop_w as usize * bpp as usize;
    let offset = rect.y0 as usize * stride + rect.x0 as usize * bpp as usize;
    let len = row_bytes * crop_h as usize;
    let bytes = if row_bytes == stride {
        pixels[offset..offset + len].to_vec()
    } else {
        let mut bytes = Vec::with_capacity(len);
        for row in 0..crop_h as usize {
            let start = offset + row * stride;
            bytes.extend_from_slice(&pixels[start..start + row_bytes]);
        }
        bytes
    };
    if bpp == 4 {
        RgbaImage::from_raw(crop_w, crop_h, bytes).map(DynamicImage::ImageRgba8)
    } else {
        RgbImage::from_raw(crop_w, crop_h, bytes).map(DynamicImage::ImageRgb8)
    }
    .ok_or_else(|| pipeline_error("Can't build image from raw pixels"))
}

#[inline]
fn cached_cell_size(cached: &CachedPage) -> CellSize {
    CellSize::new(
        cached.data.img_data.width_cell,
        cached.data.img_data.height_cell,
    )
}

fn crop_to_viewport(
    mut img: DynamicImage,
    cell_size: CellSize,
    viewport: &ViewportUpdate,
    picker: &Picker,
) -> (DynamicImage, CellSize) {
    let density = pixel_density_for_image(&img, cell_size, picker);
    let y_px = viewport.y_offset_cells.saturating_mul(density.y);
    let mut area_cell_height = cell_size.height;
    let mut area_cell_width = cell_size.width;

    if y_px < img.height() {
        area_cell_height = viewport.viewport_height_cells;
        let viewport_px = u32::from(viewport.viewport_height_cells).saturating_mul(density.y);
        let max_height = img.height().saturating_sub(y_px);
        let crop_height = viewport_px.min(max_height).max(1);
        img = img.crop_imm(0, y_px, img.width(), crop_height);
        if crop_height < viewport_px {
            if let DynamicImage::ImageRgba8(rgba) = &img {
                // Transparent pages pad below the content with transparent pixels.
                let mut padded =
                    image::ImageBuffer::from_pixel(rgba.width(), viewport_px, image::Rgba([0; 4]));
                let src = rgba.as_raw();
                let dst = padded.as_mut();
                let len = src.len().min(dst.len());
                dst[..len].copy_from_slice(&src[..len]);
                img = DynamicImage::ImageRgba8(padded);
            } else {
                let rgb = img.to_rgb8();
                let bg = rgb.get_pixel(0, 0);
                let mut padded = image::ImageBuffer::from_pixel(rgb.width(), viewport_px, *bg);
                let src = rgb.as_raw();
                let dst = padded.as_mut();
                let len = src.len().min(dst.len());
                dst[..len].copy_from_slice(&src[..len]);
                img = DynamicImage::ImageRgb8(padded);
            }
        }
    }

    // Horizontal cropping for non-Kitty protocols (iTerm2 has no viewport clipping)
    if picker.protocol_type() != ProtocolType::Kitty
        && cell_size.width > viewport.viewport_width_cells
    {
        area_cell_width = viewport.viewport_width_cells;
        let x_px = viewport.x_offset_cells.saturating_mul(density.x);
        let target_w_px = u32::from(viewport.viewport_width_cells).saturating_mul(density.x);
        if target_w_px < img.width() {
            let x_px = x_px.min(img.width().saturating_sub(1));
            let available = img.width().saturating_sub(x_px);
            img = img.crop_imm(x_px, 0, target_w_px.min(available), img.height());
        }
    }

    (img, CellSize::new(area_cell_width, area_cell_height))
}

fn normalize_for_protocol(img: &mut DynamicImage, cell_size: CellSize, picker: &Picker) {
    if matches!(
        picker.protocol_type(),
        ProtocolType::Kitty | ProtocolType::Iterm2
    ) {
        return;
    }

    let (char_width, char_height) = picker.font_size();
    let desired_w_px = u32::from(cell_size.width).saturating_mul(u32::from(char_width));
    let desired_h_px = u32::from(cell_size.height).saturating_mul(u32::from(char_height));
    if img.width() != desired_w_px || img.height() != desired_h_px {
        let target_width = desired_w_px.max(1);
        let target_height = desired_h_px.max(1);
        match resize_exact_fast(img, target_width, target_height) {
            Ok(resized) => *img = resized,
            Err(_) => {
                *img = img.resize_exact(
                    target_width,
                    target_height,
                    image::imageops::FilterType::Lanczos3,
                );
            }
        }
    }
}

fn pad_to_pixel_cell_bounds(
    img: DynamicImage,
    cell_size: CellSize,
    density: PixelDensity,
) -> DynamicImage {
    let target_width = u32::from(cell_size.width).saturating_mul(density.x);
    let target_height = u32::from(cell_size.height).saturating_mul(density.y);

    if img.width() == target_width && img.height() == target_height {
        return img;
    }

    if let DynamicImage::ImageRgba8(rgba) = img {
        return DynamicImage::ImageRgba8(pad_pixels(
            &rgba,
            target_width,
            target_height,
            image::Rgba([0; 4]),
        ));
    }
    let rgb = img.into_rgb8();
    DynamicImage::ImageRgb8(pad_pixels(
        &rgb,
        target_width,
        target_height,
        *rgb.get_pixel(0, 0),
    ))
}

fn pad_pixels<P: image::Pixel<Subpixel = u8>>(
    src: &image::ImageBuffer<P, Vec<u8>>,
    width: u32,
    height: u32,
    bg: P,
) -> image::ImageBuffer<P, Vec<u8>> {
    let mut padded = image::ImageBuffer::from_pixel(width, height, bg);
    let channels = usize::from(P::CHANNEL_COUNT);
    let src_stride = src.width() as usize * channels;
    let dst_stride = width as usize * channels;
    let copy_bytes = src_stride.min(dst_stride);
    for y in 0..height.min(src.height()) as usize {
        padded.as_mut()[y * dst_stride..y * dst_stride + copy_bytes]
            .copy_from_slice(&src.as_raw()[y * src_stride..y * src_stride + copy_bytes]);
    }
    padded
}

fn encode_fixed_area_protocol(
    img: DynamicImage,
    area: Rect,
    picker: &Picker,
) -> Result<Protocol, PipelineError> {
    match picker.protocol_type() {
        ProtocolType::Iterm2 => Ok(Protocol::ITerm2(
            Iterm2::new(img, area, picker.is_tmux())
                .map_err(|e| pipeline_error(format!("Couldn't encode iTerm2 image: {e}")))?,
        )),
        _ => picker.new_protocol(img, area, Resize::None).map_err(|e| {
            pipeline_error(format!(
                "Image conversion failed; unable to render DynamicImage into a ratatui buffer: {e}"
            ))
        }),
    }
}

fn pad_to_cell_bounds(img: DynamicImage, cell_size: CellSize, picker: &Picker) -> DynamicImage {
    let (char_width, char_height) = picker.font_size();
    let target_width = u32::from(cell_size.width) * u32::from(char_width);
    let target_height = u32::from(cell_size.height) * u32::from(char_height);

    if img.width() == target_width && img.height() == target_height {
        return img;
    }

    let rgb = img.into_rgb8();
    DynamicImage::ImageRgb8(pad_pixels(
        &rgb,
        target_width,
        target_height,
        *rgb.get_pixel(0, 0),
    ))
}

fn resize_exact_fast(
    img: &DynamicImage,
    width: u32,
    height: u32,
) -> Result<DynamicImage, PipelineError> {
    use std::num::NonZeroU32;

    let rgb = img.to_rgb8();
    let src_width = rgb.width();
    let src_height = rgb.height();
    let src_buf = rgb.into_raw();

    let src_nz_width =
        NonZeroU32::new(src_width).ok_or_else(|| pipeline_error("Invalid source width"))?;
    let src_nz_height =
        NonZeroU32::new(src_height).ok_or_else(|| pipeline_error("Invalid source height"))?;
    let dst_nz_width =
        NonZeroU32::new(width).ok_or_else(|| pipeline_error("Invalid target width"))?;
    let dst_nz_height =
        NonZeroU32::new(height).ok_or_else(|| pipeline_error("Invalid target height"))?;

    let src = fir::Image::from_vec_u8(src_nz_width, src_nz_height, src_buf, fir::PixelType::U8x3)
        .map_err(|e| pipeline_error(format!("Fast resize source error: {e}")))?;
    let mut dst = fir::Image::new(dst_nz_width, dst_nz_height, fir::PixelType::U8x3);
    let mut resizer = fir::Resizer::new(fir::ResizeAlg::Convolution(fir::FilterType::Lanczos3));
    resizer
        .resize(&src.view(), &mut dst.view_mut())
        .map_err(|e| pipeline_error(format!("Fast resize error: {e}")))?;

    let out = RgbImage::from_raw(width, height, dst.into_vec())
        .ok_or_else(|| pipeline_error("Fast resize produced invalid buffer"))?;
    Ok(DynamicImage::ImageRgb8(out))
}

fn render_page_with_viewport(
    cached: &CachedPage,
    page_num: usize,
    overlays: &OverlaySet,
    picker: &Picker,
    viewport: Option<&ViewportUpdate>,
    pid: u32,
    kitty_shm_support: bool,
) -> Result<ConvertedImage, PipelineError> {
    let PageSource::Raster(bytes) = &cached.data.img_data.source else {
        return Err(pipeline_error("Compiled pages require a viewport"));
    };
    let mut dyn_img = decode_pixels(
        bytes,
        cached.data.img_data.width_px,
        cached.data.img_data.height_px,
        cached.data.img_data.channels,
    )?;
    apply_overlays_dynamic(&mut dyn_img, overlays);

    let mut area_cell_size = cached_cell_size(cached);
    if let Some(viewport) = viewport
        && viewport.page == page_num
    {
        let (cropped, cell_size) =
            crop_to_viewport(dyn_img, cached_cell_size(cached), viewport, picker);
        dyn_img = cropped;
        area_cell_size = cell_size;
    }

    normalize_for_protocol(&mut dyn_img, area_cell_size, picker);

    encode_protocol(
        dyn_img,
        area_cell_size,
        page_num,
        picker,
        pid,
        kitty_shm_support,
    )
}

fn render_viewport_tiles(
    cached: &mut CachedPage,
    viewport: &ViewportUpdate,
    overlays: &OverlaySet,
    picker: &Picker,
) -> Result<ConvertedImage, PipelineError> {
    let (tiles, cell_size) = render_tiles(cached, viewport, overlays, picker, false)?;
    Ok(ConvertedImage::Tiled { tiles, cell_size })
}

/// Replay each contiguous run of missing rows once. Only the encoded rows
/// survive the call: overlays, cache hits and partial updates use the same
/// strip coordinates, and never require a decoded page.
fn render_tiles(
    cached: &mut CachedPage,
    viewport: &ViewportUpdate,
    overlays: &OverlaySet,
    picker: &Picker,
    updates_only: bool,
) -> Result<(Vec<TiledProtocol>, CellSize), PipelineError> {
    enum Band<'a> {
        Raster(&'a [u8]),
        Rendered(DynamicImage),
    }

    let density = cached.density(picker);
    let data = &cached.data.img_data;
    let width_cells = data.width_cell.min(viewport.viewport_width_cells);
    let cell_size = CellSize::new(width_cells, viewport.viewport_height_cells);
    let x = viewport
        .x_offset_cells
        .saturating_mul(density.x)
        .min(data.width_px.saturating_sub(1));
    let width = (u32::from(width_cells) * density.x).min(data.width_px - x);
    let start = viewport.y_offset_cells;
    let end = start
        .saturating_add(u32::from(viewport.viewport_height_cells))
        .min(data.height_px.div_ceil(density.y));
    if picker.protocol_type() == ProtocolType::Iterm2 {
        cached
            .tile_cache
            .retain(|row, _| (start..end).contains(row) && width != 0);
    }
    if width == 0 || start >= end {
        return Ok((Vec::new(), cell_size));
    }

    let mut tiles = if updates_only {
        Vec::new()
    } else {
        Vec::with_capacity((end - start) as usize)
    };
    let tile = |row: u32, protocol| TiledProtocol {
        protocol,
        y_offset_cells: (row - start) as u16,
        height_cells: 1,
    };
    let area = Rect::new(0, 0, width_cells, 1);
    let strip_cells = CellSize::new(width_cells, 1);
    let has_overlays = !overlays.is_empty();
    let mut row = start;
    while row < end {
        if let Some(hit) = cached.tile_cache.get(&row) {
            if !updates_only {
                tiles.push(tile(row, Arc::clone(hit)));
            }
            row += 1;
            continue;
        }

        let run_end = (row + 1..end)
            .find(|next| cached.tile_cache.contains_key(next))
            .unwrap_or(end);
        let top = row * density.y;
        let bottom = run_end.saturating_mul(density.y).min(data.height_px);
        let mut band = match &data.source {
            PageSource::Raster(bytes) => Band::Raster(bytes),
            PageSource::Compiled(page) => {
                let mut img = page.render_region(x, top, width, bottom - top)?;
                if has_overlays {
                    apply_overlays_dynamic(
                        &mut img,
                        &overlays.for_tile(x, width, top, bottom - top),
                    );
                }
                Band::Rendered(img)
            }
        };
        while row < run_end {
            let y = row * density.y;
            let height = density.y.min(data.height_px - y);
            let mut img = match &mut band {
                Band::Rendered(img) if img.height() == height => std::mem::take(img),
                Band::Rendered(img) => copy_pixel_region(
                    img.as_bytes(),
                    width,
                    bottom - top,
                    img.color().channel_count(),
                    PixelRect {
                        x0: 0,
                        y0: y - top,
                        x1: width,
                        y1: y - top + height,
                    },
                )?,
                Band::Raster(bytes) => copy_pixel_region(
                    bytes,
                    data.width_px,
                    data.height_px,
                    data.channels,
                    PixelRect {
                        x0: x,
                        y0: y,
                        x1: x + width,
                        y1: y + height,
                    },
                )?,
            };
            if has_overlays && matches!(&band, Band::Raster(_)) {
                apply_overlays_dynamic(&mut img, &overlays.for_tile(x, width, y, height));
            }
            let img = if picker.protocol_type() == ProtocolType::Iterm2 {
                pad_to_pixel_cell_bounds(img, strip_cells, density)
            } else {
                pad_to_cell_bounds(img, strip_cells, picker)
            };
            let protocol = Arc::new(encode_fixed_area_protocol(img, area, picker)?);
            cached.tile_cache.insert(row, Arc::clone(&protocol));
            tiles.push(tile(row, protocol));
            row += 1;
        }
    }
    Ok((tiles, cell_size))
}

static SHM_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn next_shm_name(pid: u32, page_num: usize) -> String {
    let unique = SHM_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("/bookokrat_{unique}-{pid}-page-{page_num}")
}

fn encode_protocol(
    img: DynamicImage,
    cell_size: CellSize,
    page_num: usize,
    picker: &Picker,
    pid: u32,
    kitty_shm_support: bool,
) -> Result<ConvertedImage, PipelineError> {
    match picker.protocol_type() {
        ProtocolType::Kitty => {
            let is_rgba = matches!(img, DynamicImage::ImageRgba8(_));
            let (data, width, height) = if is_rgba {
                // Already straight alpha: the worker unpremultiplies right after
                // rasterization, which is what Kitty f=32 expects.
                let rgba = img.into_rgba8();
                let width = rgba.width();
                let height = rgba.height();
                (rgba.into_raw(), width, height)
            } else {
                let rgb = img.to_rgb8();
                let width = rgb.width();
                let height = rgb.height();
                (rgb.into_raw(), width, height)
            };
            let id = page_image_id(page_num);
            let img = if kitty_shm_support {
                let shm_name = next_shm_name(pid, page_num);
                let shm_result = if is_rgba {
                    super::kittyv2::Image::create_shm_from_rgba(&data, width, height, &shm_name, id)
                } else {
                    super::kittyv2::Image::create_shm_from_rgb(&data, width, height, &shm_name, id)
                };
                match shm_result {
                    Ok((shm_img, _shm_size)) => shm_img,
                    Err(e) => {
                        log::warn!(
                            "SHM transfer failed for page {page_num}, falling back to direct: {e:?}"
                        );
                        if is_rgba {
                            super::kittyv2::Image::from_rgba_bytes(data, width, height, id)
                        } else {
                            super::kittyv2::Image::from_rgb_bytes(data, width, height, id)
                        }
                    }
                }
            } else if is_rgba {
                super::kittyv2::Image::from_rgba_bytes(data, width, height, id)
            } else {
                super::kittyv2::Image::from_rgb_bytes(data, width, height, id)
            };

            Ok(ConvertedImage::Kitty {
                img: ImageState::Queued(img),
                cell_size,
            })
        }
        ProtocolType::Iterm2 => {
            let area = Rect {
                x: 0,
                y: 0,
                width: cell_size.width,
                height: cell_size.height,
            };
            let density = pixel_density_for_image(&img, cell_size, picker);
            let img = pad_to_pixel_cell_bounds(img, cell_size, density);
            Ok(ConvertedImage::Generic(encode_fixed_area_protocol(
                img, area, picker,
            )?))
        }
        _ => Ok(ConvertedImage::Generic({
            let area = Rect {
                x: 0,
                y: 0,
                width: cell_size.width,
                height: cell_size.height,
            };
            let img = pad_to_cell_bounds(img, cell_size, picker);
            encode_fixed_area_protocol(img, area, picker)?
        })),
    }
}

/// Append the four border strips of an outline box to `out`. `stroke` is the
/// border thickness in pixels.
fn push_box_strokes(out: &mut Vec<PixelRect>, x0: u32, y0: u32, x1: u32, y1: u32, stroke: u32) {
    let t = stroke.max(1);
    let top = y0.saturating_add(t).min(y1);
    let left = x0.saturating_add(t).min(x1);
    let bottom = y1.saturating_sub(t).max(y0);
    let right = x1.saturating_sub(t).max(x0);
    let edges = [
        PixelRect::new(x0, y0, x1, top),    // top
        PixelRect::new(x0, bottom, x1, y1), // bottom
        PixelRect::new(x0, y0, left, y1),   // left
        PixelRect::new(right, y0, x1, y1),  // right
    ];
    out.extend(edges.into_iter().flatten());
}

/// Build outline-box stroke rects for link clickboxes from the shared visual-box
/// geometry (see [`link_visual_boxes`]). For each box we emit the four border
/// strips, with thickness scaled to the box height. Using the same geometry the
/// reader hit-tests against keeps the outline and the clickable area identical.
///
/// Returned rects are the final stroke geometry (the border strips), drawn
/// directly.
fn link_box_stroke_rects(links: &[LinkRect], lines: &[LineBounds]) -> Vec<PixelRect> {
    let mut out = Vec::new();
    for link in links {
        for (x0, y0, x1, y1) in link_visual_boxes(link, lines) {
            let stroke = ((y1 - y0) as f32 * 0.05).round().max(1.0) as u32;
            push_box_strokes(&mut out, x0, y0, x1, y1, stroke);
        }
    }
    out
}

fn apply_overlays(img: &mut RgbImage, overlays: &OverlaySet) {
    apply_highlight_rects(img, &overlays.highlights);
    if overlays.comments_are_underlines {
        // Comments are already in underline coordinates (tile rendering)
        draw_underline_rects_direct(img, &overlays.comments, COMMENT_UNDERLINE_RGB);
    } else {
        // Comments are selection rects, calculate underline position
        apply_underline_rects(img, &overlays.comments, COMMENT_UNDERLINE_RGB);
    }
    // Link strokes are final outline-box geometry — draw directly.
    draw_underline_rects_direct(img, &overlays.link_strokes, LINK_STROKE_RGB);
    apply_rects_op(img, &overlays.selection, OverlayOp::Selection);
    apply_rects_op(img, &overlays.visual, OverlayOp::Selection);
    if let Some(cursor) = overlays.cursor {
        apply_rects_op(img, std::slice::from_ref(&cursor), OverlayOp::Cursor);
    }
}

fn apply_highlight_rects(img: &mut RgbImage, rects: &[HighlightPixelRect]) {
    if rects.is_empty() {
        return;
    }
    let width = img.width() as usize;
    let height = img.height() as usize;
    let stride = width * 3;
    let buf = img.as_mut();

    for highlight in rects {
        let Some(clamped) = highlight.rect.clamp_to(width as u32, height as u32) else {
            continue;
        };
        if clamped.y1 <= clamped.y0 || clamped.x1 <= clamped.x0 {
            continue;
        }
        let alpha = u16::from(highlight.alpha);
        let inv_alpha = 255u16.saturating_sub(alpha);
        for y in clamped.y0..clamped.y1 {
            let row_start = y as usize * stride;
            for x in clamped.x0..clamped.x1 {
                let px_start = row_start + x as usize * 3;
                if px_start + 2 >= buf.len() {
                    continue;
                }
                buf[px_start] = blend_channel(buf[px_start], highlight.rgb.r, alpha, inv_alpha);
                buf[px_start + 1] =
                    blend_channel(buf[px_start + 1], highlight.rgb.g, alpha, inv_alpha);
                buf[px_start + 2] =
                    blend_channel(buf[px_start + 2], highlight.rgb.b, alpha, inv_alpha);
            }
        }
    }
}

fn blend_channel(base: u8, overlay: u8, alpha: u16, inv_alpha: u16) -> u8 {
    (((u16::from(base) * inv_alpha) + (u16::from(overlay) * alpha)) / 255) as u8
}

fn apply_overlays_dynamic(img: &mut DynamicImage, overlays: &OverlaySet) {
    if let DynamicImage::ImageRgb8(rgb) = img {
        apply_overlays(rgb, overlays);
        return;
    }
    if let DynamicImage::ImageRgba8(rgba) = img {
        apply_overlays_rgba(rgba, overlays);
        return;
    }

    let mut rgb = img.to_rgb8();
    apply_overlays(&mut rgb, overlays);
    *img = DynamicImage::ImageRgb8(rgb);
}

/// Underline rect for a comment (purple line drawn just below the text).
fn comment_underline_rect(rect: PixelRect) -> PixelRect {
    const UNDERLINE_THICKNESS: u32 = 3;
    const UNDERLINE_OFFSET: u32 = 2;
    let y0 = rect.y1.saturating_add(UNDERLINE_OFFSET);
    PixelRect {
        x0: rect.x0,
        y0,
        x1: rect.x1,
        y1: y0.saturating_add(UNDERLINE_THICKNESS),
    }
}

/// Paint a rect over an RGBA buffer, applying `f` to the RGB channels and
/// forcing alpha to opaque so overlays stay visible over transparent page areas.
fn paint_rgba_rect(
    buf: &mut [u8],
    width: usize,
    height: usize,
    rect: PixelRect,
    mut f: impl FnMut(&mut u8, &mut u8, &mut u8),
) {
    let Some(c) = rect.clamp_to(width as u32, height as u32) else {
        return;
    };
    if c.y1 <= c.y0 || c.x1 <= c.x0 {
        return;
    }
    let stride = width * 4;
    for y in c.y0..c.y1 {
        let row = y as usize * stride;
        for x in c.x0..c.x1 {
            let p = row + x as usize * 4;
            if p + 3 >= buf.len() {
                continue;
            }
            let (mut r, mut g, mut b) = (buf[p], buf[p + 1], buf[p + 2]);
            f(&mut r, &mut g, &mut b);
            buf[p] = r;
            buf[p + 1] = g;
            buf[p + 2] = b;
            buf[p + 3] = 255;
        }
    }
}

/// Scalar overlay application for RGBA (transparent) pages. Mirrors
/// `apply_overlays` but writes to 4-channel pixels and keeps overlays opaque.
fn apply_overlays_rgba(img: &mut RgbaImage, overlays: &OverlaySet) {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let buf = img.as_mut();

    for highlight in &overlays.highlights {
        let alpha = u16::from(highlight.alpha);
        let inv_alpha = 255u16.saturating_sub(alpha);
        let rgb = highlight.rgb;
        paint_rgba_rect(buf, width, height, highlight.rect, |r, g, b| {
            *r = blend_channel(*r, rgb.r, alpha, inv_alpha);
            *g = blend_channel(*g, rgb.g, alpha, inv_alpha);
            *b = blend_channel(*b, rgb.b, alpha, inv_alpha);
        });
    }

    for rect in &overlays.comments {
        let underline = if overlays.comments_are_underlines {
            *rect
        } else {
            comment_underline_rect(*rect)
        };
        paint_rgba_rect(buf, width, height, underline, |r, g, b| {
            *r = COMMENT_UNDERLINE_RGB.0;
            *g = COMMENT_UNDERLINE_RGB.1;
            *b = COMMENT_UNDERLINE_RGB.2;
        });
    }

    // Link strokes are final outline-box geometry — paint them as-is.
    for rect in &overlays.link_strokes {
        paint_rgba_rect(buf, width, height, *rect, |r, g, b| {
            *r = LINK_STROKE_RGB.0;
            *g = LINK_STROKE_RGB.1;
            *b = LINK_STROKE_RGB.2;
        });
    }

    for rect in overlays.selection.iter().chain(overlays.visual.iter()) {
        paint_rgba_rect(buf, width, height, *rect, |r, g, b| {
            *r = r.saturating_add(40);
            *g = g.saturating_sub(20);
            *b = b.saturating_sub(60);
        });
    }

    if let Some(cursor) = overlays.cursor {
        paint_rgba_rect(buf, width, height, cursor, |r, g, b| {
            *r = 255 - *r;
            *g = 255 - *g;
            *b = 255 - *b;
        });
    }
}

fn comment_rects_for_page(
    rects: &[SelectionRect],
    page_num: usize,
    scale_factor: f32,
) -> Vec<PixelRect> {
    let scale = f64::from(scale_factor);
    rects
        .iter()
        .filter(|rect| rect.page == page_num)
        .filter_map(|rect| {
            let topleft_x = (f64::from(rect.topleft_x) * scale).round() as u32;
            let topleft_y = (f64::from(rect.topleft_y) * scale).round() as u32;
            let bottomright_x = (f64::from(rect.bottomright_x) * scale).round() as u32;
            let bottomright_y = (f64::from(rect.bottomright_y) * scale).round() as u32;
            PixelRect::new(topleft_x, topleft_y, bottomright_x, bottomright_y)
        })
        .collect()
}

#[derive(Copy, Clone, Debug)]
enum OverlayOp {
    // Retained for the SIMD overlay tests; comments are drawn as solid
    // underlines, not via the tinting SIMD path.
    #[cfg_attr(not(test), allow(dead_code))]
    Comment,
    Selection,
    Cursor,
}

mod simd_overlay {
    use wide::u8x16;

    const SEL_ADD_0: u8x16 = u8x16::new([40, 0, 0, 40, 0, 0, 40, 0, 0, 40, 0, 0, 40, 0, 0, 40]);
    const SEL_ADD_1: u8x16 = u8x16::new([0, 0, 40, 0, 0, 40, 0, 0, 40, 0, 0, 40, 0, 0, 40, 0]);
    const SEL_ADD_2: u8x16 = u8x16::new([0, 40, 0, 0, 40, 0, 0, 40, 0, 0, 40, 0, 0, 40, 0, 0]);
    const SEL_SUB_0: u8x16 = u8x16::new([0, 20, 60, 0, 20, 60, 0, 20, 60, 0, 20, 60, 0, 20, 60, 0]);
    const SEL_SUB_1: u8x16 =
        u8x16::new([20, 60, 0, 20, 60, 0, 20, 60, 0, 20, 60, 0, 20, 60, 0, 20]);
    const SEL_SUB_2: u8x16 =
        u8x16::new([60, 0, 20, 60, 0, 20, 60, 0, 20, 60, 0, 20, 60, 0, 20, 60]);

    const CMT_ADD_0: u8x16 = u8x16::new([0, 0, 20, 0, 0, 20, 0, 0, 20, 0, 0, 20, 0, 0, 20, 0]);
    const CMT_ADD_1: u8x16 = u8x16::new([0, 20, 0, 0, 20, 0, 0, 20, 0, 0, 20, 0, 0, 20, 0, 0]);
    const CMT_ADD_2: u8x16 = u8x16::new([20, 0, 0, 20, 0, 0, 20, 0, 0, 20, 0, 0, 20, 0, 0, 20]);
    const CMT_SUB_0: u8x16 = u8x16::new([15, 0, 0, 15, 0, 0, 15, 0, 0, 15, 0, 0, 15, 0, 0, 15]);
    const CMT_SUB_1: u8x16 = u8x16::new([0, 0, 15, 0, 0, 15, 0, 0, 15, 0, 0, 15, 0, 0, 15, 0]);
    const CMT_SUB_2: u8x16 = u8x16::new([0, 15, 0, 0, 15, 0, 0, 15, 0, 0, 15, 0, 0, 15, 0, 0]);

    const ONES: u8x16 = u8x16::new([255; 16]);

    #[inline]
    pub fn apply_row_simd(row: &mut [u8], op: super::OverlayOp) {
        let len = row.len();

        let chunks_48 = len / 48;
        let simd_end = chunks_48 * 48;

        let (simd_part, remainder) = row.split_at_mut(simd_end);

        for chunk in simd_part.chunks_exact_mut(48) {
            let (c0, rest) = chunk.split_at_mut(16);
            let (c1, c2) = rest.split_at_mut(16);

            let mut v0 = u8x16::new(c0.try_into().unwrap());
            let mut v1 = u8x16::new(c1.try_into().unwrap());
            let mut v2 = u8x16::new(c2.try_into().unwrap());

            match op {
                super::OverlayOp::Comment => {
                    v0 = v0.saturating_add(CMT_ADD_0).saturating_sub(CMT_SUB_0);
                    v1 = v1.saturating_add(CMT_ADD_1).saturating_sub(CMT_SUB_1);
                    v2 = v2.saturating_add(CMT_ADD_2).saturating_sub(CMT_SUB_2);
                }
                super::OverlayOp::Selection => {
                    v0 = v0.saturating_add(SEL_ADD_0).saturating_sub(SEL_SUB_0);
                    v1 = v1.saturating_add(SEL_ADD_1).saturating_sub(SEL_SUB_1);
                    v2 = v2.saturating_add(SEL_ADD_2).saturating_sub(SEL_SUB_2);
                }
                super::OverlayOp::Cursor => {
                    v0 = ONES - v0;
                    v1 = ONES - v1;
                    v2 = ONES - v2;
                }
            }

            c0.copy_from_slice(v0.as_array());
            c1.copy_from_slice(v1.as_array());
            c2.copy_from_slice(v2.as_array());
        }

        for px in remainder.chunks_exact_mut(3) {
            match op {
                super::OverlayOp::Comment => {
                    px[0] = px[0].saturating_sub(15);
                    px[2] = px[2].saturating_add(20);
                }
                super::OverlayOp::Selection => {
                    px[0] = px[0].saturating_add(40);
                    px[1] = px[1].saturating_sub(20);
                    px[2] = px[2].saturating_sub(60);
                }
                super::OverlayOp::Cursor => {
                    px[0] = 255 - px[0];
                    px[1] = 255 - px[1];
                    px[2] = 255 - px[2];
                }
            }
        }
    }
}

fn apply_rects_op(img: &mut RgbImage, rects: &[PixelRect], op: OverlayOp) {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let stride = width * 3;
    let buf = img.as_mut();

    let mut clamped = Vec::new();
    let mut total_pixels: u64 = 0;
    for rect in rects {
        let Some(rect) = rect.clamp_to(width as u32, height as u32) else {
            continue;
        };
        let rect_pixels =
            u64::from(rect.x1.saturating_sub(rect.x0)) * u64::from(rect.y1.saturating_sub(rect.y0));
        if rect_pixels == 0 {
            continue;
        }
        total_pixels = total_pixels.saturating_add(rect_pixels);
        clamped.push(rect);
    }

    if clamped.is_empty() {
        return;
    }

    let use_parallel = total_pixels >= 200_000 && height >= 4;
    if !use_parallel {
        for rect in &clamped {
            for y in rect.y0..rect.y1 {
                let row_start = y as usize * stride;
                let start = row_start + rect.x0 as usize * 3;
                let end = row_start + rect.x1 as usize * 3;
                let row = &mut buf[start..end];
                simd_overlay::apply_row_simd(row, op);
            }
        }
        return;
    }

    buf.par_chunks_mut(stride).enumerate().for_each(|(y, row)| {
        let y = y as u32;
        for rect in &clamped {
            if y < rect.y0 || y >= rect.y1 {
                continue;
            }
            let start = rect.x0 as usize * 3;
            let end = rect.x1 as usize * 3;
            let row = &mut row[start..end];
            simd_overlay::apply_row_simd(row, op);
        }
    });
}

/// Purple underline matching EPUB comments (base_0e from Oceanic Next theme).
const COMMENT_UNDERLINE_RGB: (u8, u8, u8) = (0xC5, 0x94, 0xC5);
/// Orange outline for link clickboxes (base_09 from Oceanic Next theme).
const LINK_STROKE_RGB: (u8, u8, u8) = (0xF9, 0x91, 0x57);

/// Apply overlay operation below each rect (underline effect)
fn apply_underline_rects(img: &mut RgbImage, rects: &[PixelRect], color: (u8, u8, u8)) {
    const UNDERLINE_THICKNESS: u32 = 3;
    const UNDERLINE_OFFSET: u32 = 2; // Gap between text bottom and underline
    let (underline_r, underline_g, underline_b) = color;

    let width = img.width() as usize;
    let height = img.height() as usize;
    let stride = width * 3;
    let buf = img.as_mut();

    for rect in rects {
        // Draw underline BELOW the rect (after the text baseline)
        let underline_y0 = rect.y1.saturating_add(UNDERLINE_OFFSET);
        let underline_y1 = underline_y0.saturating_add(UNDERLINE_THICKNESS);

        let underline_rect = PixelRect {
            x0: rect.x0,
            y0: underline_y0,
            x1: rect.x1,
            y1: underline_y1,
        };

        let Some(clamped) = underline_rect.clamp_to(width as u32, height as u32) else {
            continue;
        };
        if clamped.y1 <= clamped.y0 || clamped.x1 <= clamped.x0 {
            continue;
        }

        // Draw solid underline in the requested color
        for y in clamped.y0..clamped.y1 {
            let row_start = y as usize * stride;
            for x in clamped.x0..clamped.x1 {
                let px_start = row_start + x as usize * 3;
                if px_start + 2 < buf.len() {
                    buf[px_start] = underline_r;
                    buf[px_start + 1] = underline_g;
                    buf[px_start + 2] = underline_b;
                }
            }
        }
    }
}

/// Draw underlines directly at rect coordinates (for tile rendering where
/// underline positions are pre-computed in for_tile).
fn draw_underline_rects_direct(img: &mut RgbImage, rects: &[PixelRect], color: (u8, u8, u8)) {
    let (underline_r, underline_g, underline_b) = color;

    let width = img.width() as usize;
    let height = img.height() as usize;
    let stride = width * 3;
    let buf = img.as_mut();

    for rect in rects {
        let Some(clamped) = rect.clamp_to(width as u32, height as u32) else {
            continue;
        };
        if clamped.y1 <= clamped.y0 || clamped.x1 <= clamped.x0 {
            continue;
        }

        for y in clamped.y0..clamped.y1 {
            let row_start = y as usize * stride;
            for x in clamped.x0..clamped.x1 {
                let px_start = row_start + x as usize * 3;
                if px_start + 2 < buf.len() {
                    buf[px_start] = underline_r;
                    buf[px_start + 1] = underline_g;
                    buf[px_start + 2] = underline_b;
                }
            }
        }
    }
}

pub fn run_conversion_loop(
    sender: Sender<Result<RenderedFrame, PipelineError>>,
    receiver: Receiver<ConversionCommand>,
    picker: Picker,
    prerender: usize,
    kitty_shm_support: bool,
    show_link_underlines: bool,
) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
    use std::time::{Duration, Instant};

    log::info!("Converter using protocol: {:?}", picker.protocol_type());
    let mut engine =
        ConverterEngine::new(picker, prerender, kitty_shm_support, show_link_underlines);
    let mut iteration = 0;
    let mut has_work = false;
    let mut last_stats_log = Instant::now();
    let stats_interval = Duration::from_secs(10);

    loop {
        // Periodic memory stats logging
        if last_stats_log.elapsed() >= stats_interval {
            engine.log_memory_stats();
            last_stats_log = Instant::now();
        }

        // Process all pending messages (non-blocking)
        while let Ok(msg) = receiver.try_recv() {
            engine.handle_msg(msg, &sender)?;
            iteration = 0;
            has_work = true;
        }

        // Do work if available
        if has_work {
            match engine.next_page(&mut iteration) {
                Ok(Some(img)) => sender.send(Ok(img))?,
                Ok(None) => has_work = false,
                Err(e) => sender.send(Err(e))?,
            }
        } else {
            // No work - block until a message arrives
            match receiver.recv() {
                Ok(msg) => {
                    engine.handle_msg(msg, &sender)?;
                    iteration = 0;
                    has_work = true;
                }
                Err(_) => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_pixels_builds_rgba_when_channels_is_four() {
        let pixels = vec![10, 20, 30, 40, 50, 60, 70, 80]; // 2 RGBA pixels
        let img = decode_pixels(&pixels, 2, 1, 4).expect("rgba decode");
        assert!(matches!(img, DynamicImage::ImageRgba8(_)));
        assert_eq!(img.width(), 2);

        let rgb = vec![1, 2, 3, 4, 5, 6]; // 2 RGB pixels
        let img = decode_pixels(&rgb, 2, 1, 3).expect("rgb decode");
        assert!(matches!(img, DynamicImage::ImageRgb8(_)));
    }

    #[test]
    fn decode_pixels_rejects_size_mismatch() {
        // 4-channel decode needs width*height*4 bytes; give it RGB-sized data.
        assert!(decode_pixels(&[0; 6], 2, 1, 4).is_err());
    }

    fn raster_page(page_num: usize) -> Arc<PageData> {
        Arc::new(PageData {
            img_data: super::super::ImageData {
                source: PageSource::Raster((0..16 * 48 * 3).map(|i| i as u8).collect()),
                width_px: 16,
                height_px: 48,
                width_cell: 8,
                height_cell: 12,
                channels: 3,
            },
            page_num,
            scale_factor: 1.0,
            requested_scale: 1.0,
            achieved_scale: 1.0,
            render_area_width_cells: 8,
            render_area_height_cells: 3,
            line_bounds: vec![],
            link_rects: vec![],
            page_height_px: 48.0,
        })
    }

    #[test]
    fn iterm_scroll_keeps_only_overlapping_rows_and_drops_old_generations() {
        let mut picker = Picker::from_fontsize((2, 4));
        picker.set_protocol_type(ProtocolType::Iterm2);
        let mut engine = ConverterEngine::new(picker, 2, false, false);
        let (send, recv) = flume::unbounded();
        engine
            .handle_msg(ConversionCommand::SetPageCount(2), &send)
            .unwrap();
        let page = raster_page(0);
        engine
            .handle_msg(ConversionCommand::EnqueuePage(Arc::clone(&page)), &send)
            .unwrap();
        assert!(
            engine.next_page(&mut 0).unwrap().is_none(),
            "no viewport yet"
        );
        let mut vp = ViewportUpdate {
            page: 0,
            x_offset_cells: 1,
            y_offset_cells: 2,
            viewport_width_cells: 5,
            viewport_height_cells: 3,
        };
        engine
            .handle_msg(ConversionCommand::UpdateViewport(vp), &send)
            .unwrap();
        let first = recv.try_recv().unwrap().unwrap();
        let ConvertedImage::Tiled { tiles: first, .. } = first.image else {
            panic!("missing strips")
        };
        vp.y_offset_cells += 1;
        engine
            .handle_msg(ConversionCommand::UpdateViewport(vp), &send)
            .unwrap();
        let next = recv.try_recv().unwrap().unwrap();
        let ConvertedImage::Tiled { tiles: next, .. } = next.image else {
            panic!("missing strips")
        };
        assert!(Arc::ptr_eq(&first[1].protocol, &next[0].protocol));
        assert!(Arc::ptr_eq(&first[2].protocol, &next[1].protocol));
        let cache = &engine.page_cache[0].as_ref().unwrap().tile_cache;
        assert_eq!(cache.len(), 3);
        assert!(!cache.contains_key(&2));

        vp.x_offset_cells += 1;
        engine
            .handle_msg(ConversionCommand::UpdateViewport(vp), &send)
            .unwrap();
        let panned = recv.try_recv().unwrap().unwrap();
        let ConvertedImage::Tiled { tiles: panned, .. } = panned.image else {
            panic!("missing strips")
        };
        assert!(!Arc::ptr_eq(&next[0].protocol, &panned[0].protocol));
        engine.update_cache_for_page(0, &page);
        assert_eq!(engine.page_cache[0].as_ref().unwrap().tile_cache.len(), 3);
        engine.update_cache_for_page(0, &raster_page(0));
        assert!(engine.page_cache[0].as_ref().unwrap().tile_cache.is_empty());
        assert!(!engine.sent_for_viewport.contains(&0));
    }

    #[test]
    fn iterm_dual_cursor_updates_are_relative_to_each_page_viewport() {
        let mut picker = Picker::from_fontsize((2, 4));
        picker.set_protocol_type(ProtocolType::Iterm2);
        let mut engine = ConverterEngine::new(picker, 2, false, false);
        let (send, recv) = flume::unbounded();
        engine
            .handle_msg(ConversionCommand::SetPageCount(2), &send)
            .unwrap();
        for page in 0..2 {
            engine.update_cache_for_page(page, &raster_page(page));
        }
        let left = ViewportUpdate {
            page: 0,
            x_offset_cells: 1,
            y_offset_cells: 2,
            viewport_width_cells: 5,
            viewport_height_cells: 3,
        };
        let right = ViewportUpdate {
            page: 1,
            x_offset_cells: 0,
            y_offset_cells: 7,
            ..left
        };
        engine
            .handle_msg(
                ConversionCommand::UpdateDualViewport(vec![left, right]),
                &send,
            )
            .unwrap();
        assert_eq!(recv.drain().count(), 2);
        let mut cursor = CursorRect {
            page: 0,
            x: 4,
            y: 12,
            width: 2,
            height: 4,
        };
        engine
            .handle_msg(ConversionCommand::UpdateCursor(Some(cursor.clone())), &send)
            .unwrap();
        let strip = |frame: RenderedFrame| {
            let ConvertedImage::TileUpdate { tiles, .. } = frame.image else {
                panic!("not a strip update")
            };
            assert_eq!(tiles.len(), 1);
            (frame.index, tiles[0].y_offset_cells)
        };
        assert_eq!(strip(recv.try_recv().unwrap().unwrap()), (0, 1));
        cursor.page = 1;
        cursor.y = 28;
        engine
            .handle_msg(ConversionCommand::UpdateCursor(Some(cursor)), &send)
            .unwrap();
        let mut updates = recv
            .drain()
            .map(|frame| strip(frame.unwrap()))
            .collect::<Vec<_>>();
        updates.sort_unstable();
        assert_eq!(updates, [(0, 1), (1, 0)]);
        engine
            .handle_msg(ConversionCommand::UpdateViewport(right), &send)
            .unwrap();
        assert!(engine.page_cache[0].as_ref().unwrap().tile_cache.is_empty());
        assert!(!engine.last_viewport_by_page.contains_key(&0));
    }

    fn link(x0: u32, y0: u32, x1: u32, y1: u32) -> LinkRect {
        LinkRect {
            x0,
            y0,
            x1,
            y1,
            target: crate::pdf::types::LinkTarget::External { uri: "u".into() },
        }
    }

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

    #[test]
    fn box_strokes_form_four_edges() {
        let mut out = Vec::new();
        push_box_strokes(&mut out, 10, 20, 110, 50, 2);
        assert_eq!(out.len(), 4);
        // top, bottom, left, right
        assert_eq!(
            (out[0].x0, out[0].y0, out[0].x1, out[0].y1),
            (10, 20, 110, 22)
        );
        assert_eq!(
            (out[1].x0, out[1].y0, out[1].x1, out[1].y1),
            (10, 48, 110, 50)
        );
        assert_eq!(
            (out[2].x0, out[2].y0, out[2].x1, out[2].y1),
            (10, 20, 12, 50)
        );
        assert_eq!(
            (out[3].x0, out[3].y0, out[3].x1, out[3].y1),
            (108, 20, 110, 50)
        );
    }

    #[test]
    fn link_box_strokes_clip_overhang_to_text() {
        // Real data from a wrapped link (page 18 of the LLM book). The second
        // fragment's annotation box overhangs into the left margin (x0=66 vs
        // text x0=102) and is taller than the glyphs.
        let links = vec![link(66, 297, 285, 310)];
        let lines = vec![line(102.0, 300.9, 474.1, 310.4)];

        let rects = link_box_stroke_rects(&links, &lines);
        // One box → four border strips.
        assert_eq!(rects.len(), 4);
        // The box left edge is clipped to the text start (≈102), not the raw 66.
        let min_x = rects.iter().map(|r| r.x0).min().unwrap();
        assert!(min_x >= 100 && min_x < 105, "min_x={min_x}");
        // The box hugs the glyph box vertically (around line y 300..310).
        let min_y = rects.iter().map(|r| r.y0).min().unwrap();
        let max_y = rects.iter().map(|r| r.y1).max().unwrap();
        assert!(min_y >= 298 && min_y <= 301, "min_y={min_y}");
        assert!(max_y >= 310 && max_y <= 313, "max_y={max_y}");
    }

    #[test]
    fn link_box_falls_back_when_no_text_line() {
        // A link over a figure (no overlapping text line) gets a box around the
        // raw clickbox.
        let links = vec![link(10, 10, 50, 30)];
        let lines = vec![line(0.0, 100.0, 200.0, 110.0)];
        let rects = link_box_stroke_rects(&links, &lines);
        assert_eq!(rects.len(), 4);
        let min_x = rects.iter().map(|r| r.x0).min().unwrap();
        let max_x = rects.iter().map(|r| r.x1).max().unwrap();
        assert_eq!((min_x, max_x), (10, 50));
    }

    fn apply_row_scalar(row: &mut [u8], op: OverlayOp) {
        for px in row.chunks_exact_mut(3) {
            match op {
                OverlayOp::Comment => {
                    px[0] = px[0].saturating_sub(15);
                    px[2] = px[2].saturating_add(20);
                }
                OverlayOp::Selection => {
                    px[0] = px[0].saturating_add(40);
                    px[1] = px[1].saturating_sub(20);
                    px[2] = px[2].saturating_sub(60);
                }
                OverlayOp::Cursor => {
                    px[0] = 255 - px[0];
                    px[1] = 255 - px[1];
                    px[2] = 255 - px[2];
                }
            }
        }
    }

    #[test]
    fn simd_overlay_matches_scalar() {
        let sizes = [15, 48, 96, 100, 144, 200, 300];
        let ops = [OverlayOp::Comment, OverlayOp::Selection, OverlayOp::Cursor];

        for &size in &sizes {
            for &op in &ops {
                let original: Vec<u8> = (0..size).map(|i| (i * 17) as u8).collect();
                let mut simd_data = original.clone();
                let mut scalar_data = original.clone();

                simd_overlay::apply_row_simd(&mut simd_data, op);
                apply_row_scalar(&mut scalar_data, op);

                assert_eq!(
                    simd_data, scalar_data,
                    "SIMD and scalar mismatch for size={size}, op={op:?}"
                );
            }
        }
    }

    #[test]
    fn simd_overlay_edge_cases() {
        let ops = [OverlayOp::Comment, OverlayOp::Selection, OverlayOp::Cursor];

        for &op in &ops {
            let mut empty: Vec<u8> = vec![];
            simd_overlay::apply_row_simd(&mut empty, op);
            assert!(empty.is_empty());

            let original = vec![100u8, 150, 200];
            let mut simd_data = original.clone();
            let mut scalar_data = original.clone();
            simd_overlay::apply_row_simd(&mut simd_data, op);
            apply_row_scalar(&mut scalar_data, op);
            assert_eq!(
                simd_data, scalar_data,
                "Single pixel mismatch for op={op:?}"
            );

            let original = vec![100u8, 150, 200, 50, 75, 125];
            let mut simd_data = original.clone();
            let mut scalar_data = original.clone();
            simd_overlay::apply_row_simd(&mut simd_data, op);
            apply_row_scalar(&mut scalar_data, op);
            assert_eq!(simd_data, scalar_data, "Two pixel mismatch for op={op:?}");
        }
    }
}
