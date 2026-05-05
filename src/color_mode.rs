use crate::terminal;
use ratatui::style::Color;

/// Detect if the terminal supports true color (24-bit RGB)
pub fn supports_true_color() -> bool {
    terminal::supports_true_color()
}

/// Convert RGB color to nearest 256-color palette index
fn rgb_to_256color(r: u8, g: u8, b: u8) -> u8 {
    let avg = (r as u16 + g as u16 + b as u16) / 3;
    let max_diff = r.abs_diff(g).max(r.abs_diff(b)).max(g.abs_diff(b));

    if avg < 80 && max_diff < 30 {
        let gray_index = if avg <= 8 {
            0
        } else {
            ((avg - 8) * 23 / (238 - 8)).min(23) as u8
        };
        return 232 + gray_index;
    }

    let r_index = (r as u16 * 5 / 255) as u8;
    let g_index = (g as u16 * 5 / 255) as u8;
    let b_index = (b as u16 * 5 / 255) as u8;

    let cube_color = 16 + 36 * r_index + 6 * g_index + b_index;

    let cube_r = if r_index == 0 { 0 } else { 55 + r_index * 40 };
    let cube_g = if g_index == 0 { 0 } else { 55 + g_index * 40 };
    let cube_b = if b_index == 0 { 0 } else { 55 + b_index * 40 };

    let cube_dist = ((r as i32 - cube_r as i32).pow(2)
        + (g as i32 - cube_g as i32).pow(2)
        + (b as i32 - cube_b as i32).pow(2)) as u32;

    if max_diff < 40 {
        let gray_index = if avg <= 8 {
            0
        } else {
            ((avg - 8) * 23 / (238 - 8)).min(23) as u8
        };
        let gray_color = 232 + gray_index;

        let gray_value = if gray_index == 0 {
            8
        } else {
            8 + gray_index * 10
        };

        let gray_dist = ((r as i32 - gray_value as i32).pow(2)
            + (g as i32 - gray_value as i32).pow(2)
            + (b as i32 - gray_value as i32).pow(2)) as u32;

        if gray_dist < cube_dist {
            return gray_color;
        }
    }

    cube_color
}

pub fn smart_color(rgb: u32) -> Color {
    if supports_true_color() {
        Color::from_u32(rgb)
    } else {
        let r = ((rgb >> 16) & 0xFF) as u8;
        let g = ((rgb >> 8) & 0xFF) as u8;
        let b = (rgb & 0xFF) as u8;

        let color_index = rgb_to_256color(r, g, b);
        Color::Indexed(color_index)
    }
}

pub fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Reset => None,
        Color::Black => Some((0x00, 0x00, 0x00)),
        Color::Red => Some((0x80, 0x00, 0x00)),
        Color::Green => Some((0x00, 0x80, 0x00)),
        Color::Yellow => Some((0x80, 0x80, 0x00)),
        Color::Blue => Some((0x00, 0x00, 0x80)),
        Color::Magenta => Some((0x80, 0x00, 0x80)),
        Color::Cyan => Some((0x00, 0x80, 0x80)),
        Color::Gray => Some((0xC0, 0xC0, 0xC0)),
        Color::DarkGray => Some((0x80, 0x80, 0x80)),
        Color::LightRed => Some((0xFF, 0x00, 0x00)),
        Color::LightGreen => Some((0x00, 0xFF, 0x00)),
        Color::LightYellow => Some((0xFF, 0xFF, 0x00)),
        Color::LightBlue => Some((0x00, 0x00, 0xFF)),
        Color::LightMagenta => Some((0xFF, 0x00, 0xFF)),
        Color::LightCyan => Some((0x00, 0xFF, 0xFF)),
        Color::White => Some((0xFF, 0xFF, 0xFF)),
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(index) => Some(indexed_color_to_rgb(index)),
    }
}

fn indexed_color_to_rgb(index: u8) -> (u8, u8, u8) {
    const ANSI_COLORS: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00),
        (0x80, 0x00, 0x00),
        (0x00, 0x80, 0x00),
        (0x80, 0x80, 0x00),
        (0x00, 0x00, 0x80),
        (0x80, 0x00, 0x80),
        (0x00, 0x80, 0x80),
        (0xC0, 0xC0, 0xC0),
        (0x80, 0x80, 0x80),
        (0xFF, 0x00, 0x00),
        (0x00, 0xFF, 0x00),
        (0xFF, 0xFF, 0x00),
        (0x00, 0x00, 0xFF),
        (0xFF, 0x00, 0xFF),
        (0x00, 0xFF, 0xFF),
        (0xFF, 0xFF, 0xFF),
    ];

    match index {
        0..=15 => ANSI_COLORS[index as usize],
        16..=231 => {
            let color = index - 16;
            let r = color / 36;
            let g = (color % 36) / 6;
            let b = color % 6;

            (xterm_component(r), xterm_component(g), xterm_component(b))
        }
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            (gray, gray, gray)
        }
    }
}

fn xterm_component(value: u8) -> u8 {
    if value == 0 { 0 } else { 55 + value * 40 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_256color_pure_colors() {
        // Pure red
        assert_eq!(rgb_to_256color(255, 0, 0), 196);
        // Pure green
        assert_eq!(rgb_to_256color(0, 255, 0), 46);
        // Pure blue
        assert_eq!(rgb_to_256color(0, 0, 255), 21);
    }

    #[test]
    fn test_rgb_to_256color_grayscale() {
        // Pure black maps to grayscale palette (232)
        let black_idx = rgb_to_256color(0, 0, 0);
        assert_eq!(black_idx, 232);

        // Very dark blue-gray (Oceanic Next background) maps to grayscale palette
        let dark_idx = rgb_to_256color(27, 43, 52);
        assert!((232..=235).contains(&dark_idx)); // Very dark grayscale

        // Pure white maps to RGB cube (231 is white in the cube)
        let white_idx = rgb_to_256color(255, 255, 255);
        assert_eq!(white_idx, 231);

        // Mid-gray should use grayscale palette
        let gray_idx = rgb_to_256color(128, 128, 128);
        assert!(gray_idx >= 232); // Grayscale palette
    }

    #[test]
    fn test_rgb_to_256color_mixed() {
        // Test a mid-tone color
        let idx = rgb_to_256color(128, 128, 128);
        assert!(idx >= 16); // Should be in valid range
    }

    #[test]
    fn test_color_to_rgb_indexed_ansi_colors() {
        assert_eq!(color_to_rgb(Color::Indexed(0)), Some((0x00, 0x00, 0x00)));
        assert_eq!(color_to_rgb(Color::Indexed(15)), Some((0xFF, 0xFF, 0xFF)));
    }

    #[test]
    fn test_color_to_rgb_indexed_cube_colors() {
        assert_eq!(color_to_rgb(Color::Indexed(16)), Some((0x00, 0x00, 0x00)));
        assert_eq!(color_to_rgb(Color::Indexed(231)), Some((0xFF, 0xFF, 0xFF)));
    }

    #[test]
    fn test_color_to_rgb_indexed_grayscale_colors() {
        assert_eq!(color_to_rgb(Color::Indexed(232)), Some((8, 8, 8)));
        assert_eq!(color_to_rgb(Color::Indexed(255)), Some((238, 238, 238)));
    }

    #[test]
    fn test_color_to_rgb_named_colors() {
        assert_eq!(color_to_rgb(Color::Black), Some((0x00, 0x00, 0x00)));
        assert_eq!(color_to_rgb(Color::White), Some((0xFF, 0xFF, 0xFF)));
        assert_eq!(color_to_rgb(Color::DarkGray), Some((0x80, 0x80, 0x80)));
        assert_eq!(color_to_rgb(Color::Reset), None);
    }
}
