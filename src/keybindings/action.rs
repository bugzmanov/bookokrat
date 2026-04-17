use serde::{Deserialize, Serialize};

/// All bindable actions in the application.
///
/// Each variant maps to a snake_case string in the YAML config file.
/// Actions that don't make sense in a given context are no-ops.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Explicitly disable a binding
    Nop,

    // === General ===
    Quit,
    ForceRedraw,
    Suspend,
    SwitchFocus,

    // === Navigation (movement in lists/content) ===
    MoveDown,
    MoveUp,
    MoveLeft,
    MoveRight,
    ScrollHalfDown,
    ScrollHalfUp,
    ScrollPageDown,
    ScrollPageUp,
    GoTop,
    GoBottom,
    Select,

    // === Chapters/Pages ===
    NextChapter,
    PrevChapter,
    NextPage,
    PrevPage,

    // === Paragraphs ===
    ParagraphForward,
    ParagraphBackward,

    // === Search ===
    StartSearch,
    ToggleGlobalSearch,
    NextSearchMatch,
    PrevSearchMatch,

    // === Panels/Popups ===
    ToggleHelp,
    ToggleReadingHistory,
    ToggleBookStats,
    ToggleZenMode,
    ToggleCommentsViewer,
    OpenBookSearch,
    OpenBookSearchFresh,
    OpenSettings,
    OpenThemeSelector,
    OpenExternalViewer,
    ShrinkNavPanel,
    ExpandNavPanel,
    ResetNavPanelWidth,

    // === Content operations ===
    AddComment,
    DeleteComment,
    EditComment,
    CopySelection,
    CopyChapterText,
    CopyTocItem,
    LookupSelection,
    FollowLink,

    // === Vim normal mode ===
    ToggleNormalMode,
    EnterVisualMode,
    EnterVisualLineMode,
    StartYank,
    FindForward,
    FindBackward,
    TillForward,
    TillBackward,
    RepeatFind,
    RepeatFindReverse,
    WordForward,
    WordBackward,
    WordEnd,
    LineStart,
    LineEnd,
    FirstNonBlank,

    // === Jump list ===
    JumpForward,
    JumpBackward,

    // === Display ===
    ToggleProfiling,
    ToggleRawHtml,
    ToggleJustifyText,
    IncreaseMargin,
    DecreaseMargin,

    // === PDF-specific ===
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFitWidth,
    PanLeft,
    PanRight,
    GoToPage,
    ToggleInvertImages,
    TogglePdfTheming,
    TogglePdfWatching,
    TogglePdfPageLayout,
    TogglePdfRenderMode,
    DumpDebugState,
    SynctexInverse,
    ScrollDown,
    ScrollUp,

    // === Navigation panel-specific ===
    ToggleSortOrder,
    CollapseAll,
    ExpandAll,
    Collapse,
    Expand,
    SwitchNavMode,

    // === History/Comments viewer ===
    DeleteEntry,
    CopyEntry,
    ExportComments,

    // === Comment navigation (PDF) ===
    EnterCommentNav,

    // === Context-dependent cancel/escape ===
    Cancel,

    // === Popup tab switching ===
    NextTab,
    PrevTab,

    /// Unknown action from a newer config version.
    /// Treated as no-op, logged as warning at load time.
    #[serde(other)]
    Unknown,
}

impl Action {
    pub fn is_nop(&self) -> bool {
        matches!(self, Action::Nop | Action::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let action = Action::ScrollHalfDown;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"scroll_half_down\"");
        let parsed: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, action);
    }

    #[test]
    fn serde_nop() {
        let parsed: Action = serde_json::from_str("\"nop\"").unwrap();
        assert_eq!(parsed, Action::Nop);
        assert!(parsed.is_nop());
    }

    #[test]
    fn serde_unknown_action() {
        let parsed: Action = serde_json::from_str("\"some_future_action\"").unwrap();
        assert_eq!(parsed, Action::Unknown);
        assert!(parsed.is_nop());
    }

    #[test]
    fn serde_yaml_roundtrip() {
        let action = Action::ToggleHelp;
        let yaml = serde_yaml::to_string(&action).unwrap();
        let parsed: Action = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, action);
    }

    #[test]
    fn all_major_actions_deserialize() {
        let actions = [
            "quit",
            "move_down",
            "scroll_half_down",
            "toggle_help",
            "add_comment",
            "toggle_normal_mode",
            "zoom_in",
            "start_search",
            "cancel",
            "switch_focus",
        ];
        for name in actions {
            let json = format!("\"{name}\"");
            let parsed: Action = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("failed to parse action '{name}': {e}"));
            assert_ne!(parsed, Action::Unknown, "action '{name}' parsed as Unknown");
        }
    }
}
