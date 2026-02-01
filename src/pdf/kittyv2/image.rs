//! Image types for kittyv2 protocol.
//!
//! These types are used to represent images ready for transmission via
//! the Kitty graphics protocol.

use std::borrow::Cow;
use std::num::NonZeroU32;

use image::DynamicImage;

use super::kgfx::MemoryRegion;

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
        /// POSIX shared memory name (e.g., "/kgfx_12345").
        path: String,
        /// Shared memory size in bytes.
        size: usize,
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
                transmission: Transmission::SharedMemory { path, size },
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
