//! PDF reader widget components

pub mod region;
pub mod state;
pub mod types;

pub use region::{ImageRegion, TextRegion};
pub use state::{
    CommentEditMode, CommentInputState, FocusedPanel, InputAction, PdfReaderState, PopupWindow,
    SEPARATOR_HEIGHT,
};
pub use types::{
    DisplayBatch, ImageRequest, LastRender, PageJumpMode, PdfDisplayPlan, PdfDisplayRequest,
    PendingScroll, PrevFrame, RenderLayout, RenderedInfo, VisiblePageUiInfo,
};
