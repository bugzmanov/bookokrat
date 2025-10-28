//! Protocol backends for the widgets

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use image::{DynamicImage, ImageBuffer, Rgba, imageops};
use ratatui::{buffer::Buffer, layout::Rect};

use self::{
    halfblocks::Halfblocks,
    iterm2::Iterm2,
    kitty::{Kitty, StatefulKitty},
    sixel::Sixel,
};
use super::{FontSize, ResizeEncodeRender, Result, ViewportOptions};

use super::Resize;

pub mod halfblocks;
pub mod iterm2;
pub mod kitty;
pub mod sixel;

trait ProtocolTrait: Send + Sync {
    /// Render the currently resized and encoded data to the buffer.
    fn render(&self, area: Rect, buf: &mut Buffer);

    // Get the area of the image.
    #[allow(dead_code)]
    fn area(&self) -> Rect;
}

trait StatefulProtocolTrait: ProtocolTrait {
    /// Resize the image and encode it for rendering. The result should be stored statefully so
    /// that next call for the given area does not need to redo the work.
    ///
    /// This can be done in a background thread, and the result is stored in this [StatefulProtocol].
    fn resize_encode(&mut self, img: DynamicImage, area: Rect) -> Result<()>;
}

/// A fixed-size image protocol for the [crate::Image] widget.
#[derive(Clone)]
pub enum Protocol {
    Halfblocks(Halfblocks),
    Sixel(Sixel),
    Kitty(Kitty),
    ITerm2(Iterm2),
}

impl Protocol {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        let inner: &dyn ProtocolTrait = match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        };
        inner.render(area, buf);
    }
    pub fn area(&self) -> Rect {
        let inner: &dyn ProtocolTrait = match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        };
        inner.area()
    }
}

/// A stateful resizing image protocol for the [crate::StatefulImage] widget.
///
/// The [crate::thread::ThreadProtocol] widget also uses this, and is the reason why resizing is
/// split from rendering.
pub struct StatefulProtocol {
    source: ImageSource,
    font_size: FontSize,
    hash: u64,
    protocol_type: StatefulProtocolType,
    last_encoding_result: Option<Result<()>>,
    // Cache the last resize type to detect if we're just scrolling
    last_resize: Option<Resize>,
    // Cache for viewport strips - maps (y_offset, width, height) to encoded protocol
    // We DON'T include position because we need to render cached viewports at different positions
    viewport_cache: std::collections::HashMap<(u32, u16, u16), StatefulProtocolType>,
    // Track last viewport options to detect scrolling
    last_viewport: Option<ViewportOptions>,
}

#[derive(Clone)]
pub enum StatefulProtocolType {
    Halfblocks(Halfblocks),
    Sixel(Sixel),
    Kitty(StatefulKitty),
    ITerm2(Iterm2),
}

impl StatefulProtocolType {
    fn inner_trait(&self) -> &dyn StatefulProtocolTrait {
        match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        }
    }
    fn inner_trait_mut(&mut self) -> &mut dyn StatefulProtocolTrait {
        match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        }
    }
}

impl StatefulProtocol {
    pub fn new(
        source: ImageSource,
        font_size: FontSize,
        protocol_type: StatefulProtocolType,
    ) -> Self {
        Self {
            source,
            font_size,
            hash: u64::default(),
            protocol_type,
            last_encoding_result: None,
            last_resize: None,
            viewport_cache: std::collections::HashMap::new(),
            last_viewport: None,
        }
    }

    pub fn size_for(&self, resize: Resize, area: Rect) -> Rect {
        resize.render_area(&self.source, self.font_size, area)
    }

    pub fn protocol_type(&self) -> &StatefulProtocolType {
        &self.protocol_type
    }

    pub fn protocol_type_owned(self) -> StatefulProtocolType {
        self.protocol_type
    }

    /// This returns the latest Result returned when encoding, and none if there was no encoding since the last result read. It is encouraged but not required to handle it
    pub fn last_encoding_result(&mut self) -> Option<Result<()>> {
        self.last_encoding_result.take()
    }

    // Get the background color that fills in when resizing.
    pub fn background_color(&self) -> Rgba<u8> {
        self.source.background_color
    }

    fn last_encoding_area(&self) -> Rect {
        self.protocol_type.inner_trait().area()
    }
}

impl ResizeEncodeRender for StatefulProtocol {
    fn resize_encode(&mut self, resize: &Resize, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Special handling for viewport mode with tiling for Kitty
        if let Resize::Viewport(viewport_opts) = resize {
            // Store the viewport options to detect scrolling changes
            self.last_viewport = Some(*viewport_opts);

            // Enable tiling for Kitty protocol
            if let StatefulProtocolType::Kitty(ref mut kitty) = self.protocol_type {
                // Enable tiling if not already enabled
                if kitty.tiled_data.is_none() {
                    kitty.enable_tiling(self.font_size);
                }

                // For tiled mode, we encode the full image once
                if self.hash != self.source.hash
                    || kitty.tiled_data.as_ref().is_none_or(|t| t.tiles.is_empty())
                {
                    // For tiling, we need to encode the full image, not just the visible area
                    // Calculate the area that would contain the entire image
                    let full_image_area = ImageSource::round_pixel_size_to_cells(
                        self.source.image.width(),
                        self.source.image.height(),
                        self.font_size,
                    );
                    let result = kitty.resize_encode(self.source.image.clone(), full_image_area);
                    if result.is_ok() {
                        self.hash = self.source.hash;
                    }
                    self.last_encoding_result = Some(result);
                }

                self.last_resize = Some(resize.clone());
                return;
            }

            // For non-Kitty protocols, fall back to the old viewport handling
            self.viewport_cache.clear();

            // If not cached, encode it fresh
            let img = resize.resize(&self.source, self.font_size, area, self.background_color());

            // Encode with the actual area for now
            let encode_area = area;

            // Create a fresh protocol for this viewport
            let mut new_protocol = match &self.protocol_type {
                StatefulProtocolType::Halfblocks(_) => {
                    StatefulProtocolType::Halfblocks(Default::default())
                }
                StatefulProtocolType::Sixel(s) => StatefulProtocolType::Sixel(sixel::Sixel {
                    data: String::new(),
                    area: Rect::default(),
                    is_tmux: s.is_tmux,
                }),
                StatefulProtocolType::Kitty(k) => {
                    // This case is handled above, but kept for completeness
                    let mut kitty = kitty::StatefulKitty::new(k.is_tmux);
                    kitty.enable_tiling(self.font_size);
                    StatefulProtocolType::Kitty(kitty)
                }
                StatefulProtocolType::ITerm2(i) => StatefulProtocolType::ITerm2(iterm2::Iterm2 {
                    data: String::new(),
                    area: Rect::default(),
                    is_tmux: i.is_tmux,
                }),
            };

            let result = new_protocol
                .inner_trait_mut()
                .resize_encode(img, encode_area);

            if result.is_ok() {
                self.protocol_type = new_protocol;
                self.hash = self.source.hash;
            }

            self.last_encoding_result = Some(result);
            self.last_resize = Some(resize.clone());
            return;
        }

        // Clear viewport cache and tracking when switching modes
        if !matches!(resize, Resize::Viewport(_)) {
            self.viewport_cache.clear();
            self.last_viewport = None;
        }

        // Normal path for non-viewport or first viewport render
        let img = resize.resize(&self.source, self.font_size, area, self.background_color());

        let result = self
            .protocol_type
            .inner_trait_mut()
            .resize_encode(img, area);

        if result.is_ok() {
            self.hash = self.source.hash;

            // Viewport caching disabled for now due to position-dependent encoding issues
        }

        self.last_encoding_result = Some(result);
        self.last_resize = Some(resize.clone());
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Special handling for Kitty with viewport mode
        if let (Some(Resize::Viewport(viewport_opts)), StatefulProtocolType::Kitty(kitty)) =
            (self.last_resize.as_ref(), &mut self.protocol_type)
        {
            if kitty.tiled_data.is_some() {
                kitty.render_viewport(area, buf, viewport_opts.y_offset);
                return;
            }
        }

        self.protocol_type.inner_trait_mut().render(area, buf);
    }

    fn needs_resize(&self, resize: &Resize, area: Rect) -> Option<Rect> {
        // Check if viewport has changed (for scrolling)
        if let Resize::Viewport(viewport_opts) = resize {
            if self.last_viewport != Some(*viewport_opts) {
                // Viewport position changed, need to re-render different tiles
                return Some(resize.render_area(&self.source, self.font_size, area));
            }
        }

        resize.needs_resize(
            &self.source,
            self.font_size,
            self.last_encoding_area(),
            area,
            self.source.hash != self.hash,
        )
    }
}
#[derive(Clone)]
/// Image source for [crate::protocol::StatefulProtocol]s
///
/// A `[StatefulProtocol]` needs to resize the ImageSource to its state when the available area
/// changes. A `[Protocol]` only needs it once.
///
/// # Examples
/// ```text
/// use image::{DynamicImage, ImageBuffer, Rgb};
/// use ratatui_image::ImageSource;
///
/// let image: ImageBuffer::from_pixel(300, 200, Rgb::<u8>([255, 0, 0])).into();
/// let source = ImageSource::new(image, "filename.png", (7, 14));
/// assert_eq!((43, 14), (source.rect.width, source.rect.height));
/// ```
///
pub struct ImageSource {
    /// The original image without resizing.
    pub image: DynamicImage,
    /// The area that the [`ImageSource::image`] covers, but not necessarily fills.
    pub desired: Rect,
    /// TODO: document this; when image changes but it doesn't need a resize, force a render.
    pub hash: u64,
    /// The background color that should be used for padding or background when resizing.
    pub background_color: Rgba<u8>,
}

impl ImageSource {
    /// Create a new image source
    pub fn new(
        mut image: DynamicImage,
        font_size: FontSize,
        background_color: Rgba<u8>,
    ) -> ImageSource {
        let desired =
            ImageSource::round_pixel_size_to_cells(image.width(), image.height(), font_size);

        let mut state = DefaultHasher::new();
        image.as_bytes().hash(&mut state);
        let hash = state.finish();

        // We only need to underlay the background color here if it's not completely transparent.
        if background_color.0[3] != 0 {
            let mut bg: DynamicImage =
                ImageBuffer::from_pixel(image.width(), image.height(), background_color).into();
            imageops::overlay(&mut bg, &image, 0, 0);
            image = bg;
        }

        ImageSource {
            image,
            desired,
            hash,
            background_color,
        }
    }
    /// Round an image pixel size to the nearest matching cell size, given a font size.
    pub fn round_pixel_size_to_cells(
        img_width: u32,
        img_height: u32,
        (char_width, char_height): FontSize,
    ) -> Rect {
        let width = (img_width as f32 / char_width as f32).ceil() as u16;
        let height = (img_height as f32 / char_height as f32).ceil() as u16;
        Rect::new(0, 0, width, height)
    }
}
