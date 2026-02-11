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
use image::{DynamicImage, RgbImage};
use ratatui::layout::Rect;
use rayon::prelude::*;

use crate::vendored::ratatui_image::{
    Resize,
    picker::{Picker, ProtocolType},
    protocol::Protocol,
};

use super::kittyv2::ImageId;
use super::normal_mode::{CursorRect, VisualRect};
use super::selection::SelectionRect;
use super::types::{PageData, VecExt as _, ViewportUpdate};

type PipelineError = super::request::WorkerFault;

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

            if let Some(idx) = self.start.checked_add_signed(offset) {
                if self.max_range.contains(&idx) {
                    return Some(idx);
                }
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
            }
        }
        true
    }
}

pub struct RenderedFrame {
    pub index: usize,
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
    comments: Vec<PixelRect>,
    /// When true, `comments` contains pre-computed underline coordinates (for tile rendering).
    /// When false, `comments` contains selection rects and underline position is calculated.
    comments_are_underlines: bool,
    selection: Vec<PixelRect>,
    visual: Vec<PixelRect>,
    cursor: Option<PixelRect>,
}

impl OverlaySet {
    fn is_empty(&self) -> bool {
        self.comments.is_empty()
            && self.selection.is_empty()
            && self.visual.is_empty()
            && self.cursor.is_none()
    }

    fn for_tile(&self, tile_y: u32, tile_height: u32) -> Self {
        let tile_end = tile_y.saturating_add(tile_height);
        let clip = |rects: &[PixelRect]| -> Vec<PixelRect> {
            rects
                .iter()
                .filter(|rect| rect.intersects_y(tile_y, tile_end))
                .filter_map(|rect| {
                    let local = rect.offset_y(tile_y);
                    PixelRect::new(local.x0, local.y0, local.x1, local.y1.min(tile_height))
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
                    // Convert to underline coordinates (page-space)
                    let underline_y0 = rect.y1.saturating_add(UNDERLINE_OFFSET);
                    let underline_y1 = underline_y0.saturating_add(UNDERLINE_THICKNESS);
                    // Check if underline intersects this tile
                    if underline_y1 <= tile_y || underline_y0 >= tile_end {
                        return None;
                    }
                    // Convert to tile-local coordinates
                    let local_y0 = underline_y0.saturating_sub(tile_y);
                    let local_y1 = underline_y1.saturating_sub(tile_y).min(tile_height);
                    PixelRect::new(rect.x0, local_y0, rect.x1, local_y1)
                })
                .collect()
        };

        let cursor = self.cursor.and_then(|rect| {
            if rect.intersects_y(tile_y, tile_end) {
                PixelRect::new(
                    rect.x0,
                    rect.y0.saturating_sub(tile_y),
                    rect.x1,
                    rect.y1.saturating_sub(tile_y).min(tile_height),
                )
            } else {
                None
            }
        });

        Self {
            comments: clip_comments(&self.comments),
            comments_are_underlines: true, // Tile rendering pre-computes underline positions
            selection: clip(&self.selection),
            visual: clip(&self.visual),
            cursor,
        }
    }
}

struct CachedPage {
    data: Arc<PageData>,
    decoded: Option<DynamicImage>,
    tile_cache: HashMap<u32, Arc<Protocol>>,
}

pub enum ConversionCommand {
    SetPageCount(usize),
    NavigateTo(usize),
    EnqueuePage(Arc<PageData>),
    UpdateViewport(ViewportUpdate),
    UpdateSelection(Vec<SelectionRect>),
    UpdateComments(Vec<SelectionRect>),
    UpdateCursor(Option<CursorRect>),
    UpdateVisual(Vec<VisualRect>),
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
    comment_cache: HashMap<usize, CommentCacheEntry>,
    visual_rects: Vec<VisualRect>,
    cursor_rect: Option<CursorRect>,
    viewport: Option<ViewportUpdate>,
    tiled_pages: HashSet<usize>,
    sent_for_viewport: HashSet<usize>,
    /// Pages that need cursor re-rendering once they arrive in cache.
    pending_cursor_pages: HashSet<usize>,
}

#[derive(Clone)]
struct CommentCacheEntry {
    #[expect(dead_code)]
    scale_factor: f32,
    rects: Vec<PixelRect>,
}

impl ConverterEngine {
    fn new(picker: Picker, prerender: usize, kitty_shm_support: bool) -> Self {
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
            comment_cache: HashMap::new(),
            visual_rects: Vec::new(),
            cursor_rect: None,
            viewport: None,
            tiled_pages: HashSet::new(),
            sent_for_viewport: HashSet::new(),
            pending_cursor_pages: HashSet::new(),
        }
    }

    fn next_page(&mut self, iteration: &mut usize) -> Result<Option<RenderedFrame>, PipelineError> {
        if self.images.is_empty() {
            return Ok(None);
        }
        if *iteration >= self.prerender {
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

        let Some((page_info, new_iter, page_num)) =
            self.pick_candidate(focus_page, idx_start..idx_end)
        else {
            return Ok(None);
        };

        self.update_cache_for_page(page_num, &page_info);

        if let Some(new_iter) = self.should_skip_render(page_num, new_iter) {
            *iteration = new_iter;
            return Ok(None);
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

        Ok(Some(RenderedFrame {
            index: page_info.page_num,
            image: img,
        }))
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
        match self.picker.protocol_type() {
            ProtocolType::Iterm2 => {
                if self.sent_for_viewport.contains(&page_num) {
                    return Some(new_iter);
                }

                if page_num != self.page {
                    let viewport_matches =
                        self.viewport.as_ref().is_some_and(|vp| vp.page == page_num);

                    if !viewport_matches {
                        return Some(new_iter);
                    }
                }

                None
            }
            ProtocolType::Kitty => {
                // Skip if page was already sent. The image is stored in Kitty's
                // memory by ID and can be re-displayed without re-uploading.
                if self.sent_for_viewport.contains(&page_num) {
                    Some(new_iter)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn update_cache_for_page(&mut self, page_num: usize, page_info: &Arc<PageData>) {
        if page_num >= self.page_cache.len() {
            return;
        }

        // Preserve existing tile cache if dimensions match, otherwise start fresh
        let existing_tile_cache = self.page_cache[page_num]
            .as_ref()
            .filter(|cached| {
                cached.data.img_data.width_cell == page_info.img_data.width_cell
                    && cached.data.img_data.height_cell == page_info.img_data.height_cell
                    && (cached.data.scale_factor - page_info.scale_factor).abs() < 0.001
            })
            .map(|cached| cached.tile_cache.clone())
            .unwrap_or_default();

        self.page_cache[page_num] = Some(CachedPage {
            data: Arc::clone(page_info),
            decoded: None,
            tile_cache: existing_tile_cache,
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
                self.tiled_pages.clear();
            }
            ConversionCommand::NavigateTo(new_page) => {
                log::trace!("Converter: NavigateTo({new_page})");
                self.page = new_page;
                // Update SHM protection to cover pages near current
                super::kittyv2::set_viewport_position(new_page as i64);
                // Clear decoded images for pages far from current to save memory
                self.clear_distant_decoded(new_page, 20);
                // Also drop distant pixel buffers to cap memory even if no render happens.
                self.clear_distant_pixels(new_page, 5);
            }
            ConversionCommand::UpdateViewport(new_viewport) => {
                let old_viewport = self.viewport.replace(new_viewport);
                // Drop distant pixel buffers when viewport moves, even if render is skipped.
                self.clear_distant_pixels(new_viewport.page, 5);

                if self.picker.protocol_type() != ProtocolType::Kitty {
                    let is_same_page =
                        old_viewport.is_some_and(|old| old.page == new_viewport.page);

                    let already_sent = self.sent_for_viewport.contains(&new_viewport.page);

                    if is_same_page || !already_sent {
                        let overlays = self.build_overlay_set(new_viewport.page);
                        if let Some(Some(cached)) = self.page_cache.get_mut(new_viewport.page) {
                            match render_viewport_tiles(
                                cached,
                                &new_viewport,
                                &overlays,
                                &self.picker,
                            ) {
                                Ok(img) => {
                                    self.tiled_pages.insert(new_viewport.page);
                                    self.sent_for_viewport.insert(new_viewport.page);
                                    sender.send(Ok(RenderedFrame {
                                        index: new_viewport.page,
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
                    let use_tiles = self.picker.protocol_type() == ProtocolType::Iterm2;
                    let result = if use_tiles {
                        render_viewport_tiles(cached, &new_viewport, &overlays, &self.picker)
                    } else {
                        render_page_with_viewport(
                            cached,
                            new_viewport.page,
                            &overlays,
                            &self.picker,
                            self.viewport.as_ref(),
                            self.pid,
                            self.kitty_shm_support,
                        )
                    };

                    match result {
                        Ok(img) => {
                            if use_tiles {
                                self.tiled_pages.insert(new_viewport.page);
                            } else {
                                self.tiled_pages.remove(&new_viewport.page);
                            }
                            self.sent_for_viewport.insert(new_viewport.page);

                            sender.send(Ok(RenderedFrame {
                                index: new_viewport.page,
                                image: img,
                            }))?;
                        }
                        Err(e) => {
                            sender.send(Err(e))?;
                        }
                    }
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
            ConversionCommand::UpdateCursor(new_cursor) => {
                log::trace!(
                    "Converter: UpdateCursor page={:?}",
                    new_cursor.as_ref().map(|c| c.page)
                );
                let old_cursor = std::mem::replace(&mut self.cursor_rect, new_cursor.clone());
                self.reconvert_cursor_change(old_cursor.as_ref(), new_cursor.as_ref(), sender)?;
            }
            ConversionCommand::UpdateVisual(new_visual) => {
                let old_visual = std::mem::replace(&mut self.visual_rects, new_visual.clone());
                self.invalidate_tiles_for_changed_pages(&old_visual, &new_visual);
                self.reconvert_changed_visual(&old_visual, &new_visual, sender)?;
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
                // Clear overlay state to prevent stale rendering after cache invalidation
                self.cursor_rect = None;
                self.visual_rects.clear();
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

    fn invalidate_tiles_for_changed_pages<T: PageScoped>(&mut self, old: &[T], new: &[T]) {
        let affected = Self::collect_affected_pages(old, new);
        self.invalidate_tiles_for_pages(&affected);
    }

    fn reconvert_pages(
        &mut self,
        affected: &HashSet<usize>,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        // For tile-based protocols (non-Kitty), use tile rendering to ensure
        // overlays are correctly applied per-tile
        let use_tiles = self.picker.protocol_type() != ProtocolType::Kitty;

        for page_num in affected {
            // Remove from sent set so the page will be re-rendered with new overlays
            self.sent_for_viewport.remove(page_num);

            // Check if page is cached and has pixel data
            let has_pixels = self
                .page_cache
                .get(*page_num)
                .and_then(|opt| opt.as_ref())
                .map(|c| !c.data.img_data.pixels.is_empty())
                .unwrap_or(false);

            if !has_pixels {
                continue;
            }

            let overlays = self.build_overlay_set(*page_num);

            if use_tiles {
                // Use tile rendering for non-Kitty protocols
                if let Some(viewport) = self.viewport {
                    if viewport.page == *page_num {
                        if let Some(Some(cached)) = self.page_cache.get_mut(*page_num) {
                            match render_viewport_tiles(cached, &viewport, &overlays, &self.picker)
                            {
                                Ok(img) => {
                                    self.tiled_pages.insert(*page_num);
                                    self.sent_for_viewport.insert(*page_num);
                                    sender.send(Ok(RenderedFrame {
                                        index: *page_num,
                                        image: img,
                                    }))?;
                                }
                                Err(e) => {
                                    sender.send(Err(e))?;
                                }
                            }
                        }
                        continue;
                    }
                }
            }

            // Fall back to full page rendering (for Kitty or when viewport doesn't match)
            let Some(Some(cached)) = self.page_cache.get(*page_num) else {
                continue;
            };
            match render_page_with_viewport(
                cached,
                *page_num,
                &overlays,
                &self.picker,
                self.viewport.as_ref(),
                self.pid,
                self.kitty_shm_support,
            ) {
                Ok(img) => {
                    self.sent_for_viewport.insert(*page_num);
                    sender.send(Ok(RenderedFrame {
                        index: *page_num,
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

    #[expect(dead_code)]
    fn reconvert_changed_pages<T: PageScoped>(
        &mut self,
        old: &[T],
        new: &[T],
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let affected = Self::collect_affected_pages(old, new);
        self.reconvert_pages(&affected, sender)
    }

    fn reconvert_changed_visual(
        &mut self,
        old: &[VisualRect],
        new: &[VisualRect],
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let can_use_tiles = self.picker.protocol_type() == ProtocolType::Iterm2;

        if can_use_tiles && self.viewport.is_some() {
            self.render_visual_tile_updates(old, new, sender)
        } else {
            self.render_visual_full_pages(old, new, sender)
        }
    }

    fn render_visual_full_pages(
        &mut self,
        old: &[VisualRect],
        new: &[VisualRect],
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let mut affected: HashSet<usize> = HashSet::new();
        for rect in old {
            affected.insert(rect.page);
        }
        for rect in new {
            affected.insert(rect.page);
        }

        for page_num in affected {
            self.sent_for_viewport.remove(&page_num);

            let Some(Some(cached)) = self.page_cache.get(page_num) else {
                continue;
            };

            if cached.data.img_data.pixels.is_empty() {
                continue;
            }

            let overlays = self.build_overlay_set(page_num);
            match render_page_with_viewport(
                cached,
                page_num,
                &overlays,
                &self.picker,
                self.viewport.as_ref(),
                self.pid,
                self.kitty_shm_support,
            ) {
                Ok(img) => {
                    self.sent_for_viewport.insert(page_num);
                    sender.send(Ok(RenderedFrame {
                        index: page_num,
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

    fn render_visual_tile_updates(
        &mut self,
        old: &[VisualRect],
        new: &[VisualRect],
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let Some(viewport) = self.viewport.as_ref() else {
            return Ok(());
        };
        let (_, char_height) = self.picker.font_size();
        let tile_height_px = u32::from(char_height);

        // Collect affected tiles from old and new visual rects
        let mut affected_tiles: HashSet<(usize, u32)> = HashSet::new();
        for rect in old.iter().chain(new.iter()) {
            let start_tile = rect.y / tile_height_px;
            let end_tile = (rect.y + rect.height).div_ceil(tile_height_px);
            for tile_idx in start_tile..end_tile {
                affected_tiles.insert((rect.page, tile_idx));
            }
        }

        // Group tiles by page
        let mut pages_to_tiles: HashMap<usize, Vec<u32>> = HashMap::new();
        for (page, tile_idx) in affected_tiles {
            pages_to_tiles.entry(page).or_default().push(tile_idx);
        }

        for (page_num, tile_indices) in pages_to_tiles {
            let overlays = self.build_overlay_set(page_num);
            let Some(Some(cached)) = self.page_cache.get_mut(page_num) else {
                continue;
            };

            let cell_size = CellSize::new(
                cached.data.img_data.width_cell,
                viewport.viewport_height_cells,
            );
            let decoded = match take_decoded(cached) {
                Ok(img) => img,
                Err(e) => {
                    sender.send(Err(e))?;
                    continue;
                }
            };

            let tiles = match render_specific_tiles(
                &decoded,
                cell_size,
                viewport,
                &tile_indices,
                &overlays,
                &self.picker,
            ) {
                Ok(tiles) => tiles,
                Err(e) => {
                    sender.send(Err(e))?;
                    continue;
                }
            };

            cached.decoded = Some(decoded);

            if !tiles.is_empty() {
                log::trace!(
                    "Visual tile update: page={} tiles={}",
                    page_num,
                    tiles.len()
                );
                sender.send(Ok(RenderedFrame {
                    index: page_num,
                    image: ConvertedImage::TileUpdate { tiles, cell_size },
                }))?;
            }
        }

        Ok(())
    }

    fn reconvert_cursor_change(
        &mut self,
        old_cursor: Option<&CursorRect>,
        new_cursor: Option<&CursorRect>,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let affected_page = new_cursor
            .map(|c| c.page)
            .or_else(|| old_cursor.map(|c| c.page));
        let can_use_tiles = self.picker.protocol_type() == ProtocolType::Iterm2;
        let page_is_tiled = affected_page.is_some_and(|p| self.tiled_pages.contains(&p));

        if can_use_tiles && !page_is_tiled {
            let mut tile_base_error = false;
            if let Some(vp) = self.viewport.as_ref() {
                if let Some(page_num) = affected_page {
                    let overlays = self.build_overlay_set(page_num);
                    if let Some(Some(cached)) = self.page_cache.get_mut(page_num) {
                        match render_viewport_tiles(cached, vp, &overlays, &self.picker) {
                            Ok(img) => {
                                sender.send(Ok(RenderedFrame {
                                    index: page_num,
                                    image: img,
                                }))?;
                                self.tiled_pages.insert(page_num);
                            }
                            Err(e) => {
                                sender.send(Err(e))?;
                                tile_base_error = true;
                            }
                        }
                    }
                }
            }
            if !tile_base_error {
                self.render_cursor_tile_updates(old_cursor, new_cursor, sender)?;
            }
        } else if can_use_tiles && page_is_tiled {
            self.render_cursor_tile_updates(old_cursor, new_cursor, sender)?;
        } else {
            self.render_cursor_full_pages(old_cursor, new_cursor, sender)?;
        }
        Ok(())
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
            if cached.data.img_data.pixels.is_empty() {
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
            match render_page_with_viewport(
                cached,
                page_num,
                &overlays,
                &self.picker,
                self.viewport.as_ref(),
                self.pid,
                self.kitty_shm_support,
            ) {
                Ok(img) => {
                    self.sent_for_viewport.insert(page_num);
                    sender.send(Ok(RenderedFrame {
                        index: page_num,
                        image: img,
                    }))?;
                }
                Err(e) => {
                    log::warn!("render_cursor_full_pages: failed to render page {page_num}: {e:?}");
                    sender.send(Err(e))?;
                }
            }
        }

        Ok(())
    }

    fn render_cursor_tile_updates(
        &mut self,
        old_cursor: Option<&CursorRect>,
        new_cursor: Option<&CursorRect>,
        sender: &Sender<Result<RenderedFrame, PipelineError>>,
    ) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
        let Some(viewport) = self.viewport.as_ref() else {
            return Ok(());
        };
        let (_, char_height) = self.picker.font_size();
        let tile_height_px = u32::from(char_height);

        let old_cursor = old_cursor.map(|cursor| expand_cursor_rect(cursor, &self.picker));
        let new_cursor = new_cursor.map(|cursor| expand_cursor_rect(cursor, &self.picker));

        let mut affected_tiles: HashSet<(usize, u32)> = HashSet::new();
        if let Some(cursor) = old_cursor.as_ref() {
            let start_tile = cursor.y / tile_height_px;
            let end_tile = (cursor.y + cursor.height).div_ceil(tile_height_px);
            for tile_idx in start_tile..end_tile {
                affected_tiles.insert((cursor.page, tile_idx));
            }
        }
        if let Some(cursor) = new_cursor.as_ref() {
            let start_tile = cursor.y / tile_height_px;
            let end_tile = (cursor.y + cursor.height).div_ceil(tile_height_px);
            for tile_idx in start_tile..end_tile {
                affected_tiles.insert((cursor.page, tile_idx));
            }
        }

        let mut pages_to_tiles: HashMap<usize, Vec<u32>> = HashMap::new();
        for (page, tile_idx) in affected_tiles {
            pages_to_tiles.entry(page).or_default().push(tile_idx);
        }

        for (page_num, tile_indices) in pages_to_tiles {
            let overlays = self.build_overlay_set_with_cursor(page_num, new_cursor.as_ref());
            let Some(Some(cached)) = self.page_cache.get_mut(page_num) else {
                continue;
            };

            let cell_size = CellSize::new(
                cached.data.img_data.width_cell,
                viewport.viewport_height_cells,
            );
            let decoded = match take_decoded(cached) {
                Ok(img) => img,
                Err(e) => {
                    sender.send(Err(e))?;
                    continue;
                }
            };

            let tiles = match render_specific_tiles(
                &decoded,
                cell_size,
                viewport,
                &tile_indices,
                &overlays,
                &self.picker,
            ) {
                Ok(tiles) => tiles,
                Err(e) => {
                    sender.send(Err(e))?;
                    continue;
                }
            };

            cached.decoded = Some(decoded);

            if !tiles.is_empty() {
                log::trace!(
                    "Cursor tile update: page={} tiles={}",
                    page_num,
                    tiles.len()
                );
                sender.send(Ok(RenderedFrame {
                    index: page_num,
                    image: ConvertedImage::TileUpdate { tiles, cell_size },
                }))?;
            }
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

        // Use tile rendering for all non-Kitty protocols (matches reconvert_pages logic)
        if self.picker.protocol_type() != ProtocolType::Kitty {
            if let Some(viewport) = self.viewport.as_ref() {
                if viewport.page == page_num {
                    return render_viewport_tiles(cached, viewport, overlays, &self.picker);
                }
            }
        }

        render_page_with_viewport(
            cached,
            page_num,
            overlays,
            &self.picker,
            self.viewport.as_ref(),
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

        if let Some(Some(_cached)) = self.page_cache.get(page_num) {
            if let Some(cached_comments) = self.comment_cache.get(&page_num) {
                log::debug!(
                    "get_page_overlays: page={} comment_rects={}",
                    page_num,
                    cached_comments.rects.len()
                );
                overlays.comments.clone_from(&cached_comments.rects);
            }
        } else {
            log::debug!("get_page_overlays: page={page_num} - no page cache or no comments");
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

        if let Some(cursor) = cursor {
            if cursor.page == page_num {
                let expanded = expand_cursor_rect(cursor, &self.picker);
                let x1 = expanded.x.saturating_add(expanded.width);
                let y1 = expanded.y.saturating_add(expanded.height);
                if let Some(rect) = PixelRect::new(expanded.x, expanded.y, x1, y1) {
                    overlays.cursor = Some(rect);
                }
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
            cache.insert(
                rect.page,
                CommentCacheEntry {
                    scale_factor: cached.data.scale_factor,
                    rects: rects_px,
                },
            );
        }
        cache
    }

    fn update_comment_cache_for_page(&mut self, page_num: usize, scale_factor: f32) {
        let rects_px = comment_rects_for_page(&self.comment_rects, page_num, scale_factor);
        if rects_px.is_empty() {
            self.comment_cache.remove(&page_num);
            return;
        }
        self.comment_cache.insert(
            page_num,
            CommentCacheEntry {
                scale_factor,
                rects: rects_px,
            },
        );
    }

    /// Clear decoded images for pages far from the current page to save memory.
    /// Keeps decoded images only for pages within `radius` of `current_page`.
    fn clear_distant_decoded(&mut self, current_page: usize, radius: usize) {
        let start = current_page.saturating_sub(radius);
        let end = current_page.saturating_add(radius);

        for (i, cached) in self.page_cache.iter_mut().enumerate() {
            if let Some(page) = cached {
                if i < start || i > end {
                    page.decoded = None;
                }
            }
        }
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
                if page.decoded.is_some() || !page.tile_cache.is_empty() {
                    log::trace!("Clearing decoded/tile cache for distant page {i}");
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
        let mut decoded_count = 0usize;
        let mut tile_cache_count = 0usize;

        for cached in self.page_cache.iter().flatten() {
            cached_pages += 1;
            pixel_bytes += cached.data.img_data.pixels.len();
            if cached.decoded.is_some() {
                decoded_count += 1;
            }
            tile_cache_count += cached.tile_cache.len();
        }

        let pixel_mb = pixel_bytes as f64 / (1024.0 * 1024.0);
        log::info!(
            "Converter memory: {} cached pages ({:.1} MB pixels), {} decoded, {} tiles, {} sent",
            cached_pages,
            pixel_mb,
            decoded_count,
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
                let has_pixels = !cached.data.img_data.pixels.is_empty();
                let has_decoded = cached.decoded.is_some();
                let in_sent = self.sent_for_viewport.contains(&i);
                log::info!(
                    "    page {i}: pixels={has_pixels}, decoded={has_decoded}, sent={in_sent}"
                );
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

fn decode_rgb(pixels: &[u8], width: u32, height: u32) -> Result<RgbImage, PipelineError> {
    let expected = width
        .checked_mul(height)
        .and_then(|v| v.checked_mul(3))
        .ok_or_else(|| pipeline_error("RGB size overflow"))? as usize;
    if pixels.len() != expected {
        return Err(pipeline_error(format!(
            "RGB buffer size mismatch: expected {expected}, got {}",
            pixels.len()
        )));
    }
    RgbImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| pipeline_error("Can't build RGB image from raw pixels"))
}

#[inline]
fn cached_cell_size(cached: &CachedPage) -> CellSize {
    CellSize::new(
        cached.data.img_data.width_cell,
        cached.data.img_data.height_cell,
    )
}

fn take_decoded(cached: &mut CachedPage) -> Result<DynamicImage, PipelineError> {
    if cached.decoded.is_none() {
        let rgb = decode_rgb(
            &cached.data.img_data.pixels,
            cached.data.img_data.width_px,
            cached.data.img_data.height_px,
        )?;
        cached.decoded = Some(DynamicImage::ImageRgb8(rgb));
    }
    Ok(cached.decoded.take().expect("decoded should be present"))
}

fn crop_to_viewport(
    mut img: DynamicImage,
    cell_size: CellSize,
    viewport: &ViewportUpdate,
    picker: &Picker,
) -> (DynamicImage, CellSize) {
    let (_, char_height) = picker.font_size();
    let y_px = viewport
        .y_offset_cells
        .saturating_mul(u32::from(char_height));
    let mut area_cell_height = cell_size.height;

    if y_px < img.height() {
        area_cell_height = viewport.viewport_height_cells;
        let viewport_px =
            u32::from(viewport.viewport_height_cells).saturating_mul(u32::from(char_height));
        let max_height = img.height().saturating_sub(y_px);
        let crop_height = viewport_px.min(max_height).max(1);
        img = img.crop_imm(0, y_px, img.width(), crop_height);
        if crop_height < viewport_px {
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

    (img, CellSize::new(cell_size.width, area_cell_height))
}

fn normalize_for_protocol(img: &mut DynamicImage, cell_size: CellSize, picker: &Picker) {
    if picker.protocol_type() != ProtocolType::Kitty {
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
                        image::imageops::FilterType::Nearest,
                    );
                }
            }
        }
    }
}

fn pad_to_cell_bounds(img: DynamicImage, cell_size: CellSize, picker: &Picker) -> DynamicImage {
    let (char_width, char_height) = picker.font_size();
    let target_width = u32::from(cell_size.width) * u32::from(char_width);
    let target_height = u32::from(cell_size.height) * u32::from(char_height);

    if img.width() == target_width && img.height() == target_height {
        return img;
    }

    let rgb = img.to_rgb8();
    let bg = *rgb.get_pixel(0, 0);
    let mut padded = image::ImageBuffer::from_pixel(target_width, target_height, bg);

    let src_width = rgb.width();
    let copy_width = src_width.min(target_width) as usize;
    let copy_height = rgb.height().min(target_height);
    for y in 0..copy_height {
        let src_row_start = (y * src_width) as usize * 3;
        let dst_row_start = (y * target_width) as usize * 3;
        padded.as_mut()[dst_row_start..dst_row_start + copy_width * 3]
            .copy_from_slice(&rgb.as_raw()[src_row_start..src_row_start + copy_width * 3]);
    }

    DynamicImage::ImageRgb8(padded)
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
    let mut resizer = fir::Resizer::new(fir::ResizeAlg::Nearest);
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
    let mut img = decode_rgb(
        &cached.data.img_data.pixels,
        cached.data.img_data.width_px,
        cached.data.img_data.height_px,
    )?;
    apply_overlays(&mut img, overlays);
    let mut dyn_img = DynamicImage::ImageRgb8(img);

    let mut area_cell_size = cached_cell_size(cached);
    if picker.protocol_type() == ProtocolType::Kitty {
        if let Some(viewport) = viewport {
            if viewport.page == page_num {
                let (cropped, cell_size) =
                    crop_to_viewport(dyn_img, cached_cell_size(cached), viewport, picker);
                dyn_img = cropped;
                area_cell_size = cell_size;
            }
        }
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
    let decoded = take_decoded(cached)?;
    let (_char_width, char_height) = picker.font_size();
    let tile_height_px = u32::from(char_height);
    let viewport_y_px = viewport
        .y_offset_cells
        .saturating_mul(u32::from(char_height));
    let viewport_h_px =
        u32::from(viewport.viewport_height_cells).saturating_mul(u32::from(char_height));
    let image_height = decoded.height();
    let max_height = image_height.saturating_sub(viewport_y_px);
    let visible_h_px = viewport_h_px.min(max_height).max(1);

    let start_tile = viewport_y_px / tile_height_px;
    let end_tile = (viewport_y_px + visible_h_px).div_ceil(tile_height_px);

    let mut tiles = Vec::new();
    let mut cached_count = 0u32;
    let mut encoded_count = 0u32;
    let mut dynamic_count = 0u32;
    for tile_idx in start_tile..end_tile {
        let tile_y_px = tile_idx * tile_height_px;
        if tile_y_px >= image_height {
            break;
        }

        let tile_actual_h_px = tile_height_px.min(image_height - tile_y_px);
        let tile_h_cells = ((tile_actual_h_px as f32) / f32::from(char_height)).ceil() as u16;
        let tile_y_cells = (tile_y_px / u32::from(char_height)) as i32;
        let viewport_y_cells = viewport.y_offset_cells as i32;
        let offset_cells = tile_y_cells - viewport_y_cells;
        if offset_cells < 0 || offset_cells >= i32::from(viewport.viewport_height_cells) {
            continue;
        }

        // Check overlays for THIS tile specifically, not the whole page
        let local = overlays.for_tile(tile_y_px, tile_actual_h_px);
        let tile_has_overlay = !local.is_empty();

        let tile_area = Rect {
            x: 0,
            y: 0,
            width: cached.data.img_data.width_cell,
            height: tile_h_cells,
        };

        let protocol = if !tile_has_overlay {
            // No overlays on this tile - can use cache
            if let Some(existing) = cached.tile_cache.get(&tile_idx) {
                cached_count += 1;
                Arc::clone(existing)
            } else {
                encoded_count += 1;
                let tile_img = decoded.crop_imm(0, tile_y_px, decoded.width(), tile_actual_h_px);
                // Pad to exact cell boundaries so Resize::None can be used
                let tile_img = pad_to_cell_bounds(
                    tile_img,
                    CellSize::new(cached.data.img_data.width_cell, tile_h_cells),
                    picker,
                );
                let new_protocol = picker.new_protocol(tile_img, tile_area, Resize::None).map_err(
                    |e| {
                        pipeline_error(format!(
                            "Image conversion failed; unable to render DynamicImage into a ratatui buffer: {e}"
                        ))
                    },
                )?;
                let arc = Arc::new(new_protocol);
                cached.tile_cache.insert(tile_idx, Arc::clone(&arc));
                arc
            }
        } else {
            // This tile has overlays - must re-encode (don't cache overlay versions)
            dynamic_count += 1;
            let mut tile_img = decoded.crop_imm(0, tile_y_px, decoded.width(), tile_actual_h_px);
            apply_overlays_dynamic(&mut tile_img, &local);
            // Pad to exact cell boundaries so Resize::None can be used
            let tile_img = pad_to_cell_bounds(
                tile_img,
                CellSize::new(cached.data.img_data.width_cell, tile_h_cells),
                picker,
            );
            Arc::new(picker.new_protocol(tile_img, tile_area, Resize::None).map_err(|e| {
                pipeline_error(format!(
                    "Image conversion failed; unable to render DynamicImage into a ratatui buffer: {e}"
                ))
            })?)
        };

        tiles.push(TiledProtocol {
            protocol,
            y_offset_cells: offset_cells as u16,
            height_cells: tile_h_cells,
        });
    }

    if encoded_count > 0 || dynamic_count > 0 || log::log_enabled!(log::Level::Trace) {
        log::debug!(
            "render_viewport_tiles: page={} viewport_y={} tiles={}..{} cached={} encoded={} overlay_tiles={} tile_cache_size={}",
            viewport.page,
            viewport.y_offset_cells,
            start_tile,
            end_tile,
            cached_count,
            encoded_count,
            dynamic_count,
            cached.tile_cache.len()
        );
    }

    cached.decoded = Some(decoded);

    Ok(ConvertedImage::Tiled {
        tiles,
        cell_size: CellSize::new(
            cached.data.img_data.width_cell,
            viewport.viewport_height_cells,
        ),
    })
}

fn render_specific_tiles(
    decoded: &DynamicImage,
    cell_size: CellSize,
    viewport: &ViewportUpdate,
    tile_indices: &[u32],
    overlays: &OverlaySet,
    picker: &Picker,
) -> Result<Vec<TiledProtocol>, PipelineError> {
    let (_, char_height) = picker.font_size();
    let tile_height_px = u32::from(char_height);
    let image_height = decoded.height();
    let viewport_y_cells = viewport.y_offset_cells as i32;
    let viewport_height_cells = i32::from(viewport.viewport_height_cells);

    let mut tiles = Vec::new();
    for &tile_idx in tile_indices {
        let tile_y_px = tile_idx * tile_height_px;
        if tile_y_px >= image_height {
            continue;
        }

        let tile_y_cells = (tile_y_px / u32::from(char_height)) as i32;
        let offset_cells = tile_y_cells - viewport_y_cells;
        if offset_cells < 0 || offset_cells >= viewport_height_cells {
            continue;
        }

        let tile_actual_h_px = tile_height_px.min(image_height - tile_y_px);
        let tile_h_cells = ((tile_actual_h_px as f32) / f32::from(char_height)).ceil() as u16;

        let mut tile_img = decoded.crop_imm(0, tile_y_px, decoded.width(), tile_actual_h_px);
        let local = overlays.for_tile(tile_y_px, tile_actual_h_px);
        apply_overlays_dynamic(&mut tile_img, &local);
        // Pad to exact cell boundaries so Resize::None can be used
        let tile_img = pad_to_cell_bounds(
            tile_img,
            CellSize::new(cell_size.width, tile_h_cells),
            picker,
        );

        let tile_area = Rect {
            x: 0,
            y: 0,
            width: cell_size.width,
            height: tile_h_cells,
        };
        let protocol = picker
            .new_protocol(tile_img, tile_area, Resize::None)
            .map_err(|e| pipeline_error(format!("Couldn't encode tile: {e}")))?;

        tiles.push(TiledProtocol {
            protocol: Arc::new(protocol),
            y_offset_cells: offset_cells as u16,
            height_cells: tile_h_cells,
        });
    }

    Ok(tiles)
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
            let rgb = img.to_rgb8();
            let width = rgb.width();
            let height = rgb.height();
            let data = rgb.into_raw();
            let img = if kitty_shm_support {
                let shm_name = next_shm_name(pid, page_num);
                match super::kittyv2::Image::create_shm_from_rgb(
                    &data,
                    width,
                    height,
                    &shm_name,
                    page_image_id(page_num),
                ) {
                    Ok((shm_img, shm_size)) => {
                        let _ = shm_size;
                        shm_img
                    }
                    Err(e) => {
                        log::warn!(
                            "SHM transfer failed for page {page_num}, falling back to direct: {e:?}"
                        );
                        super::kittyv2::Image::from_rgb_bytes(
                            data,
                            width,
                            height,
                            page_image_id(page_num),
                        )
                    }
                }
            } else {
                super::kittyv2::Image::from_rgb_bytes(data, width, height, page_image_id(page_num))
            };

            Ok(ConvertedImage::Kitty {
                img: ImageState::Queued(img),
                cell_size,
            })
        }
        _ => Ok(ConvertedImage::Generic({
            let area = Rect {
                x: 0,
                y: 0,
                width: cell_size.width,
                height: cell_size.height,
            };
            let img = pad_to_cell_bounds(img, cell_size, picker);
            // After padding to cell bounds, we can use Resize::None
            picker.new_protocol(img, area, Resize::None).map_err(|e| {
                pipeline_error(format!(
                    "Image conversion failed; unable to render DynamicImage into a ratatui buffer: {e}"
                ))
            })?
        })),
    }
}

fn apply_overlays(img: &mut RgbImage, overlays: &OverlaySet) {
    if overlays.comments_are_underlines {
        // Comments are already in underline coordinates (tile rendering)
        draw_underline_rects_direct(img, &overlays.comments);
    } else {
        // Comments are selection rects, calculate underline position
        apply_underline_rects(img, &overlays.comments, OverlayOp::Comment);
    }
    apply_rects_op(img, &overlays.selection, OverlayOp::Selection);
    apply_rects_op(img, &overlays.visual, OverlayOp::Selection);
    if let Some(cursor) = overlays.cursor {
        apply_rects_op(img, std::slice::from_ref(&cursor), OverlayOp::Cursor);
    }
}

fn apply_overlays_dynamic(img: &mut DynamicImage, overlays: &OverlaySet) {
    if let DynamicImage::ImageRgb8(rgb) = img {
        apply_overlays(rgb, overlays);
        return;
    }

    let mut rgb = img.to_rgb8();
    apply_overlays(&mut rgb, overlays);
    *img = DynamicImage::ImageRgb8(rgb);
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

            c0.copy_from_slice(v0.as_array_ref());
            c1.copy_from_slice(v1.as_array_ref());
            c2.copy_from_slice(v2.as_array_ref());
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

/// Apply overlay operation below each rect (underline effect)
fn apply_underline_rects(img: &mut RgbImage, rects: &[PixelRect], _op: OverlayOp) {
    const UNDERLINE_THICKNESS: u32 = 3;
    const UNDERLINE_OFFSET: u32 = 2; // Gap between text bottom and underline
    // Purple color matching EPUB comments (base_0e from Oceanic Next theme: 0xC594C5)
    const UNDERLINE_R: u8 = 0xC5; // 197
    const UNDERLINE_G: u8 = 0x94; // 148
    const UNDERLINE_B: u8 = 0xC5; // 197

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

        // Draw solid purple underline matching EPUB comment style
        for y in clamped.y0..clamped.y1 {
            let row_start = y as usize * stride;
            for x in clamped.x0..clamped.x1 {
                let px_start = row_start + x as usize * 3;
                if px_start + 2 < buf.len() {
                    buf[px_start] = UNDERLINE_R;
                    buf[px_start + 1] = UNDERLINE_G;
                    buf[px_start + 2] = UNDERLINE_B;
                }
            }
        }
    }
}

/// Draw underlines directly at rect coordinates (for tile rendering where
/// underline positions are pre-computed in for_tile).
fn draw_underline_rects_direct(img: &mut RgbImage, rects: &[PixelRect]) {
    // Purple color matching EPUB comments (base_0e from Oceanic Next theme)
    const UNDERLINE_R: u8 = 0xC5; // 197
    const UNDERLINE_G: u8 = 0x94; // 148
    const UNDERLINE_B: u8 = 0xC5; // 197

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
                    buf[px_start] = UNDERLINE_R;
                    buf[px_start + 1] = UNDERLINE_G;
                    buf[px_start + 2] = UNDERLINE_B;
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
) -> Result<(), SendError<Result<RenderedFrame, PipelineError>>> {
    use std::time::{Duration, Instant};

    log::info!("Converter using protocol: {:?}", picker.protocol_type());
    let mut engine = ConverterEngine::new(picker, prerender, kitty_shm_support);
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
