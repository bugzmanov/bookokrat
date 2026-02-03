use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

pub const CURRENT_VERSION: u32 = 2;
const SETTINGS_FILENAME: &str = "config.yaml";
const LEGACY_SETTINGS_FILENAME: &str = ".bookokrat_settings.yaml";
const APP_NAME: &str = "bookokrat";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlTheme {
    pub scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub base00: String,
    pub base01: String,
    pub base02: String,
    pub base03: String,
    pub base04: String,
    pub base05: String,
    pub base06: String,
    pub base07: String,
    pub base08: String,
    pub base09: String,
    #[serde(alias = "base0A")]
    pub base0a: String,
    #[serde(alias = "base0B")]
    pub base0b: String,
    #[serde(alias = "base0C")]
    pub base0c: String,
    #[serde(alias = "base0D")]
    pub base0d: String,
    #[serde(alias = "base0E")]
    pub base0e: String,
    #[serde(alias = "base0F")]
    pub base0f: String,
}

/// PDF render mode for Kitty terminals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PdfRenderMode {
    /// Page-at-a-time mode (lower memory usage)
    #[default]
    Page,
    /// Continuous scroll mode (300-500MB memory usage)
    Scroll,
}

impl PdfRenderMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PdfRenderMode::Page => "Page",
            PdfRenderMode::Scroll => "Scroll",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,

    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default)]
    pub margin: u16,

    #[serde(default)]
    pub transparent_background: bool,

    #[serde(default = "default_pdf_scale")]
    pub pdf_scale: f32,

    #[serde(default)]
    pub pdf_pan_shift: u16,

    #[serde(default)]
    pub pdf_render_mode: PdfRenderMode,

    #[serde(default = "default_true")]
    pub pdf_enabled: bool,

    /// True if user has seen/configured PDF settings (used for migration prompt)
    #[serde(default)]
    pub pdf_settings_configured: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_themes: Vec<YamlTheme>,
}

fn default_true() -> bool {
    true
}

fn default_pdf_scale() -> f32 {
    1.0
}

fn default_version() -> u32 {
    CURRENT_VERSION
}

fn default_theme() -> String {
    "Oceanic Next".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            theme: default_theme(),
            margin: 0,
            transparent_background: false,
            pdf_scale: default_pdf_scale(),
            pdf_pan_shift: 0,
            pdf_render_mode: PdfRenderMode::default(),
            pdf_enabled: true,
            pdf_settings_configured: true, // New installs are considered configured
            custom_themes: Vec::new(),
        }
    }
}

static SETTINGS: LazyLock<RwLock<Settings>> = LazyLock::new(|| RwLock::new(Settings::default()));

fn preferred_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|config| config.join(APP_NAME).join(SETTINGS_FILENAME))
}

fn legacy_config_path() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(LEGACY_SETTINGS_FILENAME))
}

fn find_existing_config() -> Option<PathBuf> {
    if let Some(path) = preferred_config_path() {
        if path.exists() {
            return Some(path);
        }
    }
    if let Some(path) = legacy_config_path() {
        if path.exists() {
            return Some(path);
        }
    }
    None
}

pub fn load_settings() {
    if let Some(path) = find_existing_config() {
        load_settings_from_path(&path);
    } else {
        let Some(path) = preferred_config_path() else {
            warn!("Could not determine config directory, using default settings");
            return;
        };
        info!("Settings file not found, creating with defaults at {path:?}");
        if let Ok(settings) = SETTINGS.read() {
            save_settings_to_file(&settings, &path);
        }
    }
}

fn load_settings_from_path(path: &PathBuf) {
    match fs::read_to_string(path) {
        Ok(content) => match serde_yaml::from_str::<Settings>(&content) {
            Ok(mut settings) => {
                debug!("Loaded settings from {path:?}");

                if settings.version < CURRENT_VERSION {
                    migrate_settings(&mut settings);
                    save_settings_to_file(&settings, path);
                }

                if let Ok(mut global) = SETTINGS.write() {
                    *global = settings;
                }
            }
            Err(e) => {
                error!("Failed to parse settings file {path:?}: {e}");
            }
        },
        Err(e) => {
            error!("Failed to read settings file {path:?}: {e}");
        }
    }
}

fn migrate_settings(settings: &mut Settings) {
    info!(
        "Migrating settings from v{} to v{}",
        settings.version, CURRENT_VERSION
    );

    // Future migrations go here:
    // if settings.version < 2 {
    //     migrate_v1_to_v2(settings);
    // }

    settings.version = CURRENT_VERSION;
}

pub fn save_settings() {
    let path = find_existing_config().or_else(preferred_config_path);
    let Some(path) = path else {
        warn!("Could not determine config directory, cannot save settings");
        return;
    };

    if let Ok(settings) = SETTINGS.read() {
        save_settings_to_file(&settings, &path);
    }
}

fn save_settings_to_file(settings: &Settings, path: &PathBuf) {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                error!("Failed to create config directory {parent:?}: {e}");
                return;
            }
        }
    }

    let content = generate_settings_yaml(settings);

    match fs::write(path, content) {
        Ok(()) => debug!("Saved settings to {path:?}"),
        Err(e) => error!("Failed to save settings to {path:?}: {e}"),
    }
}

fn generate_settings_yaml(settings: &Settings) -> String {
    let mut content = String::new();

    content.push_str(&format!("version: {}\n", settings.version));
    content.push_str(&format!("theme: \"{}\"\n", settings.theme));
    content.push_str(&format!("margin: {}\n", settings.margin));
    content.push_str(&format!(
        "transparent_background: {}\n",
        settings.transparent_background
    ));
    content.push_str(&format!("pdf_scale: {}\n", settings.pdf_scale));
    content.push_str(&format!("pdf_pan_shift: {}\n", settings.pdf_pan_shift));
    let mode_str = match settings.pdf_render_mode {
        PdfRenderMode::Page => "page",
        PdfRenderMode::Scroll => "scroll",
    };
    content.push_str(&format!("pdf_render_mode: {}\n", mode_str));
    content.push_str(&format!("pdf_enabled: {}\n", settings.pdf_enabled));
    content.push_str(&format!(
        "pdf_settings_configured: {}\n",
        settings.pdf_settings_configured
    ));
    content.push('\n');

    content.push_str(CUSTOM_THEMES_TEMPLATE);

    if !settings.custom_themes.is_empty() {
        content.push_str("custom_themes:\n");
        for theme in &settings.custom_themes {
            content.push_str(&format!("  - scheme: \"{}\"\n", theme.scheme));
            if let Some(author) = &theme.author {
                content.push_str(&format!("    author: \"{author}\"\n"));
            }
            content.push_str(&format!("    base00: \"{}\"\n", theme.base00));
            content.push_str(&format!("    base01: \"{}\"\n", theme.base01));
            content.push_str(&format!("    base02: \"{}\"\n", theme.base02));
            content.push_str(&format!("    base03: \"{}\"\n", theme.base03));
            content.push_str(&format!("    base04: \"{}\"\n", theme.base04));
            content.push_str(&format!("    base05: \"{}\"\n", theme.base05));
            content.push_str(&format!("    base06: \"{}\"\n", theme.base06));
            content.push_str(&format!("    base07: \"{}\"\n", theme.base07));
            content.push_str(&format!("    base08: \"{}\"\n", theme.base08));
            content.push_str(&format!("    base09: \"{}\"\n", theme.base09));
            content.push_str(&format!("    base0A: \"{}\"\n", theme.base0a));
            content.push_str(&format!("    base0B: \"{}\"\n", theme.base0b));
            content.push_str(&format!("    base0C: \"{}\"\n", theme.base0c));
            content.push_str(&format!("    base0D: \"{}\"\n", theme.base0d));
            content.push_str(&format!("    base0E: \"{}\"\n", theme.base0e));
            content.push_str(&format!("    base0F: \"{}\"\n", theme.base0f));
            content.push('\n');
        }
    } else {
        content.push_str("custom_themes: []\n");
    }

    content
}

const CUSTOM_THEMES_TEMPLATE: &str = r#"# ============================================================================
# Custom Themes
# ============================================================================
# Add your own themes below. Find Base16 themes at:
# https://github.com/tinted-theming/schemes
#
# Example:
#   - scheme: "My Custom Theme"
#     author: "Your Name"
#     base00: "1F1F28"    # Main background
#     base01: "2A2A37"    # Lighter background (status bars)
#     base02: "223249"    # Selection background
#     base03: "727169"    # Comments, muted text
#     base04: "C8C093"    # Dark foreground
#     base05: "DCD7BA"    # Default text
#     base06: "DCD7BA"    # Light foreground
#     base07: "E6E0C2"    # Brightest text
#     base08: "C34043"    # Red (errors)
#     base09: "FFA066"    # Orange (constants)
#     base0A: "DCA561"    # Yellow (search)
#     base0B: "98BB6C"    # Green (strings)
#     base0C: "7FB4CA"    # Cyan
#     base0D: "7E9CD8"    # Blue (links)
#     base0E: "957FB8"    # Purple (keywords)
#     base0F: "D27E99"    # Brown/Pink

"#;

// Public API for accessing/modifying settings

pub fn get_theme_name() -> String {
    SETTINGS
        .read()
        .map(|s| s.theme.clone())
        .unwrap_or_else(|_| default_theme())
}

pub fn set_theme_name(name: &str) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.theme = name.to_string();
    }
    save_settings();
}

pub fn get_margin() -> u16 {
    SETTINGS.read().map(|s| s.margin).unwrap_or(0)
}

pub fn set_margin(margin: u16) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.margin = margin;
    }
    save_settings();
}

pub fn is_transparent_background() -> bool {
    SETTINGS
        .read()
        .map(|s| s.transparent_background)
        .unwrap_or(false)
}

pub fn set_transparent_background(transparent: bool) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.transparent_background = transparent;
    }
    save_settings();
}

pub fn get_custom_themes() -> Vec<YamlTheme> {
    SETTINGS
        .read()
        .map(|s| s.custom_themes.clone())
        .unwrap_or_default()
}

pub fn get_pdf_scale() -> f32 {
    SETTINGS
        .read()
        .map(|s| s.pdf_scale)
        .unwrap_or_else(|_| default_pdf_scale())
}

pub fn set_pdf_scale(scale: f32) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.pdf_scale = scale;
    }
    save_settings();
}

pub fn get_pdf_pan_shift() -> u16 {
    SETTINGS.read().map(|s| s.pdf_pan_shift).unwrap_or(0)
}

pub fn set_pdf_pan_shift(pan_shift: u16) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.pdf_pan_shift = pan_shift;
    }
    save_settings();
}

pub fn get_pdf_render_mode() -> PdfRenderMode {
    SETTINGS
        .read()
        .map(|s| s.pdf_render_mode)
        .unwrap_or_default()
}

pub fn set_pdf_render_mode(mode: PdfRenderMode) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.pdf_render_mode = mode;
    }
    save_settings();
}

pub fn is_pdf_enabled() -> bool {
    SETTINGS.read().map(|s| s.pdf_enabled).unwrap_or(true)
}

pub fn set_pdf_enabled(enabled: bool) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.pdf_enabled = enabled;
    }
    save_settings();
}

pub fn is_pdf_settings_configured() -> bool {
    SETTINGS
        .read()
        .map(|s| s.pdf_settings_configured)
        .unwrap_or(true)
}

pub fn set_pdf_settings_configured(configured: bool) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.pdf_settings_configured = configured;
    }
    save_settings();
}

/// Called on app startup to fix incompatible settings when switching terminals
/// (e.g., from Kitty to WezTerm with Scroll mode selected)
pub fn fix_incompatible_pdf_settings() {
    let caps = crate::terminal::detect_terminal();
    let current_mode = get_pdf_render_mode();

    // If user switched from Kitty to non-Kitty terminal with Scroll mode, silently fix it
    if !caps.pdf.supports_scroll_mode && current_mode == PdfRenderMode::Scroll {
        if let Ok(mut settings) = SETTINGS.write() {
            settings.pdf_render_mode = PdfRenderMode::Page;
        }
        save_settings();
    }
}
