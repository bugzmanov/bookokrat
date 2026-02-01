use anyhow::Result;
use base64_simd;
use fast_image_resize as fr;
use image::{DynamicImage, GenericImageView, ImageBuffer};
use imagesize;
use log::{debug, warn};
#[cfg(feature = "svg")]
use resvg::tiny_skia::Pixmap;
#[cfg(feature = "svg")]
use resvg::usvg;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::image_storage::ImageStorage;

/// Abstraction for managing book images
/// Encapsulates the relationship with ImageStorage and provides
/// a clean API for working with book images
#[derive(Clone)]
pub struct BookImages {
    storage: Arc<ImageStorage>,
    current_epub_path: Option<PathBuf>,
}

impl BookImages {
    const SVG_PADDING_PX: u32 = 10;
    /// Create a new BookImages instance with the provided ImageStorage
    pub fn new(storage: Arc<ImageStorage>) -> Self {
        Self {
            storage,
            current_epub_path: None,
        }
    }

    /// Load images for a specific EPUB book
    pub fn load_book(&mut self, epub_path: &Path) -> Result<()> {
        debug!("Loading images for book: {epub_path:?}");

        // Extract images if not already extracted
        self.storage.extract_images(epub_path)?;

        // Store the current book path
        self.current_epub_path = Some(epub_path.to_path_buf());

        Ok(())
    }

    /// Get the size of an image with chapter context for better path resolution
    pub fn get_image_size_with_context(
        &self,
        image_src: &str,
        chapter_path: Option<&str>,
    ) -> Option<(u32, u32)> {
        let epub_path = self.current_epub_path.as_ref()?;

        if let Some((svg_data, resources_dir)) =
            self.svg_data_and_resources_dir(epub_path, image_src, chapter_path)
        {
            if let Some((width, height)) =
                self.svg_intrinsic_size(&svg_data, resources_dir.as_deref())
            {
                debug!("SVG image '{image_src}' size: {width}x{height}");
                return Some((width, height));
            }

            debug!("SVG image '{image_src}' using default size 800x600");
            return Some((800, 600));
        }

        // Delegate path resolution to ImageStorage with chapter context
        let image_path =
            self.storage
                .resolve_image_path_with_context(epub_path, image_src, chapter_path)?;

        // Get image dimensions without loading the full image data
        match imagesize::size(&image_path) {
            Ok(size) => {
                let (width, height) = (size.width as u32, size.height as u32);
                debug!("Image '{image_src}' size: {width}x{height}");
                Some((width, height))
            }
            Err(e) => {
                warn!("Failed to get image size for '{image_src}' from {image_path:?}: {e}");
                None
            }
        }
    }

    /// Get a DynamicImage from its source path (as referenced in the book text)
    pub fn get_image(&self, image_src: &str) -> Option<DynamicImage> {
        self.get_image_with_context(image_src, None)
    }

    /// Get a DynamicImage with chapter context for better path resolution
    pub fn get_image_with_context(
        &self,
        image_src: &str,
        chapter_path: Option<&str>,
    ) -> Option<DynamicImage> {
        let epub_path = self.current_epub_path.as_ref()?;

        if let Some((svg_data, resources_dir)) =
            self.svg_data_and_resources_dir(epub_path, image_src, chapter_path)
        {
            return self.render_svg_to_image(image_src, &svg_data, resources_dir.as_deref(), None);
        }

        // Delegate path resolution to ImageStorage with chapter context
        let image_path =
            self.storage
                .resolve_image_path_with_context(epub_path, image_src, chapter_path)?;

        // Load and return the image
        match image::open(&image_path) {
            Ok(img) => {
                debug!("Successfully loaded image: {image_src}");
                Some(img)
            }
            Err(e) => {
                warn!("Failed to load image '{image_src}' from {image_path:?}: {e}");
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
        self.load_and_resize_image_with_context(
            image_src,
            target_height_cells,
            cell_width,
            cell_height,
            None,
        )
    }

    /// Load and resize an image with chapter context
    pub fn load_and_resize_image_with_context(
        &self,
        image_src: &str,
        target_height_cells: u16,
        cell_width: u16,
        cell_height: u16,
        chapter_path: Option<&str>,
    ) -> Option<(DynamicImage, u16, u16)> {
        let epub_path = self.current_epub_path.as_ref()?;

        if let Some((svg_data, resources_dir)) =
            self.svg_data_and_resources_dir(epub_path, image_src, chapter_path)
        {
            if let Some((width, height)) =
                self.svg_intrinsic_size(&svg_data, resources_dir.as_deref())
            {
                let target_height_in_pixels = target_height_cells as u32 * cell_height as u32;
                let padding = Self::SVG_PADDING_PX;
                let inner_height = target_height_in_pixels.saturating_sub(padding * 2);
                if inner_height == 0 {
                    warn!("SVG target height too small for padding: {image_src}");
                    return None;
                }

                let scale = inner_height as f32 / height as f32;
                let inner_width = (width as f32 * scale) as u32;
                let new_width = inner_width + padding * 2;
                let new_height = inner_height + padding * 2;
                let width_cells = (new_width as f32 / cell_width as f32).ceil() as u16;

                if let Some(rendered) = self.render_svg_to_image(
                    image_src,
                    &svg_data,
                    resources_dir.as_deref(),
                    Some((new_width, new_height)),
                ) {
                    return Some((rendered, width_cells, target_height_cells));
                }

                warn!("Failed to render SVG image: {image_src}");
                return None;
            }
        }

        // Get the image with chapter context
        let img = self.get_image_with_context(image_src, chapter_path)?;

        let (img_width, img_height) = img.dimensions();

        // Calculate target dimensions for scaling
        let target_height_in_pixels = target_height_cells as u32 * cell_height as u32;
        let scale = target_height_in_pixels as f32 / img_height as f32;
        let new_width = (img_width as f32 * scale) as u32;
        let new_height = target_height_in_pixels;

        debug!("Resizing {image_src} from {img_width}x{img_height} to {new_width}x{new_height}");

        // Use fast_image_resize for better performance
        let scaled_image = match self.fast_resize_image(&img, new_width, new_height) {
            Ok(resized) => resized,
            Err(e) => {
                warn!("Fast resize failed for {image_src}: {e}, falling back to slow resize");
                img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
            }
        };

        // Calculate width in cells
        let width_cells = (new_width as f32 / cell_width as f32).ceil() as u16;

        Some((scaled_image, width_cells, target_height_cells))
    }

    /// Resize an image to specific dimensions using fast_image_resize
    pub fn resize_image_to(
        &self,
        src_image: &DynamicImage,
        new_width: u32,
        new_height: u32,
    ) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        self.fast_resize_image(src_image, new_width, new_height)
    }

    fn svg_data_and_resources_dir(
        &self,
        epub_path: &Path,
        image_src: &str,
        chapter_path: Option<&str>,
    ) -> Option<(Vec<u8>, Option<PathBuf>)> {
        if let Some(svg_data) = Self::decode_svg_data_uri(image_src) {
            let resources_dir = chapter_path.and_then(|chapter| {
                self.storage
                    .resolve_chapter_dir_with_context(epub_path, chapter)
            });
            return Some((svg_data, resources_dir));
        }

        let image_path =
            self.storage
                .resolve_image_path_with_context(epub_path, image_src, chapter_path)?;

        if image_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("svg"))
            != Some(true)
        {
            return None;
        }

        let svg_data = std::fs::read(&image_path).ok()?;
        let resources_dir = image_path.parent().map(|path| path.to_path_buf());
        Some((svg_data, resources_dir))
    }

    #[cfg(feature = "svg")]
    fn svg_intrinsic_size(
        &self,
        svg_data: &[u8],
        resources_dir: Option<&Path>,
    ) -> Option<(u32, u32)> {
        let tree = self.parse_svg_tree(svg_data, resources_dir)?;
        let size = tree.size().to_int_size();
        let width = size.width();
        let height = size.height();
        if width == 0 || height == 0 {
            None
        } else {
            Some((width, height))
        }
    }

    #[cfg(not(feature = "svg"))]
    fn svg_intrinsic_size(
        &self,
        _svg_data: &[u8],
        _resources_dir: Option<&Path>,
    ) -> Option<(u32, u32)> {
        warn!("SVG support disabled; enable feature \"svg\" to render SVG images.");
        None
    }

    #[cfg(feature = "svg")]
    fn render_svg_to_image(
        &self,
        image_src: &str,
        svg_data: &[u8],
        resources_dir: Option<&Path>,
        target_size: Option<(u32, u32)>,
    ) -> Option<DynamicImage> {
        let tree = self.parse_svg_tree(svg_data, resources_dir)?;
        let base_size = tree.size().to_int_size();
        let base_width = base_size.width();
        let base_height = base_size.height();
        let (width, height) = target_size.unwrap_or((
            base_width.saturating_add(Self::SVG_PADDING_PX * 2),
            base_height.saturating_add(Self::SVG_PADDING_PX * 2),
        ));

        if width == 0 || height == 0 || base_width == 0 || base_height == 0 {
            warn!("SVG image has invalid size: {image_src}");
            return None;
        }

        let padding = Self::svg_padding_for_size(width, height);
        let inner_width = width.saturating_sub(padding * 2);
        let inner_height = height.saturating_sub(padding * 2);
        if inner_width == 0 || inner_height == 0 {
            warn!("SVG image too small for padding: {image_src}");
            return None;
        }

        let mut pixmap = Pixmap::new(width, height)?;
        let scale_x = inner_width as f32 / base_width as f32;
        let scale_y = inner_height as f32 / base_height as f32;
        let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y)
            .post_translate(padding as f32, padding as f32);
        let mut pixmap_mut = pixmap.as_mut();
        resvg::render(&tree, transform, &mut pixmap_mut);

        let mut rgba = pixmap.data().to_vec();
        Self::unpremultiply_rgba(&mut rgba);
        let image_buffer = ImageBuffer::from_raw(width, height, rgba)?;
        Some(DynamicImage::ImageRgba8(image_buffer))
    }

    #[cfg(not(feature = "svg"))]
    fn render_svg_to_image(
        &self,
        image_src: &str,
        _svg_data: &[u8],
        _resources_dir: Option<&Path>,
        _target_size: Option<(u32, u32)>,
    ) -> Option<DynamicImage> {
        warn!("SVG support disabled; cannot render '{image_src}'. Enable feature \"svg\".");
        None
    }

    #[cfg(feature = "svg")]
    fn parse_svg_tree(&self, svg_data: &[u8], resources_dir: Option<&Path>) -> Option<usvg::Tree> {
        let mut options = usvg::Options::default();
        if let Some(dir) = resources_dir {
            options.resources_dir = Some(dir.to_path_buf());
        }

        if Self::svg_needs_fonts(svg_data) {
            options.fontdb_mut().load_system_fonts();
        }

        usvg::Tree::from_data(svg_data, &options).ok()
    }

    #[cfg(feature = "svg")]
    fn svg_needs_fonts(svg_data: &[u8]) -> bool {
        let svg_text = String::from_utf8_lossy(svg_data);
        svg_text.contains("<text") || svg_text.contains("font-family")
    }

    fn decode_svg_data_uri(data_uri: &str) -> Option<Vec<u8>> {
        if !data_uri.starts_with("data:image/svg+xml") {
            return None;
        }

        let (meta, data) = data_uri.split_once(',')?;
        if meta.contains(";base64") {
            base64_simd::STANDARD.decode_to_vec(data.trim()).ok()
        } else {
            Some(Self::percent_decode_bytes(data))
        }
    }

    fn percent_decode_bytes(input: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        let mut chars = input.as_bytes().iter().copied().peekable();

        while let Some(b) = chars.next() {
            if b == b'%' {
                let hi = chars.next();
                let lo = chars.next();
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    let hex = [hi, lo];
                    if let Ok(hex_str) = std::str::from_utf8(&hex) {
                        if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                            out.push(byte);
                            continue;
                        }
                    }
                    out.push(b'%');
                    out.push(hi);
                    out.push(lo);
                } else {
                    out.push(b'%');
                    if let Some(hi) = hi {
                        out.push(hi);
                    }
                    if let Some(lo) = lo {
                        out.push(lo);
                    }
                }
            } else {
                out.push(b);
            }
        }

        out
    }

    #[cfg(feature = "svg")]
    fn svg_padding_for_size(width: u32, height: u32) -> u32 {
        let max_pad = width.min(height) / 4;
        Self::SVG_PADDING_PX.min(max_pad)
    }

    fn unpremultiply_rgba(data: &mut [u8]) {
        for pixel in data.chunks_mut(4) {
            let alpha = pixel[3];
            if alpha == 0 {
                pixel[0] = 0;
                pixel[1] = 0;
                pixel[2] = 0;
                continue;
            }
            let a = alpha as u32;
            pixel[0] = ((pixel[0] as u32 * 255 + a / 2) / a).min(255) as u8;
            pixel[1] = ((pixel[1] as u32 * 255 + a / 2) / a).min(255) as u8;
            pixel[2] = ((pixel[2] as u32 * 255 + a / 2) / a).min(255) as u8;
        }
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
mod tests {}
