mod canvas;
mod encoder;
mod probe;
mod registry;
mod types;

pub use canvas::TerminalCanvas;
pub use encoder::PixelEncoder;
pub use probe::probe_capabilities;
pub use registry::FrameRegistry;
pub use types::{
    Format, FrameHandle, FrameSpec, OperationBatch, RemovalTarget, ResponseMode, ScreenPlacement,
    SubmissionError, SubmissionOutcome, TransferMode,
};
