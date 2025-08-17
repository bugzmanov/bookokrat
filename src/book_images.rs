use anyhow::Result;
use fast_image_resize as fr;
use image::{DynamicImage, GenericImageView, ImageBuffer};
use imagesize;
use log::{debug, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::image_storage::ImageStorage;

/// Abstraction for managing book images
/// Encapsulates the relationship with ImageStorage and provides
/// a clean API for working with book images
#[derive(Clone)]
pub struct BookImages {
    storage: Arc<ImageStorage>,
    current_epub_path: Option<PathBuf>,
}

impl BookImages {
    /// Create a new BookImages instance with the provided ImageStorage
    pub fn new(storage: Arc<ImageStorage>) -> Self {
        Self {
            storage,
            current_epub_path: None,
        }
    }

    /// Create a new BookImages instance with images stored in the project temp directory
    pub fn new_in_project_temp() -> Result<Self> {
        let storage = ImageStorage::new_in_project_temp()?;
        Ok(Self {
            storage: Arc::new(storage),
            current_epub_path: None,
        })
    }

    /// Create a new BookImages instance with a custom storage directory
    pub fn with_storage_dir(base_dir: PathBuf) -> Result<Self> {
        let storage = ImageStorage::new(base_dir)?;
        Ok(Self {
            storage: Arc::new(storage),
            current_epub_path: None,
        })
    }

    /// Load images for a specific EPUB book
    pub fn load_book(&mut self, epub_path: &Path) -> Result<()> {
        debug!("Loading images for book: {:?}", epub_path);

        // Extract images if not already extracted
        self.storage.extract_images(epub_path)?;

        // Store the current book path
        self.current_epub_path = Some(epub_path.to_path_buf());

        Ok(())
    }

    /// Get the size of an image from its source path (as referenced in the book text)
    /// Returns (width, height) if the image exists and can be loaded
    pub fn get_image_size(&self, image_src: &str) -> Option<(u32, u32)> {
        let epub_path = self.current_epub_path.as_ref()?;

        // Delegate path resolution to ImageStorage
        let image_path = self.storage.resolve_image_path(epub_path, image_src)?;

        // Check if it's an SVG file
        if image_path.extension().and_then(|ext| ext.to_str()) == Some("svg") {
            // For SVG files, use a default size since they're scalable
            // We could parse the SVG to get viewBox dimensions, but for now use defaults
            debug!("SVG image '{}' using default size 800x600", image_src);
            return Some((800, 600));
        }

        // Get image dimensions without loading the full image data
        match imagesize::size(&image_path) {
            Ok(size) => {
                let (width, height) = (size.width as u32, size.height as u32);
                debug!("Image '{}' size: {}x{}", image_src, width, height);
                Some((width, height))
            }
            Err(e) => {
                warn!(
                    "Failed to get image size for '{}' from {:?}: {}",
                    image_src, image_path, e
                );
                None
            }
        }
    }

    /// Get a DynamicImage from its source path (as referenced in the book text)
    pub fn get_image(&self, image_src: &str) -> Option<DynamicImage> {
        let epub_path = self.current_epub_path.as_ref()?;

        // Delegate path resolution to ImageStorage
        let image_path = self.storage.resolve_image_path(epub_path, image_src)?;

        // Check if it's an SVG file
        if image_path.extension().and_then(|ext| ext.to_str()) == Some("svg") {
            // For SVG files, we can't load them as DynamicImage
            // Return None or consider rasterizing the SVG
            debug!("SVG images not supported for DynamicImage: {}", image_src);
            return None;
        }

        // Load and return the image
        match image::open(&image_path) {
            Ok(img) => {
                debug!("Successfully loaded image: {}", image_src);
                Some(img)
            }
            Err(e) => {
                warn!(
                    "Failed to load image '{}' from {:?}: {}",
                    image_src, image_path, e
                );
                None
            }
        }
    }

    /// Load and resize an image for display
    /// Returns (resized_image, width_cells, height_cells)
    pub fn load_and_resize_image(
        &self,
        image_src: &str,
        target_height_cells: u16,
        cell_width: u16,
        cell_height: u16,
    ) -> Option<(DynamicImage, u16, u16)> {
        // Get the image
        let img = self.get_image(image_src)?;

        let (img_width, img_height) = img.dimensions();

        // Calculate target dimensions for scaling
        let target_height_in_pixels = target_height_cells as u32 * cell_height as u32;
        let scale = target_height_in_pixels as f32 / img_height as f32;
        let new_width = (img_width as f32 * scale) as u32;
        let new_height = target_height_in_pixels;

        debug!(
            "Resizing {} from {}x{} to {}x{}",
            image_src, img_width, img_height, new_width, new_height
        );

        // Use fast_image_resize for better performance
        let scaled_image = match self.fast_resize_image(&img, new_width, new_height) {
            Ok(resized) => resized,
            Err(e) => {
                warn!(
                    "Fast resize failed for {}: {}, falling back to slow resize",
                    image_src, e
                );
                img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
            }
        };

        // Calculate width in cells
        let width_cells = (new_width as f32 / cell_width as f32).ceil() as u16;

        Some((scaled_image, width_cells, target_height_cells))
    }

    /// Fast resize using fast_image_resize crate for better performance
    fn fast_resize_image(
        &self,
        src_image: &DynamicImage,
        new_width: u32,
        new_height: u32,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        // Convert to RGBA8 for processing
        let src_rgba = src_image.to_rgba8();
        let (src_width, src_height) = src_rgba.dimensions();

        // Create source image view
        let src_image_view = fr::Image::from_vec_u8(
            std::num::NonZeroU32::new(src_width).ok_or("Invalid width")?,
            std::num::NonZeroU32::new(src_height).ok_or("Invalid height")?,
            src_rgba.into_raw(),
            fr::PixelType::U8x4,
        )?;

        // Create destination image
        let dst_width = std::num::NonZeroU32::new(new_width).ok_or("Invalid target width")?;
        let dst_height = std::num::NonZeroU32::new(new_height).ok_or("Invalid target height")?;
        let mut dst_image = fr::Image::new(dst_width, dst_height, fr::PixelType::U8x4);

        // Create resizer with Lanczos3 algorithm for quality
        let mut resizer = fr::Resizer::new(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3));
        resizer.resize(&src_image_view.view(), &mut dst_image.view_mut())?;

        // Convert back to DynamicImage
        let dst_buffer = dst_image.into_vec();
        let image_buffer = ImageBuffer::from_raw(new_width, new_height, dst_buffer)
            .ok_or("Failed to create ImageBuffer")?;

        Ok(DynamicImage::ImageRgba8(image_buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
}
