//! PDF rendering infrastructure

mod cache;
mod converter;
pub mod kittyv2;
mod parsing;
mod request;
mod service;
mod state;
mod types;
mod worker;
mod zoom;

pub use cache::{CacheKey, PageCache};
pub use converter::{
    CellSize, ConversionCommand, ConvertedImage, ImageState, RenderedFrame, TiledProtocol,
    run_conversion_loop,
};
pub use parsing::page_numbers::{PageNumberTracker, detect_page_number, sample_targets};
pub use parsing::toc::{TocEntry, TocTarget};
pub use request::{
    PageSelectionBounds, RenderParams, RenderRequest, RenderResponse, RequestId, WorkerFault,
};
pub use service::{DocumentInfo, RenderService};
pub use state::{Command, Effect, RenderState};
pub use types::*;
pub use zoom::*;

/// Default number of render workers
pub const DEFAULT_WORKERS: usize = 2;

/// Default page cache size (each page can be 5-15MB of pixel data)
pub const DEFAULT_CACHE_SIZE: usize = 30;

/// Default prefetch radius (pages before/after current)
pub const DEFAULT_PREFETCH_RADIUS: usize = 10;

/// Maximum width/height for Kitty protocol images
pub const KITTY_MAX_DIMENSION: f32 = 10_000.0;
