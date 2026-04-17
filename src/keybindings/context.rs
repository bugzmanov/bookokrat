use serde::{Deserialize, Serialize};

/// Contexts determine which set of keybindings are active.
///
/// Each context has its own complete set of default bindings (built from layers).
/// The dispatch code checks `Global` first, then the specific context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyContext {
    Global,
    Navigation,
    EpubContent,
    EpubNormal,
    PdfStandard,
    PdfNormal,
    PopupHelp,
    PopupHistory,
    PopupSearch,
    PopupStats,
    PopupComments,
    PopupSettings,
}

/// Config file group keys that expand to multiple contexts.
/// Groups are resolved at load time — the internal keymap is always per-context.
pub enum ConfigGroup {
    /// Applies to all contexts except Global.
    All,
    /// Applies to EpubNormal + PdfNormal.
    Normal,
    /// Applies to all popup contexts.
    Popup,
    /// A specific context (not a group).
    Context(KeyContext),
}

impl KeyContext {
    pub const ALL: &[KeyContext] = &[
        KeyContext::Global,
        KeyContext::Navigation,
        KeyContext::EpubContent,
        KeyContext::EpubNormal,
        KeyContext::PdfStandard,
        KeyContext::PdfNormal,
        KeyContext::PopupHelp,
        KeyContext::PopupHistory,
        KeyContext::PopupSearch,
        KeyContext::PopupStats,
        KeyContext::PopupComments,
        KeyContext::PopupSettings,
    ];

    /// Short config key used in the YAML file.
    pub fn config_key(self) -> &'static str {
        match self {
            KeyContext::Global => "global",
            KeyContext::Navigation => "nav",
            KeyContext::EpubContent => "content",
            KeyContext::EpubNormal => "epub_normal",
            KeyContext::PdfStandard => "pdf",
            KeyContext::PdfNormal => "pdf_normal",
            KeyContext::PopupHelp => "popup.help",
            KeyContext::PopupHistory => "popup.history",
            KeyContext::PopupSearch => "popup.search",
            KeyContext::PopupStats => "popup.stats",
            KeyContext::PopupComments => "popup.comments",
            KeyContext::PopupSettings => "popup.settings",
        }
    }

    /// Look up a context from its config key name.
    pub fn from_config_key(key: &str) -> Option<KeyContext> {
        KeyContext::ALL
            .iter()
            .find(|c| c.config_key() == key)
            .copied()
    }
}

/// Contexts that the `all` group expands to (everything except Global).
pub const ALL_GROUP: &[KeyContext] = &[
    KeyContext::Navigation,
    KeyContext::EpubContent,
    KeyContext::EpubNormal,
    KeyContext::PdfStandard,
    KeyContext::PdfNormal,
    KeyContext::PopupHelp,
    KeyContext::PopupHistory,
    KeyContext::PopupSearch,
    KeyContext::PopupStats,
    KeyContext::PopupComments,
    KeyContext::PopupSettings,
];

/// Contexts that the `normal` group expands to.
pub const NORMAL_GROUP: &[KeyContext] = &[KeyContext::EpubNormal, KeyContext::PdfNormal];

/// Contexts that the `popup` group expands to.
pub const POPUP_GROUP: &[KeyContext] = &[
    KeyContext::PopupHelp,
    KeyContext::PopupHistory,
    KeyContext::PopupSearch,
    KeyContext::PopupStats,
    KeyContext::PopupComments,
    KeyContext::PopupSettings,
];

/// Resolve a config file key to a group or specific context.
pub fn resolve_config_key(key: &str) -> Option<ConfigGroup> {
    match key {
        "all" => Some(ConfigGroup::All),
        "normal" => Some(ConfigGroup::Normal),
        "popup" => Some(ConfigGroup::Popup),
        _ => KeyContext::from_config_key(key).map(ConfigGroup::Context),
    }
}

/// Get the list of concrete contexts for a config group.
pub fn group_contexts(group: &ConfigGroup) -> Vec<KeyContext> {
    match group {
        ConfigGroup::All => ALL_GROUP.to_vec(),
        ConfigGroup::Normal => NORMAL_GROUP.to_vec(),
        ConfigGroup::Popup => POPUP_GROUP.to_vec(),
        ConfigGroup::Context(ctx) => vec![*ctx],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_key_roundtrip() {
        for ctx in KeyContext::ALL {
            let key = ctx.config_key();
            let resolved = KeyContext::from_config_key(key);
            assert_eq!(resolved, Some(*ctx), "roundtrip failed for {ctx:?}");
        }
    }

    #[test]
    fn unknown_config_key() {
        assert_eq!(KeyContext::from_config_key("nonexistent"), None);
    }

    #[test]
    fn all_variants_covered() {
        assert_eq!(KeyContext::ALL.len(), 12);
    }

    #[test]
    fn group_all_excludes_global() {
        assert!(!ALL_GROUP.contains(&KeyContext::Global));
        assert_eq!(ALL_GROUP.len(), 11);
    }

    #[test]
    fn resolve_groups() {
        assert!(matches!(resolve_config_key("all"), Some(ConfigGroup::All)));
        assert!(matches!(
            resolve_config_key("normal"),
            Some(ConfigGroup::Normal)
        ));
        assert!(matches!(
            resolve_config_key("popup"),
            Some(ConfigGroup::Popup)
        ));
        assert!(matches!(
            resolve_config_key("nav"),
            Some(ConfigGroup::Context(KeyContext::Navigation))
        ));
        assert!(resolve_config_key("garbage").is_none());
    }

    #[test]
    fn group_expansion() {
        let normal = group_contexts(&ConfigGroup::Normal);
        assert_eq!(normal.len(), 2);
        assert!(normal.contains(&KeyContext::EpubNormal));
        assert!(normal.contains(&KeyContext::PdfNormal));
    }
}
