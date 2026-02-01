//! PDF reader widget state
//!
//! Contains the main state struct for the PDF reader widget,
//! including rendering state, input state, and navigation.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::vendored::ratatui_image::FontSize;
use crate::vendored::tui_textarea::TextArea;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::inputs::{KeySeq, MouseTracker};
use crate::jump_list::JumpList;
use crate::notification::NotificationManager;
use crate::pdf::{
    CursorRect, ExtractionRequest, NormalModeState, PageNumberTracker, SelectionRect,
    TextSelection, TocEntry, ViewportUpdate, VisualRect, Zoom,
};
use crate::theme::Base16Palette;
use crate::widget::hud_message::{HudMessage, HudMode};

use super::types::{LastRender, PageJumpMode, PendingScroll, PrevFrame, RenderedInfo};
use crate::comments::{BookComments, CommentTarget};

/// Default jump list size
const DEFAULT_JUMP_LIST_SIZE: usize = 100;

/// Default notification duration
const DEFAULT_NOTIFICATION_DURATION: Duration = Duration::from_secs(5);
const HUD_NORMAL_DURATION: Duration = Duration::from_secs(2);
const HUD_ERROR_DURATION: Duration = Duration::from_secs(5);

/// Focus state for the PDF reader
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPanel {
    #[default]
    Content,
    Popup(PopupWindow),
}

/// Popup window types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupWindow {
    Help,
    GoToPage,
    GoToLink,
    TableOfContents,
    CommentInput,
    CommentsList,
}

impl FocusedPanel {
    pub fn is_popup(&self) -> bool {
        matches!(self, Self::Popup(_))
    }

    pub fn popup(&self) -> Option<PopupWindow> {
        match self {
            Self::Popup(p) => Some(*p),
            Self::Content => None,
        }
    }
}

/// Comment editing mode
#[derive(Clone, Debug)]
pub enum CommentEditMode {
    Creating,
    Editing { comment_id: String },
}

/// Comment input state
#[derive(Default)]
pub struct CommentInputState {
    pub textarea: Option<TextArea<'static>>,
    pub edit_mode: Option<CommentEditMode>,
    pub target: Option<CommentTarget>,
    /// The selected/quoted text for the comment
    pub quoted_text: Option<String>,
}

/// A search match on the current page
#[derive(Clone, Debug)]
pub struct PageSearchMatch {
    /// Line index in the page's line_bounds
    pub line_idx: usize,
    /// Character index within the line (start of match)
    pub char_idx: usize,
    /// Length of the match in characters
    pub length: usize,
}

/// Page search state for vim-style / search in normal mode
#[derive(Default)]
pub struct PageSearchState {
    /// The textarea for entering the search query
    pub input: Option<TextArea<'static>>,
    /// The current search query (after Enter)
    pub query: Option<String>,
    /// All matches on the current page
    pub matches: Vec<PageSearchMatch>,
    /// Index of the current match (for n/N navigation)
    pub current_match: usize,
    /// The page these matches are for (to invalidate when page changes)
    pub matches_page: usize,
    /// Last cursor position in the search input
    pub input_cursor: Option<(usize, usize)>,
}

impl PageSearchState {
    pub fn is_input_active(&self) -> bool {
        self.input.is_some()
    }

    pub fn has_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    pub fn clear_input(&mut self) {
        self.input = None;
    }

    pub fn clear_search(&mut self) {
        self.query = None;
        self.matches.clear();
        self.current_match = 0;
    }

    pub fn current(&self) -> Option<&PageSearchMatch> {
        self.matches.get(self.current_match)
    }

    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.current_match = (self.current_match + 1) % self.matches.len();
        }
    }

    pub fn prev_match(&mut self) {
        if !self.matches.is_empty() {
            self.current_match = if self.current_match == 0 {
                self.matches.len() - 1
            } else {
                self.current_match - 1
            };
        }
    }
}

impl CommentInputState {
    pub fn clear(&mut self) {
        self.textarea = None;
        self.edit_mode = None;
        self.target = None;
        self.quoted_text = None;
    }

    pub fn is_active(&self) -> bool {
        self.textarea.is_some()
    }
}

/// Actions returned from input handling
pub enum InputAction {
    Redraw,
    JumpingToPage {
        page: usize,
        viewport: Option<ViewportUpdate>,
    },
    CommentNavJump {
        page: usize,
        viewport: Option<ViewportUpdate>,
        selection_rects: Vec<SelectionRect>,
    },
    QuitApp,
    ToggleInvertImages,
    RenderScale {
        factor: f32,
        viewport: Option<ViewportUpdate>,
    },
    ViewportChanged(ViewportUpdate),
    ToggleProfiling,
    SelectionChanged(Vec<SelectionRect>),
    CommentSaved {
        rects: Vec<SelectionRect>,
        cursor_rect: Option<CursorRect>,
    },
    CommentDeleted {
        rects: Vec<SelectionRect>,
        selection_rects: Vec<SelectionRect>,
    },
    CopySelection(ExtractionRequest),
    CursorChanged(Option<CursorRect>, Option<ViewportUpdate>),
    VisualChanged(Vec<VisualRect>, Option<ViewportUpdate>),
    YankText(String, Option<CursorRect>),
    ExitNormalMode,
    ExitVisualMode(Option<CursorRect>),
    OpenExternalLink(String),
    ThemeChanged {
        black: i32,
        white: i32,
    },
    DumpDebugState,
}

/// Separator height between pages in continuous scroll
pub const SEPARATOR_HEIGHT: u16 = 1;

/// Main PDF reader widget state
pub struct PdfReaderState {
    /// Document name
    pub name: String,
    /// Document title from metadata
    pub doc_title: Option<String>,
    /// Current page index (0-based)
    pub page: usize,
    /// Last render information
    pub last_render: LastRender,
    /// Go-to-page dialog input (current page number being typed)
    pub go_to_page_input: Option<usize>,
    /// Rendered page information
    pub rendered: Vec<RenderedInfo>,
    /// Whether terminal supports Kitty graphics protocol
    pub is_kitty: bool,
    /// Zoom state (only for Kitty terminals)
    pub zoom: Option<Zoom>,
    /// Color palette
    pub palette: Base16Palette,
    /// Theme index in palette list
    pub theme_index: usize,
    /// Text selection state
    pub selection: TextSelection,
    /// Normal mode (vim cursor) state
    pub normal_mode: NormalModeState,
    /// Non-Kitty zoom factor
    pub non_kitty_zoom_factor: f32,
    /// Non-Kitty scroll offset
    pub non_kitty_scroll_offset: u32,
    /// Whether terminal is iTerm2/WezTerm
    pub is_iterm: bool,
    /// Coordinate info for mouse mapping
    pub coord_info: Option<(Rect, FontSize)>,
    /// Mouse tracker for click detection
    pub mouse_tracker: MouseTracker,
    /// Whether mouse down was seen (for selection)
    pub mouse_down_seen: bool,
    /// Key sequence tracker for vim motions
    pub key_seq: KeySeq,
    /// Pending scroll operation
    pub pending_scroll: Option<PendingScroll>,
    /// Previous frame for scroll optimization
    pub prev_frame: Option<PrevFrame>,
    /// Jump list for navigation history
    pub jump_list: JumpList,
    /// Page number tracker for content/PDF page mapping
    pub page_numbers: PageNumberTracker,
    /// Table of contents entries
    pub toc_entries: Vec<TocEntry>,
    /// Whether comments are enabled
    pub comments_enabled: bool,
    /// Whether terminal supports PDF comments (Kitty/iTerm2 protocols)
    pub supports_comments: bool,
    /// Book comments storage (unified with EPUB comments)
    pub book_comments: Option<Arc<Mutex<BookComments>>>,
    /// Document ID for comments
    pub comments_doc_id: String,
    /// Comment input state
    pub comment_input: CommentInputState,
    /// Comment selection rectangles for overlay rendering
    pub comment_rects: Vec<SelectionRect>,
    /// Whether comment navigation is active
    pub comment_nav_active: bool,
    /// Current page for comment navigation
    pub comment_nav_page: usize,
    /// Current index in comment navigation
    pub comment_nav_index: usize,
    /// Go to page mode (content vs PDF)
    pub go_to_page_mode: PageJumpMode,
    /// Error message for go to page
    pub go_to_page_error: Option<String>,
    /// Currently focused panel
    pub focused_panel: FocusedPanel,
    /// Notification manager
    pub notifications: NotificationManager,
    /// Last viewport update sent to converter (non-Kitty)
    pub last_sent_viewport: Option<ViewportUpdate>,
    /// Last Kitty cache window (page indices) used to bound terminal cache
    pub last_kitty_cache_window: Option<(usize, usize)>,
    /// Pages with active Kitty placements from the last display pass
    pub kitty_visible_pages: HashSet<usize>,
    /// Whether Kitty delete-by-range is safe on this terminal
    pub kitty_delete_range_supported: bool,
    /// Pending search highlight: (page, query) to apply when page data arrives
    pub pending_search_highlight: Option<(usize, String)>,
    /// Transient HUD message for the bottom title area
    pub hud_message: Option<HudMessage>,
    /// Page search state for vim-style / search in normal mode
    pub page_search: PageSearchState,
}

impl PdfReaderState {
    /// Create a new PDF reader state
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        is_kitty: bool,
        is_iterm: bool,
        initial_page: usize,
        zoom_factor: f32,
        pan_from_left: u16,
        global_scroll_offset: u32,
        palette: Base16Palette,
        theme_index: usize,
        comments_enabled: bool,
        supports_comments: bool,
        book_comments: Option<Arc<Mutex<BookComments>>>,
        comments_doc_id: String,
    ) -> Self {
        let zoom_factor = Zoom::clamp_factor(zoom_factor);
        let zoom = if is_kitty {
            Some(Zoom {
                factor: zoom_factor,
                cell_pan_from_left: pan_from_left,
                global_scroll_offset,
            })
        } else {
            None
        };

        Self {
            name,
            doc_title: None,
            page: initial_page,
            go_to_page_input: None,
            last_render: LastRender::default(),
            rendered: vec![],
            is_kitty,
            zoom,
            palette,
            theme_index,
            selection: TextSelection::new(),
            normal_mode: NormalModeState::new(),
            non_kitty_zoom_factor: if is_kitty { 1.0 } else { zoom_factor },
            non_kitty_scroll_offset: 0,
            is_iterm,
            coord_info: None,
            mouse_tracker: MouseTracker::new(),
            mouse_down_seen: false,
            key_seq: KeySeq::new(),
            pending_scroll: None,
            prev_frame: None,
            jump_list: JumpList::new(DEFAULT_JUMP_LIST_SIZE),
            page_numbers: PageNumberTracker::new(),
            toc_entries: Vec::new(),
            comments_enabled,
            supports_comments,
            book_comments,
            comments_doc_id,
            comment_input: CommentInputState::default(),
            comment_rects: Vec::new(),
            comment_nav_active: false,
            comment_nav_page: 0,
            comment_nav_index: 0,
            go_to_page_mode: PageJumpMode::Pdf,
            go_to_page_error: None,
            focused_panel: FocusedPanel::default(),
            notifications: NotificationManager::with_default_duration(
                DEFAULT_NOTIFICATION_DURATION,
            ),
            last_sent_viewport: None,
            last_kitty_cache_window: None,
            kitty_visible_pages: HashSet::new(),
            kitty_delete_range_supported: false,
            pending_search_highlight: None,
            hud_message: None,
            page_search: PageSearchState::default(),
        }
    }

    pub fn set_zoom_hud(&mut self, zoom_factor: f32) {
        let percent = (zoom_factor * 100.0).round() as u32;
        self.set_hud_message(
            format!("Zoom {percent}%"),
            HudMode::Normal,
            HUD_NORMAL_DURATION,
        );
    }

    pub fn set_error_hud(&mut self, message: String) {
        self.set_hud_message(message, HudMode::Error, HUD_ERROR_DURATION);
    }

    pub fn update_hud_message(&mut self) -> bool {
        if self
            .hud_message
            .as_ref()
            .is_some_and(HudMessage::is_expired)
        {
            self.hud_message = None;
            return true;
        }
        false
    }

    pub fn dismiss_error_hud(&mut self) -> bool {
        if self
            .hud_message
            .as_ref()
            .is_some_and(|hud| hud.mode == HudMode::Error)
        {
            self.hud_message = None;
            return true;
        }
        false
    }

    pub(crate) fn set_hud_message(&mut self, message: String, mode: HudMode, duration: Duration) {
        self.hud_message = Some(HudMessage::new(message, duration, mode));
    }

    pub fn set_doc_title(&mut self, title: Option<String>) {
        self.doc_title = title;
    }

    pub fn update_page_from_scroll(&mut self, page: usize) -> bool {
        if page == self.page {
            return false;
        }
        self.page = page;
        if self.comment_nav_active {
            self.comment_nav_page = page;
            self.comment_nav_index = 0;
        }
        true
    }

    pub fn force_redraw(&mut self) {
        self.last_render.rect = Rect::default();
    }

    pub fn bg_color(&self) -> Color {
        self.palette.base_00
    }

    pub fn fg_color(&self) -> Color {
        self.palette.base_05
    }

    pub fn accent_color(&self) -> Color {
        self.palette.base_0c
    }

    pub fn muted_color(&self) -> Color {
        self.palette.base_03
    }

    pub fn estimated_page_height_cells(&self) -> u16 {
        self.rendered
            .iter()
            .find_map(|page| {
                page.img
                    .as_ref()
                    .map(|img| img.cell_dimensions().as_tuple().1)
            })
            .unwrap_or(self.last_render.img_area_height)
    }
}
