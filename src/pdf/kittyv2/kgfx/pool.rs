use std::ffi::CString;
use std::io::{self, Error, ErrorKind};
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use log::info;

/// Default region size: 32 MB.
const DEFAULT_REGION_SIZE: usize = 32 * 1024 * 1024;

/// Default pool size: 10 regions.
const DEFAULT_POOL_SIZE: usize = 10;

/// Default minimum time before a slot can be reused.
const DEFAULT_MIN_REUSE_AGE: Duration = Duration::from_secs(1);

/// A slot in the region pool.
struct PoolSlot {
    path: String,
    ptr: *mut u8,
    size: usize,
    fd: libc::c_int,
    last_used: Option<Instant>,
}

// SAFETY: Same as MemoryRegion - exclusive ownership of mapped memory.
unsafe impl Send for PoolSlot {}
unsafe impl Sync for PoolSlot {}

impl PoolSlot {
    /// Creates a new pool slot with the given path and size.
    fn create(path: &str, size: usize) -> io::Result<Self> {
        let c_path = CString::new(path)
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "path contains null byte"))?;

        // Unlink any existing region first
        unsafe {
            libc::shm_unlink(c_path.as_ptr());
        }

        let fd = unsafe {
            libc::shm_open(
                c_path.as_ptr(),
                libc::O_RDWR | libc::O_CREAT,
                (libc::S_IRUSR | libc::S_IWUSR) as libc::c_uint,
            )
        };

        if fd < 0 {
            return Err(Error::last_os_error());
        }

        if unsafe { libc::ftruncate(fd, size as libc::off_t) } < 0 {
            let err = Error::last_os_error();
            unsafe {
                libc::close(fd);
                libc::shm_unlink(c_path.as_ptr());
            }
            return Err(err);
        }

        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            let err = Error::last_os_error();
            unsafe {
                libc::close(fd);
                libc::shm_unlink(c_path.as_ptr());
            }
            return Err(err);
        }

        Ok(Self {
            path: path.to_string(),
            ptr: ptr as *mut u8,
            size,
            fd,
            last_used: None,
        })
    }

    /// Writes data to the slot.
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if data.len() > self.size {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "data too large: {} bytes exceeds slot size of {} bytes",
                    data.len(),
                    self.size
                ),
            ));
        }

        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.ptr, data.len());
        }

        Ok(())
    }

    /// Unlinks the shared memory path.
    fn unlink(&self) {
        if let Ok(c_path) = CString::new(self.path.as_str()) {
            unsafe {
                libc::shm_unlink(c_path.as_ptr());
            }
        }
    }
}

impl Drop for PoolSlot {
    fn drop(&mut self) {
        // Unmap memory
        if !self.ptr.is_null() {
            unsafe {
                libc::munmap(self.ptr as *mut libc::c_void, self.size);
            }
            self.ptr = ptr::null_mut();
        }

        // Close FD
        if self.fd >= 0 {
            unsafe {
                libc::close(self.fd);
            }
            self.fd = -1;
        }

        // Unlink - pool owns these regions and must clean them up
        self.unlink();
    }
}

/// A fixed pool of pre-allocated memory regions.
///
/// Uses round-robin slot selection to cycle through regions, preventing
/// unbounded region creation which can cause resource leaks in some terminals.
///
/// # Path Format
///
/// Pool slots use the path format: `/kgfx_pool_<pid>_<index>`
pub struct RegionPool {
    slots: Vec<PoolSlot>,
    current_index: usize,
    region_size: usize,
    prefix: String,
    initialized: bool,
    min_reuse_age: Duration,
}

impl RegionPool {
    /// Creates a new region pool with default settings.
    ///
    /// - Pool size: 10 regions
    /// - Region size: 32 MB
    ///
    /// The pool is lazily initialized on first write.
    pub fn new() -> Self {
        Self::with_config(DEFAULT_POOL_SIZE, DEFAULT_REGION_SIZE)
    }

    /// Creates a new region pool with custom settings.
    ///
    /// The pool is lazily initialized on first write.
    pub fn with_config(pool_size: usize, region_size: usize) -> Self {
        let prefix = format!("kgfxv2_pool_{}", std::process::id());
        Self::with_config_and_prefix(pool_size, region_size, prefix, DEFAULT_MIN_REUSE_AGE)
    }

    /// Creates a new region pool with custom settings and reuse age.
    ///
    /// The pool is lazily initialized on first write.
    pub fn with_config_and_reuse_age(
        pool_size: usize,
        region_size: usize,
        min_reuse_age: Duration,
    ) -> Self {
        let prefix = format!("kgfxv2_pool_{}", std::process::id());
        Self::with_config_and_prefix(pool_size, region_size, prefix, min_reuse_age)
    }

    /// Creates a new region pool with custom settings and path prefix.
    ///
    /// Primarily for testing to avoid conflicts between parallel tests.
    fn with_config_and_prefix(
        pool_size: usize,
        region_size: usize,
        prefix: String,
        min_reuse_age: Duration,
    ) -> Self {
        Self {
            slots: Vec::with_capacity(pool_size),
            current_index: 0,
            region_size,
            prefix,
            initialized: false,
            min_reuse_age,
        }
    }

    /// Initializes the pool by creating all regions.
    ///
    /// Called automatically on first write, but can be called explicitly
    /// to detect allocation failures early.
    pub fn initialize(&mut self) -> io::Result<()> {
        if self.initialized {
            return Ok(());
        }

        let pool_size = self.slots.capacity();

        for i in 0..pool_size {
            let path = format!("/{}_{}", self.prefix, i);
            match PoolSlot::create(&path, self.region_size) {
                Ok(slot) => self.slots.push(slot),
                Err(e) => {
                    // Clear any already-created slots
                    self.slots.clear();
                    return Err(Error::new(
                        e.kind(),
                        format!("failed to create pool slot {i}: {e}"),
                    ));
                }
            }
        }

        self.initialized = true;

        let total_mb = (pool_size * self.region_size) as f64 / (1024.0 * 1024.0);
        let region_mb = self.region_size as f64 / (1024.0 * 1024.0);
        info!("pool initialized: {pool_size} slots x {region_mb:.0} MB = {total_mb:.0} MB total");

        Ok(())
    }

    /// Writes data to the next slot and returns its path.
    ///
    /// Uses round-robin selection to cycle through slots. Initializes
    /// the pool on first call if not already initialized.
    ///
    /// Returns `WouldBlock` if all slots are still within the reuse age.
    pub fn write_and_get_path(&mut self, data: &[u8]) -> io::Result<String> {
        self.initialize()?;

        if self.slots.is_empty() {
            return Err(Error::other("pool is empty"));
        }

        let now = Instant::now();
        let len = self.slots.len();

        for _ in 0..len {
            let slot = &mut self.slots[self.current_index];
            let eligible = match slot.last_used {
                Some(last_used) => now.duration_since(last_used) >= self.min_reuse_age,
                None => true,
            };

            if eligible {
                slot.write(data)?;
                slot.last_used = Some(now);
                let path = slot.path.clone();
                self.current_index = (self.current_index + 1) % len;
                return Ok(path);
            }

            self.current_index = (self.current_index + 1) % len;
        }

        Err(Error::new(
            ErrorKind::WouldBlock,
            "all pool slots are in use (min reuse age not elapsed)",
        ))
    }

    /// Returns the region size in bytes.
    pub fn region_size(&self) -> usize {
        self.region_size
    }

    /// Returns the pool size (number of slots).
    pub fn pool_size(&self) -> usize {
        self.slots.capacity()
    }

    /// Returns whether the pool is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Returns the minimum reuse age configured for this pool.
    pub fn min_reuse_age(&self) -> Duration {
        self.min_reuse_age
    }

    /// Clears the pool, unlinking all regions.
    ///
    /// After calling this, the pool will reinitialize on next write.
    pub fn clear(&mut self) {
        let count = self.slots.len();
        self.slots.clear();
        self.current_index = 0;
        self.initialized = false;

        if count > 0 {
            info!("pool cleared: {count} slots released");
        }
    }
}

impl Default for RegionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RegionPool {
    fn drop(&mut self) {
        // Slots will unlink themselves when dropped
        // No additional cleanup needed
    }
}

/// Global region pool instance.
static POOL: OnceLock<Mutex<RegionPool>> = OnceLock::new();

/// Returns a reference to the global region pool.
///
/// The pool is protected by a mutex (separate from the tracker mutex)
/// and lazily initialized on first access.
pub fn pool() -> &'static Mutex<RegionPool> {
    POOL.get_or_init(|| Mutex::new(RegionPool::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    // Unique counter for test isolation when tests run in parallel
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn create_test_pool(pool_size: usize, region_size: usize) -> RegionPool {
        RegionPool::with_config_and_prefix(pool_size, region_size, test_prefix(), Duration::ZERO)
    }

    fn test_prefix() -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("kgfxv2_test{}_{}", std::process::id(), id)
    }

    #[test]
    fn test_pool_initialization() {
        let mut pool = create_test_pool(3, 1024);

        assert!(!pool.is_initialized());

        // Write triggers initialization
        let data = b"test data";
        let path = pool.write_and_get_path(data).expect("write failed");

        assert!(pool.is_initialized());
        assert!(path.contains("kgfxv2_test"));

        pool.clear();
    }

    #[test]
    fn test_round_robin() {
        let mut pool = create_test_pool(3, 1024);

        let path1 = pool.write_and_get_path(b"1").expect("write 1 failed");
        let path2 = pool.write_and_get_path(b"2").expect("write 2 failed");
        let path3 = pool.write_and_get_path(b"3").expect("write 3 failed");
        let path4 = pool.write_and_get_path(b"4").expect("write 4 failed");

        // Paths 1, 2, 3 should all be different
        assert_ne!(path1, path2);
        assert_ne!(path2, path3);
        assert_ne!(path1, path3);

        // Path 4 wraps around to path 1
        assert_eq!(path1, path4);

        pool.clear();
    }

    #[test]
    fn test_size_limit() {
        let mut pool = create_test_pool(2, 100);

        // Data larger than region size should fail
        let large_data = [0u8; 200];
        let err = pool
            .write_and_get_path(&large_data)
            .expect_err("should fail");

        assert!(err.to_string().contains("200"));
        assert!(err.to_string().contains("100"));

        pool.clear();
    }

    #[test]
    fn test_path_format() {
        let mut pool = create_test_pool(2, 1024);

        let path = pool.write_and_get_path(b"test").expect("write failed");

        // Path should start with / and contain our prefix pattern
        assert!(path.starts_with('/'), "path {path} should start with /");
        assert!(
            path.contains("kgfxv2_test"),
            "path {path} should contain kgfxv2_test"
        );

        pool.clear();
    }

    #[test]
    fn test_explicit_initialization() {
        let mut pool = create_test_pool(2, 1024);

        // Explicit initialization
        pool.initialize().expect("init failed");
        assert!(pool.is_initialized());

        // Second init is no-op
        pool.initialize().expect("second init failed");

        pool.clear();
    }

    #[test]
    fn test_default_pool_format() {
        // Test that default pool uses expected path format
        let mut pool = RegionPool::with_config(1, 1024);
        let path = pool.write_and_get_path(b"test").expect("write failed");

        let pid = std::process::id();
        let expected_prefix = format!("/kgfxv2_pool_{pid}_");
        assert!(
            path.starts_with(&expected_prefix),
            "path {path} should start with {expected_prefix}"
        );

        pool.clear();
    }
}
