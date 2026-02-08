mod lease;
mod metrics;
mod pool;
mod protocol;
mod region;
mod tracker;

pub use lease::ShmLease;
pub(crate) use metrics::{record_shm_create, record_shm_unlink_error, record_shm_unlink_success};
pub use pool::{RegionPool, pool};
pub use protocol::{
    CHUNK_LIMIT, Compression, DeleteCommand, DeleteMode, DestCells, DirectTransmit, DisplayCommand,
    Format, QueryCommand, Quiet, Response, SourceRect, TransmitCommand, is_tmux, parse_response,
    set_tmux_mode,
};
pub use region::MemoryRegion;
pub use tracker::{LifecycleTracker, tracker};

/// Cleans up all shared memory resources.
///
/// This should be called at application exit and in panic handlers
/// to ensure no leaked shared memory regions remain in /dev/shm.
pub fn cleanup_all_shms() {
    // Clear the tracker's registered regions
    tracker().lock().unwrap().cleanup_all();
    // Clear the pool's pre-allocated regions
    pool().lock().unwrap().clear();
}
