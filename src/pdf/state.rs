//! Render state management

use std::path::PathBuf;

use ratatui::layout::Rect;

use super::CellSize;
use super::request::RenderParams;

/// Current render state for a PDF document
#[derive(Clone, Debug)]
pub struct RenderState {
    /// Path to the PDF document
    pub doc_path: PathBuf,

    /// Current viewport area
    pub area: Rect,

    /// User-specified scale factor
    pub scale: f32,

    /// Whether to invert images
    pub invert_images: bool,

    /// Current page (0-indexed)
    pub current_page: usize,

    /// Total page count
    pub page_count: usize,

    /// Terminal cell dimensions
    pub cell_size: CellSize,

    /// Theme colors
    pub black: i32,
    pub white: i32,
}

impl RenderState {
    /// Create a new render state for a document
    #[must_use]
    pub fn new(doc_path: PathBuf, cell_size: CellSize, black: i32, white: i32) -> Self {
        Self {
            doc_path,
            area: Rect::default(),
            scale: 1.0,
            invert_images: true,
            current_page: 0,
            page_count: 0,
            cell_size,
            black,
            white,
        }
    }

    /// Apply a command and return resulting effects
    #[must_use]
    pub fn apply(&mut self, cmd: Command) -> Vec<Effect> {
        match cmd {
            Command::Reload => {
                vec![Effect::InvalidateCache, Effect::ReloadDocument]
            }

            Command::SetArea(area) => {
                if self.area != area {
                    self.area = area;
                    vec![Effect::InvalidateCache, Effect::RenderCurrentPage]
                } else {
                    vec![]
                }
            }

            Command::SetScale(scale) => {
                let clamped = scale.max(0.1);
                if (self.scale - clamped).abs() > f32::EPSILON {
                    self.scale = clamped;
                    vec![Effect::InvalidateCache, Effect::RenderCurrentPage]
                } else {
                    vec![]
                }
            }

            Command::ToggleInvertImages => {
                self.invert_images = !self.invert_images;
                vec![Effect::InvalidateCache, Effect::RenderCurrentPage]
            }

            Command::GoToPage(page) => {
                let clamped = page.min(self.page_count.saturating_sub(1));
                if self.current_page != clamped {
                    self.current_page = clamped;
                    vec![Effect::RenderCurrentPage, Effect::UpdatePrefetch]
                } else {
                    vec![]
                }
            }

            Command::SetPageCount(count) => {
                self.page_count = count;
                if self.current_page >= count && count > 0 {
                    self.current_page = count - 1;
                }
                vec![]
            }

            Command::PageNeedsRerender(page) => {
                vec![Effect::InvalidatePage(page), Effect::RenderPage(page)]
            }

            Command::SetColors { black, white } => {
                if self.black != black || self.white != white {
                    self.black = black;
                    self.white = white;
                    vec![
                        Effect::InvalidateCache,
                        Effect::RenderCurrentPage,
                        Effect::UpdatePrefetch,
                    ]
                } else {
                    vec![]
                }
            }
        }
    }

    /// Get render parameters from current state
    #[must_use]
    pub fn render_params(&self) -> RenderParams {
        RenderParams {
            area: self.area,
            scale: self.scale,
            invert_images: self.invert_images,
            cell_size: self.cell_size,
            black: self.black,
            white: self.white,
        }
    }
}

/// Commands that modify render state
#[derive(Clone, Debug)]
pub enum Command {
    /// Reload the document
    Reload,
    /// Set the viewport area
    SetArea(Rect),
    /// Set the scale factor
    SetScale(f32),
    /// Toggle image inversion
    ToggleInvertImages,
    /// Go to a specific page
    GoToPage(usize),
    /// Update the page count
    SetPageCount(usize),
    /// Mark a page for re-rendering
    PageNeedsRerender(usize),
    /// Update theme colors
    SetColors { black: i32, white: i32 },
}

/// Effects produced by state changes
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Invalidate entire cache
    InvalidateCache,
    /// Invalidate a specific page
    InvalidatePage(usize),
    /// Render the current page
    RenderCurrentPage,
    /// Render a specific page
    RenderPage(usize),
    /// Reload document metadata
    ReloadDocument,
    /// Update prefetch queue
    UpdatePrefetch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> RenderState {
        RenderState::new(
            PathBuf::from("test.pdf"),
            CellSize::new(10, 20),
            0,
            0xFFFFFF,
        )
    }

    #[test]
    fn toggle_invert_images_returns_invalidate_and_render() {
        let mut state = test_state();
        // Default is now true (images get themed)
        assert!(state.invert_images);

        let effects = state.apply(Command::ToggleInvertImages);
        // After toggle: images keep original colors
        assert!(!state.invert_images);
        assert_eq!(
            effects,
            vec![Effect::InvalidateCache, Effect::RenderCurrentPage]
        );

        let effects = state.apply(Command::ToggleInvertImages);
        // Toggle back: images get themed again
        assert!(state.invert_images);
        assert_eq!(
            effects,
            vec![Effect::InvalidateCache, Effect::RenderCurrentPage]
        );
    }

    #[test]
    fn set_area_no_change_returns_empty() {
        let mut state = test_state();
        state.area = Rect::new(0, 0, 100, 50);

        let effects = state.apply(Command::SetArea(Rect::new(0, 0, 100, 50)));
        assert!(effects.is_empty());
    }

    #[test]
    fn set_area_with_change_returns_invalidate_and_render() {
        let mut state = test_state();
        state.area = Rect::new(0, 0, 100, 50);

        let effects = state.apply(Command::SetArea(Rect::new(0, 0, 200, 100)));
        assert_eq!(state.area, Rect::new(0, 0, 200, 100));
        assert_eq!(
            effects,
            vec![Effect::InvalidateCache, Effect::RenderCurrentPage]
        );
    }

    #[test]
    fn go_to_page_updates_and_prefetches() {
        let mut state = test_state();
        state.page_count = 100;

        let effects = state.apply(Command::GoToPage(50));
        assert_eq!(state.current_page, 50);
        assert_eq!(
            effects,
            vec![Effect::RenderCurrentPage, Effect::UpdatePrefetch]
        );
    }

    #[test]
    fn go_to_page_clamps_to_max() {
        let mut state = test_state();
        state.page_count = 10;

        let effects = state.apply(Command::GoToPage(999));
        assert_eq!(state.current_page, 9);
        assert_eq!(
            effects,
            vec![Effect::RenderCurrentPage, Effect::UpdatePrefetch]
        );
    }

    #[test]
    fn reload_invalidates_and_reloads() {
        let mut state = test_state();
        let effects = state.apply(Command::Reload);

        assert_eq!(
            effects,
            vec![Effect::InvalidateCache, Effect::ReloadDocument]
        );
    }
}
