//! PDF reader input handling
//!
//! This module handles keyboard and mouse input for the PDF reader,
//! including zoom, pan, page navigation, selection, and normal mode.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{layout::Rect, style::Style};
use tui_textarea::{CursorMove, Key as TextAreaKey, TextArea};

use crate::bookmarks::Bookmarks;
use crate::inputs::text_area_utils::map_keys_to_input;
use crate::jump_list::JumpLocation;
use crate::navigation_panel::{CurrentBookInfo, NavigationPanel, TableOfContents};
use crate::pdf::CellSize;
use crate::pdf::{
    CursorPosition, MoveResult, PendingMotion, ScrollDirection, ViewportUpdate, VisualMode, Zoom,
    visual_rects_for_range,
};
use crate::table_of_contents::TocItem;
use crate::vendored::ratatui_image::FontSize;

const MIN_COMMENT_TEXTAREA_WIDTH: u16 = 20;
use super::state::{CommentEditMode, InputAction, PdfReaderState, SEPARATOR_HEIGHT};
use super::types::{PageJumpMode, PendingScroll};
use crate::comments::{Comment, CommentTarget, PdfSelectionRect};
use crate::settings::{PdfRenderMode, get_pdf_render_mode};

pub struct InputResponse {
    pub action: Option<InputAction>,
    pub handled: bool,
}

impl InputResponse {
    fn handled(action: Option<InputAction>) -> Self {
        Self {
            action,
            handled: true,
        }
    }

    fn unhandled() -> Self {
        Self {
            action: None,
            handled: false,
        }
    }
}

impl PdfReaderState {
    pub fn is_text_input_active(&self) -> bool {
        self.comment_input.is_active()
            || self.go_to_page_input.is_some()
            || self.page_search.is_input_active()
    }

    pub fn save_bookmark_with_throttle(
        &self,
        bookmarks: &mut Bookmarks,
        last_bookmark_save: &mut std::time::Instant,
        force: bool,
    ) {
        save_pdf_bookmark(bookmarks, self, last_bookmark_save, force);
    }

    pub fn switch_to_toc_mode(&self, navigation_panel: &mut NavigationPanel) {
        let toc_items = convert_pdf_toc_to_toc_items(&self.toc_entries);
        let current_page = self.page;
        let current_href = format!("pdf:page:{current_page}");

        let book_info = CurrentBookInfo {
            path: self.name.clone(),
            toc_items,
            current_chapter: current_page,
            current_chapter_href: Some(current_href.clone()),
            active_section: crate::markdown_text_reader::ActiveSection::new(
                current_page,
                current_href,
                None,
            ),
        };

        navigation_panel.switch_to_toc_mode(book_info);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn handle_zen_mode_toggle(
        &mut self,
        zen_mode: bool,
        terminal_width: u16,
        nav_width: u16,
        comments_dir: Option<&std::path::Path>,
        test_mode: bool,
        conversion_tx: Option<&flume::Sender<crate::pdf::ConversionCommand>>,
        service: Option<&mut crate::pdf::RenderService>,
    ) {
        // Exit normal/visual mode when toggling zen mode
        if self.normal_mode.active {
            self.normal_mode.exit_visual();
            self.normal_mode.deactivate();
        }

        // Kitty-specific: clear images via graphics protocol
        if self.is_kitty {
            let _ = crate::pdf::kittyv2::execute_display_batch(
                crate::pdf::kittyv2::DisplayBatch::Clear,
            );
        }

        // Collect comment rects but don't send yet - must send AFTER InvalidatePageCache
        // to avoid reconvert_pages rendering with stale viewport dimensions.
        // Comments are supported in terminals with image protocols (Kitty, iTerm2).
        // Underlines should be visible in both zen and ToC modes; only UI interactions are zen-only.
        let comment_rects_to_send = if self.supports_comments && zen_mode {
            if self.book_comments.is_none() {
                // In test mode, use empty comments to avoid loading persistent state
                let comments = if test_mode {
                    crate::comments::BookComments::new_empty()
                } else {
                    match crate::comments::BookComments::new(
                        std::path::Path::new(&self.name),
                        comments_dir,
                    ) {
                        Ok(c) => c,
                        Err(e) => {
                            log::info!("Failed to initialize PDF comments on zen toggle: {e}");
                            crate::comments::BookComments::new_empty()
                        }
                    }
                };
                self.book_comments = Some(std::sync::Arc::new(std::sync::Mutex::new(comments)));
                self.comments_enabled = true;
                log::info!("PDF comments enabled for zen mode");
            } else {
                self.comments_enabled = true;
            }
            Some(self.initial_comment_rects())
        } else if self.supports_comments {
            // Exiting zen mode: disable UI interactions but keep underlines visible
            self.comments_enabled = false;
            Some(self.initial_comment_rects())
        } else {
            None
        };

        let page = self.page;
        let current_factor = self.zoom.as_ref().map(|z| z.factor()).unwrap_or(1.0);
        let terminal_width = terminal_width as f32;
        let nav_width = nav_width as f32;

        if self.is_kitty {
            // Kitty: adjust zoom to keep visual size constant when viewport width changes
            let width_ratio = if zen_mode {
                (terminal_width - nav_width) / terminal_width
            } else {
                terminal_width / (terminal_width - nav_width)
            };

            let adjusted_factor = crate::pdf::Zoom::clamp_factor(current_factor * width_ratio);

            log::info!(
                "toggle_zen_mode (kitty): zen={zen_mode}, current_factor={current_factor}, width_ratio={width_ratio}, adjusted_factor={adjusted_factor}"
            );

            self.zoom = Some(crate::pdf::Zoom {
                factor: adjusted_factor,
                cell_pan_from_left: 0,
                global_scroll_offset: 0,
            });
            self.last_render.rect = ratatui::layout::Rect::default();
            crate::settings::set_pdf_scale(adjusted_factor);
            crate::settings::set_pdf_pan_shift(0);

            for rendered_info in &mut self.rendered {
                rendered_info.img = None;
            }

            if let Some(tx) = conversion_tx {
                let _ = tx.send(crate::pdf::ConversionCommand::InvalidatePageCache);
                let _ = tx.send(crate::pdf::ConversionCommand::NavigateTo(page));
                if let Some(rects) = comment_rects_to_send {
                    let _ = tx.send(crate::pdf::ConversionCommand::UpdateComments(rects));
                }
            }
        } else {
            // iTerm2/WezTerm: Don't adjust scale during zen toggle.
            // The viewport change will be handled by SetArea during the next render,
            // and the PDF will naturally fit to the new viewport size.
            // Adjusting scale here causes a race condition where the service renders
            // with the new scale but old area, producing an incorrectly sized image.

            log::info!(
                "toggle_zen_mode (iterm2): zen={}, page={}, factor={}",
                zen_mode,
                page,
                self.non_kitty_zoom_factor,
            );

            let heights = self.page_heights_scaled(self.non_kitty_zoom_factor);
            if let Some(z) = &mut self.zoom {
                z.scroll_to_page(page, &heights, super::state::SEPARATOR_HEIGHT);
            }
            self.last_render.rect = ratatui::layout::Rect::default();

            // Clear rendered images so stale images aren't displayed
            for rendered_info in &mut self.rendered {
                rendered_info.img = None;
            }

            if let Some(tx) = conversion_tx {
                let _ = tx.send(crate::pdf::ConversionCommand::InvalidatePageCache);
                let _ = tx.send(crate::pdf::ConversionCommand::NavigateTo(page));
                if let Some(rects) = comment_rects_to_send {
                    let _ = tx.send(crate::pdf::ConversionCommand::UpdateComments(rects));
                }
            }
            // Don't send SetScale - let the natural render flow with SetArea handle the viewport change
        }

        let _ = service;
    }

    pub fn handle_event(&mut self, ev: &Event) -> InputResponse {
        log::info!("PDF handle_event: ev={ev:?}");
        match ev {
            Event::Key(key) => self.handle_key_event(*key),
            Event::Mouse(mouse) => InputResponse::handled(self.handle_mouse_event(*mouse)),
            Event::Resize(_, _) => InputResponse::handled(Some(InputAction::Redraw)),
            Event::Paste(text) => InputResponse::handled(self.handle_paste(text)),
            _ => InputResponse::unhandled(),
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> InputResponse {
        log::info!(
            "handle_key_event: key={:?}, comment_input={}, comment_nav={}, normal_mode={}",
            key.code,
            self.comment_input.is_active(),
            self.comment_nav_active,
            self.normal_mode.active
        );

        let hud_dismissed = self.dismiss_error_hud();

        let mut response = if self.page_search.is_input_active() {
            InputResponse::handled(self.handle_page_search_input_key(key))
        } else if self.comment_input.is_active() {
            InputResponse::handled(self.handle_comment_input_key(key))
        } else if self.comment_nav_active {
            InputResponse::handled(self.handle_comment_nav_key(key))
        } else if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('o') | KeyCode::Char('i'))
        {
            let action = self.handle_jump_list_key(key);
            InputResponse::handled(action)
        } else if let Some(action) = self.handle_jump_list_key(key) {
            InputResponse::handled(Some(action))
        } else if self.go_to_page_input.is_some() {
            InputResponse::handled(self.handle_go_to_page_key(key))
        } else if self.normal_mode.active {
            self.handle_normal_mode_event(key)
        } else {
            let action = self.handle_standard_key_event(key);
            if action.is_some() {
                InputResponse::handled(action)
            } else {
                InputResponse::unhandled()
            }
        };

        if hud_dismissed && response.action.is_none() {
            response.action = Some(InputAction::Redraw);
            response.handled = true;
        }

        response
    }

    fn handle_comment_nav_key(&mut self, key: KeyEvent) -> Option<InputAction> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_comment_nav(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_comment_nav(-1),
            KeyCode::Char('a' | 'e') => self.start_comment_edit(),
            KeyCode::Char('d') => self.delete_current_comment(),
            KeyCode::Esc | KeyCode::BackTab => self.stop_comment_nav(),
            _ => Some(InputAction::Redraw),
        }
    }

    fn handle_jump_list_key(&mut self, key: KeyEvent) -> Option<InputAction> {
        if !key.modifiers.contains(KeyModifiers::CONTROL) {
            return None;
        }

        let current_scroll = self
            .zoom
            .as_ref()
            .map(|z| z.global_scroll_offset)
            .unwrap_or(self.non_kitty_scroll_offset);
        let current_location = Some(JumpLocation::pdf(
            self.name.clone(),
            self.page,
            current_scroll,
        ));

        let location = match key.code {
            KeyCode::Char('o') => self.jump_list.jump_back(current_location),
            KeyCode::Char('i') => self.jump_list.jump_forward(),
            _ => None,
        }?;

        if let JumpLocation::Pdf {
            page,
            scroll_offset,
            ..
        } = location
        {
            if page != self.page {
                self.set_page(page);
            } else {
                self.last_render.rect = Rect::default();
                self.clear_pending_scroll();
            }
            if let Some(ref mut zoom) = self.zoom {
                zoom.global_scroll_offset = scroll_offset;
            } else {
                self.non_kitty_scroll_offset = scroll_offset;
            }
            if self.normal_mode.active {
                self.normal_mode.deactivate();
            }
            Some(InputAction::JumpingToPage {
                page,
                viewport: self.current_viewport_update(),
            })
        } else {
            None
        }
    }

    fn handle_go_to_page_key(&mut self, key: KeyEvent) -> Option<InputAction> {
        let page = self.go_to_page_input.as_mut()?;

        match key.code {
            KeyCode::Char('g') if self.is_kitty => {
                self.scroll_to_document_top();
                self.clear_go_to_page_input();
                return Some(InputAction::Redraw);
            }
            KeyCode::Char('m') => {
                self.toggle_go_to_page_mode();
                return Some(InputAction::Redraw);
            }
            KeyCode::Char('c') => {
                if self.content_page_mode_available() {
                    self.set_go_to_page_mode(PageJumpMode::Content);
                    return Some(InputAction::Redraw);
                }
            }
            KeyCode::Char('p') => {
                self.set_go_to_page_mode(PageJumpMode::Pdf);
                return Some(InputAction::Redraw);
            }
            KeyCode::Char(c) => {
                if let Some(input_num) = c.to_digit(10) {
                    self.go_to_page_error = None;
                    *page = (*page * 10) + input_num as usize;
                    return Some(InputAction::Redraw);
                }
            }
            KeyCode::Backspace => {
                self.go_to_page_error = None;
                *page /= 10;
                return Some(InputAction::Redraw);
            }
            KeyCode::Enter => {
                let mode = self.go_to_page_input.take()?;
                return self.handle_go_to_page_enter(mode);
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.toggle_go_to_page_mode();
                return Some(InputAction::Redraw);
            }
            KeyCode::Esc => {
                self.clear_go_to_page_input();
                return Some(InputAction::Redraw);
            }
            _ => {}
        }

        None
    }

    fn handle_go_to_page_enter(&mut self, page: usize) -> Option<InputAction> {
        let rendered_len = self.rendered.len();
        let target_page = if page == 0 {
            Err("Page numbers start at 1.".to_string())
        } else {
            match self.go_to_page_mode {
                PageJumpMode::Content => {
                    if !self.page_numbers.has_offset() {
                        Err("Content page numbers are not available yet.".to_string())
                    } else {
                        self.page_numbers
                            .map_printed_to_pdf(page, rendered_len)
                            .ok_or_else(|| {
                                format!(
                                    "Cannot map content page {page} to the document page range."
                                )
                            })
                    }
                }
                PageJumpMode::Pdf => Ok(page.saturating_sub(1)),
            }
        };

        let target_page = match target_page {
            Ok(target) => target,
            Err(err) => {
                self.go_to_page_error = Some(err);
                self.go_to_page_input = Some(page);
                return Some(InputAction::Redraw);
            }
        };

        if target_page < rendered_len {
            self.go_to_page_error = None;
            self.set_page(target_page);
            Some(self.jump_to_page_action(target_page))
        } else {
            self.go_to_page_error = Some(format!(
                "Page {page} is out of range; this document contains {rendered_len} pages total."
            ));
            self.go_to_page_input = Some(page);
            Some(InputAction::Redraw)
        }
    }

    fn handle_normal_mode_event(&mut self, key: KeyEvent) -> InputResponse {
        log::info!(
            "handle_normal_mode_event: key={:?}, normal_mode.active={}",
            key.code,
            self.normal_mode.active
        );
        match key.code {
            // When search is active, n/N navigate matches; otherwise n toggles normal mode
            KeyCode::Char('n') if self.page_search.query.is_some() => {
                InputResponse::handled(self.jump_to_next_search_match())
            }
            KeyCode::Char('N') if self.page_search.query.is_some() => {
                InputResponse::handled(self.jump_to_prev_search_match())
            }
            KeyCode::Char('n') => InputResponse::handled(self.toggle_normal_mode()),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                InputResponse::handled(self.scroll_half_screen(ScrollDirection::Down))
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                InputResponse::handled(self.scroll_half_screen(ScrollDirection::Up))
            }
            KeyCode::Char(c) => {
                let action = self.handle_normal_mode_key(c);
                if action.is_some() {
                    InputResponse::handled(action)
                } else {
                    InputResponse::unhandled()
                }
            }
            KeyCode::Enter => {
                if self.go_to_page_input.is_none() {
                    if let Some(target) = self.link_target_at_cursor() {
                        return InputResponse::handled(Some(self.activate_link_target(target)));
                    }
                }
                InputResponse::handled(None)
            }
            KeyCode::Esc => InputResponse::handled(self.handle_escape_key()),
            KeyCode::BackTab => {
                // Exit normal mode and start comment navigation
                self.normal_mode.exit_visual();
                self.normal_mode.deactivate();
                InputResponse::handled(self.start_comment_nav())
            }
            _ => InputResponse::unhandled(),
        }
    }

    fn handle_standard_key_event(&mut self, key: KeyEvent) -> Option<InputAction> {
        match key.code {
            KeyCode::Char(c) => {
                if c == 'n' {
                    return self.toggle_normal_mode();
                }

                self.key_seq.push(key);
                if self.key_seq.len() > 2 {
                    let start = self.key_seq.len() - 2;
                    let last_two: Vec<KeyEvent> = self.key_seq.keys()[start..].to_vec();
                    self.key_seq.clear();
                    for key in last_two {
                        self.key_seq.push(key);
                    }
                }

                if self
                    .key_seq
                    .matches(&[KeyCode::Char('g'), KeyCode::Char('g')])
                {
                    self.key_seq.clear();
                    return self.scroll_to_page_top();
                }
                if self
                    .key_seq
                    .matches(&[KeyCode::Char(' '), KeyCode::Char('g')])
                {
                    self.key_seq.clear();
                    return self.start_go_to_page_input();
                }

                match c {
                    'j' => self.scroll_line(ScrollDirection::Down),
                    'k' => self.scroll_line(ScrollDirection::Up),
                    'H' => self.pan_horizontal(ScrollDirection::Right),
                    'L' => self.pan_horizontal(ScrollDirection::Left),
                    'l' => self.next_page(),
                    'h' => self.prev_page(),
                    'q' => Some(InputAction::QuitApp),
                    'i' => Some(InputAction::ToggleInvertImages),
                    'p' => Some(InputAction::ToggleProfiling),
                    'x' => Some(InputAction::DumpDebugState),
                    'z' if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.reset_zoom_to_fit()
                    }
                    '=' | '+' => self.zoom_in(),
                    '-' | '_' => self.zoom_out(),
                    'G' => self.scroll_to_page_bottom(),
                    'd' if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.scroll_half_screen(ScrollDirection::Down)
                    }
                    'u' if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.scroll_half_screen(ScrollDirection::Up)
                    }
                    'c' if key.modifiers.contains(KeyModifiers::CONTROL) => self.copy_selection(),
                    _ => None,
                }
            }
            KeyCode::Right => self.next_page(),
            KeyCode::Down => self.next_screen(),
            KeyCode::Left => self.prev_page(),
            KeyCode::Up => self.prev_screen(),
            KeyCode::Esc => self.handle_escape_key(),
            KeyCode::Enter => None,
            KeyCode::Tab => None,
            KeyCode::BackTab => self.start_comment_nav(),
            _ => None,
        }
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Option<InputAction> {
        match mouse.kind {
            MouseEventKind::ScrollRight => self.handle_mouse_scroll(mouse, ScrollDirection::Right),
            MouseEventKind::ScrollDown => self.handle_mouse_scroll(mouse, ScrollDirection::Down),
            MouseEventKind::ScrollLeft => self.handle_mouse_scroll(mouse, ScrollDirection::Left),
            MouseEventKind::ScrollUp => self.handle_mouse_scroll(mouse, ScrollDirection::Up),
            MouseEventKind::Down(MouseButton::Left) => {
                self.mouse_down_seen = true;
                self.handle_selection_mouse(mouse.column, mouse.row, mouse.kind)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let kind = if self.selection.is_selecting || self.mouse_down_seen {
                    mouse.kind
                } else {
                    self.mouse_down_seen = true;
                    MouseEventKind::Down(MouseButton::Left)
                };
                self.handle_selection_mouse(mouse.column, mouse.row, kind)
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let action = if self.mouse_down_seen {
                    self.handle_selection_mouse(mouse.column, mouse.row, mouse.kind)
                } else {
                    let down_action = self.handle_selection_mouse(
                        mouse.column,
                        mouse.row,
                        MouseEventKind::Down(MouseButton::Left),
                    );
                    let up_action =
                        self.handle_selection_mouse(mouse.column, mouse.row, mouse.kind);
                    down_action.or(up_action)
                };
                self.mouse_down_seen = false;
                action
            }
            MouseEventKind::Moved => {
                if self.selection.is_selecting {
                    self.handle_selection_mouse(
                        mouse.column,
                        mouse.row,
                        MouseEventKind::Drag(MouseButton::Left),
                    )
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn handle_mouse_scroll(
        &mut self,
        mouse: MouseEvent,
        direction: ScrollDirection,
    ) -> Option<InputAction> {
        if mouse.modifiers.contains(KeyModifiers::CONTROL) {
            match direction {
                ScrollDirection::Up => self.zoom_in(),
                ScrollDirection::Down => self.zoom_out(),
                _ => None,
            }
        } else {
            self.scroll_line(direction)
        }
    }

    fn handle_paste(&mut self, text: &str) -> Option<InputAction> {
        if self.comment_input.is_active() {
            if let Some(textarea) = self.comment_input.textarea.as_mut() {
                textarea.insert_str(text);
                return Some(InputAction::Redraw);
            }
        }
        None
    }

    fn handle_escape_key(&mut self) -> Option<InputAction> {
        // Clear page search first if active (pressing Esc again will exit normal mode)
        if self.normal_mode.active && self.page_search.query.is_some() {
            self.page_search.clear_search();
            return Some(InputAction::SelectionChanged(vec![]));
        }

        // Exit visual mode first
        if self.normal_mode.active && self.normal_mode.is_visual_active() {
            self.normal_mode.exit_visual();
            return Some(InputAction::ExitVisualMode(self.get_cursor_rect()));
        }

        // Exit normal mode
        if self.normal_mode.active {
            self.normal_mode.deactivate();
            return Some(InputAction::ExitNormalMode);
        }

        // Clear mouse selection (in-progress or completed)
        if self.selection.is_selecting || self.selection.has_selection() {
            self.selection.clear();
            self.mouse_down_seen = false;
            self.last_render.rect = Rect::default();
            return Some(InputAction::SelectionChanged(Vec::new()));
        }

        if self.go_to_page_input.is_some() {
            self.clear_go_to_page_input();
            return Some(InputAction::Redraw);
        }

        // Clear any search highlight (sent via UpdateSelection but not tracked in self.selection)
        Some(InputAction::SelectionChanged(Vec::new()))
    }

    pub fn jump_to_page_action(&mut self, page: usize) -> InputAction {
        self.page = page;
        self.last_render.rect = Rect::default();
        if self.is_kitty {
            // Reset scroll offset to target page for Kitty
            let offset = self.calculate_page_offset(page);
            if let Some(ref mut zoom) = self.zoom {
                zoom.global_scroll_offset = offset;
            }
        } else {
            self.non_kitty_scroll_offset = 0;
        }
        if self.normal_mode.active {
            self.normal_mode.deactivate();
        }
        InputAction::JumpingToPage {
            page,
            viewport: self.current_viewport_update(),
        }
    }

    /// Jump to a search result, navigating to the page and selecting the exact match
    pub fn jump_to_search_result(&mut self, page: usize, query: &str) -> InputAction {
        self.page = page;
        self.last_render.rect = Rect::default();
        if self.is_kitty {
            let offset = self.calculate_page_offset(page);
            if let Some(ref mut zoom) = self.zoom {
                zoom.global_scroll_offset = offset;
            }
        } else {
            self.non_kitty_scroll_offset = 0;
        }
        if self.normal_mode.active {
            self.normal_mode.deactivate();
        }

        // Find the exact match in the page's line_bounds
        let selection_rects = self.find_text_selection_rects(page, query);

        // If no rects found (page data not yet available), store as pending
        if selection_rects.is_empty() {
            self.pending_search_highlight = Some((page, query.to_string()));
        } else {
            self.pending_search_highlight = None;
        }

        InputAction::CommentNavJump {
            page,
            viewport: self.current_viewport_update(),
            selection_rects,
        }
    }

    /// Find selection rectangles for text matching the query on a page.
    /// Returns coordinates in original page space (converter handles scaling).
    pub fn find_text_selection_rects(
        &self,
        page: usize,
        query: &str,
    ) -> Vec<crate::pdf::SelectionRect> {
        use crate::pdf::SelectionRect;

        let Some(rendered) = self.rendered.get(page) else {
            return Vec::new();
        };

        if rendered.line_bounds.is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let mut rects = Vec::new();

        for line in &rendered.line_bounds {
            // Build the line text from characters
            let line_text: String = line.chars.iter().map(|c| c.c).collect();
            let line_lower = line_text.to_lowercase();

            // Find all occurrences of the query in this line
            let mut search_start = 0;
            while let Some(match_start) = line_lower[search_start..].find(&query_lower) {
                let abs_start = search_start + match_start;
                let abs_end = abs_start + query_lower.len();

                // Get character positions for the match
                if abs_start < line.chars.len() && abs_end <= line.chars.len() {
                    let start_x = line.chars[abs_start].x;
                    // For end x, use next char's x or line's x1
                    let end_x = if abs_end < line.chars.len() {
                        line.chars[abs_end].x
                    } else {
                        line.x1
                    };

                    rects.push(SelectionRect {
                        page,
                        topleft_x: start_x.round() as u32,
                        topleft_y: line.y0.round() as u32,
                        bottomright_x: end_x.round() as u32,
                        bottomright_y: line.y1.round() as u32,
                    });
                }

                search_start = abs_start + 1;
            }
        }

        rects
    }

    fn calculate_page_offset(&self, target_page: usize) -> u32 {
        // In page mode, always return 0 since we only show one page at a time
        if get_pdf_render_mode() == PdfRenderMode::Page {
            return 0;
        }

        let zoom_factor = self.zoom.as_ref().map(|z| z.factor()).unwrap_or(1.0);
        let estimated_h = self.estimated_page_height_cells();
        let mut cumulative_y: u32 = 0;

        for (page_idx, rendered_page) in self.rendered.iter().enumerate() {
            if page_idx >= target_page {
                break;
            }
            let cell_height = rendered_page
                .img
                .as_ref()
                .map(|img| img.cell_dimensions().height)
                .unwrap_or(estimated_h);
            let dest_h = ((f32::from(cell_height) * zoom_factor).ceil() as u32).max(1);
            cumulative_y += dest_h + u32::from(SEPARATOR_HEIGHT);
        }

        // If target page is beyond rendered pages, estimate remaining
        if target_page > self.rendered.len() {
            let remaining_pages = target_page - self.rendered.len();
            let est_dest_h = ((f32::from(estimated_h) * zoom_factor).ceil() as u32).max(1);
            cumulative_y += remaining_pages as u32 * (est_dest_h + u32::from(SEPARATOR_HEIGHT));
        }

        cumulative_y
    }

    fn reset_zoom_to_fit(&mut self) -> Option<InputAction> {
        if self.is_kitty {
            let fit_factor = self.fit_to_height_zoom_factor();
            self.zoom = Some(Zoom {
                factor: fit_factor,
                cell_pan_from_left: 0,
                global_scroll_offset: 0,
            });
            self.last_render.rect = Rect::default();
            crate::settings::set_pdf_scale(fit_factor);
            crate::settings::set_pdf_pan_shift(0);
            Some(InputAction::Redraw)
        } else {
            let fit_factor = self.fit_to_height_zoom_factor();
            self.non_kitty_zoom_factor = fit_factor;
            self.non_kitty_scroll_offset = 0;
            self.clear_pending_scroll();
            self.last_render.rect = Rect::default();
            crate::settings::set_pdf_scale(fit_factor);
            crate::settings::set_pdf_pan_shift(0);
            self.make_render_scale_action(fit_factor)
        }
    }

    // Navigation helpers

    pub(crate) fn current_viewport_update(&self) -> Option<ViewportUpdate> {
        if self.is_kitty {
            return None;
        }
        let height = self.last_render.img_area_height;
        let width = self.last_render.img_area_width;
        if height == 0 {
            return None;
        }
        Some(ViewportUpdate {
            page: self.page,
            y_offset_cells: self.non_kitty_scroll_offset,
            viewport_height_cells: height,
            viewport_width_cells: width,
        })
    }

    fn clear_pending_scroll(&mut self) {
        self.pending_scroll = None;
    }

    fn non_kitty_scroll_by(
        &mut self,
        direction: ScrollDirection,
        step: u32,
    ) -> Option<InputAction> {
        let viewport_height = self.last_render.img_area_height;
        if viewport_height == 0 {
            return None;
        }

        let full_height = self
            .rendered
            .get(self.page)
            .and_then(|r| r.full_cell_size.map(|size| size.height))
            .unwrap_or(viewport_height);
        let max_offset = u32::from(full_height.saturating_sub(viewport_height));

        let old_offset = self.non_kitty_scroll_offset;
        let new_offset = match direction {
            ScrollDirection::Down => (self.non_kitty_scroll_offset + step).min(max_offset),
            ScrollDirection::Up => self.non_kitty_scroll_offset.saturating_sub(step),
            _ => self.non_kitty_scroll_offset,
        };

        if new_offset == old_offset {
            return None;
        }

        self.non_kitty_scroll_offset = new_offset;
        self.last_render.rect = Rect::default();

        // Set pending_scroll for scroll optimization (only re-render new tiles)
        let delta = new_offset as i32 - old_offset as i32;
        let abs_delta = delta.unsigned_abs();
        if abs_delta != 0 && abs_delta < u32::from(viewport_height) {
            if let Ok(delta_cells) = i16::try_from(delta) {
                if self.last_render.img_area_height != 0 {
                    self.pending_scroll = Some(PendingScroll {
                        delta_cells,
                        img_area: self.last_render.img_area,
                    });
                } else {
                    self.pending_scroll = None;
                }
            } else {
                self.pending_scroll = None;
            }
        } else {
            self.pending_scroll = None;
        }

        self.current_viewport_update()
            .map(InputAction::ViewportChanged)
    }

    // Unified scroll - one line at a time (j/k keys)
    fn scroll_line(&mut self, direction: ScrollDirection) -> Option<InputAction> {
        if self.is_kitty {
            self.update_zoom(|z| z.pan(direction))
        } else {
            let factor = self.non_kitty_zoom_factor;
            let step = (2.0_f32 / factor).max(1.0) as u32;
            self.non_kitty_scroll_by(direction, step)
        }
    }

    fn pan_horizontal(&mut self, direction: ScrollDirection) -> Option<InputAction> {
        if self.is_kitty {
            let result = self.update_zoom(|z| z.pan(direction));
            if let Some(z) = &self.zoom {
                crate::settings::set_pdf_pan_shift(z.cell_pan_from_left);
            }
            result
        } else {
            let factor = self.non_kitty_zoom_factor;
            let step = (2.0_f32 / factor).max(1.0) as u32;
            self.non_kitty_scroll_by(direction, step)
        }
    }

    // Unified zoom
    fn zoom_in(&mut self) -> Option<InputAction> {
        if self.is_kitty {
            self.update_zoom_keep_page(Zoom::step_in)
        } else {
            self.non_kitty_zoom_factor =
                Zoom::clamp_factor(self.non_kitty_zoom_factor * Zoom::ZOOM_IN_RATE);
            self.set_zoom_hud(self.non_kitty_zoom_factor);
            self.clear_pending_scroll();
            self.make_render_scale_action(self.non_kitty_zoom_factor)
        }
    }

    fn zoom_out(&mut self) -> Option<InputAction> {
        if self.is_kitty {
            self.update_zoom_keep_page(Zoom::step_out)
        } else {
            self.non_kitty_zoom_factor =
                Zoom::clamp_factor(self.non_kitty_zoom_factor / Zoom::ZOOM_OUT_RATE);
            self.set_zoom_hud(self.non_kitty_zoom_factor);
            self.clear_pending_scroll();
            self.make_render_scale_action(self.non_kitty_zoom_factor)
        }
    }

    // Unified page navigation
    fn next_page(&mut self) -> Option<InputAction> {
        self.navigate_pages(1)
    }

    fn prev_page(&mut self) -> Option<InputAction> {
        self.navigate_pages(-1)
    }

    fn next_screen(&mut self) -> Option<InputAction> {
        let pages = self.last_render.pages_shown.max(1) as isize;
        self.navigate_pages(pages)
    }

    fn prev_screen(&mut self) -> Option<InputAction> {
        let pages = self.last_render.pages_shown.max(1) as isize;
        self.navigate_pages(-pages)
    }

    fn navigate_pages(&mut self, delta: isize) -> Option<InputAction> {
        let old = self.page;
        let new_page = if delta >= 0 {
            (self.page + delta as usize).min(self.rendered.len().saturating_sub(1))
        } else {
            self.page.saturating_sub((-delta) as usize)
        };
        self.set_page(new_page);

        if self.page == old {
            None
        } else {
            Some(InputAction::JumpingToPage {
                page: self.page,
                viewport: self.current_viewport_update(),
            })
        }
    }

    fn update_zoom(&mut self, f: impl FnOnce(&mut Zoom)) -> Option<InputAction> {
        if let Some(z) = &mut self.zoom {
            f(z);
        }
        if self.is_kitty {
            self.clamp_kitty_scroll_offset();
        }
        self.last_render.rect = Rect::default();
        Some(InputAction::Redraw)
    }

    #[expect(clippy::unnecessary_wraps)]
    fn update_zoom_keep_page(&mut self, f: impl FnOnce(&mut Zoom)) -> Option<InputAction> {
        let page = self.page;

        // In page mode, preserve relative scroll position within the page
        let is_page_mode = self.is_kitty && get_pdf_render_mode() == PdfRenderMode::Page;
        let old_scroll_ratio = if is_page_mode {
            self.zoom.as_ref().map(|z| {
                let old_factor = z.factor();
                let old_heights = self.page_heights_scaled(old_factor);
                let old_page_height = old_heights.get(page).copied().unwrap_or(1).max(1);
                z.global_scroll_offset as f64 / old_page_height as f64
            })
        } else {
            None
        };

        let factor = if let Some(z) = &mut self.zoom {
            f(z);
            z.factor()
        } else {
            return Some(InputAction::Redraw);
        };
        self.set_zoom_hud(factor);
        let heights = self.page_heights_scaled(factor);

        if let Some(ratio) = old_scroll_ratio {
            // Page mode: restore relative scroll position within the page
            let new_page_height = heights.get(page).copied().unwrap_or(0);
            if let Some(z) = &mut self.zoom {
                z.global_scroll_offset = (ratio * new_page_height as f64).round() as u32;
            }
        } else if let Some(z) = &mut self.zoom {
            z.scroll_to_page(page, &heights, SEPARATOR_HEIGHT);
        }

        if self.is_kitty {
            self.clamp_kitty_scroll_offset();
        }
        self.last_render.rect = Rect::default();
        crate::settings::set_pdf_scale(factor);
        Some(InputAction::Redraw)
    }

    fn scroll_to_document_top(&mut self) -> Option<InputAction> {
        if let Some(z) = &mut self.zoom {
            z.scroll_to_top();
        }
        self.last_render.rect = Rect::default();
        Some(InputAction::Redraw)
    }

    fn scroll_to_document_bottom(&mut self) -> Option<InputAction> {
        let heights = self
            .zoom
            .as_ref()
            .map(|z| self.page_heights_scaled(z.factor()))
            .unwrap_or_default();
        if let Some(z) = &mut self.zoom {
            z.scroll_to_bottom(&heights, SEPARATOR_HEIGHT);
        }
        self.last_render.rect = Rect::default();
        Some(InputAction::Redraw)
    }

    fn scroll_to_page_top(&mut self) -> Option<InputAction> {
        let viewport = self.scroll_to_page_top_with_viewport();
        if self.is_kitty {
            return Some(InputAction::Redraw);
        }
        viewport
            .map(InputAction::ViewportChanged)
            .or(Some(InputAction::Redraw))
    }

    fn scroll_to_page_bottom(&mut self) -> Option<InputAction> {
        let viewport = self.scroll_to_page_bottom_with_viewport();
        if self.is_kitty {
            return Some(InputAction::Redraw);
        }
        viewport
            .map(InputAction::ViewportChanged)
            .or(Some(InputAction::Redraw))
    }

    fn scroll_to_page_top_with_viewport(&mut self) -> Option<ViewportUpdate> {
        if self.is_kitty {
            let is_page_mode = get_pdf_render_mode() == PdfRenderMode::Page;
            if is_page_mode {
                // In page mode, just scroll to top of current page
                if let Some(z) = &mut self.zoom {
                    z.scroll_to_top();
                }
            } else if let Some(factor) = self.zoom.as_ref().map(|z| z.factor()) {
                // Scroll mode: calculate cumulative offset to page start
                let heights = self.page_heights_scaled(factor);
                if let Some(z) = &mut self.zoom {
                    z.scroll_to_page(self.page, &heights, SEPARATOR_HEIGHT);
                }
            }
            self.clamp_kitty_scroll_offset();
            self.last_render.rect = Rect::default();
            return None;
        }

        let old_offset = self.non_kitty_scroll_offset;
        self.non_kitty_scroll_offset = 0;
        self.pending_scroll = None;
        self.last_render.rect = Rect::default();

        if old_offset == 0 {
            None
        } else {
            self.current_viewport_update()
        }
    }

    fn scroll_to_page_bottom_with_viewport(&mut self) -> Option<ViewportUpdate> {
        if self.is_kitty {
            let is_page_mode = get_pdf_render_mode() == PdfRenderMode::Page;
            if let Some(factor) = self.zoom.as_ref().map(|z| z.factor()) {
                let heights = self.page_heights_scaled(factor);
                let page_height = heights.get(self.page).copied().unwrap_or(0);
                let viewport_height = u32::from(self.last_render.img_area_height.max(1));

                let target_offset = if is_page_mode {
                    // In page mode, scroll to bottom of current page (no cumulative offset)
                    if page_height > viewport_height {
                        page_height - viewport_height
                    } else {
                        0
                    }
                } else {
                    // Scroll mode: calculate cumulative offset
                    let page_start: u32 = heights
                        .iter()
                        .take(self.page)
                        .map(|&h| h + u32::from(SEPARATOR_HEIGHT))
                        .sum();
                    if page_height > viewport_height {
                        page_start + page_height - viewport_height
                    } else {
                        page_start
                    }
                };

                if let Some(z) = &mut self.zoom {
                    z.global_scroll_offset = target_offset;
                }
            }
            self.clamp_kitty_scroll_offset();
            self.last_render.rect = Rect::default();
            return None;
        }

        let viewport_height = self.last_render.img_area_height;
        if viewport_height == 0 {
            return None;
        }
        let full_height = self
            .rendered
            .get(self.page)
            .and_then(|r| r.full_cell_size.map(|size| size.height))
            .unwrap_or(viewport_height);
        let max_offset = u32::from(full_height.saturating_sub(viewport_height));

        let old_offset = self.non_kitty_scroll_offset;
        self.non_kitty_scroll_offset = max_offset;
        self.pending_scroll = None;
        self.last_render.rect = Rect::default();

        if old_offset == max_offset {
            None
        } else {
            self.current_viewport_update()
        }
    }

    fn scroll_half_screen(&mut self, direction: ScrollDirection) -> Option<InputAction> {
        let half = self.last_render.img_area_height / 2;

        // Non-Kitty: use scroll_by with half screen step
        if !self.is_kitty {
            return self.non_kitty_scroll_by(direction, u32::from(half));
        }

        if let Some(z) = &mut self.zoom {
            match direction {
                ScrollDirection::Up => {
                    z.global_scroll_offset = z.global_scroll_offset.saturating_sub(u32::from(half));
                }
                ScrollDirection::Down => {
                    z.global_scroll_offset = z.global_scroll_offset.saturating_add(u32::from(half));
                }
                _ => {}
            }
        }
        self.last_render.rect = Rect::default();

        if self.normal_mode.active {
            let lines_to_move = (half / 2).max(1) as usize;

            match direction {
                ScrollDirection::Up => {
                    for _ in 0..lines_to_move {
                        let line_bounds = self
                            .rendered
                            .get(self.normal_mode.cursor.page)
                            .map(|r| r.line_bounds.clone())
                            .unwrap_or_default();
                        let result = self.normal_mode.move_up(&line_bounds);
                        if result == MoveResult::WantsPrevPage && self.normal_mode.cursor.page > 0 {
                            let prev_page = self.normal_mode.cursor.page - 1;
                            let prev_line_bounds = self
                                .rendered
                                .get(prev_page)
                                .map(|r| r.line_bounds.clone())
                                .unwrap_or_default();
                            if !prev_line_bounds.is_empty() {
                                self.normal_mode
                                    .move_to_page_end(prev_page, &prev_line_bounds);
                            }
                        }
                    }
                }
                ScrollDirection::Down => {
                    for _ in 0..lines_to_move {
                        let line_bounds = self
                            .rendered
                            .get(self.normal_mode.cursor.page)
                            .map(|r| r.line_bounds.clone())
                            .unwrap_or_default();
                        let result = self.normal_mode.move_down(&line_bounds);
                        if result == MoveResult::WantsNextPage {
                            let next_page = self.normal_mode.cursor.page + 1;
                            if next_page < self.rendered.len() {
                                let next_line_bounds = self
                                    .rendered
                                    .get(next_page)
                                    .map(|r| r.line_bounds.clone())
                                    .unwrap_or_default();
                                if !next_line_bounds.is_empty() {
                                    self.normal_mode
                                        .move_to_page_start(next_page, &next_line_bounds);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            let viewport_changed = self.ensure_cursor_visible();
            let viewport = if viewport_changed && !self.is_kitty {
                self.current_viewport_update()
            } else {
                None
            };

            if self.normal_mode.is_visual_active() {
                let all_line_bounds = self.collect_all_line_bounds();
                return Some(InputAction::VisualChanged(
                    self.normal_mode.get_visual_rects_multi(&all_line_bounds),
                    viewport,
                ));
            }
            return Some(InputAction::CursorChanged(self.get_cursor_rect(), viewport));
        }

        Some(InputAction::Redraw)
    }

    fn page_heights_scaled(&self, zoom_factor: f32) -> Vec<u32> {
        let reference_height: Option<u16> = self.rendered.iter().find_map(|page| {
            page.img
                .as_ref()
                .map(|img| img.cell_dimensions().as_tuple().1)
        });

        let default_height = reference_height.unwrap_or(self.last_render.img_area_height);

        self.rendered
            .iter()
            .map(|r| {
                let h = r
                    .img
                    .as_ref()
                    .map_or(default_height, |img| img.cell_dimensions().as_tuple().1);
                if h == 0 {
                    ((f32::from(default_height) * zoom_factor).ceil() as u32).max(1)
                } else {
                    ((f32::from(h) * zoom_factor).ceil() as u32).max(1)
                }
            })
            .collect()
    }

    fn clamp_kitty_scroll_offset(&mut self) {
        let Some(factor) = self.zoom.as_ref().map(|z| z.factor()) else {
            return;
        };
        let viewport_height = u32::from(self.last_render.img_area_height);
        if viewport_height == 0 {
            return;
        }
        let heights = self.page_heights_scaled(factor);
        if heights.is_empty() {
            if let Some(z) = self.zoom.as_mut() {
                z.global_scroll_offset = 0;
            }
            return;
        }

        // In page mode, clamp to current page height only
        let max_offset = if get_pdf_render_mode() == PdfRenderMode::Page {
            let current_page_height = heights.get(self.page).copied().unwrap_or(0);
            current_page_height.saturating_sub(viewport_height)
        } else {
            // In scroll mode, use total document height
            let total_height: u32 = heights
                .iter()
                .map(|&h| h + u32::from(SEPARATOR_HEIGHT))
                .sum();
            total_height.saturating_sub(viewport_height)
        };

        if let Some(z) = self.zoom.as_mut() {
            if z.global_scroll_offset > max_offset {
                z.global_scroll_offset = max_offset;
            }
        }
    }

    /// Calculate which page should be visible at the current scroll offset.
    /// Returns the page index that should be at the top of the viewport.
    pub fn expected_page_from_scroll(&self) -> usize {
        // In page mode, the page is explicitly set, not derived from scroll offset
        if get_pdf_render_mode() == PdfRenderMode::Page {
            return self.page;
        }

        let Some(zoom) = &self.zoom else {
            return self.page;
        };

        // Don't calculate expected page from scroll until we have at least one
        // rendered image. Without real page heights, the calculation would be
        // inaccurate and could reset the page to 0.
        let has_any_image = self.rendered.iter().any(|page| {
            page.img
                .as_ref()
                .is_some_and(|img| img.cell_dimensions().height > 0)
        });
        if !has_any_image {
            return self.page;
        }

        let heights = self.page_heights_scaled(zoom.factor());
        let scroll_offset = zoom.global_scroll_offset;

        let mut cumulative: u32 = 0;
        for (idx, &height) in heights.iter().enumerate() {
            let page_end = cumulative + height + u32::from(SEPARATOR_HEIGHT);
            if scroll_offset < page_end {
                return idx;
            }
            cumulative = page_end;
        }

        // If scroll is past all pages, return last page
        heights.len().saturating_sub(1)
    }

    fn fit_to_height_zoom_factor(&self) -> f32 {
        let page_cell_h = self
            .rendered
            .get(self.page)
            .and_then(|r| r.full_cell_size.map(|size| size.height))
            .unwrap_or(0);

        let area_h = self.last_render.img_area_height;

        if page_cell_h == 0 || area_h == 0 {
            return 1.0;
        }

        let zoom_factor = f32::from(area_h) / f32::from(page_cell_h);
        Zoom::clamp_factor(zoom_factor)
    }

    fn make_render_scale_action(&self, zoom_factor: f32) -> Option<InputAction> {
        Some(InputAction::RenderScale {
            factor: zoom_factor,
            viewport: self.current_viewport_update(),
        })
    }

    pub(crate) fn set_page(&mut self, page: usize) {
        if page != self.page {
            self.last_render.rect = Rect::default();
            self.clear_pending_scroll();
            self.page = page;
            if self.comment_nav_active {
                self.comment_nav_page = page;
                self.comment_nav_index = 0;
            }
            if !self.is_kitty {
                self.non_kitty_scroll_offset = 0;
            }
            // Get heights before borrowing zoom mutably
            let heights = self
                .zoom
                .as_ref()
                .map(|z| self.page_heights_scaled(z.factor()))
                .unwrap_or_default();
            if let Some(ref mut zoom) = self.zoom {
                if get_pdf_render_mode() == PdfRenderMode::Page {
                    // In page mode, reset scroll to top of current page
                    zoom.global_scroll_offset = 0;
                } else {
                    // In scroll mode, scroll to the page position in document
                    zoom.scroll_to_page(page, &heights, SEPARATOR_HEIGHT);
                }
                zoom.cell_pan_from_left = 0;
            }
        }
    }

    pub fn clear_go_to_page_input(&mut self) {
        if self.go_to_page_input.is_some() {
            self.go_to_page_input = None;
            self.last_render.rect = Rect::default();
            self.go_to_page_error = None;
        }
    }

    fn start_go_to_page_input(&mut self) -> Option<InputAction> {
        let mode = if self.content_page_mode_available() {
            PageJumpMode::Content
        } else {
            PageJumpMode::Pdf
        };
        self.go_to_page_error = None;
        self.go_to_page_mode = mode;
        self.go_to_page_input = Some(0);
        Some(InputAction::Redraw)
    }

    fn set_go_to_page_mode(&mut self, mode: PageJumpMode) {
        let next = if mode == PageJumpMode::Content && !self.content_page_mode_available() {
            PageJumpMode::Pdf
        } else {
            mode
        };
        if self.go_to_page_mode != next {
            self.go_to_page_mode = next;
            self.go_to_page_error = None;
        }
    }

    fn toggle_go_to_page_mode(&mut self) {
        if !self.content_page_mode_available() {
            return;
        }
        let next = match self.go_to_page_mode {
            PageJumpMode::Content => PageJumpMode::Pdf,
            PageJumpMode::Pdf => PageJumpMode::Content,
        };
        self.set_go_to_page_mode(next);
    }

    // Selection handling

    fn handle_selection_mouse(
        &mut self,
        col: u16,
        row: u16,
        kind: MouseEventKind,
    ) -> Option<InputAction> {
        let (_, font_size) = self.coord_info?;

        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                use crate::inputs::ClickType;

                let click_type = self.mouse_tracker.detect_click_type(col, row);
                let click_count = match click_type {
                    ClickType::Single => 1,
                    ClickType::Double => 2,
                    ClickType::Triple => 3,
                };

                if let Some(point) = self.terminal_to_selection_point(col, row, font_size) {
                    if click_count == 1 {
                        if let Some(target) = self.link_target_at_point(&point) {
                            if self.selection.has_selection() {
                                self.selection.clear();
                                self.last_render.rect = Rect::default();
                            }
                            return Some(self.activate_link_target(target));
                        }
                    }

                    if self.normal_mode.active {
                        self.selection.start_at(point);

                        if let Some(cursor) = self.selection_point_to_cursor(point) {
                            self.normal_mode.cursor = cursor;
                            self.normal_mode.pending_motion = PendingMotion::None;
                            self.normal_mode.pending_g = false;
                        }

                        match click_count {
                            1 => {
                                let viewport_changed = self.ensure_cursor_visible();
                                let viewport = if viewport_changed && !self.is_kitty {
                                    self.current_viewport_update()
                                } else {
                                    None
                                };

                                if self.normal_mode.is_visual_active() {
                                    self.normal_mode.exit_visual();
                                    return Some(InputAction::ExitVisualMode(
                                        self.get_cursor_rect(),
                                    ));
                                }

                                return Some(InputAction::CursorChanged(
                                    self.get_cursor_rect(),
                                    viewport,
                                ));
                            }
                            2 => {
                                if let Some((start_x, end_x)) = self.find_word_bounds_at(&point) {
                                    let mut start_point = point;
                                    let mut end_point = point;
                                    start_point.pdf_x = start_x;
                                    end_point.pdf_x = end_x;
                                    self.selection.start_at(start_point);
                                    self.selection.update_end(end_point);
                                    self.selection.finish();
                                    return self
                                        .start_visual_from_selection(VisualMode::CharacterWise);
                                }
                            }
                            3 => {
                                if let Some((x0, x1, y0, y1)) =
                                    self.find_paragraph_bounds_at(&point)
                                {
                                    let mut start_point = point;
                                    let mut end_point = point;
                                    start_point.pdf_x = x0;
                                    start_point.pdf_y = y0;
                                    end_point.pdf_x = x1;
                                    end_point.pdf_y = y1;
                                    self.selection.start_at(start_point);
                                    self.selection.update_end(end_point);
                                    self.selection.finish();
                                    return self.start_visual_from_selection(VisualMode::LineWise);
                                }
                            }
                            _ => {}
                        }
                    }

                    match click_count {
                        2 => {
                            if let Some((start_x, end_x)) = self.find_word_bounds_at(&point) {
                                let mut start_point = point;
                                let mut end_point = point;
                                start_point.pdf_x = start_x;
                                end_point.pdf_x = end_x;
                                self.selection.start_at(start_point);
                                self.selection.update_end(end_point);
                                self.selection.finish();
                                self.last_render.rect = Rect::default();
                                return Some(InputAction::SelectionChanged(
                                    self.compute_selection_rects(),
                                ));
                            }
                        }
                        3 => {
                            if let Some((x0, x1, y0, y1)) = self.find_paragraph_bounds_at(&point) {
                                let mut start_point = point;
                                let mut end_point = point;
                                start_point.pdf_x = x0;
                                start_point.pdf_y = y0;
                                end_point.pdf_x = x1;
                                end_point.pdf_y = y1;
                                self.selection.start_at(start_point);
                                self.selection.update_end(end_point);
                                self.selection.finish();
                                self.last_render.rect = Rect::default();
                                return Some(InputAction::SelectionChanged(
                                    self.compute_selection_rects(),
                                ));
                            }
                        }
                        _ => {}
                    }

                    self.selection.start_at(point);
                    self.last_render.rect = Rect::default();
                    Some(InputAction::SelectionChanged(
                        self.compute_selection_rects(),
                    ))
                } else if self.selection.has_selection() {
                    self.selection.clear();
                    self.last_render.rect = Rect::default();
                    Some(InputAction::SelectionChanged(Vec::new()))
                } else {
                    None
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.selection.is_selecting {
                    if let Some(point) = self.terminal_to_selection_point(col, row, font_size) {
                        self.selection.update_end(point);
                        self.last_render.rect = Rect::default();
                        if self.normal_mode.active {
                            if let Some(action) =
                                self.start_visual_from_selection(VisualMode::CharacterWise)
                            {
                                return Some(action);
                            }
                        }
                        Some(InputAction::SelectionChanged(
                            self.compute_selection_rects(),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.selection.is_selecting {
                    self.selection.finish();
                    if self.normal_mode.active {
                        let (start, end) = self.selection.get_ordered_bounds()?;
                        if start.term_col != end.term_col || start.term_row != end.term_row {
                            if let Some(action) =
                                self.start_visual_from_selection(VisualMode::CharacterWise)
                            {
                                return Some(action);
                            }
                        } else {
                            self.selection.clear();
                        }
                        return None;
                    }
                    Some(InputAction::SelectionChanged(
                        self.compute_selection_rects(),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn terminal_to_selection_point(
        &self,
        term_col: u16,
        term_row: u16,
        font_size: FontSize,
    ) -> Option<crate::pdf::SelectionPoint> {
        use crate::pdf::SelectionPoint;

        let pixels_per_cell = |rendered_page: &super::types::RenderedInfo| -> (f32, f32) {
            if let (Some(pw), Some(ph), Some(cell_size)) = (
                rendered_page.pixel_w,
                rendered_page.pixel_h,
                rendered_page.full_cell_size,
            ) {
                if cell_size.width > 0 && cell_size.height > 0 {
                    return (
                        pw as f32 / cell_size.width as f32,
                        ph as f32 / cell_size.height as f32,
                    );
                }
            }
            (f32::from(font_size.0), f32::from(font_size.1))
        };

        let (img_area, _) = self.coord_info?;

        if term_col < img_area.x
            || term_col >= img_area.x + img_area.width
            || term_row < img_area.y
            || term_row >= img_area.y + img_area.height
        {
            return None;
        }

        let rel_col = term_col - img_area.x;
        let rel_row = term_row - img_area.y;

        if let Some(zoom) = self.zoom.as_ref() {
            let zoom_factor = zoom.factor();
            let scroll_offset = zoom.global_scroll_offset;
            let cell_pan_from_left = zoom.cell_pan_from_left;
            let is_page_mode = get_pdf_render_mode() == PdfRenderMode::Page;

            // In page mode, we know exactly which page we're on - no cumulative calculation needed
            let (page_idx, page_local_y, dest_w) = if is_page_mode {
                let rendered_page = self.rendered.get(self.page)?;
                let cell_size = rendered_page
                    .img
                    .as_ref()
                    .map(|img| img.cell_dimensions())
                    .or(rendered_page.full_cell_size)
                    .unwrap_or(CellSize::new(0, 0));
                if cell_size.height == 0 {
                    return None;
                }
                let dest_w = ((f32::from(cell_size.width) * zoom_factor).ceil() as u16).max(1);
                // In page mode, scroll_offset is within the current page
                let virtual_y = u32::from(rel_row) + scroll_offset;
                (self.page, virtual_y, dest_w)
            } else {
                // Scroll mode: iterate through pages with separators
                let virtual_y = u32::from(rel_row) + scroll_offset;

                let reference_height: Option<u16> = self.rendered.iter().find_map(|page| {
                    page.img
                        .as_ref()
                        .map(|img| img.cell_dimensions().height)
                        .or(page.full_cell_size.map(|size| size.height))
                });

                let mut cumulative_y: u32 = 0;
                let mut target_page = None;
                let mut local_y = 0u32;

                for (idx, rendered_page) in self.rendered.iter().enumerate() {
                    let cell_size = rendered_page
                        .img
                        .as_ref()
                        .map(|img| img.cell_dimensions())
                        .or(rendered_page.full_cell_size)
                        .unwrap_or(CellSize::new(0, 0));
                    if cell_size.height == 0 {
                        let estimated_h = reference_height.unwrap_or(img_area.height);
                        let dest_h = ((f32::from(estimated_h) * zoom_factor).ceil() as u32).max(1);
                        cumulative_y += dest_h + u32::from(SEPARATOR_HEIGHT);
                        continue;
                    }

                    let dest_h = ((f32::from(cell_size.height) * zoom_factor).ceil() as u32).max(1);
                    let dest_w = ((f32::from(cell_size.width) * zoom_factor).ceil() as u16).max(1);

                    let page_start = cumulative_y;
                    let page_end = cumulative_y + dest_h;

                    if virtual_y >= page_start && virtual_y < page_end {
                        target_page = Some((idx, dest_w));
                        local_y = virtual_y - page_start;
                        break;
                    }

                    cumulative_y = page_end + u32::from(SEPARATOR_HEIGHT);
                }

                let (idx, dest_w) = target_page?;
                (idx, local_y, dest_w)
            };

            let rendered_page = self.rendered.get(page_idx)?;
            let (px_per_cell_x, px_per_cell_y) = pixels_per_cell(rendered_page);

            let x_in_dest = if dest_w <= img_area.width {
                let x_offset = (img_area.width - dest_w) / 2;
                if rel_col < x_offset || rel_col >= x_offset + dest_w {
                    return None;
                }
                rel_col - x_offset
            } else {
                rel_col + cell_pan_from_left
            };

            let source_x_cells = f32::from(x_in_dest) / zoom_factor;
            let source_y_cells = page_local_y as f32 / zoom_factor;

            let pdf_x = source_x_cells * px_per_cell_x;
            let pdf_y = source_y_cells * px_per_cell_y;

            Some(SelectionPoint {
                term_col,
                term_row,
                page: page_idx,
                pdf_x,
                pdf_y,
            })
        } else {
            let scroll_offset = self.non_kitty_scroll_offset;

            let rendered_page = self.rendered.get(self.page)?;
            let cell_size = rendered_page
                .img
                .as_ref()
                .map(|img| img.cell_dimensions())
                .or(rendered_page.full_cell_size)
                .unwrap_or(CellSize::new(0, 0));
            if cell_size.height == 0 {
                return None;
            }
            let (px_per_cell_x, px_per_cell_y) = pixels_per_cell(rendered_page);

            if rel_col >= cell_size.width {
                return None;
            }

            let virtual_y = u32::from(rel_row) + scroll_offset;

            let pdf_x = f32::from(rel_col) * px_per_cell_x;
            let pdf_y = virtual_y as f32 * px_per_cell_y;

            Some(SelectionPoint {
                term_col,
                term_row,
                page: self.page,
                pdf_x,
                pdf_y,
            })
        }
    }

    fn start_visual_from_selection(&mut self, mode: VisualMode) -> Option<InputAction> {
        let (start, end) = self.selection.get_ordered_bounds()?;
        let start_cursor = self.selection_point_to_cursor(start)?;
        let end_cursor = self.selection_point_to_cursor(end)?;

        self.normal_mode.visual_mode = mode;
        self.normal_mode.visual_anchor = Some(start_cursor);
        self.normal_mode.cursor = end_cursor;

        let viewport_changed = self.ensure_cursor_visible();
        let viewport = if viewport_changed && !self.is_kitty {
            self.current_viewport_update()
        } else {
            None
        };

        let all_line_bounds = self.collect_all_line_bounds();
        Some(InputAction::VisualChanged(
            self.normal_mode.get_visual_rects_multi(&all_line_bounds),
            viewport,
        ))
    }

    fn link_target_at_point(
        &self,
        point: &crate::pdf::SelectionPoint,
    ) -> Option<crate::pdf::LinkTarget> {
        let rendered = self.rendered.get(point.page)?;
        let x = point.pdf_x;
        let y = point.pdf_y;
        for link in &rendered.link_rects {
            if x >= link.x0 as f32
                && x < link.x1 as f32
                && y >= link.y0 as f32
                && y < link.y1 as f32
            {
                return Some(link.target.clone());
            }
        }
        None
    }

    fn link_target_at_cursor(&self) -> Option<crate::pdf::LinkTarget> {
        let cursor = self.get_cursor_rect()?;
        let x = cursor.x.saturating_add(cursor.width / 2) as f32;
        let y = cursor.y.saturating_add(cursor.height / 2) as f32;
        let point = crate::pdf::SelectionPoint {
            term_col: 0,
            term_row: 0,
            page: cursor.page,
            pdf_x: x,
            pdf_y: y,
        };
        self.link_target_at_point(&point)
    }

    fn activate_link_target(&mut self, target: crate::pdf::LinkTarget) -> InputAction {
        match target {
            crate::pdf::LinkTarget::Internal { page } => {
                if page < self.rendered.len() {
                    if page != self.page {
                        let scroll_offset = self
                            .zoom
                            .as_ref()
                            .map(|z| z.global_scroll_offset)
                            .unwrap_or(self.non_kitty_scroll_offset);
                        self.jump_list.push(JumpLocation::Pdf {
                            path: self.name.clone(),
                            page: self.page,
                            scroll_offset,
                        });
                    }
                    self.set_page(page);
                    if self.normal_mode.active {
                        self.normal_mode.deactivate();
                    }
                    InputAction::JumpingToPage {
                        page,
                        viewport: self.current_viewport_update(),
                    }
                } else {
                    self.notify_error(format!("Cannot jump to page {page}; it is out of range"));
                    InputAction::Redraw
                }
            }
            crate::pdf::LinkTarget::External { uri } => InputAction::OpenExternalLink(uri),
        }
    }

    fn copy_selection(&mut self) -> Option<InputAction> {
        use crate::pdf::PageSelectionBounds;

        if !self.selection.has_selection() {
            return None;
        }

        let (start, end) = self.selection.get_ordered_bounds()?;

        let mut bounds = Vec::new();
        for page in start.page..=end.page {
            let (start_x, end_x, min_y, max_y) = if start.page == end.page {
                (
                    start.pdf_x,
                    end.pdf_x,
                    start.pdf_y.min(end.pdf_y),
                    start.pdf_y.max(end.pdf_y),
                )
            } else if page == start.page {
                (start.pdf_x, f32::MAX, start.pdf_y, f32::MAX)
            } else if page == end.page {
                (0.0, end.pdf_x, 0.0, end.pdf_y)
            } else {
                (0.0, f32::MAX, 0.0, f32::MAX)
            };

            bounds.push(PageSelectionBounds {
                page,
                start_x,
                end_x,
                min_y,
                max_y,
            });
        }

        if bounds.is_empty() {
            return None;
        }

        Some(InputAction::CopySelection(crate::pdf::ExtractionRequest {
            bounds,
        }))
    }

    fn compute_selection_rects(&self) -> Vec<crate::pdf::SelectionRect> {
        let Some((start, end)) = self.selection.get_ordered_bounds() else {
            return Vec::new();
        };

        if let Some(rects) = self.compute_selection_rects_precise(start, end) {
            return rects;
        }

        self.compute_selection_rects_fallback(start, end)
    }

    fn compute_selection_rects_precise(
        &self,
        start: crate::pdf::SelectionPoint,
        end: crate::pdf::SelectionPoint,
    ) -> Option<Vec<crate::pdf::SelectionRect>> {
        use crate::pdf::SelectionRect;

        let start_cursor = self.selection_point_to_cursor(start)?;
        let end_cursor = self.selection_point_to_cursor(end)?;

        let (min_page, max_page) = if start_cursor.page <= end_cursor.page {
            (start_cursor.page, end_cursor.page)
        } else {
            (end_cursor.page, start_cursor.page)
        };

        let all_line_bounds = self.collect_all_line_bounds();
        if (min_page..=max_page).any(|page| {
            all_line_bounds
                .get(page)
                .is_none_or(|bounds| bounds.is_empty())
        }) {
            return None;
        }

        let rects = visual_rects_for_range(
            start_cursor,
            end_cursor,
            VisualMode::CharacterWise,
            &all_line_bounds,
        );
        if rects.is_empty() {
            return None;
        }

        Some(
            rects
                .into_iter()
                .map(|rect| SelectionRect {
                    page: rect.page,
                    topleft_x: rect.x,
                    topleft_y: rect.y,
                    bottomright_x: rect.x.saturating_add(rect.width),
                    bottomright_y: rect.y.saturating_add(rect.height),
                })
                .collect(),
        )
    }

    fn compute_selection_rects_fallback(
        &self,
        start: crate::pdf::SelectionPoint,
        end: crate::pdf::SelectionPoint,
    ) -> Vec<crate::pdf::SelectionRect> {
        use crate::pdf::{LineBounds, SelectionRect};

        let Some((_, font_size)) = self.coord_info else {
            return Vec::new();
        };

        let get_page_dims = |page_idx: usize| -> (u32, u32) {
            self.rendered
                .get(page_idx)
                .and_then(|r| r.img.as_ref())
                .map(|img| {
                    let cell_size = img.cell_dimensions();
                    (
                        u32::from(cell_size.width) * u32::from(font_size.0),
                        u32::from(cell_size.height) * u32::from(font_size.1),
                    )
                })
                .unwrap_or((5000, 5000))
        };

        let get_line_bounds = |page_idx: usize| -> &[LineBounds] {
            self.rendered
                .get(page_idx)
                .map(|r| r.line_bounds.as_slice())
                .unwrap_or(&[])
        };

        fn find_line_at_y(bounds: &[LineBounds], y: f32) -> Option<&LineBounds> {
            bounds.iter().find(|lb| y >= lb.y0 && y <= lb.y1)
        }

        let fallback_line_height = || -> f32 {
            let scale_factor = self
                .rendered
                .get(start.page)
                .and_then(|r| r.scale_factor)
                .unwrap_or(1.0);
            14.0 * scale_factor
        };

        let start_bounds = get_line_bounds(start.page);
        let end_bounds = if start.page == end.page {
            start_bounds
        } else {
            get_line_bounds(end.page)
        };

        let start_line = find_line_at_y(start_bounds, start.pdf_y);
        let end_line = find_line_at_y(end_bounds, end.pdf_y);

        let (start_y0, start_y1) = start_line
            .map(|lb| (lb.y0, lb.y1))
            .unwrap_or_else(|| (start.pdf_y, start.pdf_y + fallback_line_height()));
        let (end_y0, end_y1) = end_line
            .map(|lb| (lb.y0, lb.y1))
            .unwrap_or_else(|| (end.pdf_y, end.pdf_y + fallback_line_height()));

        let same_line = start.page == end.page
            && start_line.is_some()
            && end_line.is_some()
            && std::ptr::eq(start_line.unwrap(), end_line.unwrap());

        if same_line {
            vec![SelectionRect {
                page: start.page,
                topleft_x: start.pdf_x.min(end.pdf_x) as u32,
                topleft_y: start_y0 as u32,
                bottomright_x: start.pdf_x.max(end.pdf_x) as u32,
                bottomright_y: start_y1 as u32,
            }]
        } else if start.page == end.page {
            let (page_width, _) = get_page_dims(start.page);
            let mut rects = Vec::new();

            rects.push(SelectionRect {
                page: start.page,
                topleft_x: start.pdf_x as u32,
                topleft_y: start_y0 as u32,
                bottomright_x: page_width,
                bottomright_y: start_y1 as u32,
            });

            let middle_top = start_y1 as u32;
            let middle_bottom = end_y0 as u32;
            if middle_bottom > middle_top {
                rects.push(SelectionRect {
                    page: start.page,
                    topleft_x: 0,
                    topleft_y: middle_top,
                    bottomright_x: page_width,
                    bottomright_y: middle_bottom,
                });
            }

            rects.push(SelectionRect {
                page: start.page,
                topleft_x: 0,
                topleft_y: end_y0 as u32,
                bottomright_x: end.pdf_x as u32,
                bottomright_y: end_y1 as u32,
            });

            rects
        } else {
            let mut rects = Vec::new();
            let (first_width, first_height) = get_page_dims(start.page);

            rects.push(SelectionRect {
                page: start.page,
                topleft_x: start.pdf_x as u32,
                topleft_y: start_y0 as u32,
                bottomright_x: first_width,
                bottomright_y: start_y1 as u32,
            });

            if start_y1 < first_height as f32 {
                rects.push(SelectionRect {
                    page: start.page,
                    topleft_x: 0,
                    topleft_y: start_y1 as u32,
                    bottomright_x: first_width,
                    bottomright_y: first_height,
                });
            }

            for page in (start.page + 1)..end.page {
                let (width, height) = get_page_dims(page);
                rects.push(SelectionRect {
                    page,
                    topleft_x: 0,
                    topleft_y: 0,
                    bottomright_x: width,
                    bottomright_y: height,
                });
            }

            let (last_width, _) = get_page_dims(end.page);

            if end_y0 > 0.0 {
                rects.push(SelectionRect {
                    page: end.page,
                    topleft_x: 0,
                    topleft_y: 0,
                    bottomright_x: last_width,
                    bottomright_y: end_y0 as u32,
                });
            }

            rects.push(SelectionRect {
                page: end.page,
                topleft_x: 0,
                topleft_y: end_y0 as u32,
                bottomright_x: end.pdf_x as u32,
                bottomright_y: end_y1 as u32,
            });

            rects
        }
    }

    fn selection_point_to_cursor(
        &self,
        point: crate::pdf::SelectionPoint,
    ) -> Option<CursorPosition> {
        let line_bounds = self
            .rendered
            .get(point.page)
            .map(|r| r.line_bounds.as_slice())?;
        if line_bounds.is_empty() {
            return None;
        }

        let mut best_line = None;
        let mut best_dist = f32::MAX;
        for (idx, line) in line_bounds.iter().enumerate() {
            if point.pdf_y >= line.y0 && point.pdf_y <= line.y1 {
                best_line = Some(idx);
                break;
            }
            let dist = if point.pdf_y < line.y0 {
                line.y0 - point.pdf_y
            } else {
                point.pdf_y - line.y1
            };
            if dist < best_dist {
                best_dist = dist;
                best_line = Some(idx);
            }
        }

        let line_idx = best_line?;
        let line = line_bounds.get(line_idx)?;
        if line.chars.is_empty() {
            return None;
        }

        let char_idx = line
            .chars
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let dist_a = (a.x - point.pdf_x).abs();
                let dist_b = (b.x - point.pdf_x).abs();
                dist_a
                    .partial_cmp(&dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)?;

        Some(CursorPosition {
            page: point.page,
            line_idx,
            char_idx,
        })
    }

    fn find_word_bounds_at(&self, point: &crate::pdf::SelectionPoint) -> Option<(f32, f32)> {
        use crate::pdf::LineBounds;

        let line_bounds = self
            .rendered
            .get(point.page)
            .map(|r| r.line_bounds.as_slice())?;

        fn find_line_at_y(bounds: &[LineBounds], y: f32) -> Option<&LineBounds> {
            bounds.iter().find(|lb| y >= lb.y0 && y <= lb.y1)
        }

        let line = find_line_at_y(line_bounds, point.pdf_y)?;
        if line.chars.is_empty() {
            return None;
        }

        let char_idx = line
            .chars
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let dist_a = (a.x - point.pdf_x).abs();
                let dist_b = (b.x - point.pdf_x).abs();
                dist_a
                    .partial_cmp(&dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)?;

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-' || c == '\'';

        let mut start_idx = char_idx;
        let mut end_idx = char_idx;

        while start_idx > 0 && is_word_char(line.chars[start_idx - 1].c) {
            start_idx -= 1;
        }

        while end_idx < line.chars.len() - 1 && is_word_char(line.chars[end_idx + 1].c) {
            end_idx += 1;
        }

        let start_x = line.chars[start_idx].x;
        let end_x = if end_idx + 1 < line.chars.len() {
            line.chars[end_idx + 1].x
        } else {
            line.x1
        };

        Some((start_x, end_x))
    }

    fn find_paragraph_bounds_at(
        &self,
        point: &crate::pdf::SelectionPoint,
    ) -> Option<(f32, f32, f32, f32)> {
        use crate::pdf::LineBounds;

        let line_bounds = self
            .rendered
            .get(point.page)
            .map(|r| r.line_bounds.as_slice())?;

        fn find_line_at_y(bounds: &[LineBounds], y: f32) -> Option<&LineBounds> {
            bounds.iter().find(|lb| y >= lb.y0 && y <= lb.y1)
        }

        let clicked_line = find_line_at_y(line_bounds, point.pdf_y)?;
        let block_id = clicked_line.block_id;

        let paragraph_lines: Vec<&LineBounds> = line_bounds
            .iter()
            .filter(|lb| lb.block_id == block_id)
            .collect();

        if paragraph_lines.is_empty() {
            return None;
        }

        let min_x0 = paragraph_lines
            .iter()
            .map(|lb| lb.x0)
            .fold(f32::MAX, f32::min);
        let max_x1 = paragraph_lines
            .iter()
            .map(|lb| lb.x1)
            .fold(f32::MIN, f32::max);
        let min_y0 = paragraph_lines
            .iter()
            .map(|lb| lb.y0)
            .fold(f32::MAX, f32::min);
        let max_y1 = paragraph_lines
            .iter()
            .map(|lb| lb.y1)
            .fold(f32::MIN, f32::max);

        Some((min_x0, max_x1, min_y0, max_y1))
    }

    // Comment functionality

    fn start_comment_input(&mut self) -> Option<InputAction> {
        if !self.comments_enabled {
            self.set_error_hud("Comments are only available in zen mode for PDFs".to_string());
            return Some(InputAction::Redraw);
        }
        if self.comment_input.is_active() {
            return None;
        }

        // Check if there's enough space for the comment textarea
        let right_margin = self.last_render.unused_width / 2;
        if right_margin < MIN_COMMENT_TEXTAREA_WIDTH {
            self.set_error_hud("Not enough space for comment editor. Try zooming out.".to_string());
            return Some(InputAction::Redraw);
        }

        let (target, quoted_text) = if self.normal_mode.is_visual_active() {
            let target = self.comment_target_from_visual();
            let text = self.extract_visual_text();
            (target, text)
        } else if self.selection.has_selection() {
            let target = self.comment_target_from_selection();
            let text = self.extract_selection_text();
            (target, text)
        } else {
            (None, None)
        };

        let target = target?;

        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Type your comment here...");
        textarea.set_placeholder_style(Style::default().fg(self.palette.base_04));

        self.comment_input.textarea = Some(textarea);
        self.comment_input.target = Some(target);
        self.comment_input.edit_mode = Some(CommentEditMode::Creating);
        self.comment_input.quoted_text = quoted_text;

        Some(InputAction::Redraw)
    }

    fn handle_comment_input_key(&mut self, key: KeyEvent) -> Option<InputAction> {
        log::info!(
            "handle_comment_input_key: key={:?}, comments_enabled={}",
            key.code,
            self.comments_enabled
        );
        if !self.comments_enabled {
            log::info!("handle_comment_input_key: comments not enabled, returning None");
            return None;
        }
        let input = map_keys_to_input(key);
        log::info!(
            "handle_comment_input_key: input={:?}",
            input.as_ref().map(|i| &i.key)
        );
        let Some(textarea) = self.comment_input.textarea.as_mut() else {
            log::info!("handle_comment_input_key: no textarea, returning None");
            return None;
        };

        if let Some(input) = input {
            if input.key == TextAreaKey::Esc {
                log::info!("Comment input: Esc pressed, calling save_comment");
                let saved = self.save_comment();
                log::info!(
                    "Comment input: save_comment returned {:?} rects",
                    saved.as_ref().map(|v| v.len())
                );
                self.normal_mode.exit_visual();
                self.selection.clear();
                let cursor_rect = self.get_cursor_rect();
                if let Some(rects) = saved {
                    log::info!(
                        "Comment input: returning CommentSaved with {} rects",
                        rects.len()
                    );
                    return Some(InputAction::CommentSaved { rects, cursor_rect });
                }
                return Some(InputAction::Redraw);
            }

            textarea.input(input);
            return Some(InputAction::Redraw);
        }

        Some(InputAction::Redraw)
    }

    fn save_comment(&mut self) -> Option<Vec<crate::pdf::SelectionRect>> {
        if !self.comments_enabled {
            return None;
        }
        let textarea = self.comment_input.textarea.as_ref()?;
        let comment_text = textarea.lines().join("\n");

        if comment_text.trim().is_empty() {
            self.comment_input.clear();
            return None;
        }

        let target = self.comment_input.target.clone()?;
        let quoted_text = self.comment_input.quoted_text.clone();
        let comments = self.book_comments.as_ref()?;

        if let Ok(mut locked) = comments.lock() {
            use chrono::Utc;

            if let Some(CommentEditMode::Editing { comment_id, .. }) = &self.comment_input.edit_mode
            {
                let _ = locked.update_comment_by_id(comment_id, comment_text);
            } else {
                let comment = Comment::with_quoted_text(
                    self.comments_doc_id.clone(),
                    target,
                    comment_text,
                    Utc::now(),
                    quoted_text,
                );
                let _ = locked.add_comment(comment);
            }
        }

        self.refresh_comment_rects();
        self.comment_input.clear();
        Some(self.comment_rects.clone())
    }

    fn comment_target_from_visual(&self) -> Option<CommentTarget> {
        let all_line_bounds = self.collect_all_line_bounds();
        let rects = self.normal_mode.get_visual_rects_multi(&all_line_bounds);
        self.comment_target_from_visual_rects(rects)
    }

    fn comment_target_from_selection(&self) -> Option<CommentTarget> {
        let rects = self.compute_selection_rects();
        self.comment_target_from_selection_rects(rects)
    }

    fn comment_target_from_visual_rects(
        &self,
        rects: Vec<crate::pdf::VisualRect>,
    ) -> Option<CommentTarget> {
        let mut pdf_rects = Vec::new();
        for rect in rects {
            let scale = self
                .rendered
                .get(rect.page)
                .and_then(|r| r.scale_factor)
                .unwrap_or(1.0);
            let inv_scale = 1.0 / scale;

            pdf_rects.push(PdfSelectionRect {
                page: rect.page,
                topleft_x: (f64::from(rect.x) * f64::from(inv_scale)).round() as u32,
                topleft_y: (f64::from(rect.y) * f64::from(inv_scale)).round() as u32,
                bottomright_x: (f64::from(rect.x.saturating_add(rect.width)) * f64::from(inv_scale))
                    .round() as u32,
                bottomright_y: (f64::from(rect.y.saturating_add(rect.height))
                    * f64::from(inv_scale))
                .round() as u32,
            });
        }
        let page = pdf_rects.iter().map(|r| r.page).min()?;
        Some(CommentTarget::pdf(page, pdf_rects))
    }

    fn comment_target_from_selection_rects(
        &self,
        rects: Vec<crate::pdf::SelectionRect>,
    ) -> Option<CommentTarget> {
        let mut pdf_rects = Vec::new();
        for rect in rects {
            let scale = self
                .rendered
                .get(rect.page)
                .and_then(|r| r.scale_factor)
                .unwrap_or(1.0);
            let inv_scale = 1.0 / scale;

            pdf_rects.push(PdfSelectionRect {
                page: rect.page,
                topleft_x: (f64::from(rect.topleft_x) * f64::from(inv_scale)).round() as u32,
                topleft_y: (f64::from(rect.topleft_y) * f64::from(inv_scale)).round() as u32,
                bottomright_x: (f64::from(rect.bottomright_x) * f64::from(inv_scale)).round()
                    as u32,
                bottomright_y: (f64::from(rect.bottomright_y) * f64::from(inv_scale)).round()
                    as u32,
            });
        }
        let page = pdf_rects.iter().map(|r| r.page).min()?;
        Some(CommentTarget::pdf(page, pdf_rects))
    }

    fn start_comment_nav(&mut self) -> Option<InputAction> {
        if !self.comments_enabled {
            return None;
        }
        let count = self.comment_count_for_page(self.page);
        if count == 0 {
            return None;
        }
        self.comment_nav_active = true;
        self.comment_nav_page = self.page;
        self.comment_nav_index = self.comment_nav_index.min(count.saturating_sub(1));
        self.last_render.rect = Rect::default();
        Some(InputAction::SelectionChanged(
            self.current_comment_selection_rects(),
        ))
    }

    fn stop_comment_nav(&mut self) -> Option<InputAction> {
        if !self.comment_nav_active {
            return None;
        }
        self.comment_nav_active = false;
        self.last_render.rect = Rect::default();
        Some(InputAction::SelectionChanged(vec![]))
    }

    fn move_comment_nav(&mut self, delta: isize) -> Option<InputAction> {
        if !self.comment_nav_active {
            return None;
        }
        if self.comment_nav_page != self.page {
            self.comment_nav_page = self.page;
            self.comment_nav_index = 0;
        }
        let count = self.comment_count_for_page(self.comment_nav_page);
        if count == 0 {
            self.comment_nav_active = false;
            self.last_render.rect = Rect::default();
            return Some(InputAction::Redraw);
        }
        let max_idx = count.saturating_sub(1);
        if delta.is_negative() && self.comment_nav_index == 0 {
            if let Some(prev_page) = (0..self.comment_nav_page)
                .rev()
                .find(|&page| self.comment_count_for_page(page) > 0)
            {
                let prev_comments = self.comments_for_page(prev_page);
                let prev_count = prev_comments.len();
                self.set_page(prev_page);
                self.comment_nav_page = prev_page;
                self.comment_nav_index = prev_count.saturating_sub(1);
                let scale_factor = self
                    .rendered
                    .get(prev_page)
                    .and_then(|r| r.scale_factor)
                    .unwrap_or(1.0);
                let selection_rects = prev_comments
                    .last()
                    .map(|c| Self::comment_selection_rects(c, scale_factor))
                    .unwrap_or_default();
                return Some(InputAction::CommentNavJump {
                    page: prev_page,
                    viewport: self.current_viewport_update(),
                    selection_rects,
                });
            }
            return Some(InputAction::SelectionChanged(
                self.current_comment_selection_rects(),
            ));
        }

        if delta.is_positive() && self.comment_nav_index == max_idx {
            let last_page = self.rendered.len().saturating_sub(1);
            if let Some(next_page) = ((self.comment_nav_page + 1)..=last_page)
                .find(|&page| self.comment_count_for_page(page) > 0)
            {
                let next_comments = self.comments_for_page(next_page);
                self.set_page(next_page);
                self.comment_nav_page = next_page;
                self.comment_nav_index = 0;
                let scale_factor = self
                    .rendered
                    .get(next_page)
                    .and_then(|r| r.scale_factor)
                    .unwrap_or(1.0);
                let selection_rects = next_comments
                    .first()
                    .map(|c| Self::comment_selection_rects(c, scale_factor))
                    .unwrap_or_default();
                return Some(InputAction::CommentNavJump {
                    page: next_page,
                    viewport: self.current_viewport_update(),
                    selection_rects,
                });
            }
            return Some(InputAction::SelectionChanged(
                self.current_comment_selection_rects(),
            ));
        }

        let next = if delta.is_negative() {
            self.comment_nav_index.saturating_sub(delta.unsigned_abs())
        } else {
            self.comment_nav_index.saturating_add(delta as usize)
        }
        .min(max_idx);
        if next != self.comment_nav_index {
            self.comment_nav_index = next;
            self.last_render.rect = Rect::default();
        }
        Some(InputAction::SelectionChanged(
            self.current_comment_selection_rects(),
        ))
    }

    fn delete_current_comment(&mut self) -> Option<InputAction> {
        if !self.comment_nav_active {
            return None;
        }
        let comments = self.comments_for_page(self.comment_nav_page);
        if comments.is_empty() {
            return Some(InputAction::Redraw);
        }
        if self.comment_nav_index >= comments.len() {
            self.comment_nav_index = comments.len().saturating_sub(1);
        }
        let comment_id = comments.get(self.comment_nav_index)?.id.clone();
        let Some(comments_handle) = self.book_comments.as_ref() else {
            return Some(InputAction::Redraw);
        };
        if let Ok(mut locked) = comments_handle.lock() {
            let _ = locked.delete_comment_by_id(&comment_id);
        }
        self.refresh_comment_rects();

        let mut selection_rects = Vec::new();
        let count = self.comment_count_for_page(self.comment_nav_page);
        if count > 0 {
            self.comment_nav_index = self.comment_nav_index.min(count.saturating_sub(1));
            selection_rects = self.current_comment_selection_rects();
        } else {
            // No more comments - exit comment nav mode and clear any selection
            self.comment_nav_active = false;
            self.selection.clear();
        }

        Some(InputAction::CommentDeleted {
            rects: self.comment_rects.clone(),
            selection_rects,
        })
    }

    fn start_comment_edit(&mut self) -> Option<InputAction> {
        if !self.comments_enabled || self.comment_input.is_active() {
            return None;
        }
        if !self.comment_nav_active {
            return None;
        }

        // Check if there's enough space for the comment textarea
        let right_margin = self.last_render.unused_width / 2;
        if right_margin < MIN_COMMENT_TEXTAREA_WIDTH {
            self.set_error_hud("Not enough space for comment editor. Try zooming out.".to_string());
            return Some(InputAction::Redraw);
        }

        let comments = self.comments_for_page(self.comment_nav_page);
        let comment = comments.get(self.comment_nav_index)?;
        let lines = comment
            .content
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        let mut textarea = TextArea::new(lines);
        textarea.set_placeholder_text("Type your comment here...");
        textarea.set_placeholder_style(Style::default().fg(self.palette.base_04));
        self.comment_input.textarea = Some(textarea);
        self.comment_input.target = Some(comment.target.clone());
        self.comment_input.edit_mode = Some(CommentEditMode::Editing {
            comment_id: comment.id.clone(),
        });
        self.last_render.rect = Rect::default();
        Some(InputAction::Redraw)
    }

    fn comment_selection_rects(
        comment: &Comment,
        scale_factor: f32,
    ) -> Vec<crate::pdf::SelectionRect> {
        let CommentTarget::Pdf { rects, .. } = &comment.target else {
            return Vec::new();
        };
        rects
            .iter()
            .map(|rect| crate::pdf::SelectionRect {
                page: rect.page,
                topleft_x: (f64::from(rect.topleft_x) * f64::from(scale_factor)).round() as u32,
                topleft_y: (f64::from(rect.topleft_y) * f64::from(scale_factor)).round() as u32,
                bottomright_x: (f64::from(rect.bottomright_x) * f64::from(scale_factor)).round()
                    as u32,
                bottomright_y: (f64::from(rect.bottomright_y) * f64::from(scale_factor)).round()
                    as u32,
            })
            .collect()
    }

    fn current_comment_selection_rects(&self) -> Vec<crate::pdf::SelectionRect> {
        if !self.comment_nav_active {
            return Vec::new();
        }
        let comments = self.comments_for_page(self.comment_nav_page);
        let Some(comment) = comments.get(self.comment_nav_index) else {
            return Vec::new();
        };
        let scale_factor = self
            .rendered
            .get(self.comment_nav_page)
            .and_then(|r| r.scale_factor)
            .unwrap_or(1.0);
        Self::comment_selection_rects(comment, scale_factor)
    }

    fn comments_for_page(&self, page: usize) -> Vec<Comment> {
        if !self.comments_enabled {
            return Vec::new();
        }
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

    fn comment_count_for_page(&self, page: usize) -> usize {
        if !self.comments_enabled {
            return 0;
        }
        let Some(comments) = self.book_comments.as_ref() else {
            return 0;
        };
        let Ok(locked) = comments.lock() else {
            return 0;
        };
        locked.get_page_comments(&self.comments_doc_id, page).len()
    }

    pub fn refresh_comment_rects(&mut self) {
        if !self.comments_enabled {
            self.comment_rects.clear();
            return;
        }
        self.comment_rects = self.collect_comment_rects_normalized();
    }

    pub fn initial_comment_rects(&mut self) -> Vec<crate::pdf::SelectionRect> {
        // Return comment rects if book_comments exists, regardless of comments_enabled.
        // This allows underlines to be visible in ToC mode while UI interactions are zen-only.
        if self.book_comments.is_none() {
            self.comment_rects.clear();
            return Vec::new();
        }
        let rects = self.collect_comment_rects_normalized();
        self.comment_rects = rects.clone();
        rects
    }

    fn collect_comment_rects_normalized(&self) -> Vec<crate::pdf::SelectionRect> {
        let Some(comments) = self.book_comments.as_ref() else {
            return Vec::new();
        };
        let Ok(locked) = comments.lock() else {
            return Vec::new();
        };

        locked
            .get_doc_comments(&self.comments_doc_id)
            .into_iter()
            .flat_map(|comment| {
                let CommentTarget::Pdf { rects, .. } = &comment.target else {
                    return Vec::new();
                };
                rects
                    .iter()
                    .map(|rect| crate::pdf::SelectionRect {
                        page: rect.page,
                        topleft_x: rect.topleft_x,
                        topleft_y: rect.topleft_y,
                        bottomright_x: rect.bottomright_x,
                        bottomright_y: rect.bottomright_y,
                    })
                    .collect()
            })
            .collect()
    }

    // Normal mode handling

    fn toggle_normal_mode(&mut self) -> Option<InputAction> {
        if self.normal_mode.active {
            self.normal_mode.deactivate();
            Some(InputAction::ExitNormalMode)
        } else {
            // Normal/visual mode requires actual Kitty terminal, not just Kitty protocol
            if self.is_iterm {
                self.set_error_hud("PDF normal mode is not supported in iTerm".to_string());
                return Some(InputAction::Redraw);
            }
            if !self.is_kitty {
                let (page, line_bounds) = {
                    let current_bounds = self
                        .rendered
                        .get(self.page)
                        .map(|r| r.line_bounds.as_slice())
                        .unwrap_or(&[]);

                    if !current_bounds.is_empty() {
                        (self.page, current_bounds)
                    } else if let Some(last_pos) = self.normal_mode.get_last_position() {
                        let last_bounds = self
                            .rendered
                            .get(last_pos.page)
                            .map(|r| r.line_bounds.as_slice())
                            .unwrap_or(&[]);
                        if !last_bounds.is_empty() {
                            (last_pos.page, last_bounds)
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                };

                if let Some(last_pos) = self.normal_mode.get_last_position() {
                    if last_pos.page == page && last_pos.line_idx < line_bounds.len() {
                        self.normal_mode
                            .activate_at(page, last_pos.line_idx, last_pos.char_idx);
                        let cursor_rect = self.normal_mode.get_cursor_rect(line_bounds);
                        let viewport = self.current_viewport_update();
                        return Some(InputAction::CursorChanged(cursor_rect, viewport));
                    }
                }

                let target_line = if !self.is_kitty {
                    self.find_first_visible_line_non_kitty(page, line_bounds)
                        .map(|idx| (idx + 2).min(line_bounds.len().saturating_sub(1)))
                        .unwrap_or(0)
                } else {
                    2.min(line_bounds.len().saturating_sub(1))
                };

                self.normal_mode.activate_at(page, target_line, 0);
                let cursor_rect = self.normal_mode.get_cursor_rect(line_bounds);
                let viewport = self.current_viewport_update();
                return Some(InputAction::CursorChanged(cursor_rect, viewport));
            }

            if let Some(last_pos) = self.normal_mode.get_last_position() {
                if self.is_position_visible(last_pos.page, last_pos.line_idx) {
                    let line_bounds = self
                        .rendered
                        .get(last_pos.page)
                        .map(|r| r.line_bounds.as_slice())
                        .unwrap_or(&[]);

                    self.normal_mode.activate_at(
                        last_pos.page,
                        last_pos.line_idx,
                        last_pos.char_idx,
                    );
                    let cursor_rect = self.normal_mode.get_cursor_rect(line_bounds);
                    return Some(InputAction::CursorChanged(cursor_rect, None));
                }
            }

            if let Some((page, line_idx)) = self.find_first_visible_line() {
                let line_bounds = self
                    .rendered
                    .get(page)
                    .map(|r| r.line_bounds.as_slice())
                    .unwrap_or(&[]);

                let target_line = (line_idx + 2).min(line_bounds.len().saturating_sub(1));

                self.normal_mode.activate_at(page, target_line, 0);
                let cursor_rect = self.normal_mode.get_cursor_rect(line_bounds);
                Some(InputAction::CursorChanged(cursor_rect, None))
            } else {
                // Fallback: activate at current page even if line_bounds not yet available
                // This handles the case when page is still rendering
                let page = self.page;
                let line_bounds = self
                    .rendered
                    .get(page)
                    .map(|r| r.line_bounds.as_slice())
                    .unwrap_or(&[]);

                self.normal_mode.activate_at(page, 0, 0);
                let cursor_rect = self.normal_mode.get_cursor_rect(line_bounds);
                Some(InputAction::CursorChanged(cursor_rect, None))
            }
        }
    }

    fn find_first_visible_line_non_kitty(
        &self,
        _page: usize,
        line_bounds: &[crate::pdf::LineBounds],
    ) -> Option<usize> {
        if self.is_kitty {
            return Some(0);
        }

        let (_, font_size) = self.coord_info?;
        let scroll_offset_px = self.non_kitty_scroll_offset * u32::from(font_size.1);

        for (idx, line) in line_bounds.iter().enumerate() {
            if line.y1 as u32 > scroll_offset_px {
                return Some(idx);
            }
        }

        if line_bounds.is_empty() {
            None
        } else {
            Some(line_bounds.len() - 1)
        }
    }

    fn is_position_visible(&self, page: usize, line_idx: usize) -> bool {
        let Some(zoom) = self.zoom.as_ref() else {
            return false;
        };
        let Some((img_area, font_size)) = self.coord_info else {
            return false;
        };
        let Some(rendered_page) = self.rendered.get(page) else {
            return false;
        };
        let Some(line) = rendered_page.line_bounds.get(line_idx) else {
            return false;
        };

        let zoom_factor = zoom.factor();
        let scroll_offset = zoom.global_scroll_offset;
        let viewport_height = u32::from(img_area.height);
        let is_page_mode = get_pdf_render_mode() == PdfRenderMode::Page;

        // In page mode, position is only visible if on current page
        if is_page_mode && page != self.page {
            return false;
        }

        let cumulative_y: u32 = if is_page_mode {
            // In page mode, no cumulative calculation - scroll is within current page
            0
        } else {
            // Scroll mode: calculate cumulative height up to target page
            let estimated_h = self.estimated_page_height_cells();
            let mut cumulative = 0u32;
            for (idx, r) in self.rendered.iter().enumerate() {
                if idx >= page {
                    break;
                }
                let cell_height = r
                    .img
                    .as_ref()
                    .map(|img| img.cell_dimensions().height)
                    .unwrap_or(estimated_h);
                let dest_h = ((f32::from(cell_height) * zoom_factor).ceil() as u32).max(1);
                cumulative += dest_h + u32::from(SEPARATOR_HEIGHT);
            }
            cumulative
        };

        let line_top_cells = ((line.y0 / f32::from(font_size.1)) * zoom_factor).floor() as u32;
        let line_bottom_cells = ((line.y1 / f32::from(font_size.1)) * zoom_factor).ceil() as u32;

        let global_line_top = cumulative_y + line_top_cells;
        let global_line_bottom = cumulative_y + line_bottom_cells;

        let viewport_top = scroll_offset;
        let viewport_bottom = scroll_offset + viewport_height;

        global_line_bottom > viewport_top && global_line_top < viewport_bottom
    }

    fn find_first_visible_line(&self) -> Option<(usize, usize)> {
        let zoom = self.zoom.as_ref()?;
        let (_, font_size) = self.coord_info?;

        let zoom_factor = zoom.factor();
        let scroll_offset = zoom.global_scroll_offset;
        let is_page_mode = get_pdf_render_mode() == PdfRenderMode::Page;

        if is_page_mode {
            // In page mode, only look at current page
            let rendered_page = self.rendered.get(self.page)?;
            for (line_idx, line) in rendered_page.line_bounds.iter().enumerate() {
                let line_top_cells =
                    ((line.y0 / f32::from(font_size.1)) * zoom_factor).floor() as u32;

                if line_top_cells >= scroll_offset {
                    return Some((self.page, line_idx));
                }

                let line_bottom_cells =
                    ((line.y1 / f32::from(font_size.1)) * zoom_factor).ceil() as u32;

                if line_bottom_cells > scroll_offset {
                    return Some((self.page, line_idx));
                }
            }
            None
        } else {
            // Scroll mode: iterate through all pages with separators
            let mut cumulative_y: u32 = 0;
            let estimated_h = self.estimated_page_height_cells();

            for (page_idx, rendered_page) in self.rendered.iter().enumerate() {
                let cell_height = rendered_page
                    .img
                    .as_ref()
                    .map(|img| img.cell_dimensions().height)
                    .unwrap_or(estimated_h);

                let dest_h = ((f32::from(cell_height) * zoom_factor).ceil() as u32).max(1);
                let page_start = cumulative_y;
                let page_end = cumulative_y + dest_h;

                if page_end > scroll_offset {
                    for (line_idx, line) in rendered_page.line_bounds.iter().enumerate() {
                        let line_top_cells =
                            ((line.y0 / f32::from(font_size.1)) * zoom_factor).floor() as u32;
                        let global_line_top = page_start + line_top_cells;

                        if global_line_top >= scroll_offset {
                            return Some((page_idx, line_idx));
                        }

                        let line_bottom_cells =
                            ((line.y1 / f32::from(font_size.1)) * zoom_factor).ceil() as u32;
                        let global_line_bottom = page_start + line_bottom_cells;

                        if global_line_bottom > scroll_offset {
                            return Some((page_idx, line_idx));
                        }
                    }
                }

                cumulative_y = page_end + u32::from(SEPARATOR_HEIGHT);
            }

            None
        }
    }

    fn handle_normal_mode_key(&mut self, c: char) -> Option<InputAction> {
        log::info!(
            "handle_normal_mode_key: c='{}', active={}",
            c,
            self.normal_mode.active
        );
        if !self.normal_mode.active {
            return None;
        }

        let line_bounds = self
            .rendered
            .get(self.normal_mode.cursor.page)
            .map(|r| r.line_bounds.clone())
            .unwrap_or_default();

        let cursor_moved_action = |this: &mut Self| {
            let viewport_changed = this.ensure_cursor_visible();
            let viewport = if viewport_changed && !this.is_kitty {
                this.current_viewport_update()
            } else {
                None
            };

            if this.normal_mode.is_visual_active() {
                let all_line_bounds = this.collect_all_line_bounds();
                InputAction::VisualChanged(
                    this.normal_mode.get_visual_rects_multi(&all_line_bounds),
                    viewport,
                )
            } else {
                InputAction::CursorChanged(this.get_cursor_rect(), viewport)
            }
        };

        if self.normal_mode.has_pending_char_motion() {
            let found = match self.normal_mode.pending_motion {
                PendingMotion::FindForward => self.normal_mode.find_char_forward(c, &line_bounds),
                PendingMotion::FindBackward => self.normal_mode.find_char_backward(c, &line_bounds),
                PendingMotion::TillForward => self.normal_mode.till_char_forward(c, &line_bounds),
                PendingMotion::TillBackward => self.normal_mode.till_char_backward(c, &line_bounds),
                PendingMotion::None => false,
            };
            self.normal_mode.pending_motion = PendingMotion::None;
            if found {
                return Some(cursor_moved_action(self));
            }
            return Some(InputAction::Redraw);
        }

        if self.normal_mode.pending_g {
            self.normal_mode.pending_g = false;
            if c == 'g' {
                self.normal_mode.move_page_top();
                let viewport = self.scroll_to_page_top_with_viewport();
                if self.normal_mode.is_visual_active() {
                    let all_line_bounds = self.collect_all_line_bounds();
                    let visual_rects = self.normal_mode.get_visual_rects_multi(&all_line_bounds);
                    return Some(InputAction::VisualChanged(visual_rects, viewport));
                }
                return Some(InputAction::CursorChanged(self.get_cursor_rect(), viewport));
            }
            return Some(InputAction::Redraw);
        }

        match c {
            'h' => {
                self.normal_mode.move_left(&line_bounds);
                Some(cursor_moved_action(self))
            }
            'j' => {
                let result = self.normal_mode.move_down(&line_bounds);
                if result == MoveResult::WantsNextPage {
                    let next_page = self.normal_mode.cursor.page + 1;
                    if next_page < self.rendered.len() {
                        let next_line_bounds = self
                            .rendered
                            .get(next_page)
                            .map(|r| r.line_bounds.clone())
                            .unwrap_or_default();
                        if !next_line_bounds.is_empty() {
                            self.normal_mode
                                .move_to_page_start(next_page, &next_line_bounds);
                        }
                    }
                }
                Some(cursor_moved_action(self))
            }
            'k' => {
                let result = self.normal_mode.move_up(&line_bounds);
                if result == MoveResult::WantsPrevPage && self.normal_mode.cursor.page > 0 {
                    let prev_page = self.normal_mode.cursor.page - 1;
                    let prev_line_bounds = self
                        .rendered
                        .get(prev_page)
                        .map(|r| r.line_bounds.clone())
                        .unwrap_or_default();
                    if !prev_line_bounds.is_empty() {
                        self.normal_mode
                            .move_to_page_end(prev_page, &prev_line_bounds);
                    }
                }
                Some(cursor_moved_action(self))
            }
            'l' => {
                self.normal_mode.move_right(&line_bounds);
                Some(cursor_moved_action(self))
            }
            'w' => {
                self.normal_mode.move_word_forward(&line_bounds);
                Some(cursor_moved_action(self))
            }
            'b' => {
                self.normal_mode.move_word_backward(&line_bounds);
                Some(cursor_moved_action(self))
            }
            'e' => {
                // move_word_end not available, use move_word_forward as fallback
                self.normal_mode.move_word_forward(&line_bounds);
                Some(cursor_moved_action(self))
            }
            '0' => {
                self.normal_mode.move_line_start();
                Some(cursor_moved_action(self))
            }
            '$' => {
                self.normal_mode.move_line_end(&line_bounds);
                Some(cursor_moved_action(self))
            }
            '^' => {
                self.normal_mode.move_first_non_whitespace(&line_bounds);
                Some(cursor_moved_action(self))
            }
            'g' => {
                self.normal_mode.pending_g = true;
                Some(InputAction::Redraw)
            }
            'G' => {
                self.normal_mode.move_page_bottom(&line_bounds);
                let viewport = self.scroll_to_page_bottom_with_viewport();
                if self.normal_mode.is_visual_active() {
                    let all_line_bounds = self.collect_all_line_bounds();
                    let visual_rects = self.normal_mode.get_visual_rects_multi(&all_line_bounds);
                    Some(InputAction::VisualChanged(visual_rects, viewport))
                } else {
                    Some(InputAction::CursorChanged(self.get_cursor_rect(), viewport))
                }
            }
            'f' => {
                self.normal_mode.pending_motion = PendingMotion::FindForward;
                Some(InputAction::Redraw)
            }
            'F' => {
                self.normal_mode.pending_motion = PendingMotion::FindBackward;
                Some(InputAction::Redraw)
            }
            't' => {
                self.normal_mode.pending_motion = PendingMotion::TillForward;
                Some(InputAction::Redraw)
            }
            'T' => {
                self.normal_mode.pending_motion = PendingMotion::TillBackward;
                Some(InputAction::Redraw)
            }
            ';' => {
                self.normal_mode.repeat_find(&line_bounds);
                Some(cursor_moved_action(self))
            }
            'v' => {
                self.normal_mode.toggle_visual_char();
                let all_line_bounds = self.collect_all_line_bounds();
                let visual_rects = self.normal_mode.get_visual_rects_multi(&all_line_bounds);
                Some(InputAction::VisualChanged(visual_rects, None))
            }
            'V' => {
                self.normal_mode.toggle_visual_line();
                let all_line_bounds = self.collect_all_line_bounds();
                let visual_rects = self.normal_mode.get_visual_rects_multi(&all_line_bounds);
                Some(InputAction::VisualChanged(visual_rects, None))
            }
            'a' if self.normal_mode.is_visual_active() || self.selection.has_selection() => {
                self.start_comment_input()
            }
            'y' if self.normal_mode.is_visual_active() => {
                let text = self.extract_visual_text();
                let start_pos = self.normal_mode.get_visual_range().map(|(start, _)| start);
                if let Some(start) = start_pos {
                    self.normal_mode.cursor = start;
                }
                self.normal_mode.exit_visual();
                let cursor_rect = self.get_cursor_rect();
                if let Some(text) = text {
                    Some(InputAction::YankText(text, cursor_rect))
                } else {
                    Some(InputAction::ExitVisualMode(cursor_rect))
                }
            }
            '=' | '+' => {
                if self.is_kitty {
                    self.update_zoom_keep_page(Zoom::step_in)
                } else {
                    self.non_kitty_zoom_factor =
                        Zoom::clamp_factor(self.non_kitty_zoom_factor * Zoom::ZOOM_IN_RATE);
                    self.set_zoom_hud(self.non_kitty_zoom_factor);
                    self.clear_pending_scroll();
                    self.make_render_scale_action(self.non_kitty_zoom_factor)
                }
            }
            '-' | '_' => {
                if self.is_kitty {
                    self.update_zoom_keep_page(Zoom::step_out)
                } else {
                    self.non_kitty_zoom_factor =
                        Zoom::clamp_factor(self.non_kitty_zoom_factor / Zoom::ZOOM_OUT_RATE);
                    self.set_zoom_hud(self.non_kitty_zoom_factor);
                    self.clear_pending_scroll();
                    self.make_render_scale_action(self.non_kitty_zoom_factor)
                }
            }
            '/' => self.start_page_search(),
            _ => None,
        }
    }

    fn ensure_cursor_visible(&mut self) -> bool {
        if !self.is_kitty {
            return self.ensure_cursor_visible_non_kitty();
        }

        let is_page_mode = get_pdf_render_mode() == PdfRenderMode::Page;
        let estimated_h = self.estimated_page_height_cells();
        let Some(zoom) = self.zoom.as_mut() else {
            return false;
        };
        let Some((img_area, font_size)) = self.coord_info else {
            return false;
        };

        let cursor = &self.normal_mode.cursor;
        let Some(rendered_page) = self.rendered.get(cursor.page) else {
            return false;
        };
        let Some(line) = rendered_page.line_bounds.get(cursor.line_idx) else {
            return false;
        };

        let zoom_factor = zoom.factor();

        // In page mode, cursor must be on current page - if not, we need to navigate
        if is_page_mode && cursor.page != self.page {
            // Can't scroll to different page in page mode - would need page navigation
            return false;
        }

        let cumulative_y: u32 = if is_page_mode {
            // In page mode, no cumulative calculation
            0
        } else {
            // Scroll mode: calculate cumulative height
            let mut cumulative = 0u32;
            for (idx, r) in self.rendered.iter().enumerate() {
                if idx >= cursor.page {
                    break;
                }
                let cell_height = r
                    .img
                    .as_ref()
                    .map(|img| img.cell_dimensions().height)
                    .unwrap_or(estimated_h);
                let dest_h = ((f32::from(cell_height) * zoom_factor).ceil() as u32).max(1);
                cumulative += dest_h + u32::from(SEPARATOR_HEIGHT);
            }
            cumulative
        };

        let cell_height = rendered_page
            .img
            .as_ref()
            .map(|img| img.cell_dimensions().height)
            .unwrap_or(0);
        if cell_height == 0 {
            return false;
        }

        let line_top_pixels = line.y0;
        let line_bottom_pixels = line.y1;
        let line_top_cells =
            ((line_top_pixels / f32::from(font_size.1)) * zoom_factor).floor() as u32;
        let line_bottom_cells =
            ((line_bottom_pixels / f32::from(font_size.1)) * zoom_factor).ceil() as u32;

        let cursor_top = cumulative_y + line_top_cells;
        let cursor_bottom = cumulative_y + line_bottom_cells;

        let viewport_top = zoom.global_scroll_offset;
        let viewport_bottom = viewport_top + u32::from(img_area.height);

        if cursor_top < viewport_top {
            zoom.global_scroll_offset = cursor_top;
            true
        } else if cursor_bottom > viewport_bottom {
            zoom.global_scroll_offset = cursor_bottom.saturating_sub(u32::from(img_area.height));
            true
        } else {
            false
        }
    }

    fn ensure_cursor_visible_non_kitty(&mut self) -> bool {
        let cursor = &self.normal_mode.cursor;

        if cursor.page != self.page {
            self.page = cursor.page;
            self.non_kitty_scroll_offset = 0;
            self.last_render.rect = Rect::default();
            return true;
        }

        let Some(rendered_page) = self.rendered.get(cursor.page) else {
            return false;
        };
        let Some(line) = rendered_page.line_bounds.get(cursor.line_idx) else {
            return false;
        };
        let Some((_, font_size)) = self.coord_info else {
            return false;
        };

        let line_top_pixels = line.y0;
        let line_bottom_pixels = line.y1;
        let cursor_top_cells = (line_top_pixels / f32::from(font_size.1)).floor() as u32;
        let cursor_bottom_cells = (line_bottom_pixels / f32::from(font_size.1)).ceil() as u32;

        let viewport_top = self.non_kitty_scroll_offset;
        let viewport_height = u32::from(self.last_render.img_area_height);
        if viewport_height == 0 {
            return false;
        }
        let viewport_bottom = viewport_top + viewport_height;

        let old_offset = self.non_kitty_scroll_offset;

        if cursor_top_cells < viewport_top {
            self.non_kitty_scroll_offset = cursor_top_cells;
        } else if cursor_bottom_cells > viewport_bottom {
            self.non_kitty_scroll_offset = cursor_bottom_cells.saturating_sub(viewport_height);
        }

        if self.non_kitty_scroll_offset != old_offset {
            self.last_render.rect = Rect::default();
            true
        } else {
            false
        }
    }

    pub fn get_cursor_rect(&self) -> Option<crate::pdf::CursorRect> {
        if !self.normal_mode.active {
            return None;
        }
        let line_bounds = self
            .rendered
            .get(self.normal_mode.cursor.page)
            .map(|r| r.line_bounds.as_slice())
            .unwrap_or(&[]);
        self.normal_mode.get_cursor_rect(line_bounds)
    }

    fn collect_all_line_bounds(&self) -> Vec<Vec<crate::pdf::LineBounds>> {
        self.rendered
            .iter()
            .map(|r| r.line_bounds.clone())
            .collect()
    }

    pub fn notify_error(&mut self, msg: impl Into<String>) {
        self.notifications.error(msg);
    }

    pub fn notify_info(&mut self, msg: impl Into<String>) {
        self.notifications.info(msg);
    }

    fn extract_visual_text(&self) -> Option<String> {
        let (start, end) = self.normal_mode.get_visual_range()?;
        let all_line_bounds = self.collect_all_line_bounds();
        let is_line_wise = self.normal_mode.visual_mode == crate::pdf::VisualMode::LineWise;

        let mut result = String::new();

        for page in start.page..=end.page {
            let Some(page_bounds) = all_line_bounds.get(page) else {
                continue;
            };

            let start_line = if page == start.page {
                start.line_idx
            } else {
                0
            };
            let end_line = if page == end.page {
                end.line_idx
            } else {
                page_bounds.len().saturating_sub(1)
            };

            for line_idx in start_line..=end_line {
                let Some(line) = page_bounds.get(line_idx) else {
                    continue;
                };

                let (char_start, char_end) = if is_line_wise {
                    (0, line.chars.len())
                } else {
                    let cs = if page == start.page && line_idx == start.line_idx {
                        start.char_idx
                    } else {
                        0
                    };
                    let ce = if page == end.page && line_idx == end.line_idx {
                        (end.char_idx + 1).min(line.chars.len())
                    } else {
                        line.chars.len()
                    };
                    (cs, ce)
                };

                for char_info in line
                    .chars
                    .iter()
                    .skip(char_start)
                    .take(char_end - char_start)
                {
                    result.push(char_info.c);
                }

                if line_idx < end_line || page < end.page {
                    result.push('\n');
                }
            }

            if page < end.page {
                result.push('\n');
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    fn extract_selection_text(&self) -> Option<String> {
        let (start_pt, end_pt) = self.selection.get_ordered_bounds()?;
        let start = self.selection_point_to_cursor(start_pt)?;
        let end = self.selection_point_to_cursor(end_pt)?;

        let all_line_bounds = self.collect_all_line_bounds();

        let mut result = String::new();

        for page in start.page..=end.page {
            let Some(page_bounds) = all_line_bounds.get(page) else {
                continue;
            };

            let start_line = if page == start.page {
                start.line_idx
            } else {
                0
            };
            let end_line = if page == end.page {
                end.line_idx
            } else {
                page_bounds.len().saturating_sub(1)
            };

            for line_idx in start_line..=end_line {
                let Some(line) = page_bounds.get(line_idx) else {
                    continue;
                };

                let cs = if page == start.page && line_idx == start.line_idx {
                    start.char_idx
                } else {
                    0
                };
                let ce = if page == end.page && line_idx == end.line_idx {
                    (end.char_idx + 1).min(line.chars.len())
                } else {
                    line.chars.len()
                };

                for char_info in line.chars.iter().skip(cs).take(ce - cs) {
                    result.push(char_info.c);
                }

                if line_idx < end_line || page < end.page {
                    result.push('\n');
                }
            }

            if page < end.page {
                result.push('\n');
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    // Page search methods (vim-style / search in normal mode)

    fn start_page_search(&mut self) -> Option<InputAction> {
        use tui_textarea::TextArea;

        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        textarea.set_block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::NONE)
                .padding(ratatui::widgets::Padding::horizontal(0)),
        );
        if let Some(query) = self.page_search.query.as_ref() {
            textarea.insert_str(query);
        }
        if let Some((row, col)) = self.page_search.input_cursor {
            textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
        }

        self.page_search.input = Some(textarea);
        Some(InputAction::Redraw)
    }

    fn handle_page_search_input_key(&mut self, key: KeyEvent) -> Option<InputAction> {
        let textarea = self.page_search.input.as_mut()?;

        match key.code {
            KeyCode::Enter => {
                self.page_search.input_cursor = Some(textarea.cursor());
                let query: String = textarea.lines().join("");
                self.page_search.clear_input();

                if query.is_empty() {
                    self.page_search.clear_search();
                    return Some(InputAction::SelectionChanged(vec![]));
                }

                self.execute_page_search(&query);
                self.page_search.query = Some(query);

                if self.page_search.has_matches() {
                    self.jump_to_current_search_match()
                } else {
                    self.set_error_hud("Pattern not found".to_string());
                    Some(InputAction::SelectionChanged(vec![]))
                }
            }
            KeyCode::Esc => {
                self.page_search.input_cursor = Some(textarea.cursor());
                self.page_search.clear_input();
                Some(InputAction::SelectionChanged(vec![]))
            }
            _ => {
                if let Some(input) = map_keys_to_input(key) {
                    textarea.input(input);
                }
                // Live search highlighting while typing
                let query: String = self
                    .page_search
                    .input
                    .as_ref()
                    .map(|ta| ta.lines().join(""))
                    .unwrap_or_default();
                if query.is_empty() {
                    Some(InputAction::SelectionChanged(vec![]))
                } else {
                    let page = self.normal_mode.cursor.page;
                    let rects = self.find_text_selection_rects(page, &query);
                    Some(InputAction::SelectionChanged(rects))
                }
            }
        }
    }

    fn execute_page_search(&mut self, query: &str) {
        use super::state::PageSearchMatch;

        let page = self.normal_mode.cursor.page;
        let Some(rendered) = self.rendered.get(page) else {
            self.page_search.matches.clear();
            self.page_search.matches_page = page;
            return;
        };

        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        for (line_idx, line) in rendered.line_bounds.iter().enumerate() {
            let line_text: String = line.chars.iter().map(|c| c.c).collect();
            let line_lower = line_text.to_lowercase();

            let mut search_start = 0;
            while let Some(match_start) = line_lower[search_start..].find(&query_lower) {
                let abs_start = search_start + match_start;
                let char_len = query.chars().count();

                if abs_start < line.chars.len() {
                    matches.push(PageSearchMatch {
                        line_idx,
                        char_idx: abs_start,
                        length: char_len,
                    });
                }

                search_start = abs_start + 1;
            }
        }

        self.page_search.matches = matches;
        self.page_search.matches_page = page;
        self.page_search.current_match = 0;

        self.find_closest_match_to_cursor();
    }

    fn find_closest_match_to_cursor(&mut self) {
        if self.page_search.matches.is_empty() {
            return;
        }

        let cursor_line = self.normal_mode.cursor.line_idx;
        let cursor_char = self.normal_mode.cursor.char_idx;

        for (idx, m) in self.page_search.matches.iter().enumerate() {
            if m.line_idx > cursor_line || (m.line_idx == cursor_line && m.char_idx >= cursor_char)
            {
                self.page_search.current_match = idx;
                return;
            }
        }

        self.page_search.current_match = 0;
    }

    fn jump_to_current_search_match(&mut self) -> Option<InputAction> {
        let m = self.page_search.current()?.clone();

        self.normal_mode.cursor.line_idx = m.line_idx;
        self.normal_mode.cursor.char_idx = m.char_idx;

        let viewport_changed = self.ensure_cursor_visible();
        let viewport = if viewport_changed && !self.is_kitty {
            self.current_viewport_update()
        } else {
            None
        };

        let match_count = self.page_search.matches.len();
        let current_idx = self.page_search.current_match + 1;
        self.set_hud_message(
            format!("[{current_idx}/{match_count}]"),
            crate::widget::hud_message::HudMode::Normal,
            std::time::Duration::from_secs(2),
        );

        if self.normal_mode.is_visual_active() {
            let all_line_bounds = self.collect_all_line_bounds();
            Some(InputAction::VisualChanged(
                self.normal_mode.get_visual_rects_multi(&all_line_bounds),
                viewport,
            ))
        } else {
            Some(InputAction::CursorChanged(self.get_cursor_rect(), viewport))
        }
    }

    fn jump_to_next_search_match(&mut self) -> Option<InputAction> {
        if !self.page_search.has_matches() {
            if let Some(query) = self.page_search.query.clone() {
                if self.page_search.matches_page != self.normal_mode.cursor.page {
                    self.execute_page_search(&query);
                }
            }
            if !self.page_search.has_matches() {
                self.set_error_hud("No search matches".to_string());
                return Some(InputAction::Redraw);
            }
        }

        self.page_search.next_match();
        self.jump_to_current_search_match()
    }

    fn jump_to_prev_search_match(&mut self) -> Option<InputAction> {
        if !self.page_search.has_matches() {
            if let Some(query) = self.page_search.query.clone() {
                if self.page_search.matches_page != self.normal_mode.cursor.page {
                    self.execute_page_search(&query);
                }
            }
            if !self.page_search.has_matches() {
                self.set_error_hud("No search matches".to_string());
                return Some(InputAction::Redraw);
            }
        }

        self.page_search.prev_match();
        self.jump_to_current_search_match()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOutcome {
    None,
    Quit,
}

impl PdfReaderState {
    #[allow(clippy::too_many_arguments)]
    pub fn apply_input_action(
        &mut self,
        action: InputAction,
        service: Option<&mut crate::pdf::RenderService>,
        conversion_tx: Option<&flume::Sender<crate::pdf::ConversionCommand>>,
        notifications: &mut crate::notification::NotificationManager,
        bookmarks: &mut Bookmarks,
        last_bookmark_save: &mut std::time::Instant,
        table_of_contents: &mut TableOfContents,
        toc_height: usize,
        profiler: &std::sync::Arc<std::sync::Mutex<Option<pprof::ProfilerGuard<'static>>>>,
    ) -> InputOutcome {
        let send_conversion = |cmd: crate::pdf::ConversionCommand| {
            if let Some(tx) = conversion_tx {
                let _ = tx.send(cmd);
            }
        };

        match action {
            InputAction::QuitApp => {
                save_pdf_bookmark(bookmarks, self, last_bookmark_save, true);
                return InputOutcome::Quit;
            }
            InputAction::Redraw => {}
            InputAction::JumpingToPage { page, viewport } => {
                if let Some(service) = service {
                    service.apply_command(crate::pdf::Command::GoToPage(page));
                }
                send_conversion(crate::pdf::ConversionCommand::NavigateTo(page));
                update_pdf_toc_active(table_of_contents, self, page, toc_height);
                if let Some(viewport) = viewport
                    && !self.is_kitty
                {
                    send_conversion(crate::pdf::ConversionCommand::UpdateViewport(viewport));
                }
                if !self.normal_mode.active {
                    send_conversion(crate::pdf::ConversionCommand::UpdateCursor(None));
                }
                save_pdf_bookmark(bookmarks, self, last_bookmark_save, false);
            }
            InputAction::ViewportChanged(viewport) => {
                if !self.is_kitty {
                    send_conversion(crate::pdf::ConversionCommand::UpdateViewport(viewport));
                }
                save_pdf_bookmark(bookmarks, self, last_bookmark_save, false);
            }
            InputAction::RenderScale { factor, .. } => {
                for rendered_info in &mut self.rendered {
                    rendered_info.img = None;
                }
                if let Some(service) = service {
                    service.apply_command(crate::pdf::Command::SetScale(factor));
                    service.request_page(self.page);
                }
                send_conversion(crate::pdf::ConversionCommand::InvalidatePageCache);
            }
            InputAction::ThemeChanged { black, white } => {
                for rendered_info in &mut self.rendered {
                    rendered_info.img = None;
                }
                if let Some(service) = service {
                    service.apply_command(crate::pdf::Command::SetColors { black, white });
                    service.request_page(self.page);
                }
                send_conversion(crate::pdf::ConversionCommand::InvalidatePageCache);
            }
            InputAction::OpenExternalLink(url) => {
                open_url(&url);
            }
            InputAction::YankText(text, cursor_rect) => {
                if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(&text)) {
                    log::error!("Failed to copy to clipboard: {e}");
                } else {
                    notifications.info(format!("Copied {} chars", text.len()));
                }
                if !self.is_kitty {
                    self.force_redraw();
                }
                send_conversion(crate::pdf::ConversionCommand::UpdateCursor(cursor_rect));
                send_conversion(crate::pdf::ConversionCommand::UpdateVisual(vec![]));
            }
            InputAction::CopySelection(_request) => {}
            InputAction::ToggleProfiling => {
                toggle_profiling(profiler);
            }
            InputAction::ToggleInvertImages => {
                for rendered_info in &mut self.rendered {
                    rendered_info.img = None;
                }
                if let Some(service) = service {
                    service.apply_command(crate::pdf::Command::ToggleInvertImages);
                    service.request_page(self.page);
                }
                send_conversion(crate::pdf::ConversionCommand::InvalidatePageCache);
            }
            InputAction::SelectionChanged(rects) => {
                if !self.is_kitty {
                    self.force_redraw();
                }
                send_conversion(crate::pdf::ConversionCommand::UpdateSelection(rects));
            }
            InputAction::CursorChanged(cursor, viewport) => {
                if !self.is_kitty {
                    self.force_redraw();
                }
                send_conversion(crate::pdf::ConversionCommand::UpdateCursor(cursor));
                if let Some(viewport) = viewport {
                    send_conversion(crate::pdf::ConversionCommand::UpdateViewport(viewport));
                }
            }
            InputAction::VisualChanged(rects, viewport) => {
                if !self.is_kitty {
                    self.force_redraw();
                }
                send_conversion(crate::pdf::ConversionCommand::UpdateVisual(rects));
                if let Some(viewport) = viewport {
                    send_conversion(crate::pdf::ConversionCommand::UpdateViewport(viewport));
                }
            }
            InputAction::ExitNormalMode => {
                send_conversion(crate::pdf::ConversionCommand::UpdateCursor(None));
                send_conversion(crate::pdf::ConversionCommand::UpdateVisual(vec![]));
                if !self.is_kitty {
                    self.force_redraw();
                }
            }
            InputAction::ExitVisualMode(cursor) => {
                send_conversion(crate::pdf::ConversionCommand::UpdateCursor(cursor));
                send_conversion(crate::pdf::ConversionCommand::UpdateVisual(vec![]));
                if !self.is_kitty {
                    self.force_redraw();
                }
            }
            InputAction::CommentNavJump {
                page,
                viewport,
                selection_rects,
            } => {
                send_conversion(crate::pdf::ConversionCommand::NavigateTo(page));
                if let Some(viewport) = viewport {
                    send_conversion(crate::pdf::ConversionCommand::UpdateViewport(viewport));
                }
                // Send selection rects to highlight the matched content
                if !selection_rects.is_empty() {
                    send_conversion(crate::pdf::ConversionCommand::UpdateSelection(
                        selection_rects,
                    ));
                }
            }
            InputAction::CommentSaved { rects, .. } => {
                log::info!("CommentSaved: sending {} rects to converter", rects.len());
                for (i, r) in rects.iter().enumerate() {
                    log::info!(
                        "  rect[{}]: page={} ({},{}) - ({},{})",
                        i,
                        r.page,
                        r.topleft_x,
                        r.topleft_y,
                        r.bottomright_x,
                        r.bottomright_y
                    );
                }
                send_conversion(crate::pdf::ConversionCommand::UpdateComments(rects));
                notifications.info("Saved comment");
            }
            InputAction::CommentDeleted {
                rects,
                selection_rects,
            } => {
                send_conversion(crate::pdf::ConversionCommand::UpdateComments(rects));
                send_conversion(crate::pdf::ConversionCommand::UpdateSelection(
                    selection_rects.clone(),
                ));
                notifications.warn("Deleted comment");
                // Clear visual overlay when all comments are deleted
                if selection_rects.is_empty() {
                    send_conversion(crate::pdf::ConversionCommand::UpdateVisual(vec![]));
                }
            }
            InputAction::DumpDebugState => {
                log::info!("=== PDF READER DEBUG DUMP ===");
                log::info!("  current_page={}", self.page);
                log::info!("  is_kitty={}", self.is_kitty);
                log::info!("  rendered.len={}", self.rendered.len());
                for (i, r) in self.rendered.iter().enumerate() {
                    log::info!(
                        "  rendered[{}]: has_img={}, pixel_w={:?}, pixel_h={:?}",
                        i,
                        r.img.is_some(),
                        r.pixel_w,
                        r.pixel_h
                    );
                }
                log::info!("=== END PDF READER DUMP ===");
                // Also trigger converter dump
                send_conversion(crate::pdf::ConversionCommand::DumpState);
            }
        }

        InputOutcome::None
    }
}

fn toggle_profiling(
    profiler: &std::sync::Arc<std::sync::Mutex<Option<pprof::ProfilerGuard<'static>>>>,
) {
    let mut profiler_lock = profiler.lock().unwrap();

    if profiler_lock.is_none() {
        log::debug!("Profiling started");
        *profiler_lock = Some(pprof::ProfilerGuard::new(1000).unwrap());
    } else {
        log::debug!("Profiling stopped and saved");

        if let Some(guard) = profiler_lock.take() {
            if let Ok(report) = guard.report().build() {
                let file = std::fs::File::create("flamegraph.svg").unwrap();
                report.flamegraph(file).unwrap();
            } else {
                log::debug!("Could not build profile report");
            }
        }
    }
}

fn open_url(url: &str) {
    if let Err(e) = open::that(url) {
        log::error!("Failed to open URL {url}: {e}");
    }
}

pub(crate) fn save_pdf_bookmark(
    bookmarks: &mut Bookmarks,
    pdf_reader: &PdfReaderState,
    last_bookmark_save: &mut std::time::Instant,
    force: bool,
) {
    let page = if pdf_reader.is_kitty {
        pdf_reader.expected_page_from_scroll()
    } else {
        pdf_reader.page
    };
    let chapter_href = page.to_string();
    let scroll_position = pdf_reader
        .zoom
        .as_ref()
        .map(|z| z.global_scroll_offset as usize)
        .unwrap_or(0);

    let zoom_factor = pdf_reader.zoom.as_ref().map(|z| z.factor);
    bookmarks.update_bookmark(
        &pdf_reader.name,
        chapter_href,
        Some(scroll_position),
        None,
        Some(pdf_reader.rendered.len().max(1)),
        Some(page),
        zoom_factor,
    );

    let now = std::time::Instant::now();
    if force || now.duration_since(*last_bookmark_save) > std::time::Duration::from_millis(500) {
        if let Err(e) = bookmarks.save() {
            log::error!("Failed to save PDF bookmark: {e}");
        }
        *last_bookmark_save = now;
    }
}

pub(crate) fn update_pdf_toc_active(
    table_of_contents: &mut TableOfContents,
    pdf_reader: &PdfReaderState,
    page: usize,
    toc_height: usize,
) {
    use crate::pdf::TocTarget;

    let mut target_page = page;
    let n_pages = pdf_reader.rendered.len();
    let mut best_match = None;

    for entry in &pdf_reader.toc_entries {
        let page_idx = match &entry.target {
            TocTarget::InternalPage(page) => Some(*page),
            TocTarget::PrintedPage(printed) => pdf_reader
                .page_numbers
                .map_printed_to_pdf(*printed, n_pages),
            TocTarget::External(_) => None,
        };

        if let Some(page_idx) = page_idx
            && page_idx <= page
            && best_match.is_none_or(|current| page_idx > current)
        {
            best_match = Some(page_idx);
        }
    }

    if let Some(best_match) = best_match {
        target_page = best_match;
    }

    let current_href = format!("pdf:page:{target_page}");
    if let Some(info) = table_of_contents.get_current_book_info() {
        if info.active_section.chapter_href == current_href {
            return;
        }
    }
    table_of_contents.set_active_from_hint(&current_href, None, Some(toc_height));
}

pub(crate) fn should_route_mouse_to_ui(
    mouse_event: &MouseEvent,
    has_popup: bool,
    zen_mode: bool,
    nav_panel_width: u16,
    help_area: Rect,
) -> bool {
    if has_popup {
        return true;
    }

    if zen_mode {
        return false;
    }

    if mouse_event.column < nav_panel_width {
        return true;
    }

    help_area.height > 0 && mouse_event.row >= help_area.y
}

pub(crate) fn get_pdf_chapter_title(
    toc_entries: &[crate::pdf::TocEntry],
    current_page: usize,
) -> Option<String> {
    use crate::pdf::TocTarget;

    let mut current_chapter: Option<&str> = None;

    for entry in toc_entries {
        let entry_page = match &entry.target {
            TocTarget::InternalPage(page) => Some(*page),
            TocTarget::PrintedPage(_) => None,
            TocTarget::External(_) => None,
        };

        if let Some(page) = entry_page {
            if page <= current_page {
                current_chapter = Some(&entry.title);
            } else {
                break;
            }
        }
    }

    current_chapter.map(|s| s.to_string())
}

pub(crate) fn convert_pdf_toc_to_toc_items(entries: &[crate::pdf::TocEntry]) -> Vec<TocItem> {
    use crate::pdf::TocTarget;

    let mut result = Vec::new();
    let mut i = 0;

    while i < entries.len() {
        let entry = &entries[i];
        let href = match &entry.target {
            TocTarget::InternalPage(page) => format!("pdf:page:{page}"),
            TocTarget::External(url) => format!("pdf:external:{url}"),
            TocTarget::PrintedPage(printed) => format!("pdf:printed:{printed}"),
        };

        if entry.level == 0 {
            let children_start = i + 1;
            let mut children_end = children_start;

            while children_end < entries.len() && entries[children_end].level > 0 {
                children_end += 1;
            }

            if children_end > children_start {
                let child_entries = &entries[children_start..children_end];
                let children = convert_pdf_toc_children(child_entries, 1);

                result.push(TocItem::Section {
                    title: entry.title.clone(),
                    href: Some(href),
                    anchor: None,
                    children,
                    is_expanded: true,
                });
                i = children_end;
            } else {
                result.push(TocItem::Chapter {
                    title: entry.title.clone(),
                    href,
                    anchor: None,
                });
                i += 1;
            }
        } else {
            result.push(TocItem::Chapter {
                title: entry.title.clone(),
                href,
                anchor: None,
            });
            i += 1;
        }
    }

    result
}

pub(crate) fn convert_pdf_toc_children(
    entries: &[crate::pdf::TocEntry],
    current_level: usize,
) -> Vec<TocItem> {
    use crate::pdf::TocTarget;

    let mut result = Vec::new();
    let mut i = 0;

    while i < entries.len() {
        let entry = &entries[i];
        let href = match &entry.target {
            TocTarget::InternalPage(page) => format!("pdf:page:{page}"),
            TocTarget::External(url) => format!("pdf:external:{url}"),
            TocTarget::PrintedPage(printed) => format!("pdf:printed:{printed}"),
        };

        if entry.level == current_level {
            let children_start = i + 1;
            let mut children_end = children_start;

            while children_end < entries.len() && entries[children_end].level > current_level {
                children_end += 1;
            }

            if children_end > children_start {
                let child_entries = &entries[children_start..children_end];
                let children = convert_pdf_toc_children(child_entries, current_level + 1);

                result.push(TocItem::Section {
                    title: entry.title.clone(),
                    href: Some(href),
                    anchor: None,
                    children,
                    is_expanded: true,
                });
                i = children_end;
            } else {
                result.push(TocItem::Chapter {
                    title: entry.title.clone(),
                    href,
                    anchor: None,
                });
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    result
}

pub(crate) fn clamp_and_set_page(pdf_reader: &mut PdfReaderState, page: usize) -> usize {
    let page = if pdf_reader.rendered.is_empty() {
        0
    } else {
        page.min(pdf_reader.rendered.len() - 1)
    };

    pdf_reader.set_page(page);
    pdf_reader.force_redraw();

    page
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn navigate_pdf_to_page(
    pdf_reader: &mut PdfReaderState,
    page: usize,
    service: Option<&mut crate::pdf::RenderService>,
    conversion_tx: Option<&flume::Sender<crate::pdf::ConversionCommand>>,
    table_of_contents: &mut crate::table_of_contents::TableOfContents,
    toc_height: usize,
    bookmarks: &mut Bookmarks,
    last_bookmark_save: &mut std::time::Instant,
) {
    let page = clamp_and_set_page(pdf_reader, page);

    if let Some(service) = service {
        service.apply_command(crate::pdf::Command::GoToPage(page));
        service.request_page(page);
        service.request_page(page.saturating_add(1));
        if page > 0 {
            service.request_page(page - 1);
        }
    }

    if let Some(tx) = conversion_tx {
        let _ = tx.send(crate::pdf::ConversionCommand::NavigateTo(page));
    }

    let current_href = format!("pdf:page:{page}");
    table_of_contents.set_active_from_hint(&current_href, None, Some(toc_height));

    save_pdf_bookmark(bookmarks, pdf_reader, last_bookmark_save, false);
}

pub(crate) fn apply_theme_to_pdf_reader(
    pdf_reader: &mut PdfReaderState,
    palette: &crate::theme::Base16Palette,
    theme_index: usize,
    service: Option<&mut crate::pdf::RenderService>,
    conversion_tx: Option<&flume::Sender<crate::pdf::ConversionCommand>>,
) {
    pdf_reader.palette = palette.clone();
    pdf_reader.theme_index = theme_index;
    pdf_reader.force_redraw();

    for rendered_info in &mut pdf_reader.rendered {
        rendered_info.img = None;
    }

    let (br, bg, bb) = extract_pdf_rgb(&palette.base_00);
    let (fr, fg, fb) = extract_pdf_rgb(&palette.base_05);
    let black = (br as i32) << 16 | (bg as i32) << 8 | bb as i32;
    let white = (fr as i32) << 16 | (fg as i32) << 8 | fb as i32;

    if let Some(service) = service {
        service.apply_command(crate::pdf::Command::SetColors { black, white });
        service.request_page(pdf_reader.page);
    }
    if let Some(tx) = conversion_tx {
        let _ = tx.send(crate::pdf::ConversionCommand::InvalidatePageCache);
    }
}

fn extract_pdf_rgb(color: &ratatui::style::Color) -> (u8, u8, u8) {
    match color {
        ratatui::style::Color::Rgb(r, g, b) => (*r, *g, *b),
        _ => (0, 0, 0),
    }
}
