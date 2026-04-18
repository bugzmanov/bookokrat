use std::collections::HashMap;
use std::path::PathBuf;

use log::{error, info, warn};

use super::action::Action;
use super::context::KeyContext;
use super::defaults::default_keymap;
use super::keymap::Keymap;
use super::notation::{format_key_binding, parse_key_binding};

const KEYBINDINGS_FILENAME: &str = "keybindings.yaml";

/// Load the keymap: defaults + user overrides from config file.
pub fn load_keymap() -> Keymap {
    let mut keymap = default_keymap();

    if let Some(config_path) = keybindings_config_path() {
        if config_path.exists() {
            info!("Loading keybindings from {:?}", config_path);
            match load_and_apply(&config_path, &mut keymap) {
                Ok(count) => {
                    info!("Applied {count} keybinding overrides from config");
                }
                Err(e) => {
                    error!("Failed to load keybindings config: {e}");
                    info!("Using default keybindings");
                    keymap = default_keymap();
                }
            }
        }
    }

    keymap
}

fn keybindings_config_path() -> Option<PathBuf> {
    crate::settings::preferred_config_dir().map(|dir| dir.join(KEYBINDINGS_FILENAME))
}

fn load_and_apply(path: &PathBuf, keymap: &mut Keymap) -> Result<usize, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let raw: HashMap<String, HashMap<String, String>> = serde_yaml::from_str(&content)
        .map_err(|e| format!("invalid YAML in {}: {e}", path.display()))?;

    // Apply groups first (all, normal, popup), then specific contexts.
    // This ensures specific context overrides beat group overrides.
    let mut group_keys = Vec::new();
    let mut context_keys = Vec::new();

    for (config_key, bindings) in &raw {
        match config_key.as_str() {
            "all" | "normal" | "popup" => group_keys.push((config_key.as_str(), bindings)),
            _ => context_keys.push((config_key.as_str(), bindings)),
        }
    }

    // Stable order: all < normal < popup
    group_keys.sort_by_key(|(k, _)| match *k {
        "all" => 0,
        "normal" => 1,
        "popup" => 2,
        _ => 3,
    });

    let mut total_applied = 0;

    for (config_key, bindings) in group_keys {
        let Some(group) = super::context::resolve_config_key(config_key) else {
            continue;
        };
        for ctx_id in super::context::group_contexts(&group) {
            total_applied += apply_context_overrides(keymap, ctx_id, bindings);
        }
    }

    for (config_key, bindings) in context_keys {
        let Some(group) = super::context::resolve_config_key(config_key) else {
            warn!(
                "Unknown context '{}' in keybindings config, skipping",
                config_key
            );
            continue;
        };
        for ctx_id in super::context::group_contexts(&group) {
            total_applied += apply_context_overrides(keymap, ctx_id, bindings);
        }
    }

    Ok(total_applied)
}

/// Format the default keymap as a YAML string suitable for use as a
/// `~/.config/bookokrat/keybindings.yaml` starting template.
pub fn print_default_keybindings() -> String {
    let keymap = default_keymap();
    let mut out = String::new();

    out.push_str("# Default bookokrat keybindings.\n");
    out.push_str("# Copy to ~/.config/bookokrat/keybindings.yaml and edit.\n");
    out.push_str("#\n");
    out.push_str("# Values: any action name listed below, or \"nop\" to disable the binding.\n");
    out.push_str("# Groups (apply before per-context overrides):\n");
    out.push_str("#   all    -> every context except `global`\n");
    out.push_str("#   normal -> epub_normal + pdf_normal\n");
    out.push_str("#   popup  -> every popup.* context\n");
    out.push('\n');

    for ctx in KeyContext::ALL {
        let Some(ctx_map) = keymap.context(*ctx) else {
            continue;
        };
        let mut bindings = ctx_map.all_bindings();
        if bindings.is_empty() {
            continue;
        }
        bindings.sort_by(|(k1, _), (k2, _)| format_key_binding(k1).cmp(&format_key_binding(k2)));

        // Pre-format each line so we can align trailing `# description` comments.
        let rows: Vec<(String, &'static str)> = bindings
            .iter()
            .map(|(key, action)| {
                let prefix = format!(
                    "  \"{}\": {}",
                    format_key_binding(key),
                    action_to_yaml_value(action),
                );
                (prefix, action.description())
            })
            .collect();
        let comment_col = rows.iter().map(|(p, _)| p.len()).max().unwrap_or(0) + 2;

        out.push_str(&format!("{}:\n", ctx.config_key()));
        for (prefix, desc) in rows {
            let pad = comment_col.saturating_sub(prefix.len());
            out.push_str(&format!("{prefix}{:pad$}# {desc}\n", ""));
        }
        out.push('\n');
    }

    out
}

fn action_to_yaml_value(action: &Action) -> String {
    serde_yaml::to_string(action)
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn apply_context_overrides(
    keymap: &mut Keymap,
    context: KeyContext,
    overrides: &HashMap<String, String>,
) -> usize {
    let mut applied = 0;
    let ctx = keymap.context_mut(context);

    for (notation, action_str) in overrides {
        let binding = match parse_key_binding(notation) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "Invalid key notation '{}' in context '{}': {e}",
                    notation,
                    context.config_key()
                );
                continue;
            }
        };

        let action: Action = match serde_yaml::from_str(&format!("\"{}\"", action_str)) {
            Ok(a) => a,
            Err(e) => {
                warn!(
                    "Invalid action '{}' in context '{}': {e}",
                    action_str,
                    context.config_key()
                );
                continue;
            }
        };

        if action == Action::Unknown {
            warn!(
                "Unknown action '{}' for key '{}' in context '{}'",
                action_str,
                notation,
                context.config_key()
            );
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

    fn make_keymap_with_yaml(yaml: &str) -> Result<Keymap, String> {
        let mut keymap = default_keymap();
        let raw: HashMap<String, HashMap<String, String>> =
            serde_yaml::from_str(yaml).map_err(|e| format!("invalid yaml: {e}"))?;

        // Replicate the group-aware loading from load_and_apply
        let mut group_keys = Vec::new();
        let mut context_keys = Vec::new();
        for (k, v) in &raw {
            match k.as_str() {
                "all" | "normal" | "popup" => group_keys.push((k.as_str(), v)),
                _ => context_keys.push((k.as_str(), v)),
            }
        }
        group_keys.sort_by_key(|(k, _)| match *k {
            "all" => 0,
            "normal" => 1,
            "popup" => 2,
            _ => 3,
        });
        for (k, bindings) in group_keys {
            if let Some(group) = super::super::context::resolve_config_key(k) {
                for ctx_id in super::super::context::group_contexts(&group) {
                    apply_context_overrides(&mut keymap, ctx_id, bindings);
                }
            }
        }
        for (k, bindings) in context_keys {
            if let Some(group) = super::super::context::resolve_config_key(k) {
                for ctx_id in super::super::context::group_contexts(&group) {
                    apply_context_overrides(&mut keymap, ctx_id, bindings);
                }
            }
        }
        Ok(keymap)
    }

    // === Valid YAML parsing ===

    #[test]
    fn empty_yaml_uses_defaults() {
        let keymap = make_keymap_with_yaml("{}").unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "?"),
            LookupResult::Found(Action::ToggleHelp)
        );
    }

    #[test]
    fn override_single_context() {
        let yaml = r#"
content:
  "j": "scroll_up"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
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
    fn override_multiple_contexts() {
        let yaml = r#"
global:
  "?": "quit"
nav:
  "j": "move_up"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "?"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "j"),
            LookupResult::Found(Action::MoveUp)
        );
    }

    // === Group semantics ===

    #[test]
    fn all_group_applies_to_every_non_global_context() {
        let yaml = r#"
all:
  "<C-n>": "move_down"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        // Should be in nav, content, pdf, normals, popups
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
        let yaml = r#"
normal:
  "x": "quit"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubNormal, "x"),
            LookupResult::Found(Action::Quit)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::PdfNormal, "x"),
            LookupResult::Found(Action::Quit)
        );
        // NOT in content or nav
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "x"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn popup_group_applies_to_all_popups() {
        let yaml = r#"
popup:
  "x": "quit"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
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
        // NOT in content
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "x"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn specific_context_overrides_group() {
        let yaml = r#"
all:
  "x": "quit"
content:
  "x": "scroll_down"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        // content gets the specific override
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "x"),
            LookupResult::Found(Action::ScrollDown)
        );
        // nav gets the group binding
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "x"),
            LookupResult::Found(Action::Quit)
        );
    }

    // === Merge semantics ===

    #[test]
    fn nop_disables_binding() {
        let yaml = r#"
content:
  "p": "nop"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "p"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn add_new_binding() {
        let yaml = r#"
nav:
  "<C-n>": "move_down"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
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
        let yaml = r#"
content:
  "j": "scroll_up"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "j"),
            LookupResult::Found(Action::MoveDown)
        );
    }

    // === Error handling ===

    #[test]
    fn invalid_notation_skipped() {
        let yaml = r#"
content:
  "<C-": "scroll_down"
  "k": "scroll_up"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "k"),
            LookupResult::Found(Action::ScrollUp)
        );
    }

    #[test]
    fn unknown_context_skipped() {
        let yaml = r#"
garbage:
  "j": "move_down"
content:
  "k": "scroll_up"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "k"),
            LookupResult::Found(Action::ScrollUp)
        );
    }

    #[test]
    fn unknown_action_still_bound() {
        let yaml = r#"
content:
  "j": "future_action_v2"
"#;
        let keymap = make_keymap_with_yaml(yaml).unwrap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "j"),
            LookupResult::Found(Action::Unknown)
        );
    }

    #[test]
    fn invalid_yaml_returns_error() {
        let yaml = "this is not: [valid: yaml";
        assert!(make_keymap_with_yaml(yaml).is_err());
    }

    #[test]
    fn load_keymap_returns_defaults_when_no_file() {
        let keymap = load_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "?"),
            LookupResult::Found(Action::ToggleHelp)
        );
    }

    #[test]
    fn print_default_keybindings_roundtrips_through_loader() {
        // The printed YAML must be loadable by the same loader users run at startup,
        // and the reloaded keymap must behave identically to the defaults.
        let yaml = print_default_keybindings();
        let reloaded = make_keymap_with_yaml(&yaml)
            .expect("printed default keybindings must parse as valid config YAML");

        // Spot-check a binding from every context to catch notation regressions.
        let defaults = default_keymap();
        let probes = [
            (KeyContext::Global, "?"),
            (KeyContext::Navigation, "j"),
            (KeyContext::EpubContent, "j"),
            (KeyContext::EpubNormal, "w"),
            (KeyContext::PdfStandard, "l"),
            (KeyContext::PdfNormal, "gd"),
            (KeyContext::PopupHelp, "?"),
            (KeyContext::PopupHistory, "dd"),
            (KeyContext::PopupComments, "dd"),
            (KeyContext::PopupSettings, "<Tab>"),
        ];
        for (ctx, key) in probes {
            assert_eq!(
                lookup(&reloaded, ctx, key),
                lookup(&defaults, ctx, key),
                "mismatch at {ctx:?} / {key}"
            );
        }
    }
}
