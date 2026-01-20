//! PDF reader widget components

mod navigation;
pub mod region;
pub mod state;
pub mod types;

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
pub use state::{
    CommentEditMode, CommentInputState, FocusedPanel, InputAction, PdfReaderState, PopupWindow,
    SEPARATOR_HEIGHT,
};
pub use types::{
    DisplayBatch, ImageRequest, LastRender, PageJumpMode, PdfDisplayPlan, PdfDisplayRequest,
    PendingScroll, PrevFrame, RenderLayout, RenderedInfo, VisiblePageUiInfo,
};
