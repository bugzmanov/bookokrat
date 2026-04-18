use std::collections::HashMap;
use std::path::PathBuf;

use log::{error, info, warn};

use super::action::Action;
use super::context::KeyContext;
use super::defaults::default_keymap;
use super::keymap::Keymap;
use super::notation::{format_key_binding, parse_key_binding};

const KEYBINDINGS_FILENAME: &str = "keybindings.toml";

/// A single issue encountered while loading `keybindings.toml`.
///
/// Carries a best-effort line number (1-indexed) so the UI can render each
/// error next to the offending line. `line` is `None` only when we genuinely
/// can't locate the source (e.g., a missing file or a read error before any
/// content is parsed).
#[derive(Debug, Clone)]
pub struct LoadError {
    pub line: Option<usize>,
    pub message: String,
}

impl LoadError {
    fn with_line(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }

    fn no_line(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }
}

/// Load the keymap: defaults + user overrides. Returns any issues alongside
/// the resulting keymap so callers can surface them in the UI.
pub fn load_keymap() -> (Keymap, Vec<LoadError>) {
    let mut keymap = default_keymap();
    let mut errors = Vec::new();

    if let Some(config_path) = keybindings_config_path() {
        if config_path.exists() {
            info!("Loading keybindings from {:?}", config_path);
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    let count = apply_toml(&content, &mut keymap, &mut errors);
                    info!("Applied {count} keybinding overrides from config");
                }
                Err(e) => {
                    error!("Failed to read keybindings config: {e}");
                    errors.push(LoadError::no_line(format!(
                        "Failed to read {}: {e}",
                        config_path.display()
                    )));
                    keymap = default_keymap();
                }
            }
        } else {
            write_template_if_missing(&config_path);
        }
    }

    (keymap, errors)
}

fn keybindings_config_path() -> Option<PathBuf> {
    crate::settings::preferred_config_dir().map(|dir| dir.join(KEYBINDINGS_FILENAME))
}

/// Human-friendly representation of the keybindings config path — absolute
/// on Windows, tilde-abbreviated on Unix when it sits under `$HOME`.
/// Falls back to the bare filename if we can't resolve a config dir.
fn keybindings_config_path_display() -> String {
    let Some(path) = keybindings_config_path() else {
        return KEYBINDINGS_FILENAME.to_string();
    };
    #[cfg(unix)]
    if let Some(home) = std::env::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

/// Parse TOML content and apply overrides to the keymap. Returns the number
/// of bindings successfully applied; pushes `LoadError`s for each issue.
///
/// TOML's dotted keys and nested tables are equivalent, so users may write
/// `popup.help."?" = "cancel"` (flat, one-line) or
/// `[popup.help]` + `"?" = "cancel"` (nested block) interchangeably.
/// Both produce the same parsed tree.
fn apply_toml(content: &str, keymap: &mut Keymap, errors: &mut Vec<LoadError>) -> usize {
    let root: toml::Table = match content.parse() {
        Ok(table) => table,
        Err(e) => {
            let line = e
                .span()
                .map(|s| byte_to_line(content, s.start))
                .unwrap_or(1);
            errors.push(LoadError::with_line(
                line,
                // toml's Display includes a multi-line caret diagram; take
                // just the first line which is the plain message.
                e.to_string()
                    .lines()
                    .next()
                    .unwrap_or("invalid TOML")
                    .to_string(),
            ));
            return 0;
        }
    };

    // Walk the tree, flattening each "leaf context" (a sub-table whose
    // immediate children are all strings) to its dotted-path name.
    let mut by_context: HashMap<String, HashMap<String, String>> = HashMap::new();
    walk_toml(&root, String::new(), &mut by_context, content, errors);

    // Apply groups (all/normal/popup) first so specific contexts override.
    let (mut groups, mut specifics): (Vec<_>, Vec<_>) = by_context
        .iter()
        .partition(|(k, _)| matches!(k.as_str(), "all" | "normal" | "popup"));
    groups.sort_by_key(|(k, _)| match k.as_str() {
        "all" => 0,
        "normal" => 1,
        "popup" => 2,
        _ => 3,
    });
    specifics.sort_by_key(|(k, _)| (*k).clone());

    let mut total = 0;
    for (key, bindings) in groups.into_iter().chain(specifics.into_iter()) {
        let Some(group) = super::context::resolve_config_key(key) else {
            errors.push(LoadError::with_line(
                find_context_line(content, key).unwrap_or(1),
                format!("unknown context '{key}' (valid: global, nav, content, epub_normal, pdf, pdf_normal, popup.help, popup.history, popup.search, popup.stats, popup.comments, popup.settings, or groups all/normal/popup)"),
            ));
            continue;
        };
        for ctx_id in super::context::group_contexts(&group) {
            total += apply_context_overrides(keymap, ctx_id, bindings, content, key, errors);
        }
    }
    total
}

/// Recursively walk a TOML table, routing each string-leaf to the context
/// identified by its dotted path and each nested sub-table to the caller.
/// Non-string, non-table values become `LoadError`s since they can't express
/// a valid binding.
fn walk_toml(
    table: &toml::Table,
    prefix: String,
    out: &mut HashMap<String, HashMap<String, String>>,
    content: &str,
    errors: &mut Vec<LoadError>,
) {
    let mut local: HashMap<String, String> = HashMap::new();
    for (k, v) in table {
        match v {
            toml::Value::String(s) => {
                local.insert(k.clone(), s.clone());
            }
            toml::Value::Table(inner) => {
                let sub = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                walk_toml(inner, sub, out, content, errors);
            }
            _ => {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                errors.push(LoadError::with_line(
                    find_binding_line(content, &prefix, k).unwrap_or(1),
                    format!("{path}: binding value must be a string (got {})", type_name(v)),
                ));
            }
        }
    }
    if !prefix.is_empty() && !local.is_empty() {
        out.entry(prefix).or_default().extend(local);
    }
}

fn type_name(v: &toml::Value) -> &'static str {
    match v {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "boolean",
        toml::Value::Array(_) => "array",
        toml::Value::Datetime(_) => "datetime",
        toml::Value::Table(_) => "table",
    }
}

/// Byte offset → 1-indexed line number.
fn byte_to_line(content: &str, byte: usize) -> usize {
    content[..byte.min(content.len())].matches('\n').count() + 1
}

/// Best-effort line lookup for a specific binding's assignment. Tries the
/// flat form `<context>."<key>"` first, then the grouped form where `<key>`
/// appears after a `[<context>]` section header.
fn find_binding_line(content: &str, context: &str, key: &str) -> Option<usize> {
    let flat_needle = format!("{context}.{key}");
    for (i, line) in content.lines().enumerate() {
        if line.contains(&flat_needle) {
            return Some(i + 1);
        }
    }
    // Also try with quoted context (e.g., user wrote `"popup.help"."?"`)
    let quoted_flat = format!("\"{context}\".{key}");
    for (i, line) in content.lines().enumerate() {
        if line.contains(&quoted_flat) {
            return Some(i + 1);
        }
    }
    // Grouped form: scan for `[context]`, then the key until the next section.
    let section_open = format!("[{context}]");
    let mut in_section = false;
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == section_open {
            in_section = true;
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = false;
            continue;
        }
        if in_section && line.contains(key) {
            return Some(i + 1);
        }
    }
    None
}

/// Best-effort line lookup for a context section header or dotted prefix.
fn find_context_line(content: &str, context: &str) -> Option<usize> {
    let section_open = format!("[{context}]");
    let dotted_prefix = format!("{context}.");
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == section_open {
            return Some(i + 1);
        }
        if line.contains(&dotted_prefix) {
            return Some(i + 1);
        }
    }
    None
}

/// Format the default keymap as flat, greppable TOML.
///
/// Each line is a valid TOML dotted-key assignment and is self-contained:
/// `<context>."<key>" = "<action>"    # description`. Piping the output
/// into `~/.config/bookokrat/keybindings.toml` yields a working config that
/// re-applies every default.
pub fn print_default_keybindings() -> String {
    let keymap = default_keymap();
    let mut out = String::new();

    out.push_str("# Bookokrat default keybindings — flat TOML reference.\n");
    out.push_str("# Format:  <context>.\"<key>\" = \"<action>\"   # description\n");
    out.push_str("#\n");
    out.push_str(&format!(
        "# To override, edit {}. Either form works:\n",
        keybindings_config_path_display()
    ));
    out.push_str("#     content.\"j\" = \"scroll_up\"            # flat, one line per binding\n");
    out.push_str("#     [content]                             # or group in a table\n");
    out.push_str("#     \"j\" = \"scroll_up\"\n");
    out.push_str("# Use \"nop\" as the action to disable a default.\n");
    out.push_str("#\n");
    out.push_str("# Groups (apply before per-context overrides):\n");
    out.push_str("#   all    -> every context except `global`\n");
    out.push_str("#   normal -> epub_normal + pdf_normal\n");
    out.push_str("#   popup  -> every popup.* context\n");
    out.push('\n');

    // Collect rows across every context, build the full `<context>."<key>"`
    // lhs as a single column, then align everything globally so the file
    // reads like a table regardless of which context a line is in.
    let mut rows: Vec<(String, String, &'static str)> = Vec::new();
    for ctx in KeyContext::ALL {
        let Some(ctx_map) = keymap.context(*ctx) else {
            continue;
        };
        let mut bindings = ctx_map.all_bindings();
        if bindings.is_empty() {
            continue;
        }
        bindings.sort_by(|(k1, _), (k2, _)| format_key_binding(k1).cmp(&format_key_binding(k2)));
        for (key, action) in bindings {
            let lhs = format!("{}.\"{}\"", ctx.config_key(), format_key_binding(&key),);
            let rhs = format!("\"{}\"", action_to_toml_value(&action));
            rows.push((lhs, rhs, action.description()));
        }
    }

    let lhs_w = rows.iter().map(|(l, _, _)| l.len()).max().unwrap_or(0);
    let rhs_w = rows.iter().map(|(_, r, _)| r.len()).max().unwrap_or(0);

    for (lhs, rhs, desc) in rows {
        out.push_str(&format!("{lhs:<lhs_w$} = {rhs:<rhs_w$}  # {desc}\n"));
    }

    out
}

/// Format the default keymap as TOML with `[context]` section headers.
///
/// Same content as `print_default_keybindings()`, just grouped by context
/// — friendlier for reading top-down. Columns align per-section so each
/// block is its own neat table.
pub fn print_default_keybindings_grouped() -> String {
    let keymap = default_keymap();
    let mut out = String::new();

    out.push_str("# Bookokrat default keybindings — grouped TOML reference.\n");
    out.push_str("# Same data as `--print-default-keybindings`, organized by context.\n");
    out.push_str("#\n");
    out.push_str(&format!(
        "# To override, edit {} — either keep this\n",
        keybindings_config_path_display()
    ));
    out.push_str("# grouped shape or use per-line dotted keys:\n");
    out.push_str("#     content.\"j\" = \"scroll_up\"            # one-line form\n");
    out.push_str("# Use \"nop\" as the action to disable a default.\n");
    out.push_str("#\n");
    out.push_str("# Groups (apply before per-context overrides):\n");
    out.push_str("#   all    -> every context except `global`\n");
    out.push_str("#   normal -> epub_normal + pdf_normal\n");
    out.push_str("#   popup  -> every popup.* context\n");

    for ctx in KeyContext::ALL {
        let Some(ctx_map) = keymap.context(*ctx) else {
            continue;
        };
        let mut bindings = ctx_map.all_bindings();
        if bindings.is_empty() {
            continue;
        }
        bindings.sort_by(|(k1, _), (k2, _)| format_key_binding(k1).cmp(&format_key_binding(k2)));

        let rows: Vec<(String, String, &'static str)> = bindings
            .iter()
            .map(|(key, action)| {
                (
                    format!("\"{}\"", format_key_binding(key)),
                    format!("\"{}\"", action_to_toml_value(action)),
                    action.description(),
                )
            })
            .collect();
        let key_w = rows.iter().map(|(k, _, _)| k.len()).max().unwrap_or(0);
        let act_w = rows.iter().map(|(_, a, _)| a.len()).max().unwrap_or(0);

        out.push('\n');
        out.push_str(&format!("[{}]\n", ctx.config_key()));
        for (key, action, desc) in rows {
            out.push_str(&format!("{key:<key_w$} = {action:<act_w$}  # {desc}\n"));
        }
    }

    out
}

/// Write the default template to `path` if no file exists there.
/// Called once at startup from `load_keymap`. Failures are logged but
/// non-fatal — the app still starts with built-in defaults.
fn write_template_if_missing(path: &PathBuf) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(
                    "Failed to create keybindings config dir {}: {e}",
                    parent.display()
                );
                return;
            }
        }
    }
    match std::fs::write(path, USER_STUB) {
        Ok(_) => info!("Wrote keybindings stub to {}", path.display()),
        Err(e) => warn!(
            "Failed to write keybindings stub to {}: {e}",
            path.display()
        ),
    }
}

/// First-launch stub. Deliberately does NOT enumerate defaults — the full
/// list lives in `--print-default-keybindings` so this file never drifts
/// out of sync with the binary. Users add overrides below the examples.
const USER_STUB: &str = "\
# Bookokrat keybindings — user overrides go here.
#
# Run `bookokrat --print-default-keybindings` to see every default binding
# and available action. To override, write one line per binding using dotted
# TOML keys, or group bindings under a `[context]` header. Examples:
#
#   content.\"j\" = \"scroll_up\"         # reverse scroll direction
#   global.\"<C-q>\" = \"nop\"            # disable the Ctrl+Q suspend default
#
#   # equivalent grouped form:
#   [content]
#   \"j\" = \"scroll_up\"
#
# Groups `all`, `normal`, `popup` apply across multiple contexts.
# Use \"nop\" as the action to disable a default binding.
";

fn action_to_toml_value(action: &Action) -> String {
    // Serde-serialize to extract the snake_case name. JSON is simplest —
    // trim the enclosing quotes.
    serde_json::to_string(action)
        .ok()
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn apply_context_overrides(
    keymap: &mut Keymap,
    context: KeyContext,
    overrides: &HashMap<String, String>,
    content: &str,
    config_key: &str,
    errors: &mut Vec<LoadError>,
) -> usize {
    let mut applied = 0;
    let ctx = keymap.context_mut(context);

    for (notation, action_str) in overrides {
        let binding = match parse_key_binding(notation) {
            Ok(b) => b,
            Err(e) => {
                let quoted = format!("\"{notation}\"");
                errors.push(LoadError::with_line(
                    find_binding_line(content, config_key, &quoted).unwrap_or(1),
                    format!("{config_key}.\"{notation}\": invalid key notation — {e}"),
                ));
                continue;
            }
        };

        let action: Action = match serde_yaml::from_str(&format!("\"{}\"", action_str)) {
            Ok(a) => a,
            Err(e) => {
                let quoted = format!("\"{notation}\"");
                errors.push(LoadError::with_line(
                    find_binding_line(content, config_key, &quoted).unwrap_or(1),
                    format!("{config_key}.\"{notation}\": cannot parse action \"{action_str}\" — {e}"),
                ));
                continue;
            }
        };

        if action == Action::Unknown {
            let quoted = format!("\"{notation}\"");
            errors.push(LoadError::with_line(
                find_binding_line(content, config_key, &quoted).unwrap_or(1),
                format!(
                    "{config_key}.\"{notation}\": unknown action \"{action_str}\" — run `bookokrat --print-default-keybindings` for the list"
                ),
            ));
        }

        if action == Action::Nop {
            ctx.unbind(&binding);
        } else {
            ctx.bind(binding, action);
        }
        applied += 1;
    }

    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybindings::keymap::LookupResult;

    fn lookup(keymap: &Keymap, context: KeyContext, notation: &str) -> LookupResult {
        let binding = parse_key_binding(notation).unwrap();
        keymap.lookup(context, binding.keys())
    }

    fn make_keymap_with_toml(src: &str) -> Keymap {
        let mut keymap = default_keymap();
        let mut errors = Vec::new();
        apply_toml(src, &mut keymap, &mut errors);
        keymap
    }

    fn load_errors(src: &str) -> Vec<LoadError> {
        let mut keymap = default_keymap();
        let mut errors = Vec::new();
        apply_toml(src, &mut keymap, &mut errors);
        errors
    }

    // === Valid TOML parsing ===

    #[test]
    fn empty_toml_uses_defaults() {
        let keymap = make_keymap_with_toml("");
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "?"),
            LookupResult::Found(Action::ToggleHelp)
        );
    }

    #[test]
    fn override_single_context() {
        let src = r#"
[content]
"j" = "scroll_up"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "j"),
            LookupResult::Found(Action::ScrollUp)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "k"),
            LookupResult::Found(Action::ScrollUp)
        );
    }

    #[test]
    fn override_flat_dotted_form() {
        // Same override as `override_single_context`, written as a single
        // dotted-key line. TOML treats these as equivalent.
        let src = r#"content."j" = "scroll_up""#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "j"),
            LookupResult::Found(Action::ScrollUp)
        );
    }

    #[test]
    fn override_mixed_flat_and_grouped() {
        // Users can freely mix per-line dotted form and grouped tables in
        // the same file.
        let src = r#"
content."j" = "scroll_up"

[nav]
"x" = "quit"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "j"),
            LookupResult::Found(Action::ScrollUp)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "x"),
            LookupResult::Found(Action::Quit)
        );
    }

    #[test]
    fn override_multiple_contexts() {
        let src = r#"
[global]
"?" = "quit"
[nav]
"j" = "move_up"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "?"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "j"),
            LookupResult::Found(Action::MoveUp)
        );
    }

    #[test]
    fn popup_subcontext_nested_form() {
        // [popup.help] is TOML's native hierarchical form for the
        // "popup.help" context. Must route to PopupHelp, not the `popup` group.
        let src = r#"
[popup.help]
"q" = "quit"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::PopupHelp, "q"),
            LookupResult::Found(Action::Quit)
        );
        // Shouldn't leak to other popup contexts
        assert_eq!(
            lookup(&keymap, KeyContext::PopupHistory, "q"),
            LookupResult::NoMatch
        );
    }

    // === Group semantics ===

    #[test]
    fn all_group_applies_to_every_non_global_context() {
        let src = r#"
[all]
"<C-n>" = "move_down"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "<C-n>"),
            LookupResult::Found(Action::MoveDown)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "<C-n>"),
            LookupResult::Found(Action::MoveDown)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::PdfStandard, "<C-n>"),
            LookupResult::Found(Action::MoveDown)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::PopupHelp, "<C-n>"),
            LookupResult::Found(Action::MoveDown)
        );
        // NOT in global
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "<C-n>"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn normal_group_applies_to_both_normal_modes() {
        let src = r#"
[normal]
"x" = "quit"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubNormal, "x"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::PdfNormal, "x"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "x"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn popup_group_applies_to_all_popups() {
        let src = r#"
[popup]
"x" = "quit"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::PopupHelp, "x"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::PopupHistory, "x"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::PopupSettings, "x"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "x"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn specific_context_overrides_group() {
        let src = r#"
[all]
"x" = "quit"
[content]
"x" = "scroll_down"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "x"),
            LookupResult::Found(Action::ScrollDown)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "x"),
            LookupResult::Found(Action::Quit)
        );
    }

    // === Merge semantics ===

    #[test]
    fn nop_disables_binding() {
        let src = r#"
[content]
"p" = "nop"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "p"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn add_new_binding() {
        let src = r#"
[nav]
"<C-n>" = "move_down"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "<C-n>"),
            LookupResult::Found(Action::MoveDown)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "j"),
            LookupResult::Found(Action::MoveDown)
        );
    }

    #[test]
    fn override_does_not_affect_other_context() {
        let src = r#"
[content]
"j" = "scroll_up"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "j"),
            LookupResult::Found(Action::MoveDown)
        );
    }

    // === Error handling ===

    #[test]
    fn invalid_notation_skipped() {
        let src = r#"
[content]
"<C-" = "scroll_down"
"k" = "scroll_up"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "k"),
            LookupResult::Found(Action::ScrollUp)
        );
    }

    #[test]
    fn unknown_context_skipped() {
        let src = r#"
[garbage]
"j" = "move_down"
[content]
"k" = "scroll_up"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "k"),
            LookupResult::Found(Action::ScrollUp)
        );
    }

    #[test]
    fn unknown_action_still_bound() {
        let src = r#"
[content]
"j" = "future_action_v2"
"#;
        let keymap = make_keymap_with_toml(src);
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "j"),
            LookupResult::Found(Action::Unknown)
        );
    }

    #[test]
    fn invalid_toml_returns_error_with_line() {
        let src = "this is not = [valid toml";
        let errors = load_errors(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        assert_eq!(errors[0].line, Some(1));
    }

    #[test]
    fn unknown_action_reports_line() {
        let src = "\
[content]
\"j\" = \"scroll_up\"
\"k\" = \"totally_fake_action\"
";
        let errors = load_errors(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        assert_eq!(errors[0].line, Some(3));
        assert!(
            errors[0].message.contains("totally_fake_action"),
            "message = {:?}",
            errors[0].message
        );
    }

    #[test]
    fn invalid_notation_reports_line() {
        let src = "\
[content]
\"<C-\" = \"scroll_down\"
";
        let errors = load_errors(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        assert_eq!(errors[0].line, Some(2));
    }

    #[test]
    fn unknown_context_reports_line() {
        let src = "\
[garbage]
\"x\" = \"quit\"
";
        let errors = load_errors(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        assert_eq!(errors[0].line, Some(1));
        assert!(errors[0].message.contains("garbage"));
    }

    #[test]
    fn non_string_value_reports_line() {
        let src = "\
[content]
\"j\" = 42
";
        let errors = load_errors(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        assert_eq!(errors[0].line, Some(2));
    }

    #[test]
    fn flat_form_errors_report_line() {
        let src = "\
content.\"k\" = \"scroll_up\"
content.\"j\" = \"bad_action_name\"
";
        let errors = load_errors(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        assert_eq!(errors[0].line, Some(2));
    }

    #[test]
    fn load_keymap_returns_defaults_when_no_file() {
        let (keymap, errors) = load_keymap();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "?"),
            LookupResult::Found(Action::ToggleHelp)
        );
    }

    #[test]
    fn user_stub_parses_as_empty_overrides() {
        // The first-launch stub is comments-only TOML. It must parse cleanly
        // (yielding zero overrides) so untouched users still get every default.
        let keymap = make_keymap_with_toml(USER_STUB);
        let defaults = default_keymap();
        let probes = [
            (KeyContext::Global, "?"),
            (KeyContext::Global, "<C-q>"),
            (KeyContext::EpubContent, "j"),
            (KeyContext::PdfNormal, "gd"),
        ];
        for (ctx, key) in probes {
            assert_eq!(
                lookup(&keymap, ctx, key),
                lookup(&defaults, ctx, key),
                "stub shouldn't shadow default at {ctx:?}/{key}"
            );
        }
    }

    #[test]
    fn print_default_keybindings_roundtrips_to_defaults() {
        // The reference output is valid TOML. Piping it back through the
        // loader must reproduce the default keymap exactly — otherwise the
        // printer has drifted (wrong notation, wrong action name, etc.).
        let toml_src = print_default_keybindings();
        let reloaded =
            make_keymap_with_toml(&toml_src);
        assert_keymaps_equal(&reloaded, &default_keymap());
    }

    #[test]
    fn print_default_keybindings_grouped_roundtrips_to_defaults() {
        // Same invariant as the flat printer — grouped output must be valid
        // TOML and reload to exactly the defaults.
        let toml_src = print_default_keybindings_grouped();
        let reloaded = make_keymap_with_toml(&toml_src);
        assert_keymaps_equal(&reloaded, &default_keymap());
    }

    fn assert_keymaps_equal(a: &Keymap, b: &Keymap) {
        for ctx in KeyContext::ALL {
            let a_ctx = a.context(*ctx).expect("context must exist");
            let b_ctx = b.context(*ctx).expect("context must exist");
            let mut ab = a_ctx.all_bindings();
            let mut bb = b_ctx.all_bindings();
            ab.sort_by_key(|(k, action)| (format_key_binding(k), format!("{action:?}")));
            bb.sort_by_key(|(k, action)| (format_key_binding(k), format!("{action:?}")));
            assert_eq!(ab, bb, "binding set diverges in context {ctx:?}");
        }
    }
}
