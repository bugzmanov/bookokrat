//! Image types for kittyv2 protocol.
//!
//! These types are used to represent images ready for transmission via
//! the Kitty graphics protocol.

use std::borrow::Cow;
use std::num::NonZeroU32;

use image::DynamicImage;

use super::kgfx::{MemoryRegion, ShmLease};

/// Image dimensions in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

/// Kitty graphics protocol transmission types.
#[derive(Clone, Debug)]
pub enum Transmission<'a> {
    /// Shared memory transmission (`t=s`).
    SharedMemory {
        /// Owned SHM lease until transmission handoff.
        shm: Option<ShmLease>,
    },
    /// Direct (inline) transmission (`t=d`).
    Direct {
        /// Optional chunk size for splitting inline payloads.
        chunk_size: Option<usize>,
        /// Raw pixel data (RGB).
        data: Cow<'a, [u8]>,
    },
}

/// Image data ready for transmission via the kitty graphics protocol.
#[derive(Clone, Debug)]
pub struct Image<'a> {
    /// Image identifier.
    pub id: ImageId,
    /// Image dimensions.
    pub dimensions: Dimensions,
    /// Transmission type.
    pub transmission: Transmission<'a>,
}

impl Image<'_> {
    /// Create an image for direct (inline) transmission.
    pub fn from_dynamic(img: DynamicImage, id: ImageId) -> Image<'static> {
        let rgb = img.to_rgb8();
        let dimensions = Dimensions {
            width: rgb.width(),
            height: rgb.height(),
        };
        let data = rgb.into_raw();

        Image {
            id,
            dimensions,
            transmission: Transmission::Direct {
                chunk_size: None,
                data: Cow::Owned(data),
            },
        }
    }

    /// Create an image for direct (inline) transmission from raw RGB bytes.
    pub fn from_rgb_bytes(data: Vec<u8>, width: u32, height: u32, id: ImageId) -> Image<'static> {
        Image {
            id,
            dimensions: Dimensions { width, height },
            transmission: Transmission::Direct {
                chunk_size: None,
                data: Cow::Owned(data),
            },
        }
    }

    /// Create an image for shared memory transmission (`t=s`).
    ///
    /// Uses POSIX shared memory for zero-copy transmission.
    pub fn create_shm_from(
        img: DynamicImage,
        shm_name: &str,
        id: ImageId,
    ) -> Result<(Image<'static>, usize), std::io::Error> {
        let rgb = img.to_rgb8();
        let width = rgb.width();
        let height = rgb.height();
        let data = rgb.into_raw();
        Self::create_shm_from_rgb(&data, width, height, shm_name, id)
    }

    /// Create an image for shared memory transmission (`t=s`) from raw RGB bytes.
    pub fn create_shm_from_rgb(
        data: &[u8],
        width: u32,
        height: u32,
        shm_name: &str,
        id: ImageId,
    ) -> Result<(Image<'static>, usize), std::io::Error> {
        let expected = width
            .checked_mul(height)
            .and_then(|v| v.checked_mul(3))
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "RGB size overflow")
            })? as usize;
        if data.len() != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "RGB buffer size mismatch",
            ));
        }
        let size = data.len();

        // Create SHM using kittyv2's MemoryRegion
        let mut region = MemoryRegion::create(shm_name, size)?;
        region.write(data)?;
        let path = region.path().to_string();

        // Close FD but keep the SHM file for terminal to read
        region.close_fd();
        drop(region);
        Ok((
            Image {
                id,
                dimensions: Dimensions { width, height },
                transmission: Transmission::SharedMemory {
                    shm: Some(ShmLease::new(path, size)),
                },
            },
            size,
        ))
    }

    /// Get the image dimensions.
    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }
}

/// Wrapper for image ID in the Kitty protocol.
///
/// Uses the same ID for both image_id and placement_id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageId {
    /// The image ID.
    pub id: NonZeroU32,
}

impl ImageId {
    #[must_use]
    pub const fn new(id: NonZeroU32) -> Self {
        Self { id }
    }
}

/// State of an image in the protocol pipeline.
#[derive(Debug)]
pub enum ImageState {
    /// Image is uploaded to terminal memory.
    Uploaded(ImageId),
    /// Image is queued for transmission.
    Queued(Image<'static>),
}

impl ImageState {
    /// Check if image is already uploaded.
    #[must_use]
    pub fn is_uploaded(&self) -> bool {
        matches!(self, Self::Uploaded(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_shm_path(tag: &str) -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        // Keep SHM names short for platforms with tight NAME_MAX limits.
        format!("/bk_{tag}_{:x}", id)
    }

    fn should_skip_shm_test(err: &std::io::Error) -> bool {
        matches!(
            err.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
        )
    }

    struct ShmCleanup {
        path: String,
    }

    impl ShmCleanup {
        fn new(path: String) -> Self {
            Self { path }
        }
    }

    impl Drop for ShmCleanup {
        fn drop(&mut self) {
            if let Ok(c_path) = CString::new(self.path.as_str()) {
                unsafe {
                    libc::shm_unlink(c_path.as_ptr());
                }
            }
        }
    }

    fn shm_exists(path: &str) -> bool {
        let Ok(c_path) = CString::new(path) else {
            return false;
        };

        let fd = unsafe { libc::shm_open(c_path.as_ptr(), libc::O_RDONLY, 0) };
        if fd < 0 {
            return false;
        }

        unsafe {
            libc::close(fd);
        }
        true
    }

    #[test]
    fn queued_image_drop_unlinks_shm() {
        let path = unique_shm_path("queued-drop");
        let data = [255u8, 0, 0];
        let id = ImageId::new(NonZeroU32::new(1).unwrap());

        let (img, _) = match Image::create_shm_from_rgb(&data, 1, 1, &path, id) {
            Ok(v) => v,
            Err(e) if should_skip_shm_test(&e) => return,
            Err(e) => panic!("create SHM: {e}"),
        };
        assert!(shm_exists(&path), "SHM should exist before drop: {path}");

        let state = ImageState::Queued(img);
        drop(state);

        assert!(
            !shm_exists(&path),
            "queued image drop should unlink SHM: {path}"
        );
    }

    #[test]
    fn handed_off_lease_keeps_shm_until_tracker_unlinks() {
        let path = unique_shm_path("handoff");
        let _cleanup = ShmCleanup::new(path.clone());
        let data = [0u8, 255, 0];
        let id = ImageId::new(NonZeroU32::new(2).unwrap());

        let (mut img, _) = match Image::create_shm_from_rgb(&data, 1, 1, &path, id) {
            Ok(v) => v,
            Err(e) if should_skip_shm_test(&e) => return,
            Err(e) => panic!("create SHM: {e}"),
        };
        assert!(shm_exists(&path), "SHM should exist before handoff: {path}");

        let mut local_tracker = crate::pdf::kittyv2::kgfx::LifecycleTracker::new();
        if let Transmission::SharedMemory { shm } = &mut img.transmission {
            let lease = shm.take().expect("expected SHM lease");
            lease.handoff_to_tracker(0, &mut local_tracker);
        } else {
            panic!("expected shared memory transmission");
        }
        drop(img);

        assert!(
            shm_exists(&path),
            "handed-off lease should not unlink SHM (tracker owns cleanup): {path}"
        );

        local_tracker.cleanup_all();
    }
}
