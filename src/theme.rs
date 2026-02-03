use crate::color_mode::smart_color;
use crate::settings::{self, YamlTheme};
use log::{debug, warn};
use ratatui::style::Color;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, RwLock};

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

// Named theme with palette
#[derive(Clone)]
pub struct Theme {
    pub name: String,
    pub palette: Base16Palette,
    pub is_builtin: bool,
}

// Built-in theme identifiers
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuiltinTheme {
    OceanicNext,
    CatppuccinMocha,
    Kanagawa,
    KanagawaDragon,
    RosepineMoon,
    Moonfly,
}

impl BuiltinTheme {
    fn name(&self) -> &'static str {
        match self {
            BuiltinTheme::OceanicNext => "Oceanic Next",
            BuiltinTheme::CatppuccinMocha => "Catppuccin Mocha",
            BuiltinTheme::Kanagawa => "Kanagawa",
            BuiltinTheme::KanagawaDragon => "Kanagawa Dragon",
            BuiltinTheme::RosepineMoon => "Rosepine Moon",
            BuiltinTheme::Moonfly => "Moonfly",
        }
    }

    fn palette(&self) -> &'static Base16Palette {
        match self {
            BuiltinTheme::OceanicNext => &OCEANIC_NEXT_PALETTE,
            BuiltinTheme::CatppuccinMocha => &CATPPUCCIN_MOCHA_PALETTE,
            BuiltinTheme::Kanagawa => &KANAGAWA_PALETTE,
            BuiltinTheme::KanagawaDragon => &KANAGAWA_DRAGON_PALETTE,
            BuiltinTheme::RosepineMoon => &ROSEPINE_MOON_PALETTE,
            BuiltinTheme::Moonfly => &MOONFLY_PALETTE,
        }
    }

    fn all() -> &'static [BuiltinTheme] {
        &[
            BuiltinTheme::OceanicNext,
            BuiltinTheme::CatppuccinMocha,
            BuiltinTheme::Kanagawa,
            BuiltinTheme::KanagawaDragon,
            BuiltinTheme::RosepineMoon,
            BuiltinTheme::Moonfly,
        ]
    }
}

// Global theme storage
static CUSTOM_THEMES: LazyLock<RwLock<Vec<Theme>>> = LazyLock::new(|| RwLock::new(Vec::new()));
static CURRENT_THEME_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Load custom themes from settings and apply saved theme selection
pub fn load_custom_themes() {
    let yaml_themes = settings::get_custom_themes();

    let mut custom_themes = Vec::new();

    for yaml in yaml_themes {
        match theme_from_yaml(&yaml) {
            Ok(theme) => {
                debug!("Loaded custom theme: {}", theme.name);
                custom_themes.push(theme);
            }
            Err(e) => {
                warn!("Failed to load custom theme '{}': {}", yaml.scheme, e);
            }
        }
    }

    // Sort custom themes by name
    custom_themes.sort_by(|a, b| a.name.cmp(&b.name));

    if let Ok(mut themes) = CUSTOM_THEMES.write() {
        *themes = custom_themes;
    }

    // Apply saved theme from settings
    let saved_theme = settings::get_theme_name();
    if let Some(index) = get_theme_index_by_name(&saved_theme) {
        CURRENT_THEME_INDEX.store(index, Ordering::Relaxed);
        debug!("Applied saved theme: {saved_theme}");
    } else {
        warn!("Saved theme '{saved_theme}' not found, using default");
    }
}

fn theme_from_yaml(yaml: &YamlTheme) -> Result<Theme, String> {
    let palette = Base16Palette {
        base_00: parse_hex_color(&yaml.base00)?,
        base_01: parse_hex_color(&yaml.base01)?,
        base_02: parse_hex_color(&yaml.base02)?,
        base_03: parse_hex_color(&yaml.base03)?,
        base_04: parse_hex_color(&yaml.base04)?,
        base_05: parse_hex_color(&yaml.base05)?,
        base_06: parse_hex_color(&yaml.base06)?,
        base_07: parse_hex_color(&yaml.base07)?,
        base_08: parse_hex_color(&yaml.base08)?,
        base_09: parse_hex_color(&yaml.base09)?,
        base_0a: parse_hex_color(&yaml.base0a)?,
        base_0b: parse_hex_color(&yaml.base0b)?,
        base_0c: parse_hex_color(&yaml.base0c)?,
        base_0d: parse_hex_color(&yaml.base0d)?,
        base_0e: parse_hex_color(&yaml.base0e)?,
        base_0f: parse_hex_color(&yaml.base0f)?,
    };

    Ok(Theme {
        name: yaml.scheme.clone(),
        palette,
        is_builtin: false,
    })
}

fn parse_hex_color(hex: &str) -> Result<Color, String> {
    let hex = hex.trim_start_matches('#');
    let value = u32::from_str_radix(hex, 16).map_err(|e| format!("Invalid hex color: {e}"))?;
    Ok(smart_color(value))
}

/// Get total number of available themes (built-in + custom)
pub fn theme_count() -> usize {
    let custom_count = CUSTOM_THEMES.read().map(|t| t.len()).unwrap_or(0);
    BuiltinTheme::all().len() + custom_count
}

/// Get theme name by index
pub fn theme_name(index: usize) -> String {
    let builtin_count = BuiltinTheme::all().len();
    if index < builtin_count {
        BuiltinTheme::all()[index].name().to_string()
    } else {
        CUSTOM_THEMES
            .read()
            .ok()
            .and_then(|themes| themes.get(index - builtin_count).map(|t| t.name.clone()))
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

/// Get all theme names
pub fn all_theme_names() -> Vec<String> {
    let mut names: Vec<String> = BuiltinTheme::all()
        .iter()
        .map(|t| t.name().to_string())
        .collect();

    if let Ok(custom) = CUSTOM_THEMES.read() {
        names.extend(custom.iter().map(|t| t.name.clone()));
    }

    names
}

/// Get current theme index
pub fn current_theme_index() -> usize {
    CURRENT_THEME_INDEX.load(Ordering::Relaxed)
}

/// Set theme by index
pub fn set_theme_by_index(index: usize) {
    if index < theme_count() {
        CURRENT_THEME_INDEX.store(index, Ordering::Relaxed);
    }
}

/// Get theme index by name
pub fn get_theme_index_by_name(name: &str) -> Option<usize> {
    let builtin_count = BuiltinTheme::all().len();

    // Check built-in themes
    for (i, theme) in BuiltinTheme::all().iter().enumerate() {
        if theme.name() == name {
            return Some(i);
        }
    }

    // Check custom themes
    if let Ok(custom) = CUSTOM_THEMES.read() {
        for (i, theme) in custom.iter().enumerate() {
            if theme.name == name {
                return Some(builtin_count + i);
            }
        }
    }

    None
}

/// Set theme by name and save to settings
pub fn set_theme_by_name(name: &str) -> bool {
    if let Some(index) = get_theme_index_by_name(name) {
        CURRENT_THEME_INDEX.store(index, Ordering::Relaxed);
        settings::set_theme_name(name);
        true
    } else {
        false
    }
}

/// Set theme by index and save to settings
pub fn set_theme_by_index_and_save(index: usize) {
    if index < theme_count() {
        CURRENT_THEME_INDEX.store(index, Ordering::Relaxed);
        let name = theme_name(index);
        settings::set_theme_name(&name);
    }
}

/// Get current theme name
pub fn current_theme_name() -> String {
    theme_name(current_theme_index())
}

/// Get effective background color (transparent or theme color)
pub fn theme_background() -> Color {
    if settings::is_transparent_background() {
        Color::Reset
    } else {
        current_theme().base_00
    }
}

/// Get current theme palette
pub fn current_theme() -> &'static Base16Palette {
    let index = current_theme_index();
    let builtin_count = BuiltinTheme::all().len();

    if index < builtin_count {
        BuiltinTheme::all()[index].palette()
    } else {
        // For custom themes, we need to return a static reference
        // Since custom themes are loaded once and stored in CUSTOM_THEMES,
        // we leak the palette to get a static reference (it lives for program duration anyway)
        static CUSTOM_PALETTE_CACHE: LazyLock<RwLock<Vec<&'static Base16Palette>>> =
            LazyLock::new(|| RwLock::new(Vec::new()));

        let custom_index = index - builtin_count;

        // Check if we already have this palette cached
        if let Ok(cache) = CUSTOM_PALETTE_CACHE.read() {
            if let Some(palette) = cache.get(custom_index) {
                return palette;
            }
        }

        // Get the custom theme palette and leak it for static lifetime
        if let Ok(themes) = CUSTOM_THEMES.read() {
            if let Some(theme) = themes.get(custom_index) {
                let palette = Box::new(theme.palette.clone());
                let static_palette: &'static Base16Palette = Box::leak(palette);

                // Cache it
                if let Ok(mut cache) = CUSTOM_PALETTE_CACHE.write() {
                    while cache.len() <= custom_index {
                        cache.push(&OCEANIC_NEXT_PALETTE);
                    }
                    cache[custom_index] = static_palette;
                }

                return static_palette;
            }
        }

        // Fallback to default
        &OCEANIC_NEXT_PALETTE
    }
}

// ============================================================================
// Built-in theme palettes
// ============================================================================

// Oceanic Next theme
static OCEANIC_NEXT_PALETTE: LazyLock<Base16Palette> = LazyLock::new(|| Base16Palette {
    base_00: smart_color(0x1B2B34),
    base_01: smart_color(0x343D46),
    base_02: smart_color(0x4F5B66),
    base_03: smart_color(0x65737E),
    base_04: smart_color(0xA7ADBA),
    base_05: smart_color(0xC0C5CE),
    base_06: smart_color(0xCDD3DE),
    base_07: smart_color(0xF0F4F8),
    base_08: smart_color(0xEC5F67),
    base_09: smart_color(0xF99157),
    base_0a: smart_color(0xFAC863),
    base_0b: smart_color(0x99C794),
    base_0c: smart_color(0x5FB3B3),
    base_0d: smart_color(0x6699CC),
    base_0e: smart_color(0xC594C5),
    base_0f: smart_color(0xAB7967),
});

// Catppuccin Mocha theme
static CATPPUCCIN_MOCHA_PALETTE: LazyLock<Base16Palette> = LazyLock::new(|| Base16Palette {
    base_00: smart_color(0x1E1E2E),
    base_01: smart_color(0x313244),
    base_02: smart_color(0x45475A),
    base_03: smart_color(0x6C7086),
    base_04: smart_color(0x7F849C),
    base_05: smart_color(0xA6ADC8),
    base_06: smart_color(0xCDD6F4),
    base_07: smart_color(0xF5E0DC),
    base_08: smart_color(0xF38BA8),
    base_09: smart_color(0xFAB387),
    base_0a: smart_color(0xF9E2AF),
    base_0b: smart_color(0xA6E3A1),
    base_0c: smart_color(0x94E2D5),
    base_0d: smart_color(0x89B4FA),
    base_0e: smart_color(0xCBA6F7),
    base_0f: smart_color(0xEBA0AC),
});

// Kanagawa theme - Japanese-inspired warm tones
static KANAGAWA_PALETTE: LazyLock<Base16Palette> = LazyLock::new(|| Base16Palette {
    base_00: smart_color(0x1F1F28),
    base_01: smart_color(0x2A2A37),
    base_02: smart_color(0x223249),
    base_03: smart_color(0x727169),
    base_04: smart_color(0xC8C093),
    base_05: smart_color(0xDCD7BA),
    base_06: smart_color(0xDCD7BA),
    base_07: smart_color(0xE6E0C2),
    base_08: smart_color(0xC34043),
    base_09: smart_color(0xFFA066),
    base_0a: smart_color(0xDCA561),
    base_0b: smart_color(0x98BB6C),
    base_0c: smart_color(0x7FB4CA),
    base_0d: smart_color(0x7E9CD8),
    base_0e: smart_color(0x957FB8),
    base_0f: smart_color(0xD27E99),
});

// Kanagawa Dragon theme - darker variant with cooler tones
static KANAGAWA_DRAGON_PALETTE: LazyLock<Base16Palette> = LazyLock::new(|| Base16Palette {
    base_00: smart_color(0x181616),
    base_01: smart_color(0x0d0c0c),
    base_02: smart_color(0x2d4f67),
    base_03: smart_color(0xa6a69c),
    base_04: smart_color(0x7fb4ca),
    base_05: smart_color(0xc5c9c5),
    base_06: smart_color(0xc5c9c5),
    base_07: smart_color(0xc5c9c5),
    base_08: smart_color(0xc4746e),
    base_09: smart_color(0xe46876),
    base_0a: smart_color(0xc4b28a),
    base_0b: smart_color(0x8a9a7b),
    base_0c: smart_color(0x8ea4a2),
    base_0d: smart_color(0x8ba4b0),
    base_0e: smart_color(0xa292a3),
    base_0f: smart_color(0x7aa89f),
});

// Rosepine Moon theme - romantic palette with rose/gold/pine
static ROSEPINE_MOON_PALETTE: LazyLock<Base16Palette> = LazyLock::new(|| Base16Palette {
    base_00: smart_color(0x18191a),
    base_01: smart_color(0x212223),
    base_02: smart_color(0x363738),
    base_03: smart_color(0x6e6a86),
    base_04: smart_color(0x908caa),
    base_05: smart_color(0xe0def4),
    base_06: smart_color(0xe0def4),
    base_07: smart_color(0xe0def4),
    base_08: smart_color(0xeb6f92),
    base_09: smart_color(0xc4a7e7),
    base_0a: smart_color(0xea9a97),
    base_0b: smart_color(0xf6c177),
    base_0c: smart_color(0x9ccfd8),
    base_0d: smart_color(0x3e8fb0),
    base_0e: smart_color(0xc4a7e7),
    base_0f: smart_color(0x908caa),
});

// Moonfly theme - deep black with vibrant accent colors
static MOONFLY_PALETTE: LazyLock<Base16Palette> = LazyLock::new(|| Base16Palette {
    base_00: smart_color(0x080808), // black - main background
    base_01: smart_color(0x1c1c1c), // grey11 - lighter background
    base_02: smart_color(0x323437), // grey0 - selection background
    base_03: smart_color(0x808080), // grey50 - comments
    base_04: smart_color(0x9e9e9e), // grey62 - dark foreground
    base_05: smart_color(0xc6c6c6), // white - default foreground
    base_06: smart_color(0xe4e4e4), // grey89 - light foreground
    base_07: smart_color(0xe4e4e4), // grey89 - lightest
    base_08: smart_color(0xff5189), // crimson - red
    base_09: smart_color(0xde935f), // orange
    base_0a: smart_color(0xe3c78a), // yellow
    base_0b: smart_color(0x8cc85f), // green
    base_0c: smart_color(0x79dac8), // turquoise - cyan
    base_0d: smart_color(0x80a0ff), // blue - signature moonfly blue
    base_0e: smart_color(0xcf87e8), // violet - purple
    base_0f: smart_color(0xf09479), // coral - brown/extra
});

// Backward compatibility alias
#[allow(dead_code)]
pub static OCEANIC_NEXT: &LazyLock<Base16Palette> = &OCEANIC_NEXT_PALETTE;

// ============================================================================
// Color utilities for focus states
// ============================================================================

impl Base16Palette {
    pub fn popup_border_color(&self) -> Color {
        self.base_05
    }

    pub fn get_interface_colors(
        &self,
        is_content_mode: bool,
    ) -> (Color, Color, Color, Color, Color) {
        if is_content_mode {
            (
                self.base_03,
                self.base_07,
                self.base_02,
                self.base_02,
                self.base_06,
            )
        } else {
            (
                self.base_05,
                self.base_07,
                self.base_04,
                self.base_02,
                self.base_06,
            )
        }
    }

    pub fn get_panel_colors(&self, is_focused: bool) -> (Color, Color, Color) {
        if is_focused {
            (self.base_07, self.base_04, self.base_00)
        } else {
            (self.base_03, self.base_03, self.base_00)
        }
    }

    pub fn get_selection_colors(&self, is_focused: bool) -> (Color, Color) {
        if is_focused {
            (self.base_02, self.base_06)
        } else {
            (self.base_02, self.base_03)
        }
    }
}
