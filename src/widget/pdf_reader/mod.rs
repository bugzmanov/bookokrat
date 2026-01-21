//! PDF reader widget components
//!
//! This module provides PDF-specific widget functionality, including
//! comments, selection, and normal mode navigation.
//!
//! The core PDF rendering infrastructure is in `crate::pdf`.

mod navigation;
pub mod region;
mod rendering;
pub mod state;
pub mod types;

// Re-export from crate::pdf for widget use
pub use crate::pdf::{
    CursorPosition, CursorRect, ExtractionRequest, MoveResult, NormalModeState,
    PageSelectionBounds, PendingMotion, SelectionPoint, SelectionRect, TextSelection, VisualMode,
    VisualRect, visual_rects_for_range,
};

pub use crate::comments::{BookComments, Comment, CommentTarget, PdfSelectionRect};
pub(crate) use navigation::{
    InputOutcome, apply_theme_to_pdf_reader, navigate_pdf_to_page, should_route_mouse_to_ui,
};
pub use region::{ImageRegion, TextRegion};
pub(crate) use rendering::{
    apply_render_responses, execute_display_plan, update_non_kitty_viewport,
};
pub use state::{
    CommentEditMode, CommentInputState, FocusedPanel, InputAction, PdfReaderState, PopupWindow,
    SEPARATOR_HEIGHT,
};
pub use types::{
    DisplayBatch, ImageRequest, LastRender, PageJumpMode, PdfDisplayPlan, PdfDisplayRequest,
    PendingScroll, PrevFrame, RenderLayout, RenderedInfo, VisiblePageUiInfo,
};
