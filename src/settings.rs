use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

pub const CURRENT_VERSION: u32 = 3;
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

/// Book list sort order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BookSortOrder {
    /// Sort alphabetically by name
    #[default]
    ByName,
    /// Group by type: PDFs first, then EPUBs, each sorted by name
    ByType,
}

/// Display mode for lookup command results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LookupDisplay {
    /// Show command output in a scrollable popup
    #[default]
    Popup,
    /// Run command and forget (just show notification)
    FireAndForget,
}

/// PDF render mode for terminals with Kitty graphics protocol
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

/// PDF page layout mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PdfPageLayoutMode {
    /// Show one page at a time
    #[default]
    Single,
    /// Show two pages side by side
    Dual,
}

impl PdfPageLayoutMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PdfPageLayoutMode::Single => "Single",
            PdfPageLayoutMode::Dual => "Dual",
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

    #[serde(default)]
    pub pdf_page_layout_mode: PdfPageLayoutMode,

    /// True if user has seen/configured PDF settings (used for migration prompt)
    #[serde(default)]
    pub pdf_settings_configured: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_themes: Vec<YamlTheme>,

    #[serde(default)]
    pub justify_text: bool,

    #[serde(default)]
    pub book_sort_order: BookSortOrder,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nav_panel_width: Option<u16>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup_command: Option<String>,

    #[serde(default)]
    pub lookup_display: LookupDisplay,

    /// Editor command for SyncTeX inverse search (PDF → source).
    /// Placeholders: {file}, {line}, {column}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synctex_editor: Option<String>,
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
            pdf_page_layout_mode: PdfPageLayoutMode::default(),
            pdf_settings_configured: true, // New installs are considered configured
            custom_themes: Vec::new(),
            justify_text: false,
            book_sort_order: BookSortOrder::default(),
            lookup_command: None,
            lookup_display: LookupDisplay::default(),
            nav_panel_width: None,
            synctex_editor: None,
        }
    }
}

static SETTINGS: LazyLock<RwLock<Settings>> = LazyLock::new(|| RwLock::new(Settings::default()));

/// Canonical config directory for the app. XDG-style (`~/.config/bookokrat/`)
/// on macOS and Linux; `%APPDATA%\bookokrat\` on Windows (via `dirs`).
/// The macOS override keeps us consistent with other brew-installed TUI apps
/// (nvim, helix, starship) rather than following Apple's GUI convention.
pub fn preferred_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::home_dir().map(|h| h.join(".config").join(APP_NAME))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::config_dir().map(|c| c.join(APP_NAME))
    }
}

fn preferred_config_path() -> Option<PathBuf> {
    preferred_config_dir().map(|dir| dir.join(SETTINGS_FILENAME))
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
    if let (Some(preferred), Some(legacy)) = (preferred_config_path(), legacy_config_path())
        && preferred.exists()
        && legacy.exists()
    {
        info!(
            "Both preferred and legacy settings files exist; using {:?} and ignoring {:?}",
            preferred, legacy
        );
    }

    if let Some(path) = find_existing_config() {
        if let Err((path, err)) = load_settings_from_path(&path) {
            eprintln!("Error: Failed to parse config file:");
            eprintln!("  {}", path.display());
            eprintln!();
            eprintln!("  {err}");
            eprintln!();
            eprintln!("Fix the file manually, or delete it to start fresh.");
            std::process::exit(1);
        }
    } else {
        let Some(path) = preferred_config_path() else {
            warn!("Could not determine config directory, using default settings");
            return;
        };
        info!("Settings file not found, creating with defaults at {path:?}");
        if let Ok(mut settings) = SETTINGS.write() {
            let caps = crate::terminal::detect_terminal_with_probe();
            if caps.pdf.supports_scroll_mode {
                settings.pdf_render_mode = PdfRenderMode::Scroll;
            }
            save_settings_to_file(&settings, &path);
        }
    }
}

fn load_settings_from_path(path: &PathBuf) -> Result<(), (PathBuf, String)> {
    let content = fs::read_to_string(path).map_err(|e| {
        error!("Failed to read settings file {path:?}: {e}");
        (path.clone(), e.to_string())
    })?;

    let mut settings = serde_yaml::from_str::<Settings>(&content).map_err(|e| {
        error!("Failed to parse settings file {path:?}: {e}");
        (path.clone(), e.to_string())
    })?;

    debug!(
        "Loaded settings from {path:?}: {}",
        settings_summary(&settings)
    );

    if settings.version < CURRENT_VERSION {
        let migrated_content = migrate_settings(&mut settings, &content);
        let updated = update_settings_values(&migrated_content, &settings);
        match fs::write(path, updated) {
            Ok(()) => debug!("Migrated settings in {path:?}"),
            Err(e) => error!("Failed to save migrated settings to {path:?}: {e}"),
        }
    }

    if let Ok(mut global) = SETTINGS.write() {
        *global = settings;
    }
    Ok(())
}

fn migrate_settings(settings: &mut Settings, file_content: &str) -> String {
    info!(
        "Migrating settings from v{} to v{}",
        settings.version, CURRENT_VERSION
    );

    let mut content = file_content.to_string();

    // v2 -> v3: insert lookup command template before custom_themes section
    if settings.version < 3 && !content.contains("lookup_command") {
        let insert_text = format!("\n{}", LOOKUP_COMMAND_TEMPLATE);
        if let Some(pos) = content.find("# Custom Themes") {
            // Back up to the start of the comment block separator line
            let insert_pos = content[..pos]
                .rfind("\n# ====")
                .map(|p| p + 1)
                .unwrap_or(pos);
            content.insert_str(insert_pos, &insert_text);
        } else if let Some(pos) = content.find("custom_themes:") {
            content.insert_str(pos, &insert_text);
        } else {
            content.push_str(&insert_text);
        }
    }

    settings.version = CURRENT_VERSION;
    content
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

fn settings_backup_path(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name() else {
        return path.with_extension("bak");
    };
    let mut backup_name = file_name.to_os_string();
    backup_name.push(".bak");
    path.with_file_name(backup_name)
}

fn settings_summary(settings: &Settings) -> String {
    format!(
        "theme=\"{}\" margin={} transparent={} pdf_scale={:.3} pdf_mode={} pdf_layout={} pdf_enabled={} justify={} nav_panel_width={} lookup_command_set={} synctex_editor_set={}",
        settings.theme,
        settings.margin,
        settings.transparent_background,
        settings.pdf_scale,
        match settings.pdf_render_mode {
            PdfRenderMode::Page => "page",
            PdfRenderMode::Scroll => "scroll",
        },
        match settings.pdf_page_layout_mode {
            PdfPageLayoutMode::Single => "single",
            PdfPageLayoutMode::Dual => "dual",
        },
        settings.pdf_enabled,
        settings.justify_text,
        settings
            .nav_panel_width
            .map(|width| width.to_string())
            .unwrap_or_else(|| "null".to_string()),
        settings.lookup_command.is_some(),
        settings.synctex_editor.is_some(),
    )
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

    // If file exists, do targeted update preserving user comments and manual edits
    let content = if let Ok(existing) = fs::read_to_string(path) {
        update_settings_values(&existing, settings)
    } else {
        generate_settings_yaml(settings)
    };

    if path.exists() {
        let backup_path = settings_backup_path(path);
        if let Err(e) = fs::copy(path, &backup_path) {
            warn!("Failed to back up settings file to {backup_path:?}: {e}");
        }
    }

    // Atomic write: write to a temp file in the same directory, then rename.
    // This prevents concurrent save_settings() calls from reading a
    // half-written file and regenerating from defaults (wiping user config).
    let parent = path.parent().unwrap_or(Path::new("."));
    match tempfile::NamedTempFile::new_in(parent) {
        Ok(mut tmp) => {
            use std::io::Write;
            if let Err(e) = tmp.write_all(content.as_bytes()) {
                error!("Failed to write settings temp file: {e}");
                return;
            }
            if let Err(e) = tmp.persist(path) {
                error!("Failed to persist settings file: {e}");
            } else {
                debug!("Saved settings to {path:?}: {}", settings_summary(settings));
            }
        }
        Err(e) => {
            error!("Failed to create settings temp file: {e}");
            // Fallback to direct write
            match fs::write(path, content) {
                Ok(()) => debug!(
                    "Saved settings to {path:?} (direct): {}",
                    settings_summary(settings)
                ),
                Err(e) => error!("Failed to save settings to {path:?}: {e}"),
            }
        }
    }
}

/// Update only app-managed keys in an existing config file, preserving
/// all comments, blank lines, and user-managed sections (lookup_command,
/// lookup_display, custom_themes).
fn update_settings_values(existing_content: &str, settings: &Settings) -> String {
    use std::collections::HashSet;

    let key_values = app_managed_key_values(settings);
    let key_set: HashSet<&str> = key_values.iter().map(|(k, _)| k.as_str()).collect();
    let mut found_keys = HashSet::new();

    let mut lines: Vec<String> = existing_content.lines().map(|l| l.to_string()).collect();

    for line in lines.iter_mut() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let key_owned = match trimmed.find(':') {
            Some(pos) => trimmed[..pos].trim().to_string(),
            None => continue,
        };
        if key_set.contains(key_owned.as_str()) {
            if let Some((_, value)) = key_values.iter().find(|(k, _)| k == &key_owned) {
                *line = format!("{}: {}", key_owned, value);
                found_keys.insert(key_owned);
            }
        }
    }

    // Append any app-managed keys missing from the file
    let mut appended = false;
    for (key, value) in &key_values {
        if !found_keys.contains(key.as_str()) {
            if !appended {
                lines.push(String::new());
                appended = true;
            }
            lines.push(format!("{}: {}", key, value));
        }
    }

    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn app_managed_key_values(settings: &Settings) -> Vec<(String, String)> {
    vec![
        ("version".into(), format!("{}", settings.version)),
        ("theme".into(), format!("\"{}\"", settings.theme)),
        ("margin".into(), format!("{}", settings.margin)),
        (
            "transparent_background".into(),
            format!("{}", settings.transparent_background),
        ),
        ("pdf_scale".into(), format!("{}", settings.pdf_scale)),
        (
            "pdf_pan_shift".into(),
            format!("{}", settings.pdf_pan_shift),
        ),
        (
            "pdf_render_mode".into(),
            match settings.pdf_render_mode {
                PdfRenderMode::Page => "page".into(),
                PdfRenderMode::Scroll => "scroll".into(),
            },
        ),
        (
            "pdf_page_layout_mode".into(),
            match settings.pdf_page_layout_mode {
                PdfPageLayoutMode::Single => "single".into(),
                PdfPageLayoutMode::Dual => "dual".into(),
            },
        ),
        ("pdf_enabled".into(), format!("{}", settings.pdf_enabled)),
        (
            "pdf_settings_configured".into(),
            format!("{}", settings.pdf_settings_configured),
        ),
        ("justify_text".into(), format!("{}", settings.justify_text)),
        (
            "book_sort_order".into(),
            match settings.book_sort_order {
                BookSortOrder::ByName => "by_name".into(),
                BookSortOrder::ByType => "by_type".into(),
            },
        ),
        (
            "nav_panel_width".into(),
            settings
                .nav_panel_width
                .map(|w| format!("{}", w))
                .unwrap_or_else(|| "null".into()),
        ),
        (
            "lookup_command".into(),
            settings
                .lookup_command
                .as_ref()
                .map(|c| format!("'{}'", c.replace('\'', "''")))
                .unwrap_or_else(|| "null".into()),
        ),
        (
            "lookup_display".into(),
            match settings.lookup_display {
                LookupDisplay::Popup => "popup".into(),
                LookupDisplay::FireAndForget => "fire_and_forget".into(),
            },
        ),
        (
            "synctex_editor".into(),
            settings
                .synctex_editor
                .as_ref()
                .map(|c| format!("'{}'", c.replace('\'', "''")))
                .unwrap_or_else(|| "null".into()),
        ),
    ]
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
    let layout_mode_str = match settings.pdf_page_layout_mode {
        PdfPageLayoutMode::Single => "single",
        PdfPageLayoutMode::Dual => "dual",
    };
    content.push_str(&format!("pdf_page_layout_mode: {}\n", layout_mode_str));
    content.push_str(&format!("pdf_enabled: {}\n", settings.pdf_enabled));
    content.push_str(&format!(
        "pdf_settings_configured: {}\n",
        settings.pdf_settings_configured
    ));
    let sort_str = match settings.book_sort_order {
        BookSortOrder::ByName => "by_name",
        BookSortOrder::ByType => "by_type",
    };
    content.push_str(&format!("justify_text: {}\n", settings.justify_text));
    content.push_str(&format!("book_sort_order: {}\n", sort_str));
    if let Some(w) = settings.nav_panel_width {
        content.push_str(&format!("nav_panel_width: {}\n", w));
    }
    content.push('\n');

    if let Some(ref cmd) = settings.lookup_command {
        content.push_str(&format!("lookup_command: \"{}\"\n", cmd));
        let display_str = match settings.lookup_display {
            LookupDisplay::Popup => "popup",
            LookupDisplay::FireAndForget => "fire_and_forget",
        };
        content.push_str(&format!("lookup_display: {}\n", display_str));
    } else {
        content.push_str(LOOKUP_COMMAND_TEMPLATE);
    }
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
    }

    content
}

const CUSTOM_THEMES_TEMPLATE: &str = r#"# ============================================================================
# Custom Themes
# ============================================================================
# Add your own themes below. Find Base16 themes at:
# https://github.com/tinted-theming/schemes
#
# To add a theme, uncomment and edit the lines below:
#
# custom_themes:
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

const LOOKUP_COMMAND_TEMPLATE: &str = r#"# ============================================================================
# Lookup Command
# ============================================================================
# Shell command to run when you press Space+l on selected text.
# Use {} as a placeholder for the selected word. If no {} is present,
# the selected text is appended as a shell-escaped argument.
#
# lookup_display controls how output is shown:
#   popup          - capture stdout and show in a scrollable popup (default)
#   fire_and_forget - spawn command and move on (e.g., open a browser)
#
# Example: CLI dictionary (output shown in popup)
#   lookup_command: "dict {}"
#   lookup_display: popup
#
# Example: macOS Dictionary.app
#   lookup_command: "open dict://{}"
#   lookup_display: fire_and_forget
#
# Example: online dictionary in browser
#   lookup_command: "open 'https://www.merriam-webster.com/dictionary/{}'"
#   lookup_display: fire_and_forget

# ============================================================================
# SyncTeX inverse search: editor command to run when you Ctrl+click,
# secondary-click, or press 'gd' on a PDF rendered from LaTeX.
# Placeholders: {file}, {line}, {column}
#
# Example: neovim via remote socket
#   synctex_editor: "nvim --server /tmp/nvim.sock --remote-send '<C-\\><C-n>:e {file}<CR>:{line}<CR>'"
#
# Example: open in new terminal nvim
#   synctex_editor: "nvim +{line} {file}"

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

pub fn get_pdf_page_layout_mode() -> PdfPageLayoutMode {
    SETTINGS
        .read()
        .map(|s| s.pdf_page_layout_mode)
        .unwrap_or_default()
}

pub fn set_pdf_page_layout_mode(mode: PdfPageLayoutMode) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.pdf_page_layout_mode = mode;
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

pub fn is_justify_text() -> bool {
    SETTINGS.read().map(|s| s.justify_text).unwrap_or(false)
}

pub fn set_justify_text(justify: bool) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.justify_text = justify;
    }
    save_settings();
}

pub fn get_book_sort_order() -> BookSortOrder {
    SETTINGS
        .read()
        .map(|s| s.book_sort_order)
        .unwrap_or_default()
}

pub fn set_book_sort_order(order: BookSortOrder) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.book_sort_order = order;
    }
    save_settings();
}

pub fn get_lookup_command() -> Option<String> {
    SETTINGS.read().ok().and_then(|s| s.lookup_command.clone())
}

pub fn get_lookup_display() -> LookupDisplay {
    SETTINGS
        .read()
        .map(|s| s.lookup_display)
        .unwrap_or_default()
}

pub fn get_nav_panel_width() -> Option<u16> {
    SETTINGS.read().ok().and_then(|s| s.nav_panel_width)
}

pub fn set_nav_panel_width(width: Option<u16>) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.nav_panel_width = width;
    }
    save_settings();
}

pub fn get_synctex_editor() -> Option<String> {
    SETTINGS.read().ok().and_then(|s| s.synctex_editor.clone())
}

pub fn set_lookup_command(cmd: Option<String>) {
    if let Ok(mut s) = SETTINGS.write() {
        s.lookup_command = cmd;
    }
    save_settings();
}

pub fn set_lookup_display(mode: LookupDisplay) {
    if let Ok(mut s) = SETTINGS.write() {
        s.lookup_display = mode;
    }
    save_settings();
}

pub fn set_synctex_editor(cmd: Option<String>) {
    if let Ok(mut s) = SETTINGS.write() {
        s.synctex_editor = cmd;
    }
    save_settings();
}

pub fn set_integrations(
    lookup_command: Option<String>,
    lookup_display: LookupDisplay,
    synctex_editor: Option<String>,
) {
    if let Ok(mut settings) = SETTINGS.write() {
        settings.lookup_command = lookup_command;
        settings.lookup_display = lookup_display;
        settings.synctex_editor = synctex_editor;
    }
    save_settings();
}

/// Called on app startup to fix incompatible settings when switching terminals
/// (e.g., from a Kitty-protocol terminal to one without Kitty graphics)
pub fn fix_incompatible_pdf_settings() {
    let caps = crate::terminal::detect_terminal_with_probe();
    let current_mode = get_pdf_render_mode();

    // If user switched away from Kitty protocol with Scroll mode selected, silently fix it.
    if !caps.pdf.supports_scroll_mode && current_mode == PdfRenderMode::Scroll {
        if let Ok(mut settings) = SETTINGS.write() {
            settings.pdf_render_mode = PdfRenderMode::Page;
        }
        save_settings();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targeted_update_preserves_user_managed_sections() {
        let existing = r#"version: 2
theme: "Old Theme"
margin: 0
pdf_scale: 1
pdf_pan_shift: 0
pdf_render_mode: page
pdf_enabled: true
pdf_settings_configured: false
book_sort_order: by_name

# manual section
lookup_command: "open dict://{}"
lookup_display: fire_and_forget

custom_themes:
  - scheme: "Manual Theme"
    base00: "000000"
"#;

        let mut settings = Settings::default();
        settings.version = CURRENT_VERSION;
        settings.theme = "Oceanic Next".to_string();
        settings.margin = 3;
        settings.pdf_scale = 1.25;
        settings.pdf_render_mode = PdfRenderMode::Scroll;
        settings.lookup_command = Some("open dict://{}".to_string());
        settings.lookup_display = LookupDisplay::FireAndForget;

        let updated = update_settings_values(existing, &settings);

        assert!(updated.contains("lookup_command: 'open dict://{}'"));
        assert!(updated.contains("lookup_display: fire_and_forget"));
        assert!(updated.contains("custom_themes:\n  - scheme: \"Manual Theme\""));
        assert!(updated.contains("theme: \"Oceanic Next\""));
        assert!(updated.contains("margin: 3"));
        assert!(updated.contains("pdf_scale: 1.25"));
        assert!(updated.contains("pdf_render_mode: scroll"));
    }

    #[test]
    fn targeted_update_preserves_non_managed_keys_and_comments() {
        let existing = r#"# top comment
version: 2 # inline note
theme: "Old Theme"
margin: 1
my_custom_flag: true
"#;

        let mut settings = Settings::default();
        settings.version = CURRENT_VERSION;
        settings.theme = "New Theme".to_string();
        settings.margin = 7;

        let updated = update_settings_values(existing, &settings);

        assert!(updated.contains("# top comment"));
        assert!(updated.contains("my_custom_flag: true"));
        assert!(updated.contains("theme: \"New Theme\""));
        assert!(updated.contains("margin: 7"));
        assert!(updated.contains("version: 3"));
    }

    #[test]
    fn targeted_update_appends_missing_app_managed_keys() {
        let existing = "lookup_command: \"dict {}\"\n";
        let mut settings = Settings::default();
        settings.lookup_command = Some("dict {}".to_string());

        let updated = update_settings_values(existing, &settings);

        assert!(updated.contains("lookup_command: 'dict {}'"));
        assert!(updated.contains("version: 3"));
        assert!(updated.contains("theme: \"Oceanic Next\""));
        assert!(updated.contains("book_sort_order: by_name"));
    }

    #[test]
    fn migration_inserts_lookup_template_before_custom_themes() {
        let original = format!(
            "version: 2\n{}\n",
            CUSTOM_THEMES_TEMPLATE.trim_end_matches('\n')
        );
        let mut settings = Settings {
            version: 2,
            ..Settings::default()
        };

        let migrated = migrate_settings(&mut settings, &original);

        let lookup_pos = migrated.find("# Lookup Command").unwrap();
        let themes_pos = migrated.find("# Custom Themes").unwrap();
        assert!(lookup_pos < themes_pos);
        assert_eq!(settings.version, CURRENT_VERSION);
    }

    #[test]
    fn backup_path_appends_bak_to_filename() {
        let path = Path::new("/tmp/bookokrat/config.yaml");
        assert_eq!(
            settings_backup_path(path),
            PathBuf::from("/tmp/bookokrat/config.yaml.bak")
        );
    }

    #[test]
    fn save_settings_creates_backup_before_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.yaml");
        fs::write(&path, "theme: \"Old\"\nmargin: 11\n").unwrap();

        let settings = Settings {
            theme: "New".to_string(),
            margin: 7,
            ..Settings::default()
        };
        save_settings_to_file(&settings, &path);

        let backup = settings_backup_path(&path);
        let backup_content = fs::read_to_string(backup).unwrap();
        assert!(backup_content.contains("theme: \"Old\""));
        assert!(backup_content.contains("margin: 11"));
    }
}
