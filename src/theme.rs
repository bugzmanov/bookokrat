use once_cell::sync::Lazy;
use ratatui::style::Color;

// Color palette structure
#[allow(dead_code)]
#[derive(Clone)]
pub struct Base16Palette {
    pub base_00: Color, // Background
    pub base_01: Color, // Lighter background
    pub base_02: Color, // Selection background
    pub base_03: Color, // Comments, invisibles
    pub base_04: Color, // Dark foreground
    pub base_05: Color, // Default foreground
    pub base_06: Color, // Light foreground
    pub base_07: Color, // Light background
    pub base_08: Color, // Red
    pub base_09: Color, // Orange
    pub base_0a: Color, // Yellow
    pub base_0b: Color, // Green
    pub base_0c: Color, // Cyan
    pub base_0d: Color, // Blue
    pub base_0e: Color, // Purple
    pub base_0f: Color, // Brown
}

// Lazy initialization of the palette to support runtime color detection
#[allow(dead_code)]
pub static OCEANIC_NEXT: Lazy<Base16Palette> = Lazy::new(|| Base16Palette {
    base_00: Color::Reset,        // Background
    base_01: Color::DarkGray,    // Lighter background
    base_02: Color::DarkGray,    // Selection background
    base_03: Color::DarkGray,    // Comments, invisibles
    base_04: Color::Gray,        // Dark foreground
    base_05: Color::Reset,        // Default foreground
    base_06: Color::White,       // Light foreground
    base_07: Color::White,       // Light background
    base_08: Color::Red,         // Red
    base_09: Color::LightRed,    // Orange
    base_0a: Color::Yellow,      // Yellow
    base_0b: Color::Green,       // Green
    base_0c: Color::Cyan,        // Cyan
    base_0d: Color::Blue,        // Blue
    base_0e: Color::Magenta,     // Purple
    base_0f: Color::LightYellow, // Brown
});

// Color utilities for focus states
impl Base16Palette {
    pub fn get_interface_colors(
        &self,
        is_content_mode: bool,
    ) -> (Color, Color, Color, Color, Color) {
        if is_content_mode {
            // In reading mode, muted interface with prominent text
            (
                self.base_03,
                self.base_07,
                self.base_02,
                self.base_02,
                self.base_06,
            )
        } else {
            // In file list mode, normal colors
            (
                self.base_05,
                self.base_07,
                self.base_04,
                self.base_02,
                self.base_06,
            )
        }
    }

    // Get colors for focused/unfocused panels
    pub fn get_panel_colors(&self, is_focused: bool) -> (Color, Color, Color) {
        if is_focused {
            // Focused panel: use the brightest possible colors like in snapshots
            (
                self.base_07, // Brightest text (matches content area in snapshots)
                self.base_04, // Bright border (matches snapshot borders)
                self.base_00, // Normal background
            )
        } else {
            // Unfocused panel: significantly dimmed for dramatic contrast
            (
                self.base_03, // Even more dimmed text (darker than base_02)
                self.base_03, // Very dimmed border (darker than current)
                self.base_00, // Same background
            )
        }
    }

    // Get selection colors for focused/unfocused states
    pub fn get_selection_colors(&self, is_focused: bool) -> (Color, Color) {
        if is_focused {
            // Focused selection: bright and prominent like in snapshots
            (self.base_02, self.base_06) // selection_bg, bright selection_fg
        } else {
            // Unfocused selection: very dimmed for dramatic contrast
            (self.base_02, self.base_03) // very dimmed selection_bg, very dimmed selection_fg
        }
    }
}
