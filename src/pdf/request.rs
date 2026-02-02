//! Render request and response types

use ratatui::layout::Rect;
use std::sync::Arc;

use super::CellSize;
use super::TocEntry;
use super::types::PageData;

/// Unique identifier for render requests
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);

impl RequestId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Parameters for rendering a page
#[derive(Clone, Debug)]
pub struct RenderParams {
    /// Viewport area in terminal cells
    pub area: Rect,
    /// User-specified scale factor
    pub scale: f32,
    /// Whether to invert images
    pub invert_images: bool,
    /// Terminal cell dimensions
    pub cell_size: CellSize,
    /// Theme colors for tinting
    pub black: i32,
    pub white: i32,
}

/// Selection bounds for text extraction (pixel coordinates)
#[derive(Clone, Debug)]
pub struct PageSelectionBounds {
    pub page: usize,
    pub start_x: f32,
    pub end_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

/// Request sent to render workers
#[derive(Debug)]
pub enum RenderRequest {
    /// Render a page (high priority)
    Page {
        id: RequestId,
        page: usize,
        params: RenderParams,
    },

    /// Extract text from selection
    ExtractText {
        id: RequestId,
        bounds: Vec<PageSelectionBounds>,
        params: RenderParams,
    },

    /// Prefetch a page (low priority)
    Prefetch {
        id: RequestId,
        page: usize,
        params: RenderParams,
    },

    /// Cancel a pending request
    Cancel(RequestId),

    /// Shutdown the worker
    Shutdown,
}

/// Errors from render workers
#[derive(Debug, thiserror::Error)]
pub enum WorkerFault {
    #[error("PDF engine: {0}")]
    Pdf(#[from] mupdf::error::Error),

    #[error("{detail}")]
    Generic { detail: String },
}

impl WorkerFault {
    pub fn generic(msg: impl Into<String>) -> Self {
        Self::Generic { detail: msg.into() }
    }
}

/// Response from render workers
#[derive(Debug)]
pub enum RenderResponse {
    /// Rendered page data
    Page {
        id: RequestId,
        page: usize,
        data: Arc<PageData>,
    },

    /// Extracted text from selection
    ExtractedText { id: RequestId, text: String },

    /// Request was cancelled
    Cancelled(RequestId),

    /// Error during rendering
    Error { id: RequestId, error: WorkerFault },

    /// Document metadata (sent on load)
    DocumentInfo {
        page_count: usize,
        title: Option<String>,
        toc: Vec<TocEntry>,
        page_number_samples: Vec<(usize, i32)>,
    },

    /// Document was reloaded
    Reloaded,
}
