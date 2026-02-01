use std::io;

pub use crate::pdf::kittyv2::kgfx::Format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    SharedMemory,
    Chunked,
}

#[derive(Debug, Clone)]
pub struct FrameSpec {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub page: i64,
    pub format: Format,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHandle {
    pub image_id: u32,
    pub page: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenPlacement {
    pub column: u16,
    pub row: u16,
    pub cell_width: u16,
    pub cell_height: u16,
    /// Source rectangle for cropping (used during scroll)
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
}

impl ScreenPlacement {
    pub fn has_source_rect(&self) -> bool {
        self.source_x > 0 || self.source_y > 0 || self.source_width > 0 || self.source_height > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovalTarget {
    Everything,
    Single(FrameHandle),
    PageRange { min: i64, max: i64 },
}

#[derive(Debug, Clone)]
pub enum OperationBatch {
    Idle,
    ClearScreen,
    Render(Vec<(FrameSpec, ScreenPlacement)>),
}

#[derive(Debug)]
pub struct SubmissionOutcome {
    pub successful: Vec<FrameHandle>,
    pub failed: Vec<(i64, SubmissionError)>,
}

impl SubmissionOutcome {
    pub fn empty() -> Self {
        Self {
            successful: Vec::new(),
            failed: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum SubmissionError {
    IoFailure(io::Error),
    PoolExhausted,
    EncodingFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseMode {
    Silent,
    ErrorsOnly,
    Full,
}
