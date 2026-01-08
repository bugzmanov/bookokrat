use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use once_cell::sync::Lazy;

pub const CURRENT_VERSION: u32 = 1;
const SETTINGS_FILENAME: &str = ".bookokrat_settings.yaml";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,

    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default)]
    pub margin: u16,

    #[serde(default = "default_annotation_highlight_color")]
    pub annotation_highlight_color: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_themes: Vec<YamlTheme>,
}

fn default_version() -> u32 {
    CURRENT_VERSION
}

fn default_theme() -> String {
    "Oceanic Next".to_string()
}

fn default_annotation_highlight_color() -> String {
    "7FB4CA".to_string() // Cyan (base0C) from Kanagawa theme
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            theme: default_theme(),
            margin: 0,
            annotation_highlight_color: default_annotation_highlight_color(),
            custom_themes: Vec::new(),
        }
    }
}

static SETTINGS: Lazy<RwLock<Settings>> = Lazy::new(|| RwLock::new(Settings::default()));

fn settings_path() -> Option<PathBuf> {
    home::home_dir().map(|home| home.join(SETTINGS_FILENAME))
}

pub fn load_settings() {
    let Some(path) = settings_path() else {
        warn!("Could not determine home directory, using default settings");
        return;
    };

    if !path.exists() {
        info!(
            "Settings file not found at {:?}, creating with defaults",
            path
        );
        if let Ok(settings) = SETTINGS.read() {
            save_settings_to_file(&settings, &path);
        }
        return;
    }

    match fs::read_to_string(&path) {
        Ok(content) => match serde_yaml::from_str::<Settings>(&content) {
            Ok(mut settings) => {
                debug!("Loaded settings from {:?}", path);

                // Run migrations if needed
                if settings.version < CURRENT_VERSION {
                    migrate_settings(&mut settings);
                    save_settings_to_file(&settings, &path);
                }

                if let Ok(mut global) = SETTINGS.write() {
                    *global = settings;
                }
            }
            Err(e) => {
                error!("Failed to parse settings file {:?}: {}", path, e);
            }
        },
        Err(e) => {
            error!("Failed to read settings file {:?}: {}", path, e);
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
    let Some(path) = settings_path() else {
        warn!("Could not determine home directory, cannot save settings");
        return;
    };

    if let Ok(settings) = SETTINGS.read() {
        save_settings_to_file(&settings, &path);
    }
}

fn save_settings_to_file(settings: &Settings, path: &PathBuf) {
    let content = generate_settings_yaml(settings);

    match fs::write(path, content) {
        Ok(()) => debug!("Saved settings to {:?}", path),
        Err(e) => error!("Failed to save settings to {:?}: {}", path, e),
    }
}

fn generate_settings_yaml(settings: &Settings) -> String {
    let mut content = String::new();

    content.push_str(&format!("version: {}\n", settings.version));
    content.push_str(&format!("theme: \"{}\"\n", settings.theme));
    content.push_str(&format!("margin: {}\n", settings.margin));
    content.push('\n');
    content.push_str(
        "# Annotation highlight color (hex color without #, e.g., \"7FB4CA\" for cyan)\n",
    );
    content.push_str("# Set to \"none\" or \"disabled\" to disable highlighting\n");
    content.push_str(&format!(
        "annotation_highlight_color: \"{}\"\n",
        settings.annotation_highlight_color
    ));
    content.push('\n');

    content.push_str(CUSTOM_THEMES_TEMPLATE);

    if !settings.custom_themes.is_empty() {
        content.push_str("custom_themes:\n");
        for theme in &settings.custom_themes {
            content.push_str(&format!("  - scheme: \"{}\"\n", theme.scheme));
            if let Some(author) = &theme.author {
                content.push_str(&format!("    author: \"{}\"\n", author));
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

pub fn get_custom_themes() -> Vec<YamlTheme> {
    SETTINGS
        .read()
        .map(|s| s.custom_themes.clone())
        .unwrap_or_default()
}

pub fn get_annotation_highlight_color() -> String {
    SETTINGS
        .read()
        .map(|s| s.annotation_highlight_color.clone())
        .unwrap_or_else(|_| default_annotation_highlight_color())
}
