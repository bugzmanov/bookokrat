//! Tile-based image handling for efficient vertical scrolling
//!
//! This module provides a tile-based approach where images are pre-sliced into
//! horizontal strips that match the terminal's row height. This allows efficient
//! viewport rendering without re-encoding the entire image.

use super::{FontSize, protocol::StatefulProtocolType};
use image::DynamicImage;
use std::collections::HashMap;

/// A single horizontal tile of an image
#[derive(Clone)]
pub struct ImageTile {
    /// The Y position of this tile in the original image (in pixels)
    pub y_offset: u32,
    /// The height of this tile (should match font height)
    pub height: u32,
    /// The encoded protocol data for this tile
    pub protocol_data: String,
}

/// A tiled image that can be efficiently rendered at different viewports
#[derive(Clone)]
pub struct TiledImage {
    /// Original image dimensions
    pub width: u32,
    pub height: u32,
    /// Font size used for tiling
    pub font_size: FontSize,
    /// Pre-computed tiles indexed by their row number
    pub tiles: HashMap<u32, ImageTile>,
    /// Protocol type used for encoding
    pub protocol_type: StatefulProtocolType,
}

impl TiledImage {
    /// Create a new tiled image by slicing the original image into horizontal strips
    pub fn new(
        image: &DynamicImage,
        font_size: FontSize,
        protocol_type: StatefulProtocolType,
    ) -> Self {
        let (_, char_height) = font_size;
        let tile_height = char_height as u32;

        let mut tiles = HashMap::new();
        let num_tiles = image.height().div_ceil(tile_height);

        // Pre-compute tiles for the entire image
        for tile_idx in 0..num_tiles {
            let y_offset = tile_idx * tile_height;
            let actual_height = tile_height.min(image.height() - y_offset);

            // Create a sub-image for this tile
            let _tile_image = image.crop_imm(0, y_offset, image.width(), actual_height);

            // TODO: Encode this tile using the protocol
            // For now, we'll store a placeholder
            let tile = ImageTile {
                y_offset,
                height: actual_height,
                protocol_data: String::new(), // Will be filled by encode_tile
            };

            tiles.insert(tile_idx, tile);
        }

        TiledImage {
            width: image.width(),
            height: image.height(),
            font_size,
            tiles,
            protocol_type,
        }
    }

    /// Get the tiles that should be rendered for a given viewport
    pub fn get_viewport_tiles(&self, y_offset: u32, viewport_height_cells: u16) -> Vec<&ImageTile> {
        let (_, char_height) = self.font_size;
        let tile_height = char_height as u32;

        // Calculate which tiles are visible
        let start_tile = y_offset / tile_height;
        let viewport_height_pixels = viewport_height_cells as u32 * char_height as u32;
        let end_tile = (y_offset + viewport_height_pixels).div_ceil(tile_height);

        let mut visible_tiles = Vec::new();
        for tile_idx in start_tile..=end_tile {
            if let Some(tile) = self.tiles.get(&tile_idx) {
                visible_tiles.push(tile);
            }
        }

        visible_tiles
    }

    /// Calculate the cell offset for rendering a tile at a given viewport position
    pub fn calculate_tile_position(&self, tile: &ImageTile, viewport_y_offset: u32) -> i32 {
        // Calculate where this tile should be rendered relative to the viewport
        let tile_top = tile.y_offset as i32;
        let viewport_top = viewport_y_offset as i32;
        let (_, char_height) = self.font_size;

        // Convert pixel offset to cell offset
        (tile_top - viewport_top) / (char_height as i32)
    }
}
