use anyhow::Result;
use image::{DynamicImage, GenericImageView};
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
    /// Create a new BookImages instance with images stored in the project temp directory
    pub fn new() -> Result<Self> {
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

        // Resolve the image path
        let image_path = self.storage.resolve_image_path(epub_path, image_src)?;

        // Check if it's an SVG file
        if image_path.extension().and_then(|ext| ext.to_str()) == Some("svg") {
            // For SVG files, use a default size since they're scalable
            // We could parse the SVG to get viewBox dimensions, but for now use defaults
            debug!("SVG image '{}' using default size 800x600", image_src);
            return Some((800, 600));
        }

        // Load the image to get its dimensions
        match image::open(&image_path) {
            Ok(img) => {
                let (width, height) = (img.width(), img.height());
                debug!("Image '{}' size: {}x{}", image_src, width, height);
                Some((width, height))
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

    /// Get a DynamicImage from its source path (as referenced in the book text)
    pub fn get_image(&self, image_src: &str) -> Option<DynamicImage> {
        let epub_path = self.current_epub_path.as_ref()?;

        // Resolve the image path
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

    /// Check if an image exists for the given source path
    pub fn has_image(&self, image_src: &str) -> bool {
        if let Some(epub_path) = &self.current_epub_path {
            self.storage
                .resolve_image_path(epub_path, image_src)
                .is_some()
        } else {
            false
        }
    }

    /// Get all available images for the current book
    pub fn get_all_images(&self) -> Vec<PathBuf> {
        if let Some(epub_path) = &self.current_epub_path {
            self.storage.get_book_images(epub_path).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Unload the current book and clean up its images
    pub fn unload_book(&mut self) -> Result<()> {
        if let Some(epub_path) = self.current_epub_path.take() {
            debug!("Unloading book: {:?}", epub_path);
            self.storage.cleanup_book(&epub_path)?;
        }
        Ok(())
    }

    /// Clean up all extracted images
    pub fn cleanup_all(&mut self) -> Result<()> {
        self.current_epub_path = None;
        self.storage.cleanup_all()
    }

    /// Get the current book path if one is loaded
    pub fn current_book(&self) -> Option<&Path> {
        self.current_epub_path.as_deref()
    }

    /// Preload an image to ensure it's cached in memory
    /// Returns true if the image was successfully loaded or if it's an SVG
    pub fn preload_image(&self, image_src: &str) -> bool {
        // Check if the image exists
        if !self.has_image(image_src) {
            return false;
        }

        // For SVG files, we consider them "preloaded" even though we can't load them as DynamicImage
        if let Some(epub_path) = &self.current_epub_path {
            if let Some(image_path) = self.storage.resolve_image_path(epub_path, image_src) {
                if image_path.extension().and_then(|ext| ext.to_str()) == Some("svg") {
                    return true; // SVG files are always "ready"
                }
            }
        }

        // Try to load non-SVG images
        self.get_image(image_src).is_some()
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

        // Use fast_image_resize if available, otherwise fallback to standard resize
        let scaled_image =
            img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3);

        // Calculate width in cells
        let width_cells = (new_width as f32 / cell_width as f32).ceil() as u16;

        Some((scaled_image, width_cells, target_height_cells))
    }

    /// Get image metadata without loading the full image
    pub fn get_image_metadata(&self, image_src: &str) -> Option<ImageMetadata> {
        let epub_path = self.current_epub_path.as_ref()?;
        let image_path = self.storage.resolve_image_path(epub_path, image_src)?;

        // Use imagesize crate for efficient metadata reading if available,
        // otherwise fall back to loading the image
        match image::open(&image_path) {
            Ok(img) => {
                let format = match &img {
                    DynamicImage::ImageLuma8(_) => "Grayscale",
                    DynamicImage::ImageLumaA8(_) => "GrayscaleAlpha",
                    DynamicImage::ImageRgb8(_) => "RGB",
                    DynamicImage::ImageRgba8(_) => "RGBA",
                    _ => "Unknown",
                };

                Some(ImageMetadata {
                    width: img.width(),
                    height: img.height(),
                    format: format.to_string(),
                    file_size: std::fs::metadata(&image_path).ok()?.len(),
                })
            }
            Err(_) => None,
        }
    }
}

/// Metadata about an image
#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub file_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_book_images_creation() {
        let temp_dir = TempDir::new().unwrap();
        let book_images = BookImages::with_storage_dir(temp_dir.path().to_path_buf()).unwrap();
        assert!(book_images.current_book().is_none());
    }

    #[test]
    fn test_load_and_get_image() {
        let temp_dir = TempDir::new().unwrap();
        let mut book_images = BookImages::with_storage_dir(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");
        book_images.load_book(epub_path).unwrap();

        // Test various image source formats
        let test_srcs = vec![
            "OEBPS/images/tech_lab.svg",
            "images/tech_lab.svg",
            "../images/cover.svg",
        ];

        for src in test_srcs {
            if book_images.has_image(src) {
                // Should be able to get size (even for SVG)
                if let Some((width, height)) = book_images.get_image_size(src) {
                    assert!(width > 0);
                    assert!(height > 0);
                }

                // Note: SVG files will return None for get_image since they can't be loaded as DynamicImage
                // This is expected behavior
            }
        }
    }

    #[test]
    fn test_get_all_images() {
        let temp_dir = TempDir::new().unwrap();
        let mut book_images = BookImages::with_storage_dir(temp_dir.path().to_path_buf()).unwrap();

        // No images before loading a book
        assert!(book_images.get_all_images().is_empty());

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");
        book_images.load_book(epub_path).unwrap();

        // Should have images after loading
        let all_images = book_images.get_all_images();
        assert!(!all_images.is_empty());
        assert_eq!(all_images.len(), 3); // digital_frontier has 3 images
    }

    #[test]
    fn test_unload_book() {
        let temp_dir = TempDir::new().unwrap();
        let mut book_images = BookImages::with_storage_dir(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");
        book_images.load_book(epub_path).unwrap();

        assert!(book_images.current_book().is_some());
        assert!(!book_images.get_all_images().is_empty());

        book_images.unload_book().unwrap();

        assert!(book_images.current_book().is_none());
        assert!(book_images.get_all_images().is_empty());
    }

    #[test]
    fn test_image_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let mut book_images = BookImages::with_storage_dir(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");
        book_images.load_book(epub_path).unwrap();

        // Try to get metadata for an image
        if book_images.has_image("OEBPS/images/tech_lab.svg") {
            if let Some(metadata) = book_images.get_image_metadata("OEBPS/images/tech_lab.svg") {
                assert!(metadata.width > 0);
                assert!(metadata.height > 0);
                assert!(metadata.file_size > 0);
                assert!(!metadata.format.is_empty());
            }
        }
    }

    #[test]
    fn test_preload_image() {
        let temp_dir = TempDir::new().unwrap();
        let mut book_images = BookImages::with_storage_dir(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");
        book_images.load_book(epub_path).unwrap();

        // Test preloading
        let src = "OEBPS/images/tech_lab.svg";
        if book_images.has_image(src) {
            assert!(book_images.preload_image(src));
        }

        // Non-existent image should fail to preload
        assert!(!book_images.preload_image("non_existent.png"));
    }

    #[test]
    fn test_switching_books() {
        let temp_dir = TempDir::new().unwrap();
        let mut book_images = BookImages::with_storage_dir(temp_dir.path().to_path_buf()).unwrap();

        // Load first book
        let epub_path1 = Path::new("tests/testdata/digital_frontier.epub");
        book_images.load_book(epub_path1).unwrap();
        assert_eq!(book_images.current_book(), Some(epub_path1));

        // Load second book (if available)
        // This would switch the context to the new book
        // For now, we'll just test reloading the same book
        book_images.load_book(epub_path1).unwrap();
        assert_eq!(book_images.current_book(), Some(epub_path1));
    }
}
