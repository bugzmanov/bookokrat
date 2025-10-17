use ratatui::style::Color;

// Color palette structure
#[allow(dead_code)]
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

#[allow(dead_code)]
pub const OCEANIC_NEXT: Base16Palette = Base16Palette {
    base_00: Color::from_u32(0x1B2B34),
    base_01: Color::from_u32(0x343D46),
    base_02: Color::from_u32(0x4F5B66),
    base_03: Color::from_u32(0x65737E),
    base_04: Color::from_u32(0xA7ADBA),
    base_05: Color::from_u32(0xC0C5CE),
    base_06: Color::from_u32(0xCDD3DE),
    base_07: Color::from_u32(0xF0F4F8),
    base_08: Color::from_u32(0xEC5F67),
    base_09: Color::from_u32(0xF99157),
    base_0a: Color::from_u32(0xFAC863),
    base_0b: Color::from_u32(0x99C794),
    base_0c: Color::from_u32(0x5FB3B3),
    base_0d: Color::from_u32(0x6699CC),
    base_0e: Color::from_u32(0xC594C5),
    base_0f: Color::from_u32(0xAB7967),
};

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
