use super::book_images::BookImages;
use image::DynamicImage;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
use std::thread;

/// Message sent from background thread when images are loaded
#[derive(Debug)]
pub struct ImagesLoadedMessage {
    pub images: HashMap<String, DynamicImage>, // src -> image
}

/// Manages background loading of images with cancellation support
pub struct BackgroundImageLoader {
    /// Channel for receiving loaded images from background thread
    receiver: Option<Receiver<ImagesLoadedMessage>>,
    /// Track if background loading is in progress
    loading_in_progress: bool,
    /// Cancellation signal for background image loading
    loading_cancelled: Arc<AtomicBool>,
}

impl BackgroundImageLoader {
    pub fn new() -> Self {
        Self {
            receiver: None,
            loading_in_progress: false,
            loading_cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start loading images in a background thread
    /// Returns true if loading was started, false if already in progress
    pub fn start_loading(
        &mut self,
        images_to_load: Vec<(String, u16)>, // (src, height_cells)
        book_images: &BookImages,
        cell_width: u16,
        cell_height: u16,
    ) -> bool {
        // Don't start if already loading
        if self.loading_in_progress {
            debug!("Background image loading already in progress, skipping");
            return false;
        }

        // Reset cancellation flag for new loading operation
        self.loading_cancelled.store(false, Ordering::Relaxed);
        self.loading_in_progress = true;

        let (sender, receiver) = channel();
        self.receiver = Some(receiver);

        let book_images = book_images.clone();
        let cancel_flag = self.loading_cancelled.clone();

        thread::spawn(move || {
            Self::background_loading_thread(
                images_to_load,
                book_images,
                cell_width,
                cell_height,
                cancel_flag,
                sender,
            );
        });

        true
    }

    /// Cancel any ongoing background loading
    pub fn cancel_loading(&mut self) {
        if self.loading_in_progress {
            debug!("Cancelling background image loading");
            self.loading_cancelled.store(true, Ordering::Relaxed);
            self.receiver = None;
            self.loading_in_progress = false;
        }
    }

    /// Check for loaded images from background thread
    /// Returns Some(images) if images were loaded, None if no images available
    pub fn check_for_loaded_images(&mut self) -> Option<HashMap<String, DynamicImage>> {
        if let Some(ref receiver) = self.receiver {
            if let Ok(message) = receiver.try_recv() {
                debug!(
                    "Received {} loaded images from background thread",
                    message.images.len()
                );

                // Mark loading as complete
                self.loading_in_progress = false;
                self.receiver = None;

                return Some(message.images);
            }
        }
        None
    }

    /// Background thread function for loading and scaling images
    fn background_loading_thread(
        images_to_load: Vec<(String, u16)>,
        book_images: BookImages,
        cell_width: u16,
        cell_height: u16,
        cancel_flag: Arc<AtomicBool>,
        sender: Sender<ImagesLoadedMessage>,
    ) {
        let mut loaded_images = HashMap::new();
        let load_start = std::time::Instant::now();

        for (img_src, height_cells) in images_to_load {
            if cancel_flag.load(Ordering::Relaxed) {
                debug!("Background image loading cancelled");
                return;
            }

            if let Some((scaled_image, _width_cells, _height_cells_result)) =
                book_images.load_and_resize_image(&img_src, height_cells, cell_width, cell_height)
            {
                loaded_images.insert(img_src.clone(), scaled_image);
            } else {
                warn!("Failed to load and resize image: {img_src}");
            }
        }

        let load_time = load_start.elapsed();
        info!(
            "Background image loading complete: {} images loaded and scaled in {:?}",
            loaded_images.len(),
            load_time
        );

        // Send the loaded images back to the main thread
        match sender.send(ImagesLoadedMessage {
            images: loaded_images,
        }) {
            Ok(()) => {}
            Err(e) => {
                if !cancel_flag.load(Ordering::Relaxed) {
                    error!("Failed to send loaded images: {e}");
                }
            }
        }
    }
}

impl Default for BackgroundImageLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {}
