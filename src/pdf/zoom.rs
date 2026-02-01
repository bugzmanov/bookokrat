//! Zoom and pan state for PDF rendering
//!
//! Manages zoom factor, horizontal panning, and vertical scroll offset
//! for continuous scroll PDF rendering.

/// Scroll/pan direction for zoom navigation
#[derive(Clone, Copy, Debug)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ScrollDirection {
    /// Returns true if the direction is vertical (Up or Down)
    pub fn vertical(self) -> bool {
        matches!(self, Self::Up | Self::Down)
    }
}

/// Zoom and pan state for PDF viewing
#[derive(Debug)]
pub struct Zoom {
    /// Current zoom factor (1.0 = 100%)
    pub factor: f32,

    /// Horizontal pan offset in terminal cells from left edge
    pub cell_pan_from_left: u16,

    /// Vertical scroll offset in pixels for continuous scroll
    pub global_scroll_offset: u32,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            factor: 1.0,
            cell_pan_from_left: 0,
            global_scroll_offset: 0,
        }
    }
}

impl Zoom {
    /// Zoom in rate multiplier per step - 10%
    pub const ZOOM_IN_RATE: f32 = 1.1;
    /// Zoom out rate divisor per step - 5%
    pub const ZOOM_OUT_RATE: f32 = 1.05;
    /// Minimum allowed zoom factor
    pub const MIN_SCALE: f32 = 0.1;

    /// Base pan step in cells for horizontal movement
    pub const BASE_PAN_STEP_X: f32 = 4.0;
    /// Base pan step in cells for vertical movement
    pub const BASE_PAN_STEP_Y: f32 = 2.0;

    /// Returns the current zoom factor
    pub fn factor(&self) -> f32 {
        self.factor
    }

    /// Zoom in by one step
    pub fn step_in(&mut self) {
        self.factor = Self::clamp_factor(self.factor * Self::ZOOM_IN_RATE);
    }

    /// Zoom out by one step
    pub fn step_out(&mut self) {
        self.factor = Self::clamp_factor(self.factor / Self::ZOOM_OUT_RATE);
    }

    /// Pan in the given direction, adjusting step size by zoom factor
    pub fn pan(&mut self, direction: ScrollDirection) {
        let base_step = if direction.vertical() {
            Self::BASE_PAN_STEP_Y
        } else {
            Self::BASE_PAN_STEP_X
        };
        let step = (base_step / self.factor()).max(1.0) as i32;

        match direction {
            ScrollDirection::Up => {
                self.global_scroll_offset = self.global_scroll_offset.saturating_sub(step as u32);
            }
            ScrollDirection::Down => {
                self.global_scroll_offset = self.global_scroll_offset.saturating_add(step as u32);
            }
            ScrollDirection::Left => {
                self.cell_pan_from_left = self.cell_pan_from_left.saturating_sub(step as u16);
            }
            ScrollDirection::Right => {
                self.cell_pan_from_left = self.cell_pan_from_left.saturating_add(step as u16);
            }
        }
    }

    /// Scroll to position the given page at the top of the viewport
    pub fn scroll_to_page(&mut self, page: usize, page_heights: &[u32], separator_height: u16) {
        let offset: u32 = page_heights
            .iter()
            .take(page)
            .map(|&h| h + u32::from(separator_height))
            .sum();
        self.global_scroll_offset = offset;
    }

    /// Scroll to the top of the document
    pub fn scroll_to_top(&mut self) {
        self.global_scroll_offset = 0;
    }

    /// Scroll to the bottom of the document
    pub fn scroll_to_bottom(&mut self, page_heights: &[u32], separator_height: u16) {
        let total_height: u32 = page_heights
            .iter()
            .map(|&h| h + u32::from(separator_height))
            .sum();
        self.global_scroll_offset = total_height.saturating_sub(1);
    }

    /// Clamp factor to valid range, handling NaN/Inf
    pub fn clamp_factor(factor: f32) -> f32 {
        if !factor.is_finite() {
            1.0
        } else {
            factor.max(Self::MIN_SCALE)
        }
    }
}
