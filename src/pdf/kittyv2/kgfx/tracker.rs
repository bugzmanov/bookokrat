use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use log::{debug, info, warn};

/// Soft limit for queue size - cleanup starts when exceeded.
const SOFT_LIMIT: usize = 20;

/// Hard maximum queue size - forced cleanup regardless of protection.
const HARD_LIMIT: usize = 40;

/// Minimum age before normal cleanup (protects recently-used regions).
const MIN_AGE: Duration = Duration::from_secs(1);

/// Minimum age before forced cleanup (bypasses protection).
const FORCED_AGE: Duration = Duration::from_secs(5);

/// Protection radius - regions within this distance of current position are protected.
const PROTECTION_RADIUS: i64 = 2;

/// Statistics logging interval.
const LOG_INTERVAL: Duration = Duration::from_secs(10);

/// Entry in the lifecycle tracker queue.
struct TrackerEntry {
    path: String,
    size: usize,
    position: i64,
    created: Instant,
}

/// Tracks memory regions and handles cleanup with position-based protection.
///
/// Regions associated with "nearby" logical positions (e.g., adjacent document pages)
/// are protected from cleanup to prevent premature removal while the terminal
/// might still be reading them.
pub struct LifecycleTracker {
    queue: VecDeque<TrackerEntry>,
    total_size: usize,
    last_log: Instant,
    current_position: i64,
}

impl LifecycleTracker {
    /// Creates a new lifecycle tracker.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            total_size: 0,
            last_log: Instant::now(),
            current_position: 0,
        }
    }

    /// Updates the current logical position.
    ///
    /// This affects which regions are protected from cleanup.
    /// Regions within `PROTECTION_RADIUS` of this position are protected.
    pub fn set_position(&mut self, position: i64) {
        self.current_position = position;
    }

    /// Returns the current logical position.
    pub fn position(&self) -> i64 {
        self.current_position
    }

    /// Checks if a position is within the protection radius.
    fn is_protected(&self, position: i64) -> bool {
        (position - self.current_position).abs() <= PROTECTION_RADIUS
    }

    /// Registers a memory region for eventual cleanup.
    ///
    /// The region will be tracked and eventually unlinked based on:
    /// - Queue size limits (soft: 20, hard: 40)
    /// - Age requirements (1 second minimum, 5 seconds for forced)
    /// - Position protection (regions near current position are protected)
    pub fn register(&mut self, path: String, size: usize, position: i64) {
        let size_mb = size as f64 / (1024.0 * 1024.0);
        debug!("registered: {path} position={position} ({size_mb:.2} MB)");

        let entry = TrackerEntry {
            path,
            size,
            position,
            created: Instant::now(),
        };

        self.queue.push_back(entry);
        self.total_size = self.total_size.saturating_add(size);

        self.cleanup_if_needed();
        self.maybe_log_stats();
    }

    /// Performs cleanup when queue exceeds limits.
    fn cleanup_if_needed(&mut self) {
        let now = Instant::now();

        // Soft limit cleanup: remove unprotected entries that have aged
        while self.queue.len() > SOFT_LIMIT {
            // Find first unprotected entry
            let idx = self
                .queue
                .iter()
                .position(|e| !self.is_protected(e.position));

            match idx {
                Some(i) => {
                    let entry = &self.queue[i];

                    // Check age requirement
                    if now.duration_since(entry.created) < MIN_AGE {
                        break;
                    }

                    // Remove and unlink
                    let entry = self.queue.remove(i).unwrap();
                    self.total_size = self.total_size.saturating_sub(entry.size);
                    unlink_path(&entry.path);
                    debug!("unlinked: {} position={}", entry.path, entry.position);
                }
                None => break, // All entries are protected
            }
        }

        // Hard limit cleanup: remove oldest entries regardless of protection
        while self.queue.len() > HARD_LIMIT {
            if let Some(entry) = self.queue.front() {
                // Check forced cleanup age requirement
                if now.duration_since(entry.created) < FORCED_AGE {
                    break;
                }

                let entry = self.queue.pop_front().unwrap();
                self.total_size = self.total_size.saturating_sub(entry.size);
                unlink_path(&entry.path);
                debug!(
                    "unlinked (forced): {} position={}",
                    entry.path, entry.position
                );
            }
        }
    }

    /// Logs statistics if the logging interval has elapsed.
    fn maybe_log_stats(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_log) >= LOG_INTERVAL {
            let protected_count = self
                .queue
                .iter()
                .filter(|e| self.is_protected(e.position))
                .count();
            let size_mb = self.total_size as f64 / (1024.0 * 1024.0);

            info!(
                "tracker: {} regions ({} protected), {:.2} MB, position={}",
                self.queue.len(),
                protected_count,
                size_mb,
                self.current_position
            );

            self.last_log = now;
        }
    }

    /// Performs full cleanup of all tracked regions.
    ///
    /// Unlinks all regions regardless of age or protection status.
    pub fn cleanup_all(&mut self) {
        let count = self.queue.len();
        let size_mb = self.total_size as f64 / (1024.0 * 1024.0);

        while let Some(entry) = self.queue.pop_front() {
            unlink_path(&entry.path);
            debug!("unlinked: {} position={}", entry.path, entry.position);
        }

        self.total_size = 0;

        if count > 0 {
            info!("cleanup: released {count} regions ({size_mb:.2} MB)");
        }
    }

    /// Logs the current state for debugging.
    ///
    /// Outputs at info level:
    /// - Current position and protection range
    /// - Queue length and total size
    /// - Each entry with index, position, path, and protection status
    pub fn dump_state(&self) {
        let size_mb = self.total_size as f64 / (1024.0 * 1024.0);
        let protected_min = self.current_position - PROTECTION_RADIUS;
        let protected_max = self.current_position + PROTECTION_RADIUS;

        info!(
            "tracker state: position={} (protected: {}..={}), {} entries, {:.2} MB",
            self.current_position,
            protected_min,
            protected_max,
            self.queue.len(),
            size_mb
        );

        for (i, entry) in self.queue.iter().enumerate() {
            let protected = if self.is_protected(entry.position) {
                "P"
            } else {
                "-"
            };
            let entry_mb = entry.size as f64 / (1024.0 * 1024.0);
            info!(
                "  [{}] {} pos={} {} ({:.2} MB)",
                i, protected, entry.position, entry.path, entry_mb
            );
        }
    }

    /// Returns the number of tracked regions.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns true if no regions are being tracked.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns the total size of all tracked regions in bytes.
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    /// Returns the number of protected regions.
    pub fn protected_count(&self) -> usize {
        self.queue
            .iter()
            .filter(|e| self.is_protected(e.position))
            .count()
    }
}

impl Default for LifecycleTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Unlinks a shared memory path.
fn unlink_path(path: &str) {
    match CString::new(path) {
        Ok(c_path) => {
            let result = unsafe { libc::shm_unlink(c_path.as_ptr()) };
            if result < 0 {
                let err = std::io::Error::last_os_error();
                debug!("failed to unlink {path}: {err}");
            }
        }
        Err(e) => {
            warn!("invalid path for unlink: {e}");
        }
    }
}

/// Global lifecycle tracker instance.
static TRACKER: OnceLock<Mutex<LifecycleTracker>> = OnceLock::new();

/// Returns a reference to the global lifecycle tracker.
///
/// The tracker is protected by a mutex and lazily initialized on first access.
///
/// **Important:** Call `cleanup_all()` before application exit to unlink
/// any remaining shared memory regions.
pub fn tracker() -> &'static Mutex<LifecycleTracker> {
    TRACKER.get_or_init(|| Mutex::new(LifecycleTracker::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::kittyv2::kgfx::MemoryRegion;
    use std::thread;

    #[test]
    fn test_basic_cleanup_after_limit() {
        let mut tracker = LifecycleTracker::new();
        tracker.set_position(100);

        // Register 25 entries for positions far from 100
        for i in 0..25 {
            let region = MemoryRegion::create_with_pattern("kgfxv2-track-*", 100)
                .expect("failed to create region");
            tracker.register(region.path().to_string(), 100, i);
            // MemoryRegion::drop() does NOT unlink, so tracker will handle cleanup
        }

        // Wait for age requirement
        thread::sleep(Duration::from_millis(1100));

        // Trigger cleanup by registering one more
        let region = MemoryRegion::create_with_pattern("kgfxv2-track-*", 100)
            .expect("failed to create region");
        tracker.register(region.path().to_string(), 100, 50);

        // Queue should be at or below soft limit
        assert!(
            tracker.len() <= SOFT_LIMIT + 1,
            "queue length {} exceeds soft limit",
            tracker.len()
        );

        // Cleanup remaining
        tracker.cleanup_all();
    }

    #[test]
    fn test_protection_by_position() {
        let mut tracker = LifecycleTracker::new();
        tracker.set_position(10);

        // Register entries for positions 8-12 (all within radius 2 of position 10)
        let protected_positions: Vec<i64> = vec![8, 9, 10, 11, 12];
        for &pos in &protected_positions {
            let region = MemoryRegion::create_with_pattern("kgfxv2-prot-*", 100)
                .expect("failed to create region");
            tracker.register(region.path().to_string(), 100, pos);
        }

        // Register 20 more entries far away
        for i in 100..120 {
            let region = MemoryRegion::create_with_pattern("kgfxv2-prot-*", 100)
                .expect("failed to create region");
            tracker.register(region.path().to_string(), 100, i);
        }

        // Wait for age requirement
        thread::sleep(Duration::from_millis(1100));

        // Trigger cleanup
        let region = MemoryRegion::create_with_pattern("kgfxv2-prot-*", 100)
            .expect("failed to create region");
        tracker.register(region.path().to_string(), 100, 200);

        // Protected entries should still be there
        assert!(
            tracker.protected_count() >= protected_positions.len(),
            "protected count {} is less than expected {}",
            tracker.protected_count(),
            protected_positions.len()
        );

        tracker.cleanup_all();
    }

    #[test]
    fn test_age_gate_prevents_premature_cleanup() {
        let mut tracker = LifecycleTracker::new();
        tracker.set_position(1000); // Far from all entries

        // Register 30 entries rapidly (all unprotected)
        for i in 0..30 {
            let region = MemoryRegion::create_with_pattern("kgfxv2-age-*", 100)
                .expect("failed to create region");
            tracker.register(region.path().to_string(), 100, i);
        }

        // Queue may exceed 20 because entries haven't aged 1 second yet
        // (This test verifies the age gate is working)
        // The exact count depends on timing, but should be > SOFT_LIMIT
        assert!(
            tracker.len() > SOFT_LIMIT,
            "queue length {} should exceed soft limit due to age gate",
            tracker.len()
        );

        tracker.cleanup_all();
    }

    #[test]
    fn test_position_update() {
        let mut tracker = LifecycleTracker::new();

        tracker.set_position(5);
        assert_eq!(tracker.position(), 5);

        tracker.set_position(10);
        assert_eq!(tracker.position(), 10);

        // Check protection changes with position
        assert!(tracker.is_protected(8)); // 10 - 2 = 8
        assert!(tracker.is_protected(12)); // 10 + 2 = 12
        assert!(!tracker.is_protected(7)); // Outside radius
        assert!(!tracker.is_protected(13)); // Outside radius
    }

    #[test]
    fn test_saturating_size_tracking() {
        let mut tracker = LifecycleTracker::new();

        // Register an entry
        let region = MemoryRegion::create_with_pattern("kgfxv2-sat-*", 1000)
            .expect("failed to create region");
        tracker.register(region.path().to_string(), 1000, 0);
        assert_eq!(tracker.total_size(), 1000);

        // Cleanup - size should go to 0, not underflow
        tracker.cleanup_all();
        assert_eq!(tracker.total_size(), 0);
    }
}
