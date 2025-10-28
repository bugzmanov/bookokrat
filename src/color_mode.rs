use ratatui::style::Color;
use std::env;

/// Detect if the terminal supports true color (24-bit RGB)
pub fn supports_true_color() -> bool {
    if let Ok(colorterm) = env::var("COLORTERM") {
        let colorterm_lower = colorterm.to_lowercase();
        if colorterm_lower == "truecolor" || colorterm_lower == "24bit" {
            return true;
        }
    }

    if let Ok(term) = env::var("TERM") {
        let term_lower = term.to_lowercase();
        if term_lower.contains("truecolor") || term_lower.contains("24bit") {
            return true;
        }
    }

    false
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
        assert!(dark_idx >= 232 && dark_idx <= 235); // Very dark grayscale

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
}
