use crate::color_mode::{color_to_rgb, smart_color};
use crate::theme::Base16Palette;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightColor {
    Red,
    Green,
    Blue,
    Yellow,
    Purple,
}

impl HighlightColor {
    pub const ALL: [Self; 5] = [
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Yellow,
        Self::Purple,
    ];

    pub fn from_shortcut(ch: char) -> Option<Self> {
        match ch.to_ascii_lowercase() {
            'r' => Some(Self::Red),
            'g' => Some(Self::Green),
            'b' => Some(Self::Blue),
            'y' => Some(Self::Yellow),
            'p' => Some(Self::Purple),
            _ => None,
        }
    }

    pub fn shortcut(self) -> char {
        match self {
            Self::Red => 'r',
            Self::Green => 'g',
            Self::Blue => 'b',
            Self::Yellow => 'y',
            Self::Purple => 'p',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Yellow => "Yellow",
            Self::Purple => "Purple",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn as_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

pub fn highlight_accent_color(color: HighlightColor, palette: &Base16Palette) -> Color {
    match color {
        HighlightColor::Red => palette.base_08,
        HighlightColor::Green => palette.base_0b,
        HighlightColor::Blue => palette.base_0d,
        HighlightColor::Yellow => palette.base_0a,
        HighlightColor::Purple => palette.base_0e,
    }
}

pub fn highlight_accent_rgb(color: HighlightColor, palette: &Base16Palette) -> RgbColor {
    let (r, g, b) = color_to_rgb(highlight_accent_color(color, palette)).unwrap_or(match color {
        HighlightColor::Red => (0xe0, 0x5a, 0x5a),
        HighlightColor::Green => (0x6f, 0xb8, 0x6f),
        HighlightColor::Blue => (0x6a, 0x9f, 0xd8),
        HighlightColor::Yellow => (0xd9, 0xb8, 0x4c),
        HighlightColor::Purple => (0xb4, 0x7a, 0xd8),
    });
    RgbColor::new(r, g, b)
}

pub fn comment_accent_rgb(palette: &Base16Palette) -> RgbColor {
    let (r, g, b) = color_to_rgb(palette.base_0e).unwrap_or((0xc5, 0x94, 0xc5));
    RgbColor::new(r, g, b)
}

pub fn pdf_highlight_rgb(
    color: HighlightColor,
    palette: &Base16Palette,
    themed_rendering: bool,
) -> RgbColor {
    if themed_rendering {
        return highlight_accent_rgb(color, palette);
    }

    match color {
        HighlightColor::Red => RgbColor::new(0xff, 0x73, 0x6b),
        HighlightColor::Green => RgbColor::new(0x57, 0xc7, 0x84),
        HighlightColor::Blue => RgbColor::new(0x5a, 0xa8, 0xff),
        HighlightColor::Yellow => RgbColor::new(0xff, 0xd7, 0x45),
        HighlightColor::Purple => RgbColor::new(0xc7, 0x7d, 0xff),
    }
}

pub fn highlight_background_color(color: HighlightColor, palette: &Base16Palette) -> Color {
    let accent = highlight_accent_rgb(color, palette);
    let (bg_r, bg_g, bg_b) = color_to_rgb(palette.base_00).unwrap_or((0, 0, 0));
    let (fg_r, fg_g, fg_b) = color_to_rgb(palette.base_05).unwrap_or((255, 255, 255));
    let bg = RgbColor::new(bg_r, bg_g, bg_b);
    let fg = RgbColor::new(fg_r, fg_g, fg_b);

    for alpha in [0.46, 0.38, 0.30, 0.22, 0.16] {
        let mixed = mix(bg, accent, alpha);
        if contrast_ratio(fg, mixed) >= 3.0 {
            return smart_color(mixed.as_u32());
        }
    }

    smart_color(mix(bg, accent, 0.18).as_u32())
}

pub fn pdf_highlight_alpha(_color: HighlightColor, themed_rendering: bool) -> u8 {
    if themed_rendering { 76 } else { 90 }
}

fn mix(base: RgbColor, top: RgbColor, alpha: f32) -> RgbColor {
    let blend = |a: u8, b: u8| -> u8 {
        ((a as f32 * (1.0 - alpha)) + (b as f32 * alpha))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    RgbColor::new(
        blend(base.r, top.r),
        blend(base.g, top.g),
        blend(base.b, top.b),
    )
}

fn relative_luminance(color: RgbColor) -> f32 {
    fn channel(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }

    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

fn contrast_ratio(a: RgbColor, b: RgbColor) -> f32 {
    let l1 = relative_luminance(a);
    let l2 = relative_luminance(b);
    let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}
