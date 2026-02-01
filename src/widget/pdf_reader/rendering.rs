//! PDF reader view rendering
//!
//! This module handles the rendering of PDF pages to the terminal,
//! including continuous scroll mode, page display, and UI overlays.

use std::collections::HashMap;
use std::io::stdout;
use std::num::NonZeroU32;
use std::sync::Arc;

use crate::vendored::ratatui_image::{FontSize, Image};
use crossterm::{execute, terminal::BeginSynchronizedUpdate};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Position, Rect},
    prelude::{Line, Text},
    style::{Color, Modifier, Style},
    symbols::border,
    symbols::line,
    text::Span,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::notification::NotificationManager;
use crate::pdf::kittyv2::{DisplayLocation, ImageState};
use crate::pdf::{CellSize, ConvertedImage};
use crate::pdf::{Command, ConversionCommand, RenderResponse, RenderedFrame, WorkerFault};
use crate::settings::{PdfRenderMode, get_pdf_render_mode};
use crate::theme::{Base16Palette, current_theme};
use crate::{bookmarks::Bookmarks, navigation_panel::TableOfContents};

use super::navigation::{get_pdf_chapter_title, save_pdf_bookmark, update_pdf_toc_active};
use super::region::ImageRegion;
use super::state::{CommentEditMode, CommentInputState, PdfReaderState, SEPARATOR_HEIGHT};
use super::types::{
    DisplayBatch, ImageRequest, LastRender, PdfDisplayPlan, PdfDisplayRequest, PendingScroll,
    RenderLayout, RenderedInfo, VisiblePageUiInfo,
};

const KITTY_CACHE_RADIUS: usize = 10;

/// Minimum width in columns for the comment textarea to be usable
pub(crate) const MIN_COMMENT_TEXTAREA_WIDTH: u16 = 20;

pub(crate) fn convert_page_image(
    img_data: &crate::pdf::ImageData,
    picker: &crate::vendored::ratatui_image::picker::Picker,
) -> Option<ConvertedImage> {
    use crate::vendored::ratatui_image::Resize;
    use image::{DynamicImage, RgbImage};

    let rgb_img = RgbImage::from_raw(
        img_data.width_px,
        img_data.height_px,
        img_data.pixels.clone(),
    )?;
    let dyn_img = DynamicImage::ImageRgb8(rgb_img);

    let area = Rect {
        x: 0,
        y: 0,
        width: img_data.width_cell,
        height: img_data.height_cell,
    };

    match picker.new_protocol(dyn_img, area, Resize::Fit(None)) {
        Ok(protocol) => Some(ConvertedImage::Generic(protocol)),
        Err(e) => {
            log::error!("Failed to convert PDF image: {e}");
            None
        }
    }
}

/// Send cached page data to the converter if needed.
/// Only sends if the page doesn't already have an image (Queued or Uploaded).
/// This avoids unnecessary channel traffic since the converter also checks sent_for_viewport.
fn send_cached_page_to_converter(
    service: &crate::pdf::RenderService,
    tx: &flume::Sender<ConversionCommand>,
    page: usize,
    rendered: &[RenderedInfo],
) {
    // Skip if page already has an image (converter already has it or it's uploaded)
    if let Some(info) = rendered.get(page) {
        if info.img.is_some() {
            return;
        }
    }

    if let Some(cached) = service.get_cached_page(page) {
        log::trace!("Sending cached page {page} to converter (no local image)");
        let _ = tx.send(ConversionCommand::EnqueuePage(Arc::clone(&cached)));
    }
}

/// Result of apply_render_responses indicating what was updated
pub(crate) struct RenderUpdateResult {
    /// Whether any updates happened (requiring redraw)
    pub updated: bool,
    /// If a converted frame arrived, the page index it was for
    pub converted_frame_page: Option<usize>,
}

pub(crate) fn apply_render_responses(
    pdf_reader: &mut PdfReaderState,
    responses: Vec<RenderResponse>,
    conversion_tx: Option<&flume::Sender<ConversionCommand>>,
    conversion_rx: Option<&flume::Receiver<Result<RenderedFrame, WorkerFault>>>,
    picker: Option<&crate::vendored::ratatui_image::picker::Picker>,
    notifications: &mut NotificationManager,
) -> RenderUpdateResult {
    let mut updated = !responses.is_empty();
    let mut converted_frame_page = None;
    let use_kitty = pdf_reader.is_kitty;

    for response in responses {
        match response {
            RenderResponse::Page { page, data, .. } => {
                log::trace!("Received page {page} data from worker");

                while pdf_reader.rendered.len() <= page {
                    pdf_reader.rendered.push(RenderedInfo::default());
                }

                let info = &mut pdf_reader.rendered[page];
                info.pixel_w = Some(data.img_data.width_px);
                info.pixel_h = Some(data.img_data.height_px);
                info.full_cell_size = Some(CellSize::new(
                    data.img_data.width_cell,
                    data.img_data.height_cell,
                ));
                info.scale_factor = Some(data.scale_factor);
                info.line_bounds = data.line_bounds.clone();
                info.link_rects = data.link_rects.clone();
                info.page_px_height = Some(data.page_height_px);

                // Track if we have line_bounds for pending search check
                let has_line_bounds = !info.line_bounds.is_empty();

                if has_line_bounds {
                    pdf_reader
                        .page_numbers
                        .observe(page, &info.line_bounds, data.page_height_px);

                    // Update cursor if normal mode is active and cursor is on this page
                    if pdf_reader.normal_mode.active && pdf_reader.normal_mode.cursor.page == page {
                        if let Some(tx) = conversion_tx {
                            let cursor_rect =
                                pdf_reader.normal_mode.get_cursor_rect(&info.line_bounds);
                            let _ = tx.send(ConversionCommand::UpdateCursor(cursor_rect));
                        }
                    }
                }

                if let Some(tx) = conversion_tx {
                    log::trace!("Sending EnqueuePage for page {page} to converter");
                    let _ = tx.send(ConversionCommand::EnqueuePage(Arc::clone(&data)));
                } else if !use_kitty {
                    if let Some(picker) = picker {
                        if let Some(converted) = convert_page_image(&data.img_data, picker) {
                            info.img = Some(converted);
                        }
                    }
                }

                // Check for pending search highlight (after info borrow ends)
                if has_line_bounds {
                    if let Some((pending_page, ref query)) =
                        pdf_reader.pending_search_highlight.clone()
                    {
                        if pending_page == page {
                            let selection_rects = pdf_reader.find_text_selection_rects(page, query);
                            if !selection_rects.is_empty() {
                                if let Some(tx) = conversion_tx {
                                    let _ = tx
                                        .send(ConversionCommand::UpdateSelection(selection_rects));
                                }
                            }
                            pdf_reader.pending_search_highlight = None;
                        }
                    }
                }
            }
            RenderResponse::Error { error, .. } => {
                log::error!("PDF render error: {error}");
            }
            RenderResponse::ExtractedText { text, .. } => {
                if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(&text)) {
                    log::error!("Failed to copy to clipboard: {e}");
                } else {
                    notifications.info(format!("Copied {} chars", text.len()));
                }
            }
            _ => {}
        }
    }

    if let Some(rx) = conversion_rx {
        while let Ok(result) = rx.try_recv() {
            match result {
                Ok(frame) => {
                    log::trace!("Received converted frame for page {}", frame.index);
                    if let Some(info) = pdf_reader.rendered.get_mut(frame.index) {
                        // Try to merge TileUpdate into existing Tiled image
                        if matches!(frame.image, ConvertedImage::TileUpdate { .. }) {
                            if let Some(ref mut existing) = info.img {
                                if existing.merge_tile_update(frame.image) {
                                    log::trace!("Merged tile update for page {}", frame.index);
                                } else {
                                    log::warn!(
                                        "TileUpdate for page {} but no Tiled image to merge into",
                                        frame.index
                                    );
                                }
                            }
                        } else {
                            info.img = Some(frame.image);
                            log::trace!("Set img for page {}", frame.index);
                        }
                    }
                    pdf_reader.last_render.rect = Rect::default();
                    updated = true;
                    // Track the current page's frame arrival for waiting_for_page optimization
                    if frame.index == pdf_reader.page {
                        converted_frame_page = Some(frame.index);
                    }
                }
                Err(e) => {
                    log::error!("PDF conversion error: {e}");
                }
            }
        }
    }

    // For Kitty: clear Uploaded states for pages far from current viewport.
    // Kitty has a limited image cache and may evict old images. If we try to
    // display an evicted image with a stale ID, it silently fails. By clearing
    // Uploaded states for distant pages, we force re-conversion when they
    // scroll back into view.
    if use_kitty {
        let current_page = pdf_reader.page;
        let mut cleared_pages = Vec::new();
        for (idx, info) in pdf_reader.rendered.iter_mut().enumerate() {
            if idx.abs_diff(current_page) > KITTY_CACHE_RADIUS {
                if let Some(ConvertedImage::Kitty { ref img, .. }) = info.img {
                    if img.is_uploaded() {
                        log::trace!(
                            "Clearing stale Uploaded state for distant page {idx} (current={current_page})"
                        );
                        info.img = None;
                        cleared_pages.push(idx);
                    }
                }
            }
        }
        // Notify converter about cleared pages so it clears sent_for_viewport
        // This prevents desync where converter thinks page is sent but rendering
        // code has cleared it.
        if !cleared_pages.is_empty() {
            if let Some(tx) = conversion_tx {
                let _ = tx.send(ConversionCommand::DisplayFailed(cleared_pages));
            }
        }
    }

    RenderUpdateResult {
        updated,
        converted_frame_page,
    }
}

#[allow(clippy::too_many_arguments)]
impl PdfReaderState {
    pub fn render_in_area(
        &mut self,
        f: &mut ratatui::Frame,
        area: Rect,
        font_size: (u16, u16),
        text_color: Color,
        border_color: Color,
        bg_color: Color,
        mut service: Option<&mut crate::pdf::RenderService>,
        conversion_tx: Option<&flume::Sender<ConversionCommand>>,
        pending_display: &mut Option<PdfDisplayPlan>,
        bookmarks: &mut Bookmarks,
        last_bookmark_save: &mut std::time::Instant,
        table_of_contents: &mut TableOfContents,
        toc_height: usize,
    ) {
        let current_page = self.page + 1;
        let total_pages = self.rendered.len();
        let chapter_title = get_pdf_chapter_title(&self.toc_entries, self.page);
        let title_text = if let Some(chapter) = chapter_title {
            format!("[{current_page}/{total_pages}] {chapter}")
        } else {
            format!("[{current_page}/{total_pages}]")
        };

        let progress = if total_pages > 0 {
            ((current_page as f64 / total_pages as f64) * 100.0).round() as u8
        } else {
            0
        };

        let palette = current_theme();
        let mode_title = if self.page_search.is_input_active() {
            let border_style = Style::default().fg(border_color);
            let mode_style = Style::default()
                .fg(palette.base_07)
                .bg(palette.base_0b)
                .add_modifier(Modifier::BOLD);
            let search_text = self
                .page_search
                .input
                .as_ref()
                .map(|ta| ta.lines().join(""))
                .unwrap_or_default();
            Some(
                Line::from(vec![
                    Span::styled(line::HORIZONTAL, border_style),
                    Span::styled(" /", mode_style),
                    Span::styled(
                        format!("{search_text}█"),
                        Style::default().fg(palette.base_07).bg(palette.base_0b),
                    ),
                    Span::raw(" "),
                ])
                .left_aligned(),
            )
        } else if self.comment_input.is_active() {
            let border_style = Style::default().fg(border_color);
            let mode_style = Style::default()
                .fg(palette.base_07)
                .bg(palette.base_0a)
                .add_modifier(Modifier::BOLD);
            Some(
                Line::from(vec![
                    Span::styled(line::HORIZONTAL, border_style),
                    Span::styled(" Comment ", mode_style),
                ])
                .left_aligned(),
            )
        } else if self.normal_mode.is_visual_active() {
            let border_style = Style::default().fg(border_color);
            let mode_style = Style::default()
                .fg(palette.base_07)
                .bg(palette.base_0e)
                .add_modifier(Modifier::BOLD);
            Some(
                Line::from(vec![
                    Span::styled(line::HORIZONTAL, border_style),
                    Span::styled(" VISUAL ", mode_style),
                ])
                .left_aligned(),
            )
        } else if self.normal_mode.active {
            let border_style = Style::default().fg(border_color);
            let mode_style = Style::default()
                .fg(palette.base_07)
                .bg(palette.base_0d)
                .add_modifier(Modifier::BOLD);
            Some(
                Line::from(vec![
                    Span::styled(line::HORIZONTAL, border_style),
                    Span::styled(" NORMAL ", mode_style),
                ])
                .left_aligned(),
            )
        } else {
            None
        };
        let progress_title = Line::from(format!(" {progress}% ")).right_aligned();

        if self
            .hud_message
            .as_ref()
            .is_some_and(|hud| hud.is_expired())
        {
            self.hud_message = None;
        }
        let hud_message = self.hud_message.as_ref();

        let mut content_block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .title_bottom(progress_title)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().fg(text_color).bg(bg_color));
        if let Some(mode_title) = mode_title {
            content_block = content_block.title_bottom(mode_title);
        }
        if let Some(hud) = hud_message {
            content_block = content_block.title_bottom(hud.styled_line(palette));
        }
        let inner_area = content_block.inner(area);
        f.render_widget(content_block, area);

        if let Some(service) = service.as_deref_mut() {
            service.apply_command(Command::SetArea(inner_area));
        }

        let layout = RenderLayout {
            page_area: inner_area,
        };
        let previous_page = self.page;
        let is_kitty = self.is_kitty;
        let display_batch = self.render(f, &layout, font_size);

        if is_kitty {
            *pending_display = Some(build_display_plan(display_batch));
        } else {
            *pending_display = None;
            let _ = display_batch;
        }

        let service_needs_page_update = service
            .as_ref()
            .is_some_and(|service| service.state().current_page != self.page);
        let needs_initial_image = self
            .rendered
            .get(self.page)
            .is_some_and(|info| info.img.is_none());

        let expected_page = if is_kitty {
            self.page
        } else {
            self.expected_page_from_scroll()
        };
        if expected_page != self.page && self.update_page_from_scroll(expected_page) {
            save_pdf_bookmark(bookmarks, self, last_bookmark_save, false);
        }

        if self.page != previous_page || service_needs_page_update {
            if let Some(service) = service.as_deref_mut() {
                service.apply_command(Command::GoToPage(self.page));
            }
            if let Some(tx) = conversion_tx {
                let _ = tx.send(ConversionCommand::NavigateTo(self.page));
            }
        }

        if needs_initial_image {
            if let Some(service) = service.as_deref_mut() {
                service.request_page(self.page);
            }
            if is_kitty {
                if let Some(tx) = conversion_tx {
                    let _ = tx.send(ConversionCommand::NavigateTo(self.page));
                }
            }
        }

        if expected_page != self.page {
            if let Some(tx) = conversion_tx {
                let _ = tx.send(ConversionCommand::NavigateTo(expected_page));
            }
        }

        if let Some(service) = service {
            service.request_page_if_needed(expected_page);
            service.request_page_if_needed(expected_page.saturating_add(1));
            if expected_page > 0 {
                service.request_page_if_needed(expected_page - 1);
            }

            // For Kitty: if page is cached in service but converter might not have it,
            // re-send the cached data. Only sends if local state has no image.
            if is_kitty {
                if let Some(tx) = conversion_tx {
                    send_cached_page_to_converter(service, tx, expected_page, &self.rendered);
                    send_cached_page_to_converter(
                        service,
                        tx,
                        expected_page.saturating_add(1),
                        &self.rendered,
                    );
                    if expected_page > 0 {
                        send_cached_page_to_converter(
                            service,
                            tx,
                            expected_page - 1,
                            &self.rendered,
                        );
                    }
                }
            }
        }

        update_pdf_toc_active(table_of_contents, self, self.page, toc_height);
    }
}

pub(crate) fn build_display_plan(display_batch: DisplayBatch<'_>) -> PdfDisplayPlan {
    match display_batch {
        DisplayBatch::NoChange => PdfDisplayPlan::NoChange,
        DisplayBatch::Clear => PdfDisplayPlan::Clear,
        DisplayBatch::Display(requests) => PdfDisplayPlan::Display(
            requests
                .into_iter()
                .map(|request| PdfDisplayRequest {
                    page: request.page,
                    position: request.position,
                    location: request.location,
                })
                .collect(),
        ),
    }
}

pub(crate) fn execute_display_plan(
    plan: PdfDisplayPlan,
    pdf_reader: &mut PdfReaderState,
    has_popup: bool,
    conversion_tx: Option<&flume::Sender<ConversionCommand>>,
) {
    if has_popup && pdf_reader.is_kitty {
        if let Err(e) =
            crate::pdf::kittyv2::execute_display_batch(crate::pdf::kittyv2::DisplayBatch::Clear)
        {
            log::error!("Failed to clear kitty graphics for popup: {e}");
        }
        pdf_reader.last_render.rect = Rect::default();
        return;
    }

    if pdf_reader.is_kitty {
        let total_pages = pdf_reader.rendered.len();
        if total_pages > 0 {
            let current_page = pdf_reader.page.min(total_pages.saturating_sub(1));
            let window_start = current_page.saturating_sub(KITTY_CACHE_RADIUS);
            let window_end = current_page
                .saturating_add(KITTY_CACHE_RADIUS)
                .min(total_pages.saturating_sub(1));

            if pdf_reader.last_kitty_cache_window != Some((window_start, window_end)) {
                let mut deleted_pages = Vec::new();
                if pdf_reader.kitty_delete_range_supported {
                    let mut delete_ranges = Vec::new();
                    if window_start > 0 {
                        delete_ranges.push((0, window_start - 1));
                    }
                    if window_end + 1 < total_pages {
                        delete_ranges.push((window_end + 1, total_pages - 1));
                    }

                    for (start_page, end_page) in delete_ranges {
                        log::info!(
                            "kitty delete images: pages {}..{} (ids {}..{})",
                            start_page,
                            end_page,
                            start_page + 1,
                            end_page + 1
                        );
                        for page in start_page..=end_page {
                            if let Some(info) = pdf_reader.rendered.get_mut(page) {
                                if let Some(ConvertedImage::Kitty { ref img, .. }) = info.img {
                                    if img.is_uploaded() {
                                        info.img = None;
                                    }
                                }
                            }
                            deleted_pages.push(page);
                        }

                        let Some(start_id) = NonZeroU32::new(start_page as u32 + 1) else {
                            continue;
                        };
                        let Some(end_id) = NonZeroU32::new(end_page as u32 + 1) else {
                            continue;
                        };
                        if let Err(e) =
                            crate::pdf::kittyv2::delete_images_by_range(start_id, end_id)
                        {
                            log::error!(
                                "Failed to delete kitty images for pages {start_page}..{end_page}: {e}"
                            );
                        }
                    }
                } else {
                    for page in 0..window_start {
                        if let Some(info) = pdf_reader.rendered.get_mut(page) {
                            if let Some(ConvertedImage::Kitty { ref img, .. }) = info.img {
                                if img.is_uploaded() {
                                    info.img = None;
                                }
                            }
                        }
                        deleted_pages.push(page);
                        let Some(id) = NonZeroU32::new(page as u32 + 1) else {
                            continue;
                        };
                        let _ = crate::pdf::kittyv2::delete_image_by_id(id);
                    }
                    for page in (window_end + 1)..total_pages {
                        if let Some(info) = pdf_reader.rendered.get_mut(page) {
                            if let Some(ConvertedImage::Kitty { ref img, .. }) = info.img {
                                if img.is_uploaded() {
                                    info.img = None;
                                }
                            }
                        }
                        deleted_pages.push(page);
                        let Some(id) = NonZeroU32::new(page as u32 + 1) else {
                            continue;
                        };
                        let _ = crate::pdf::kittyv2::delete_image_by_id(id);
                    }
                }

                if !deleted_pages.is_empty() {
                    if let Some(tx) = conversion_tx {
                        let _ = tx.send(ConversionCommand::DisplayFailed(deleted_pages));
                    }
                }
                pdf_reader.last_kitty_cache_window = Some((window_start, window_end));
            }
        }
    }

    let (batch, clear_visible_pages) = match plan {
        PdfDisplayPlan::NoChange => (crate::pdf::kittyv2::DisplayBatch::NoChange, false),
        PdfDisplayPlan::Clear => (crate::pdf::kittyv2::DisplayBatch::Clear, true),
        PdfDisplayPlan::Display(requests) => {
            use std::collections::HashSet;

            let next_visible_pages: HashSet<usize> =
                requests.iter().map(|request| request.page).collect();
            let removed_pages: Vec<usize> = pdf_reader
                .kitty_visible_pages
                .difference(&next_visible_pages)
                .copied()
                .collect();

            if !removed_pages.is_empty() || !next_visible_pages.is_empty() {
                let mut next_pages: Vec<usize> = next_visible_pages.iter().copied().collect();
                next_pages.sort_unstable();
                let mut removed_sorted = removed_pages.clone();
                removed_sorted.sort_unstable();
                log::info!(
                    "kitty placements: prev={:?} next={:?} removed={:?}",
                    pdf_reader.kitty_visible_pages,
                    next_pages,
                    removed_sorted
                );
            }

            if !removed_pages.is_empty() {
                for page in removed_pages {
                    let Some(id) = NonZeroU32::new(page as u32 + 1) else {
                        continue;
                    };
                    if let Err(e) = crate::pdf::kittyv2::clear_placement(id) {
                        log::error!("Failed to clear kitty placement for page {page}: {e}");
                    }
                }
            }

            pdf_reader.kitty_visible_pages = next_visible_pages;

            let mut mapped = Vec::with_capacity(requests.len());
            let mut seen_pages = HashSet::new();
            let rendered_len = pdf_reader.rendered.len();
            let rendered_ptr = pdf_reader.rendered.as_mut_ptr();

            for request in requests {
                if request.page >= rendered_len || !seen_pages.insert(request.page) {
                    continue;
                }

                // SAFETY: We ensure unique page indices via seen_pages.
                let info = unsafe { &mut *rendered_ptr.add(request.page) };
                let Some(crate::pdf::ConvertedImage::Kitty { img, .. }) = info.img.as_mut() else {
                    continue;
                };

                mapped.push(crate::pdf::kittyv2::ImageRequest {
                    image: img,
                    page: request.page,
                    position: request.position,
                    location: request.location,
                });
            }

            (crate::pdf::kittyv2::DisplayBatch::Display(mapped), false)
        }
    };

    if clear_visible_pages {
        pdf_reader.kitty_visible_pages.clear();
    }

    match crate::pdf::kittyv2::execute_display_batch_with_failures(batch) {
        Ok(failed_pages) => {
            if !failed_pages.is_empty() {
                log::debug!("kittyv2 display failed for pages {failed_pages:?}");
                for page in &failed_pages {
                    if let Some(info) = pdf_reader.rendered.get_mut(*page) {
                        if let Some(ConvertedImage::Kitty { ref img, .. }) = info.img {
                            if img.is_uploaded() {
                                info.img = None;
                            }
                        }
                    }
                }
                if let Some(tx) = conversion_tx {
                    let _ = tx.send(ConversionCommand::DisplayFailed(failed_pages));
                }
            }
        }
        Err(e) => {
            log::error!("Failed to render kitty PDF batch: {e}");
        }
    }

    // Silence unused warnings
    let _ = conversion_tx;
}

pub(crate) fn update_non_kitty_viewport(
    pdf_reader: &mut PdfReaderState,
    conversion_tx: Option<&flume::Sender<ConversionCommand>>,
) {
    let viewport_to_send = {
        if pdf_reader.is_kitty {
            return;
        }
        let viewport = pdf_reader.current_viewport_update().or_else(|| {
            pdf_reader
                .coord_info
                .map(|(area, _font_size)| crate::pdf::ViewportUpdate {
                    page: pdf_reader.page,
                    y_offset_cells: pdf_reader.non_kitty_scroll_offset,
                    viewport_height_cells: area.height,
                    viewport_width_cells: area.width,
                })
        });
        let Some(viewport) = viewport else {
            return;
        };
        if pdf_reader.last_sent_viewport == Some(viewport) {
            None
        } else {
            pdf_reader.last_sent_viewport = Some(viewport);
            Some(viewport)
        }
    };

    if let (Some(viewport), Some(tx)) = (viewport_to_send, conversion_tx) {
        let _ = tx.send(ConversionCommand::UpdateViewport(viewport));
    }
}

/// Internal structure for visible page calculation
struct VisiblePageInfo {
    page_idx: usize,
    screen_y_start: u16,
    img_clip_top_px: u32,
    display_rows: u16,
    offset_dest_cells: u16,
    cell_size: CellSize,
    dest_w: u16,
    dest_h: u16,
}

impl PdfReaderState {
    /// Render single page mode (Kitty terminals with Page render mode)
    /// Shows only the current page with zoom and scroll within that page.
    #[allow(clippy::too_many_arguments)]
    fn render_single_page_kitty<'a>(
        rendered: &'a mut [RenderedInfo],
        frame: &mut Frame<'_>,
        img_area: Rect,
        font_size: FontSize,
        zoom: &mut crate::pdf::Zoom,
        current_page: usize,
        palette: &Base16Palette,
    ) -> DisplayBatch<'a> {
        let zoom_factor = zoom.factor();

        // Get the current page's rendered info
        let Some(rendered_page) = rendered.get_mut(current_page) else {
            Self::render_loading_in(frame, img_area, palette);
            return DisplayBatch::Clear;
        };

        let Some(ConvertedImage::Kitty {
            ref mut img,
            cell_size,
        }) = rendered_page.img
        else {
            Self::render_loading_in(frame, img_area, palette);
            return DisplayBatch::Clear;
        };

        if cell_size.height == 0 || cell_size.width == 0 {
            Self::render_loading_in(frame, img_area, palette);
            return DisplayBatch::Clear;
        }

        // Calculate scaled dimensions
        let dest_w = ((f32::from(cell_size.width) * zoom_factor).ceil() as u16).max(1);
        let dest_h = ((f32::from(cell_size.height) * zoom_factor).ceil() as u16).max(1);

        // Clamp vertical scroll offset to page bounds
        let max_scroll = dest_h.saturating_sub(img_area.height);
        let scroll_offset = (zoom.global_scroll_offset as u16).min(max_scroll);
        zoom.global_scroll_offset = u32::from(scroll_offset);

        // Calculate source clip based on scroll offset
        let source_y_px = if scroll_offset > 0 {
            let offset_dest = f32::from(scroll_offset);
            let source_y = (offset_dest / zoom_factor) * f32::from(font_size.1);
            source_y.floor().max(0.0) as u32
        } else {
            0
        };

        // Clamp horizontal pan
        let (source_x_cells, visible_source_w, display_x_offset, display_cols) =
            if dest_w <= img_area.width {
                let x_offset = (img_area.width - dest_w) / 2;
                zoom.cell_pan_from_left = 0;
                (0u16, cell_size.width, x_offset, dest_w)
            } else {
                let visible_source = (f32::from(img_area.width) / zoom_factor).ceil() as u16;
                let max_pan = cell_size.width.saturating_sub(visible_source);
                zoom.cell_pan_from_left = zoom.cell_pan_from_left.min(max_pan);
                let pan = zoom.cell_pan_from_left;
                let remaining_width = cell_size.width.saturating_sub(pan);
                let visible = remaining_width.min(visible_source);
                (pan, visible, 0u16, img_area.width)
            };

        // Calculate display height
        let remaining_dest_h = dest_h.saturating_sub(scroll_offset);
        let max_display_rows = remaining_dest_h.min(img_area.height);

        // Calculate source height in pixels
        let source_total_h_px = u32::from(cell_size.height) * u32::from(font_size.1);
        let available_h_px = source_total_h_px.saturating_sub(source_y_px);

        // Calculate how many rows the available source content can fill without stretching
        // This ensures pixel-perfect scaling: source_h / display_rows == font_size.1 / zoom_factor
        let rows_from_available =
            ((available_h_px as f32 * zoom_factor) / f32::from(font_size.1)).floor() as u16;
        let display_rows = rows_from_available.min(max_display_rows).max(1);

        // Calculate actual source height for the display rows
        let visible_source_h_px =
            ((f32::from(display_rows) * f32::from(font_size.1)) / zoom_factor).round() as u32;
        let visible_source_h_px = visible_source_h_px.min(available_h_px).max(1);

        // Center vertically if page is smaller than viewport
        let display_y_offset = if dest_h < img_area.height {
            (img_area.height - dest_h) / 2
        } else {
            0
        };

        let image_request = ImageRequest {
            image: img,
            page: current_page,
            position: Position {
                x: img_area.x + display_x_offset,
                y: img_area.y + display_y_offset,
            },
            location: DisplayLocation {
                x: u32::from(source_x_cells) * u32::from(font_size.0),
                y: source_y_px,
                width: u32::from(visible_source_w) * u32::from(font_size.0),
                height: visible_source_h_px,
                columns: display_cols,
                rows: display_rows,
            },
        };

        DisplayBatch::Display(vec![image_request])
    }

    /// Render continuous scroll mode (Kitty terminals)
    #[allow(clippy::too_many_arguments)]
    fn render_continuous_scroll<'a>(
        rendered: &'a mut [RenderedInfo],
        frame: &mut Frame<'_>,
        img_area: Rect,
        font_size: FontSize,
        zoom: &mut crate::pdf::Zoom,
        current_page_hint: usize,
        separator_height: u16,
        muted_color: Color,
        palette: &Base16Palette,
    ) -> (DisplayBatch<'a>, Option<usize>, Vec<VisiblePageUiInfo>) {
        let zoom_factor = zoom.factor();
        let viewport_height = u32::from(img_area.height);

        let reference_height: Option<u16> = rendered
            .iter()
            .find_map(|page| page.img.as_ref().map(|img| img.cell_dimensions().height));

        if reference_height.is_none() && zoom.global_scroll_offset > 0 {
            zoom.global_scroll_offset = 0;
        }
        let scroll_offset = zoom.global_scroll_offset;

        let mut cumulative_y: u32 = 0;
        let mut visible_info: Vec<VisiblePageInfo> = Vec::new();
        #[cfg(feature = "pdf")]
        let mut viewport_log = String::new();

        for (idx, rendered_page) in rendered.iter().enumerate() {
            let cell_size = rendered_page
                .img
                .as_ref()
                .map_or(CellSize::new(0, 0), |img| img.cell_dimensions());
            if cell_size.height == 0 {
                let estimated_h = reference_height.unwrap_or(img_area.height);
                let dest_h = ((f32::from(estimated_h) * zoom_factor).ceil() as u32).max(1);
                cumulative_y += dest_h + u32::from(separator_height);
                continue;
            }

            let dest_w = ((f32::from(cell_size.width) * zoom_factor).ceil() as u16).max(1);
            let dest_h = ((f32::from(cell_size.height) * zoom_factor).ceil() as u16).max(1);

            let page_start = cumulative_y;
            let page_end = cumulative_y + u32::from(dest_h);
            let viewport_start = scroll_offset;
            let viewport_end = scroll_offset + viewport_height;

            let mut img_clip_top_px = 0u32;
            if page_end > viewport_start && page_start < viewport_end {
                let screen_y_start = if page_start >= scroll_offset {
                    (page_start - scroll_offset) as u16
                } else {
                    0
                };
                img_clip_top_px = if scroll_offset > page_start {
                    let offset_dest = (scroll_offset - page_start) as f32;
                    let source_y_px = (offset_dest / zoom_factor) * f32::from(font_size.1);
                    source_y_px.floor().max(0.0) as u32
                } else {
                    0
                };
                let source_total_h_px = u32::from(cell_size.height) * u32::from(font_size.1);
                if img_clip_top_px >= source_total_h_px {
                    img_clip_top_px = source_total_h_px.saturating_sub(1);
                }
                let available_height = img_area.height.saturating_sub(screen_y_start);
                let offset_dest_cells = if scroll_offset > page_start {
                    (scroll_offset - page_start) as u16
                } else {
                    0
                };
                let remaining_page = dest_h.saturating_sub(offset_dest_cells);
                let display_rows = remaining_page.min(available_height);

                if display_rows > 0 {
                    visible_info.push(VisiblePageInfo {
                        page_idx: idx,
                        screen_y_start,
                        img_clip_top_px,
                        display_rows,
                        offset_dest_cells,
                        cell_size,
                        dest_w,
                        dest_h,
                    });
                }
            }
            #[cfg(feature = "pdf")]
            {
                if !viewport_log.is_empty() {
                    viewport_log.push_str(" | ");
                }
                let overlap = page_end > viewport_start && page_start < viewport_end;
                let log_entry = format!(
                    "{idx}:{page_start}-{page_end} overlap={overlap} dest_h={dest_h} clip_top_px={img_clip_top_px}"
                );
                viewport_log.push_str(&log_entry);
            }

            cumulative_y = page_end + u32::from(separator_height);
            if page_start > viewport_end {
                break;
            }
        }

        let current_page = if visible_info.is_empty() {
            None
        } else {
            const STICKY_RATIO: f32 = 0.40;

            let viewport_rows = img_area.height.max(1) as f32;
            let mut best_page = visible_info[0].page_idx;
            let mut best_ratio = visible_info[0].display_rows as f32 / viewport_rows;
            let mut sticky_ratio = None;

            for info in &visible_info {
                let ratio = info.display_rows as f32 / viewport_rows;
                if ratio > best_ratio {
                    best_ratio = ratio;
                    best_page = info.page_idx;
                }
                if info.page_idx == current_page_hint {
                    sticky_ratio = Some(ratio);
                }
            }

            if let Some(ratio) = sticky_ratio {
                if ratio >= STICKY_RATIO {
                    Some(current_page_hint)
                } else {
                    Some(best_page)
                }
            } else {
                Some(best_page)
            }
        };
        #[cfg(feature = "pdf")]
        {
            let visible_pages: Vec<usize> = visible_info.iter().map(|info| info.page_idx).collect();
            log::info!(
                "kitty viewport: scroll_offset={scroll_offset} viewport_h={viewport_height} current_hint={current_page_hint} current={current_page:?} visible={visible_pages:?} pages={{ {viewport_log} }}"
            );
        }

        if visible_info.is_empty() {
            Self::render_loading_in(frame, img_area, palette);
            return (DisplayBatch::Clear, current_page, Vec::new());
        }

        // Render separators between pages
        for (idx, info) in visible_info.iter().enumerate() {
            if idx > 0 {
                let sep_y = img_area.y + info.screen_y_start.saturating_sub(separator_height);
                if sep_y >= img_area.y && sep_y < img_area.y + img_area.height {
                    let separator_area = Rect {
                        x: img_area.x,
                        y: sep_y,
                        width: img_area.width,
                        height: separator_height,
                    };
                    let separator_text = ".".repeat(img_area.width as usize);
                    let separator =
                        Paragraph::new(separator_text).style(Style::default().fg(muted_color));
                    frame.render_widget(separator, separator_area);
                }
            }
        }

        // Clamp horizontal pan
        if let Some(first) = visible_info.first() {
            if first.dest_w > img_area.width {
                let visible_source_w = (f32::from(img_area.width) / zoom_factor).ceil() as u16;
                let max_pan = first.cell_size.width.saturating_sub(visible_source_w);
                zoom.cell_pan_from_left = zoom.cell_pan_from_left.min(max_pan);
            } else {
                zoom.cell_pan_from_left = 0;
            }
        }

        // Build image display requests
        let mut images_to_display = Vec::with_capacity(visible_info.len());
        let info_map: HashMap<usize, &VisiblePageInfo> = visible_info
            .iter()
            .map(|info| (info.page_idx, info))
            .collect();

        for (idx, rendered_page) in rendered.iter_mut().enumerate() {
            let Some(info) = info_map.get(&idx) else {
                continue;
            };

            let Some(ConvertedImage::Kitty {
                ref mut img,
                cell_size: _,
            }) = rendered_page.img
            else {
                continue;
            };

            let (source_x_cells, visible_source_w, display_x_offset, display_cols) =
                if info.dest_w <= img_area.width {
                    let x_offset = (img_area.width - info.dest_w) / 2;
                    (0u16, info.cell_size.width, x_offset, info.dest_w)
                } else {
                    let pan = zoom.cell_pan_from_left;
                    let visible_source = (f32::from(img_area.width) / zoom_factor).ceil() as u16;
                    let remaining_width = info.cell_size.width.saturating_sub(pan);
                    let visible = remaining_width.min(visible_source);
                    (pan, visible, 0u16, img_area.width)
                };

            let source_total_h_px = u32::from(info.cell_size.height) * u32::from(font_size.1);
            let available_h_px = source_total_h_px.saturating_sub(info.img_clip_top_px);
            let requested_h_px = ((f32::from(info.display_rows) * f32::from(font_size.1))
                / zoom_factor)
                .round()
                .max(1.0) as u32;
            let visible_source_h_px = requested_h_px.min(available_h_px);

            images_to_display.push(ImageRequest {
                image: img,
                page: info.page_idx,
                position: Position {
                    x: img_area.x + display_x_offset,
                    y: img_area.y + info.screen_y_start,
                },
                location: DisplayLocation {
                    x: u32::from(source_x_cells) * u32::from(font_size.0),
                    y: info.img_clip_top_px,
                    width: u32::from(visible_source_w) * u32::from(font_size.0),
                    height: visible_source_h_px,
                    columns: display_cols,
                    rows: info.display_rows,
                },
            });
        }

        let visible_pages = visible_info
            .iter()
            .map(|info| VisiblePageUiInfo {
                page_idx: info.page_idx,
                screen_y_start: info.screen_y_start,
                display_rows: info.display_rows,
                dest_w: info.dest_w,
                dest_h: info.dest_h,
                offset_dest_cells: info.offset_dest_cells,
            })
            .collect();

        if images_to_display.is_empty() {
            Self::render_loading_in(frame, img_area, palette);
            (DisplayBatch::Clear, current_page, visible_pages)
        } else {
            (
                DisplayBatch::Display(images_to_display),
                current_page,
                visible_pages,
            )
        }
    }

    /// Main render function
    #[must_use]
    pub fn render<'s>(
        &'s mut self,
        frame: &mut Frame<'_>,
        full_layout: &RenderLayout,
        font_size: FontSize,
    ) -> DisplayBatch<'s> {
        let modal_bg = self.bg_color();
        let modal_fg = self.fg_color();
        let modal_panel_bg = self.palette.base_01;
        let modal_panel_header_bg = self.palette.base_02;
        let popup_border = self.palette.popup_border_color();
        let modal_msg = self
            .go_to_page_input
            .map(|page| self.go_to_page_prompt_text(page));
        let comment_modal = self.comments_enabled && self.comment_input.is_active();
        let input_active = modal_msg.is_some();
        let bg_color = modal_bg;
        let mut fg_color = modal_fg;
        let mut muted_color = self.muted_color();

        if comment_modal {
            fg_color = self.palette.base_04;
            muted_color = self.palette.base_03;
        }

        // Fill background; dim mode covers the whole frame so panels are muted too.
        let bg_block = Block::default().style(Style::default().bg(bg_color));
        let bg_area = if comment_modal {
            frame.area()
        } else {
            full_layout.page_area
        };
        frame.render_widget(bg_block, bg_area);

        let inner_area = full_layout.page_area;

        let mut img_area = inner_area;

        let size = frame.area();
        // Determine Kitty rendering mode
        let is_kitty_with_zoom = self.zoom.is_some() && self.is_kitty;
        let use_scroll_mode = is_kitty_with_zoom && get_pdf_render_mode() == PdfRenderMode::Scroll;
        let use_page_mode = is_kitty_with_zoom && get_pdf_render_mode() == PdfRenderMode::Page;

        // NoChange optimization for Kitty modes
        // Skip if current page has a Queued image that needs to be displayed (e.g., cursor/selection update)
        let current_page_needs_display = self
            .rendered
            .get(self.page)
            .and_then(|info| info.img.as_ref())
            .is_some_and(|img| {
                matches!(
                    img,
                    ConvertedImage::Kitty {
                        img: ImageState::Queued(_),
                        ..
                    }
                )
            });

        if size == self.last_render.rect
            && is_kitty_with_zoom
            && !input_active
            && !comment_modal
            && !current_page_needs_display
        {
            frame.render_widget(ImageRegion, img_area);
            return DisplayBatch::NoChange;
        }

        // Kitty page mode (single page with zoom/scroll within page)
        if use_page_mode {
            if input_active {
                if let Some(ref msg) = modal_msg {
                    Self::render_input_modal(
                        frame,
                        inner_area,
                        msg.clone(),
                        modal_bg,
                        popup_border,
                        fg_color,
                    );
                }
                return DisplayBatch::Clear;
            }

            let pdf_area = img_area;

            // Pre-fetch data before mutable borrow of self.rendered
            let zoom_factor = self.zoom.as_ref().map(|z| z.factor()).unwrap_or(1.0);
            let base_width = self
                .rendered
                .get(self.page)
                .and_then(|page| page.img.as_ref())
                .map(|img| img.cell_dimensions().as_tuple().0)
                .unwrap_or(0);
            let sidebar_comments = if self.comments_enabled && !comment_modal {
                self.get_comments_for_sidebar(self.page)
            } else {
                Vec::new()
            };
            let comment_nav_active = self.comment_nav_active;
            let comment_nav_index = self.comment_nav_index;

            let mut zoom = self.zoom.take().unwrap();

            let result = Self::render_single_page_kitty(
                &mut self.rendered,
                frame,
                pdf_area,
                font_size,
                &mut zoom,
                self.page,
                &self.palette,
            );

            self.zoom = Some(zoom);

            // Calculate content width for sidebar positioning
            let content_width = if base_width > 0 {
                ((f32::from(base_width) * zoom_factor).ceil() as u16)
                    .max(1)
                    .min(pdf_area.width)
            } else {
                pdf_area.width
            };

            self.last_render.rect = size;
            self.last_render.img_area_height = img_area.height;
            self.last_render.img_area_width = img_area.width;
            self.last_render.img_area = img_area;
            self.last_render.unused_width = pdf_area.width.saturating_sub(content_width);
            self.coord_info = Some((img_area, font_size));

            // Render comment sidebar in page mode
            if self.comments_enabled {
                if comment_modal {
                    let right_margin = self.last_render.unused_width / 2;
                    Self::render_comment_modal(
                        &mut self.comment_input,
                        frame,
                        inner_area,
                        modal_bg,
                        right_margin,
                        modal_fg,
                        popup_border,
                        modal_panel_bg,
                        modal_panel_header_bg,
                    );
                } else if !sidebar_comments.is_empty() {
                    // Use the full pdf_area as bounds for page mode
                    let bounds = pdf_area;
                    if let Some(sidebar_area) = Self::comment_sidebar_area_with_bounds(
                        inner_area,
                        content_width,
                        &sidebar_comments,
                        bounds,
                    ) {
                        Self::render_comment_list_sidebar(
                            &sidebar_comments,
                            frame,
                            sidebar_area,
                            bg_color,
                            self.palette.base_0e,
                            muted_color,
                            comment_nav_active,
                            comment_nav_index,
                        );
                    }
                }
            }

            return result;
        }

        // Kitty continuous scroll mode
        if use_scroll_mode {
            // Check if go-to-page input modal is active (this needs to clear graphics)
            if input_active {
                if let Some(ref msg) = modal_msg {
                    Self::render_input_modal(
                        frame,
                        inner_area,
                        msg.clone(),
                        modal_bg,
                        popup_border,
                        fg_color,
                    );
                }
                return DisplayBatch::Clear;
            }

            // PDF renders at its natural position - no shrinking
            let pdf_area = img_area;

            let zoom_factor = self.zoom.as_ref().map(|z| z.factor()).unwrap_or(1.0);
            log::trace!("render: zoom_factor={zoom_factor}");
            let page_widths = self
                .rendered
                .iter()
                .map(|page| {
                    page.img
                        .as_ref()
                        .map(|img| img.cell_dimensions().as_tuple().0)
                        .unwrap_or(0)
                })
                .collect::<Vec<_>>();

            let mut zoom = self.zoom.take().unwrap();
            if self.page > 0 && zoom.global_scroll_offset == 0 {
                // Only calculate scroll offset if we have at least one rendered image
                // with valid height. Otherwise, wait for images to load before scrolling
                // to avoid using viewport height as fallback (which would be wrong).
                if let Some(reference_height) = self.rendered.iter().find_map(|page| {
                    page.img
                        .as_ref()
                        .map(|img| img.cell_dimensions().as_tuple().1)
                }) {
                    let dest_h = ((f32::from(reference_height) * zoom_factor).ceil() as u32).max(1);
                    let per_page = dest_h + u32::from(SEPARATOR_HEIGHT);
                    zoom.global_scroll_offset = per_page.saturating_mul(self.page as u32);
                }
            }

            let sidebar_page_hint = self.page;
            let mut sidebar_page_idx = None;
            let mut sidebar_comments = Vec::new();

            let sidebar_comments_by_page = if self.comments_enabled && !comment_modal {
                self.get_comments_by_page()
            } else {
                HashMap::new()
            };

            let (result, current_page, visible_pages) = Self::render_continuous_scroll(
                &mut self.rendered,
                frame,
                pdf_area,
                font_size,
                &mut zoom,
                self.page,
                SEPARATOR_HEIGHT,
                muted_color,
                &self.palette,
            );
            self.zoom = Some(zoom);

            if let Some(page) = current_page {
                self.page = page;
            }
            if self.comment_nav_active && self.comment_nav_page != self.page {
                self.comment_nav_page = self.page;
                self.comment_nav_index = 0;
            }

            let comment_nav_active = self.comment_nav_active;
            let comment_nav_index = self.comment_nav_index;
            if self.comments_enabled && !comment_modal {
                // Prefer showing comments for the current page if it's visible and has comments
                let hint_comments = sidebar_comments_by_page
                    .get(&sidebar_page_hint)
                    .cloned()
                    .unwrap_or_default();
                if !hint_comments.is_empty()
                    && visible_pages
                        .iter()
                        .any(|info| info.page_idx == sidebar_page_hint)
                {
                    sidebar_page_idx = Some(sidebar_page_hint);
                    sidebar_comments = hint_comments;
                } else {
                    // Fall back to any visible page that has comments
                    for info in &visible_pages {
                        let comments = sidebar_comments_by_page
                            .get(&info.page_idx)
                            .cloned()
                            .unwrap_or_default();
                        if !comments.is_empty() {
                            sidebar_page_idx = Some(info.page_idx);
                            sidebar_comments = comments;
                            break;
                        }
                    }
                }
            }

            let page_idx = current_page.unwrap_or(self.page);
            let base_width = page_widths.get(page_idx).copied().unwrap_or(pdf_area.width);
            let content_width = ((f32::from(base_width) * zoom_factor).ceil() as u16)
                .max(1)
                .min(pdf_area.width);

            self.coord_info = Some((pdf_area, font_size));

            if !visible_pages.is_empty() {
                self.last_render = LastRender {
                    rect: size,
                    pages_shown: 1,
                    unused_width: pdf_area.width.saturating_sub(content_width),
                    img_area_height: pdf_area.height,
                    img_area_width: pdf_area.width,
                    img_area: pdf_area,
                };
            }

            // Render comment sidebar in natural margin
            if self.comments_enabled {
                if comment_modal {
                    // Comment input modal uses right margin
                    let right_margin = self.last_render.unused_width / 2;
                    Self::render_comment_modal(
                        &mut self.comment_input,
                        frame,
                        inner_area,
                        modal_bg,
                        right_margin,
                        modal_fg,
                        popup_border,
                        modal_panel_bg,
                        modal_panel_header_bg,
                    );
                } else if let Some(page_idx) = sidebar_page_idx
                    && let Some(info) = visible_pages.iter().find(|info| info.page_idx == page_idx)
                {
                    let bounds = Rect {
                        x: pdf_area.x,
                        y: pdf_area.y + info.screen_y_start,
                        width: pdf_area.width,
                        height: info.display_rows,
                    };
                    if let Some(sidebar_area) = Self::comment_sidebar_area_with_bounds(
                        inner_area,
                        content_width,
                        &sidebar_comments,
                        bounds,
                    ) {
                        // Sidebar only shows if there's enough natural margin
                        Self::render_comment_list_sidebar(
                            &sidebar_comments,
                            frame,
                            sidebar_area,
                            bg_color,
                            self.palette.base_0e,
                            muted_color,
                            comment_nav_active,
                            comment_nav_index,
                        );
                    }
                }
            }

            return result;
        }

        // Non-Kitty mode (tiled/generic rendering)
        // Pre-fetch comments for sidebar before mutable borrow
        let sidebar_comments = if self.comments_enabled && !comment_modal {
            self.get_comments_for_sidebar(self.page)
        } else {
            Vec::new()
        };

        // Single page display (no side-by-side)
        let mut page_sizes = Vec::new();
        if let Some(page) = self.rendered.get(self.page) {
            if let Some(img) = page.img.as_ref() {
                let (w, h) = img.cell_dimensions().as_tuple();
                page_sizes.push((self.page, w, h));
            }
        }

        if page_sizes.is_empty() {
            self.coord_info = Some((img_area, font_size));
            // Don't update last_render.rect here - keep it invalid so the next frame
            // with actual images will render. Otherwise, the cache check passes and
            // images never display after TOC navigation.
            self.last_render.img_area_height = img_area.height;
            self.last_render.img_area_width = img_area.width;
            self.last_render.img_area = img_area;
            self.last_render.pages_shown = 1;
            self.last_render.unused_width = 0;

            if self.is_iterm && self.rendered.iter().any(|page| page.img.is_some()) {
                log::debug!("No pages ready to render - keeping previous frame (iTerm2)");
                return DisplayBatch::NoChange;
            }

            log::debug!("No pages ready to render - showing loading");
            Self::render_loading_in(frame, img_area, &self.palette);

            if let Some(ref msg) = modal_msg {
                Self::render_input_modal(
                    frame,
                    inner_area,
                    msg.clone(),
                    modal_bg,
                    popup_border,
                    fg_color,
                );
            }
            if comment_modal {
                let right_margin = self.last_render.unused_width / 2;
                Self::render_comment_modal(
                    &mut self.comment_input,
                    frame,
                    inner_area,
                    modal_bg,
                    right_margin,
                    modal_fg,
                    popup_border,
                    modal_panel_bg,
                    modal_panel_header_bg,
                );
            }
            DisplayBatch::Clear
        } else {
            let _ = execute!(stdout(), BeginSynchronizedUpdate);

            let total_width = page_sizes.iter().map(|(_, w, _)| w).sum::<u16>();
            self.last_render.pages_shown = page_sizes.len();

            let unused_width = img_area.width.saturating_sub(total_width);
            self.last_render.unused_width = unused_width;
            img_area.x += unused_width / 2;

            if let Some(total_height) = page_sizes.iter().map(|(_, _, h)| h).max() {
                if let Some(unused_height) = img_area.height.checked_sub(*total_height) {
                    img_area.y += unused_height / 2;
                }
            }

            let centered_img_area = img_area;

            let page_sizes = self.rendered[self.page..]
                .iter_mut()
                .enumerate()
                .take(page_sizes.len())
                .filter_map(|(idx, page)| {
                    page.img.as_mut().map(|img| {
                        let (w, h) = img.cell_dimensions().as_tuple();
                        (self.page + idx, w, h, img)
                    })
                })
                .collect::<Vec<_>>();

            let mut to_display = Vec::new();
            for (page_num, width, _height, img) in page_sizes.into_iter() {
                let render_width = width.min(img_area.width);
                let maybe_img = Self::render_single_page(
                    frame,
                    img,
                    Rect {
                        width: render_width,
                        ..img_area
                    },
                    None,
                    bg_color,
                );

                // Cursor is baked into the image by the converter for all protocols

                img_area.x += width;
                if let Some((img, pos)) = maybe_img {
                    to_display.push(ImageRequest {
                        image: img,
                        page: page_num,
                        position: pos,
                        location: DisplayLocation::default(),
                    });
                }
            }

            self.last_render.rect = size;
            self.last_render.img_area_height = centered_img_area.height;
            self.last_render.img_area_width = centered_img_area.width;
            self.last_render.img_area = centered_img_area;
            self.coord_info = Some((centered_img_area, font_size));

            // Render comment sidebar in natural margin (non-Kitty mode)
            if self.comments_enabled && !comment_modal {
                if let Some(sidebar_area) =
                    Self::comment_sidebar_area(inner_area, total_width, &sidebar_comments)
                {
                    Self::render_comment_list_sidebar(
                        &sidebar_comments,
                        frame,
                        sidebar_area,
                        bg_color,
                        self.palette.base_0e,
                        muted_color,
                        self.comment_nav_active,
                        self.comment_nav_index,
                    );
                }
            }

            if let Some(ref msg) = modal_msg {
                Self::render_input_modal(
                    frame,
                    inner_area,
                    msg.clone(),
                    modal_bg,
                    popup_border,
                    fg_color,
                );
            }
            if comment_modal {
                let right_margin = unused_width / 2;
                Self::render_comment_modal(
                    &mut self.comment_input,
                    frame,
                    inner_area,
                    modal_bg,
                    right_margin,
                    modal_fg,
                    popup_border,
                    modal_panel_bg,
                    modal_panel_header_bg,
                );
            }
            // Clear Kitty graphics when modals are active so they're visible
            if (input_active || comment_modal) && self.is_kitty {
                return DisplayBatch::Clear;
            }

            DisplayBatch::Display(to_display)
        }
    }

    fn render_single_page<'img>(
        frame: &mut Frame<'_>,
        page_img: &'img mut ConvertedImage,
        img_area: Rect,
        scroll_plan: Option<PendingScroll>,
        bg_color: Color,
    ) -> Option<(&'img mut ImageState, Position)> {
        match page_img {
            ConvertedImage::Generic(page_img) => {
                frame.render_widget(Image::new(page_img), img_area);
                None
            }
            ConvertedImage::Tiled { tiles, .. } => {
                if let Some(plan) = scroll_plan {
                    let abs_delta = plan.delta_cells.unsigned_abs();
                    if abs_delta < img_area.height {
                        let overlap_height = img_area.height - abs_delta;
                        if plan.delta_cells > 0 {
                            let overlap_area = Rect {
                                x: img_area.x,
                                y: img_area.y,
                                width: img_area.width,
                                height: overlap_height,
                            };
                            let new_area = Rect {
                                x: img_area.x,
                                y: img_area.y.saturating_add(overlap_height),
                                width: img_area.width,
                                height: abs_delta,
                            };
                            frame.render_widget(
                                Block::default().style(Style::default().bg(bg_color)),
                                new_area,
                            );
                            frame.render_widget(ImageRegion, overlap_area);
                            let new_start = overlap_height;
                            for tile in tiles {
                                let tile_start = tile.y_offset_cells;
                                let tile_end = tile_start.saturating_add(tile.height_cells);
                                if tile_start < img_area.height && tile_end > new_start {
                                    let tile_area = Rect {
                                        x: img_area.x,
                                        y: img_area.y.saturating_add(tile.y_offset_cells),
                                        width: img_area.width,
                                        height: tile.height_cells,
                                    };
                                    frame.render_widget(Image::new(&tile.protocol), tile_area);
                                }
                            }
                            return None;
                        } else if plan.delta_cells < 0 {
                            let overlap_area = Rect {
                                x: img_area.x,
                                y: img_area.y.saturating_add(abs_delta),
                                width: img_area.width,
                                height: overlap_height,
                            };
                            let new_area = Rect {
                                x: img_area.x,
                                y: img_area.y,
                                width: img_area.width,
                                height: abs_delta,
                            };
                            frame.render_widget(
                                Block::default().style(Style::default().bg(bg_color)),
                                new_area,
                            );
                            frame.render_widget(ImageRegion, overlap_area);
                            let new_end = abs_delta;
                            for tile in tiles {
                                let tile_start = tile.y_offset_cells;
                                if tile_start < new_end {
                                    let tile_area = Rect {
                                        x: img_area.x,
                                        y: img_area.y.saturating_add(tile.y_offset_cells),
                                        width: img_area.width,
                                        height: tile.height_cells,
                                    };
                                    frame.render_widget(Image::new(&tile.protocol), tile_area);
                                }
                            }
                            return None;
                        }
                    }
                }
                for tile in tiles {
                    let tile_area = Rect {
                        x: img_area.x,
                        y: img_area.y.saturating_add(tile.y_offset_cells),
                        width: img_area.width,
                        height: tile.height_cells,
                    };
                    frame.render_widget(Image::new(&tile.protocol), tile_area);
                }
                None
            }
            ConvertedImage::Kitty { img, cell_size: _ } => Some((
                img,
                Position {
                    x: img_area.x,
                    y: img_area.y,
                },
            )),
            ConvertedImage::TileUpdate { .. } => {
                log::warn!("TileUpdate reached render_single_page - should have been merged");
                None
            }
        }
    }

    fn render_loading_in(frame: &mut Frame<'_>, area: Rect, palette: &Base16Palette) {
        const LOADING_STR: &str = "[ LOADING ]";
        let inner_space = Layout::horizontal([Constraint::Length(LOADING_STR.len() as u16)])
            .flex(Flex::Center)
            .split(area);

        let loading_span = Span::styled(
            LOADING_STR,
            Style::new()
                .fg(palette.base_06)
                .bg(palette.base_01)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(loading_span, inner_space[0]);
    }

    fn render_input_modal(
        frame: &mut Frame<'_>,
        area: Rect,
        msg: Text<'static>,
        bg_color: Color,
        accent_color: Color,
        fg_color: Color,
    ) {
        let content_lines = msg.lines.len() as u16;
        let modal_width = 40u16.min(area.width.saturating_sub(4));
        let modal_height = (content_lines + 2)
            .clamp(6, 10)
            .min(area.height.saturating_sub(2));
        let modal_area = Rect {
            x: area.x + (area.width.saturating_sub(modal_width)) / 2,
            y: area.y + (area.height.saturating_sub(modal_height)) / 2,
            width: modal_width,
            height: modal_height,
        };

        frame.render_widget(Clear, modal_area);
        let modal = Paragraph::new(msg)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(accent_color))
                    .title(" Go to Page ")
                    .title_style(Style::default().fg(accent_color))
                    .style(Style::default().bg(bg_color)),
            )
            .style(Style::default().fg(fg_color).bg(bg_color))
            .wrap(Wrap { trim: false });
        frame.render_widget(modal, modal_area);
    }

    /// Renders the comment textarea modal. Returns `true` if rendered, `false` if not enough space.
    /// When returning `false`, the textarea is closed and the caller should show an error.
    #[allow(clippy::too_many_arguments)]
    fn render_comment_modal(
        comment_input: &mut CommentInputState,
        frame: &mut Frame<'_>,
        area: Rect,
        bg_color: Color,
        right_margin: u16,
        fg_color: Color,
        accent_color: Color,
        panel_bg: Color,
        header_bg: Color,
    ) -> bool {
        let Some(textarea) = comment_input.textarea.as_mut() else {
            return true;
        };

        // Check if there's enough space for the comment textarea
        if right_margin < MIN_COMMENT_TEXTAREA_WIDTH {
            comment_input.clear();
            return false;
        }

        let width = right_margin
            .clamp(MIN_COMMENT_TEXTAREA_WIDTH, 88)
            .min(area.width);
        let max_height = (area.height * 3 / 5).max(6);
        let height = area
            .height
            .saturating_sub(4)
            .min(max_height)
            .max(6)
            .min(area.height);
        let x = area.x + area.width.saturating_sub(width);
        let y = area.y + 2;

        let modal_area = Rect {
            x,
            y,
            width,
            height,
        };
        frame.render_widget(Clear, modal_area);

        let title = match comment_input.edit_mode {
            Some(CommentEditMode::Editing { .. }) => "Edit Comment (Esc to save)",
            _ => "Add Comment (Esc to save)",
        };
        let block = Block::default()
            .title(Span::styled(
                title,
                Style::default().fg(accent_color).bg(header_bg),
            ))
            .borders(Borders::ALL)
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(accent_color))
            .style(Style::default().bg(panel_bg));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        textarea.set_style(Style::default().fg(fg_color).bg(panel_bg));
        textarea.set_cursor_style(Style::default().fg(bg_color).bg(fg_color));
        textarea.set_block(Block::default());

        frame.render_widget(&*textarea, inner);
        true
    }

    fn go_to_page_prompt_text(&self, page: usize) -> Text<'static> {
        let total_pages = self.rendered.len();
        let muted = self.palette.base_04;
        let hint_key = self.palette.base_0d;
        let hint_desc = self.palette.base_03;
        let accent = self.palette.base_0c;
        let fg = self.palette.base_05;

        let mut lines = Vec::new();

        // Top margin
        lines.push(Line::from(""));

        // Page input line with cursor
        let page_str = if page == 0 {
            String::new()
        } else {
            page.to_string()
        };
        let range_hint = match self.effective_go_to_page_mode() {
            super::types::PageJumpMode::Content => self
                .page_numbers
                .content_page_range(total_pages)
                .map(|(start, end)| format!("({start}-{end})"))
                .unwrap_or_else(|| format!("(1-{total_pages})")),
            super::types::PageJumpMode::Pdf => format!("(1-{total_pages})"),
        };
        lines.push(Line::from(vec![
            Span::styled("  Page: ", Style::default().fg(fg)),
            Span::styled(
                page_str,
                Style::default().fg(fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled("▌", Style::default().fg(accent)),
            Span::styled(format!("  {range_hint}"), Style::default().fg(muted)),
        ]));

        lines.push(Line::from(""));

        // Mode toggle (only if content page mode is available)
        if self.content_page_mode_available() {
            let mode_line = match self.effective_go_to_page_mode() {
                super::types::PageJumpMode::Content => Line::from(vec![
                    Span::styled("  [", Style::default().fg(accent)),
                    Span::styled(
                        "Content",
                        Style::default().fg(fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("]", Style::default().fg(accent)),
                    Span::styled("  PDF", Style::default().fg(muted)),
                ]),
                super::types::PageJumpMode::Pdf => Line::from(vec![
                    Span::styled("  Content  ", Style::default().fg(muted)),
                    Span::styled("[", Style::default().fg(accent)),
                    Span::styled("PDF", Style::default().fg(fg).add_modifier(Modifier::BOLD)),
                    Span::styled("]", Style::default().fg(accent)),
                ]),
            };
            lines.push(mode_line);
            lines.push(Line::from(""));
        }

        // Error message if present
        if let Some(err) = self.go_to_page_error.as_ref() {
            lines.push(Line::from(Span::styled(
                format!("  {err}"),
                Style::default().fg(Color::Red),
            )));
            lines.push(Line::from(""));
        }

        // Keyboard hints (different color scheme)
        let hints = if self.content_page_mode_available() {
            vec![
                Span::styled("  Enter", Style::default().fg(hint_key)),
                Span::styled(": go  ", Style::default().fg(hint_desc)),
                Span::styled("Tab", Style::default().fg(hint_key)),
                Span::styled(": mode  ", Style::default().fg(hint_desc)),
                Span::styled("Esc", Style::default().fg(hint_key)),
                Span::styled(": cancel", Style::default().fg(hint_desc)),
            ]
        } else {
            vec![
                Span::styled("  Enter", Style::default().fg(hint_key)),
                Span::styled(": go  ", Style::default().fg(hint_desc)),
                Span::styled("Esc", Style::default().fg(hint_key)),
                Span::styled(": cancel", Style::default().fg(hint_desc)),
            ]
        };
        lines.push(Line::from(hints));

        // Bottom margin
        lines.push(Line::from(""));

        Text::from(lines)
    }

    pub fn content_page_mode_available(&self) -> bool {
        self.page_numbers.has_offset()
    }

    fn effective_go_to_page_mode(&self) -> super::types::PageJumpMode {
        if self.content_page_mode_available() {
            self.go_to_page_mode
        } else {
            super::types::PageJumpMode::Pdf
        }
    }

    /// Check if there are comments for the current page
    /// Calculate sidebar area based on natural margin
    /// Returns None if no comments or not enough margin space
    fn comment_sidebar_area(
        area: Rect,
        content_width: u16,
        comments: &[crate::comments::Comment],
    ) -> Option<Rect> {
        if comments.is_empty() || content_width >= area.width {
            return None;
        }

        let height = area.height.saturating_sub(1);
        if height == 0 {
            return None;
        }

        let min_sidebar = 24u16;
        let max_sidebar = 40u16;
        let gap_total = area.width.saturating_sub(content_width);
        let right_gap = gap_total.saturating_sub(gap_total / 2); // Right half (PDF is centered)

        if right_gap < min_sidebar {
            return None;
        }

        let sidebar_width = right_gap.min(max_sidebar);
        Some(Rect {
            x: area.x + area.width.saturating_sub(sidebar_width),
            y: area.y,
            width: sidebar_width,
            height,
        })
    }

    fn comment_sidebar_area_with_bounds(
        area: Rect,
        content_width: u16,
        comments: &[crate::comments::Comment],
        bounds: Rect,
    ) -> Option<Rect> {
        if comments.is_empty() || content_width >= area.width {
            return None;
        }

        let height = bounds.height.saturating_sub(1);
        if height == 0 {
            return None;
        }

        let min_sidebar = 24u16;
        let max_sidebar = 40u16;
        let gap_total = area.width.saturating_sub(content_width);
        let right_gap = gap_total.saturating_sub(gap_total / 2);

        if right_gap < min_sidebar {
            return None;
        }

        let sidebar_width = right_gap.min(max_sidebar);
        let y = bounds.y;

        Some(Rect {
            x: area.x + area.width.saturating_sub(sidebar_width),
            y,
            width: sidebar_width,
            height,
        })
    }

    /// Render comment list sidebar in natural margin
    #[allow(clippy::too_many_arguments)]
    fn render_comment_list_sidebar(
        comments: &[crate::comments::Comment],
        frame: &mut Frame<'_>,
        area: Rect,
        bg_color: Color,
        comment_color: Color,
        muted_color: Color,
        comment_nav_active: bool,
        comment_nav_index: usize,
    ) {
        let bg_block = Block::default().style(Style::default().bg(bg_color));
        frame.render_widget(bg_block, area);
        if comments.is_empty() {
            return;
        }

        const TOP_MARGIN: u16 = 3;
        const RIGHT_MARGIN: u16 = 3;

        let selected_style = Style::default()
            .fg(bg_color)
            .bg(comment_color)
            .add_modifier(Modifier::BOLD);
        let inner = Rect {
            x: area.x,
            y: area.y.saturating_add(TOP_MARGIN),
            width: area.width.saturating_sub(RIGHT_MARGIN),
            height: area.height.saturating_sub(TOP_MARGIN),
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let mut lines = Vec::new();
        let header_width = inner.width.max(1) as usize;

        for (idx, comment) in comments.iter().enumerate() {
            // First line: "1) content"
            let first_line = comment.content.lines().next().unwrap_or("");
            let numbered = format!("{}) {first_line}", idx + 1);
            if comment_nav_active && comment_nav_index == idx {
                let padded = format!("{numbered:<header_width$}");
                lines.push(Line::from(Span::styled(padded, selected_style)));
            } else {
                lines.push(Line::from(Span::styled(
                    numbered,
                    Style::default().fg(comment_color),
                )));
            }

            // Continuation lines (if multi-line comment)
            for line in comment.content.lines().skip(1) {
                lines.push(Line::from(Span::styled(
                    format!("   {line}"),
                    Style::default().fg(comment_color),
                )));
            }

            // Date line: "// 01-23-26 09:39"
            let date_str = format!("// {}", comment.updated_at.format("%m-%d-%y %H:%M"));
            lines.push(Line::from(Span::styled(
                date_str,
                Style::default().fg(muted_color),
            )));

            if idx + 1 < comments.len() {
                lines.push(Line::from(""));
            }
        }

        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(comment_color))
            .wrap(Wrap { trim: false })
            .scroll((0, 0));
        frame.render_widget(paragraph, inner);
    }

    /// Get comments for sidebar display (current page only)
    fn get_comments_for_sidebar(&self, page: usize) -> Vec<crate::comments::Comment> {
        let Some(comments) = self.book_comments.as_ref() else {
            return Vec::new();
        };
        let Ok(locked) = comments.lock() else {
            return Vec::new();
        };
        locked
            .get_page_comments(&self.comments_doc_id, page)
            .into_iter()
            .cloned()
            .collect()
    }

    fn get_comments_by_page(&self) -> HashMap<usize, Vec<crate::comments::Comment>> {
        let Some(comments) = self.book_comments.as_ref() else {
            return HashMap::new();
        };
        let Ok(locked) = comments.lock() else {
            return HashMap::new();
        };

        let mut by_page: HashMap<usize, Vec<crate::comments::Comment>> = HashMap::new();
        for comment in locked.get_doc_comments(&self.comments_doc_id) {
            let crate::comments::CommentTarget::Pdf { page, .. } = &comment.target else {
                continue;
            };
            by_page.entry(*page).or_default().push(comment.clone());
        }
        by_page
    }
}
