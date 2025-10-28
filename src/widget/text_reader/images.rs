use super::types::*;
use crate::images::book_images::BookImages;
use crate::markdown::{Block as MarkdownBlock, Inline, Node, TextOrInline};
use crate::ratatui_image::picker::Picker;
use crate::types::LinkInfo;
use image::DynamicImage;
use log::{debug, warn};
use std::sync::Arc;

impl crate::markdown_text_reader::MarkdownTextReader {
    fn extract_images_from_node(
        &mut self,
        node: &Node,
        book_images: &BookImages,
    ) -> Vec<(String, u16)> {
        use MarkdownBlock::*;
        match &node.block {
            Paragraph { content } => self.extract_images_from_text(content, book_images),
            Quote { content } => {
                let mut vec = Vec::new();
                for inner_node in content {
                    vec.append(&mut self.extract_images_from_node(inner_node, book_images));
                }
                vec
            }
            List { items, .. } => {
                let mut vec = Vec::new();
                for item in items {
                    for inner_node in &item.content {
                        vec.append(&mut self.extract_images_from_node(inner_node, book_images));
                    }
                }
                vec
            }
            EpubBlock { content, .. } => {
                let mut vec = Vec::new();
                for inner_node in content {
                    vec.append(&mut self.extract_images_from_node(inner_node, book_images));
                }
                vec
            }
            _ => Vec::new(),
        }
    }

    fn extract_images_from_text(
        &mut self,
        text: &crate::markdown::Text,
        book_images: &BookImages,
    ) -> Vec<(String, u16)> {
        text.iter()
            .filter_map(|item| match item {
                TextOrInline::Inline(Inline::Image { url, .. }) => Some(url),
                _ => None,
            })
            .filter_map(|url| {
                // Skip already loaded/loading images
                if let Some(img) = self.embedded_images.borrow().get(url) {
                    if matches!(
                        img.state,
                        ImageLoadState::Loaded { .. } | ImageLoadState::Loading
                    ) {
                        return None;
                    }
                }

                let chapter_path = self.current_chapter_file.as_deref();
                match book_images.get_image_size_with_context(url, chapter_path) {
                    Some((w, h)) if w >= 64 && h >= 64 => {
                        let height_cells = EmbeddedImage::height_in_cells(w, h);
                        self.embedded_images.borrow_mut().insert(
                            url.clone(),
                            EmbeddedImage {
                                src: url.clone(),
                                lines_before_image: 0,
                                height_cells,
                                width: w,
                                height: h,
                                state: ImageLoadState::NotLoaded,
                            },
                        );
                        Some((url.clone(), height_cells))
                    }
                    Some((w, h)) => {
                        warn!("Ignoring small image ({w}x{h}): {url}");
                        None
                    }
                    None => {
                        warn!("Could not get dimensions for: {url}");
                        self.embedded_images.borrow_mut().insert(
                            url.clone(),
                            EmbeddedImage::failed_img(url, "Could not read image metadata"),
                        );
                        None
                    }
                }
            })
            .collect()
    }

    pub fn preload_image_dimensions(&mut self, book_images: &BookImages) {
        if let Some(doc) = self.markdown_document.clone() {
            self.background_loader.cancel_loading();

            let mut images_to_load = vec![];

            for node in &doc.blocks {
                images_to_load.append(&mut self.extract_images_from_node(node, book_images));
            }

            debug!("Found {} images to load in document", images_to_load.len());
            if !images_to_load.is_empty() {
                if let Some(ref picker) = self.image_picker {
                    let font_size = picker.font_size();
                    let (cell_width, cell_height) = (font_size.0, font_size.1);
                    self.background_loader.start_loading(
                        images_to_load.clone(),
                        book_images,
                        cell_width,
                        cell_height,
                    );
                    for (img_src, _) in images_to_load.iter() {
                        if let Some(img_state) = self.embedded_images.borrow_mut().get_mut(img_src)
                        {
                            img_state.state = ImageLoadState::Loading;
                        }
                    }
                } else {
                    for (img, _) in images_to_load.iter() {
                        if let Some(img_state) = self.embedded_images.borrow_mut().get_mut(img) {
                            img_state.state = ImageLoadState::Unsupported;
                        }
                    }
                }
            }
        }
    }

    pub fn check_for_loaded_images(&mut self) -> bool {
        let mut any_loaded = false;

        if let Some(loaded_images) = self.background_loader.check_for_loaded_images() {
            for (img_src, image) in loaded_images {
                let mut embedded_images = self.embedded_images.borrow_mut();
                if let Some(embedded_image) = embedded_images.get_mut(&img_src) {
                    embedded_image.state = if let Some(ref picker) = self.image_picker {
                        ImageLoadState::Loaded {
                            image: Arc::new(image.clone()),
                            protocol: picker.new_resize_protocol(image),
                        }
                    } else {
                        ImageLoadState::Unsupported
                    };
                    any_loaded = true;
                } else {
                    warn!(
                        "Received loaded image '{img_src}' that is no longer in embedded_images (likely due to chapter switch)"
                    );
                }
            }
        }

        any_loaded
    }

    pub fn check_image_click(&self, x: u16, y: u16) -> Option<String> {
        // Use the inner text area if available
        let text_area = self.last_inner_text_area?;

        // Check if click is within the text area
        if x < text_area.x
            || x >= text_area.x + text_area.width
            || y < text_area.y
            || y >= text_area.y + text_area.height
        {
            return None;
        }

        // Calculate the line number that was clicked within the text area
        let clicked_line = self.scroll_offset + (y - text_area.y) as usize;

        // Check each embedded image to see if the click is within its bounds
        for (src, embedded_image) in self.embedded_images.borrow().iter() {
            let image_start = embedded_image.lines_before_image;
            let image_end = image_start + embedded_image.height_cells as usize;

            if clicked_line >= image_start && clicked_line < image_end {
                return Some(src.clone());
            }
        }

        None
    }

    pub fn get_image_picker(&self) -> Option<&Picker> {
        self.image_picker.as_ref()
    }

    pub fn get_loaded_image(&self, image_src: &str) -> Option<Arc<DynamicImage>> {
        self.embedded_images
            .borrow()
            .get(image_src)
            .and_then(|img| match &img.state {
                ImageLoadState::Loaded { image, .. } => Some(image.clone()),
                _ => None,
            })
    }

    //todo: there should be a better way
    pub fn get_link_at_position(&self, line: usize, column: usize) -> Option<&LinkInfo> {
        self.links
            .iter()
            .find(|&link| link.line == line && column >= link.start_col && column <= link.end_col)
    }
}
