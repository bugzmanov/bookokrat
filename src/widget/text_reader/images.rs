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
            MarkdownBlock::DefinitionList { items } => {
                let mut vec = Vec::new();
                for item in items {
                    // Term images render as [view image] links, not overlays — skip preloading.
                    // Definition body nodes go through render_node → render_paragraph which
                    // calls render_image_placeholder, so those need preloading.
                    for def_nodes in &item.definitions {
                        for node in def_nodes {
                            vec.append(&mut self.extract_images_from_node(node, book_images));
                        }
                    }
                }
                vec
            }
            Table { .. } => {
                // Table images are handled as [view image] clickable links,
                // not as inline overlays. Skip preloading to avoid ghost images at position 0.
                Vec::new()
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
                TextOrInline::Inline(Inline::Image { url, .. }) => Some(url.clone()),
                // Also extract images from inside links
                TextOrInline::Inline(Inline::Link {
                    text: link_text, ..
                }) => {
                    // Look for an image inside the link
                    for link_item in link_text.iter() {
                        if let TextOrInline::Inline(Inline::Image { url, .. }) = link_item {
                            return Some(url.clone());
                        }
                    }
                    None
                }
                _ => None,
            })
            .filter_map(|url| {
                // Skip already loaded/loading images
                if let Some(img) = self.embedded_images.borrow().get(&url) {
                    if matches!(
                        img.state,
                        ImageLoadState::Loaded { .. } | ImageLoadState::Loading
                    ) {
                        return None;
                    }
                }

                let chapter_path = self.current_chapter_file.as_deref();
                match book_images.get_image_size_with_context(&url, chapter_path) {
                    Some((w, h)) => {
                        let height_cells = self.image_height_in_cells(w, h);
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
                        Some((url, height_cells))
                    }
                    None => {
                        warn!("Could not get dimensions for: {url}");
                        self.embedded_images.borrow_mut().insert(
                            url.clone(),
                            EmbeddedImage::failed_img(&url, "Could not read image metadata"),
                        );
                        None
                    }
                }
            })
            .collect()
    }

    pub fn preload_image_dimensions(&mut self, book_images: &BookImages) {
        self.image_source = Some(book_images.clone());
        if let Some(doc) = self.markdown_document.clone() {
            self.background_loader.cancel_loading();

            let mut images_to_load = vec![];

            for node in &doc.blocks {
                images_to_load.append(&mut self.extract_images_from_node(node, book_images));
            }

            debug!("Found {} images to load in document", images_to_load.len());
            if self.image_viewport.is_some() {
                self.start_image_loading(images_to_load, book_images);
            }
        }
    }

    fn image_height_in_cells(&self, width: u32, height: u32) -> u16 {
        let Some((viewport_width, viewport_height)) = self.image_viewport else {
            return EmbeddedImage::height_in_cells(width, height);
        };
        let (cell_width, cell_height) = self
            .image_picker
            .as_ref()
            .map(|picker| picker.font_size())
            .unwrap_or((1, 2));

        EmbeddedImage::height_in_viewport(
            width,
            height,
            viewport_width,
            viewport_height,
            cell_width,
            cell_height,
        )
    }

    fn start_image_loading(
        &mut self,
        images_to_load: Vec<(String, u16)>,
        book_images: &BookImages,
    ) {
        if images_to_load.is_empty() {
            return;
        }

        let Some((cell_width, cell_height)) =
            self.image_picker.as_ref().map(|picker| picker.font_size())
        else {
            for (img_src, _) in images_to_load {
                if let Some(image) = self.embedded_images.borrow_mut().get_mut(&img_src) {
                    image.state = ImageLoadState::Unsupported;
                }
            }
            return;
        };

        let chapter_path = self.current_chapter_file.clone();
        if self.background_loader.start_loading_with_context(
            images_to_load.clone(),
            book_images,
            cell_width,
            cell_height,
            chapter_path,
        ) {
            for (img_src, _) in images_to_load {
                if let Some(image) = self.embedded_images.borrow_mut().get_mut(&img_src) {
                    image.state = ImageLoadState::Loading;
                }
            }
        }
    }

    pub fn prepare_images_for_viewport(&mut self, viewport_width: u16, viewport_height: u16) {
        let viewport = (viewport_width, viewport_height);
        if self.image_viewport == Some(viewport) {
            return;
        }
        self.image_viewport = Some(viewport);

        let (cell_width, cell_height) = self
            .image_picker
            .as_ref()
            .map(|picker| picker.font_size())
            .unwrap_or((1, 2));
        let mut heights_changed = false;
        let mut had_loading_images = false;

        for image in self.embedded_images.borrow_mut().values_mut() {
            if matches!(image.state, ImageLoadState::Failed { .. }) {
                continue;
            }
            had_loading_images |= matches!(image.state, ImageLoadState::Loading);

            let height_cells = EmbeddedImage::height_in_viewport(
                image.width,
                image.height,
                viewport_width,
                viewport_height,
                cell_width,
                cell_height,
            );
            if image.height_cells != height_cells {
                image.height_cells = height_cells;
                heights_changed = true;
                if !matches!(image.state, ImageLoadState::Unsupported) {
                    image.state = ImageLoadState::NotLoaded;
                }
            }
        }

        if heights_changed {
            self.cache_generation += 1;
        }
        if heights_changed && had_loading_images {
            self.background_loader.cancel_loading();
            for image in self.embedded_images.borrow_mut().values_mut() {
                if matches!(image.state, ImageLoadState::Loading) {
                    image.state = ImageLoadState::NotLoaded;
                }
            }
        }

        let images_to_load = self
            .embedded_images
            .borrow()
            .iter()
            .filter_map(|(src, image)| {
                matches!(image.state, ImageLoadState::NotLoaded)
                    .then(|| (src.clone(), image.height_cells))
            })
            .collect();
        if let Some(book_images) = self.image_source.clone() {
            self.start_image_loading(images_to_load, &book_images);
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

    pub fn invalidate_loaded_image_protocols(&mut self) {
        let Some(picker) = self.image_picker.clone() else {
            return;
        };

        for embedded_image in self.embedded_images.borrow_mut().values_mut() {
            if let ImageLoadState::Loaded { image, protocol } = &mut embedded_image.state {
                *protocol = picker.new_resize_protocol(image.as_ref().clone());
            }
        }
    }

    pub fn check_image_click(&self, x: u16, y: u16) -> Option<String> {
        // Click must land in one of the rendered text columns (left column is
        // `last_inner_text_area`; `dual.right_column` is set only in dual mode).
        let within = |area: ratatui::layout::Rect| {
            x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
        };
        let in_text_area = self.last_inner_text_area.is_some_and(within)
            || self.dual.right_column.is_some_and(within);
        if !in_text_area {
            return None;
        }

        // Hit-test against the actual rendered image rectangle, not only line range.
        // This prevents treating off-image horizontal clicks as image clicks.
        for (src, image_rect) in &self.last_rendered_image_rects {
            if x >= image_rect.x
                && x < image_rect.x + image_rect.width
                && y >= image_rect.y
                && y < image_rect.y + image_rect.height
            {
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

    pub fn is_embedded_image(&self, url: &str) -> bool {
        self.embedded_images.borrow().contains_key(url)
    }

    /// Check if a screen click position has a link (for linked images)
    pub fn check_link_at_screen_position(&self, x: u16, y: u16) -> Option<LinkInfo> {
        let text_area = self.last_inner_text_area?;
        let (clicked_line, clicked_col) = self.screen_to_text_coords(x, y, text_area)?;

        self.get_link_at_position(clicked_line, clicked_col)
            .cloned()
    }

    //todo: there should be a better way
    pub fn get_link_at_position(&self, line: usize, column: usize) -> Option<&LinkInfo> {
        self.links
            .iter()
            .find(|&link| link.line == line && column >= link.start_col && column <= link.end_col)
    }
}
