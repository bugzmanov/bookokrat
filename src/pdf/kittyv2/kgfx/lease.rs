use std::ffi::CString;

use super::{
    LifecycleTracker, record_shm_create, record_shm_unlink_error, record_shm_unlink_success,
};

/// Owned lifecycle for a shared-memory payload until handed off to tracker.
#[derive(Clone, Debug)]
pub struct ShmLease {
    path: String,
    size: usize,
    cleanup_on_drop: bool,
}

impl ShmLease {
    /// Creates a new SHM lease that will unlink on drop by default.
    pub fn new(path: String, size: usize) -> Self {
        record_shm_create();
        Self {
            path,
            size,
            cleanup_on_drop: true,
        }
    }

    /// Returns the SHM path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the SHM payload size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Transfers cleanup responsibility to tracker ownership.
    pub fn handoff_to_tracker(mut self, position: i64, tracker: &mut LifecycleTracker) {
        self.cleanup_on_drop = false;
        let path = std::mem::take(&mut self.path);
        tracker.register(path, self.size, position);
    }
}

impl Drop for ShmLease {
    fn drop(&mut self) {
        if !self.cleanup_on_drop || self.path.is_empty() {
            return;
        }

        if let Ok(c_path) = CString::new(self.path.as_str()) {
            let result = unsafe { libc::shm_unlink(c_path.as_ptr()) };
            if result < 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::ENOENT) {
                    record_shm_unlink_success();
                } else {
                    record_shm_unlink_error();
                }
            } else {
                record_shm_unlink_success();
            }
        } else {
            record_shm_unlink_error();
        }
    }
}
