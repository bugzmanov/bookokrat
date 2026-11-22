//! MuPDF page programs shared between a render worker and the viewport encoder.

use image::{DynamicImage, RgbImage, RgbaImage};
use mupdf::{Colorspace, Device, DisplayList, IRect, Matrix, Pixmap};

use super::WorkerFault;
use super::worker::PagePaint;

/// Recorded page contents and scale-dependent paint metadata, not a page raster.
/// Only the display list, never the originating document or page, crosses threads.
pub struct CompiledPage {
    list: DisplayList,
    transform: Matrix,
    bbox: IRect,
    paint: PagePaint,
}

impl CompiledPage {
    pub(super) fn new(list: DisplayList, transform: Matrix, bbox: IRect, paint: PagePaint) -> Self {
        Self {
            list,
            transform,
            bbox,
            paint,
        }
    }

    /// Render a crop in full-page raster coordinates, truncating at the page edges.
    pub(crate) fn render_region(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<DynamicImage, WorkerFault> {
        let width = width.min((self.bbox.width() as u32).saturating_sub(x));
        let height = height.min((self.bbox.height() as u32).saturating_sub(y));
        if width == 0 || height == 0 {
            return Err(WorkerFault::generic("Empty PDF viewport region"));
        }

        // Preserve device-space coordinates (including the page's own origin).
        // MuPDF can then paint with the original transform into just this band.
        let left = self.bbox.x0 + x as i32;
        let top = self.bbox.y0 + y as i32;
        let clip = IRect::new(left, top, left + width as i32, top + height as i32);
        let rgb = Colorspace::device_rgb();
        let mut pixmap = Pixmap::new_with_rect(&rgb, clip, self.paint.transparent)?;
        if self.paint.transparent {
            pixmap.clear()?;
        } else {
            pixmap.clear_with(255)?;
        }
        {
            let device = Device::from_pixmap(&pixmap)?;
            self.list.run(&device, &self.transform, clip.into())?;
        }
        let (pixels, channels) = self.paint.pixels(&mut pixmap)?;
        let image = match channels {
            3 => RgbImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgb8),
            4 => RgbaImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgba8),
            _ => None,
        };
        image.ok_or_else(|| WorkerFault::generic("Invalid PDF viewport pixel data"))
    }
}
