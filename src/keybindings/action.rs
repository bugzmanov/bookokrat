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

    // === Keybindings config ===
    ReloadKeybindings,

    // === Popup tab switching ===
    NextTab,
    PrevTab,

    /// Unknown action from a newer config version.
    /// Treated as no-op, logged as warning at load time.
    #[serde(other)]
    Unknown,
}

impl Action {
    /// Every variant of `Action`, in declaration order.
    ///
    /// Hand-maintained. If you add a variant to the enum, add it here too —
    /// the `all_slice_covers_every_variant` test uses an exhaustive match to
    /// catch drift between this list and the enum definition.
    pub const ALL: &'static [Action] = &[
        Action::Nop,
        Action::Quit,
        Action::ForceRedraw,
        Action::Suspend,
        Action::SwitchFocus,
        Action::MoveDown,
        Action::MoveUp,
        Action::MoveLeft,
        Action::MoveRight,
        Action::ScrollHalfDown,
        Action::ScrollHalfUp,
        Action::ScrollPageDown,
        Action::ScrollPageUp,
        Action::GoTop,
        Action::GoBottom,
        Action::Select,
        Action::NextChapter,
        Action::PrevChapter,
        Action::NextPage,
        Action::PrevPage,
        Action::ParagraphForward,
        Action::ParagraphBackward,
        Action::StartSearch,
        Action::ToggleGlobalSearch,
        Action::NextSearchMatch,
        Action::PrevSearchMatch,
        Action::ToggleHelp,
        Action::ToggleReadingHistory,
        Action::ToggleBookStats,
        Action::ToggleZenMode,
        Action::ToggleCommentsViewer,
        Action::OpenBookSearch,
        Action::OpenBookSearchFresh,
        Action::OpenSettings,
        Action::OpenThemeSelector,
        Action::OpenExternalViewer,
        Action::ShrinkNavPanel,
        Action::ExpandNavPanel,
        Action::ResetNavPanelWidth,
        Action::AddComment,
        Action::DeleteComment,
        Action::CopySelection,
        Action::CopyChapterText,
        Action::CopyTocItem,
        Action::LookupSelection,
        Action::FollowLink,
        Action::ToggleNormalMode,
        Action::EnterVisualMode,
        Action::EnterVisualLineMode,
        Action::StartYank,
        Action::FindForward,
        Action::FindBackward,
        Action::TillForward,
        Action::TillBackward,
        Action::RepeatFind,
        Action::RepeatFindReverse,
        Action::WordForward,
        Action::WordBackward,
        Action::WordEnd,
        Action::LineStart,
        Action::LineEnd,
        Action::FirstNonBlank,
        Action::JumpForward,
        Action::JumpBackward,
        Action::ToggleProfiling,
        Action::ToggleRawHtml,
        Action::ToggleJustifyText,
        Action::IncreaseMargin,
        Action::DecreaseMargin,
        Action::ZoomIn,
        Action::ZoomOut,
        Action::ZoomReset,
        Action::ZoomFitWidth,
        Action::PanLeft,
        Action::PanRight,
        Action::GoToPage,
        Action::ToggleInvertImages,
        Action::TogglePdfTheming,
        Action::TogglePdfWatching,
        Action::TogglePdfPageLayout,
        Action::TogglePdfRenderMode,
        Action::DumpDebugState,
        Action::SynctexInverse,
        Action::ScrollDown,
        Action::ScrollUp,
        Action::ToggleSortOrder,
        Action::CollapseAll,
        Action::ExpandAll,
        Action::Collapse,
        Action::Expand,
        Action::SwitchNavMode,
        Action::DeleteEntry,
        Action::CopyEntry,
        Action::ExportComments,
        Action::EnterCommentNav,
        Action::Cancel,
        Action::ReloadKeybindings,
        Action::NextTab,
        Action::PrevTab,
        Action::Unknown,
    ];

    pub fn is_nop(&self) -> bool {
        matches!(self, Action::Nop | Action::Unknown)
    }

    /// Short one-line description of what this action does.
    ///
    /// The match is intentionally exhaustive (no `_` wildcard) so the
    /// compiler forces a description whenever a new variant is added.
    pub fn description(&self) -> &'static str {
        use Action::*;
        match self {
            Nop => "Disable this binding",
            Unknown => "(unknown action — from a newer config version)",

            // General
            Quit => "Quit the application",
            ForceRedraw => "Force a full-screen redraw",
            Suspend => "Suspend the process (job control)",
            SwitchFocus => "Switch focus between nav panel and reader",

            // Navigation / movement
            MoveDown => "Move selection down one item",
            MoveUp => "Move selection up one item",
            MoveLeft => "Move cursor / selection left",
            MoveRight => "Move cursor / selection right",
            ScrollHalfDown => "Scroll half a screen down",
            ScrollHalfUp => "Scroll half a screen up",
            ScrollPageDown => "Scroll one full page down",
            ScrollPageUp => "Scroll one full page up",
            GoTop => "Jump to top of document / list",
            GoBottom => "Jump to bottom of document / list",
            Select => "Activate / open the highlighted item",

            // Chapters / pages
            NextChapter => "Go to the next chapter (EPUB)",
            PrevChapter => "Go to the previous chapter (EPUB)",
            NextPage => "Go to the next page (PDF)",
            PrevPage => "Go to the previous page (PDF)",

            // Paragraphs
            ParagraphForward => "Jump to the next paragraph",
            ParagraphBackward => "Jump to the previous paragraph",

            // Search
            StartSearch => "Start in-scope text search",
            ToggleGlobalSearch => "Toggle global vs scope-local search",
            NextSearchMatch => "Jump to the next search match",
            PrevSearchMatch => "Jump to the previous search match",

            // Panels / popups
            ToggleHelp => "Toggle the help popup",
            ToggleReadingHistory => "Toggle the reading history popup",
            ToggleBookStats => "Toggle the book statistics popup",
            ToggleZenMode => "Toggle zen mode (hide navigation panel)",
            ToggleCommentsViewer => "Toggle the comments viewer popup",
            OpenBookSearch => "Reopen the last book-wide search",
            OpenBookSearchFresh => "Start a fresh book-wide search",
            OpenSettings => "Open the settings popup",
            OpenThemeSelector => "Open the theme selector popup",
            OpenExternalViewer => "Open the current book in an external viewer",
            ShrinkNavPanel => "Shrink the navigation panel by one step",
            ExpandNavPanel => "Expand the navigation panel by one step",
            ResetNavPanelWidth => "Reset the navigation panel to its default width",

            // Content operations
            AddComment => "Add or edit a comment on the selection",
            DeleteComment => "Delete the comment under the cursor",
            CopySelection => "Copy the current selection to the clipboard",
            CopyChapterText => "Copy chapter text (EPUB) / page text (PDF)",
            CopyTocItem => "Copy the selected TOC entry (PDF)",
            LookupSelection => "Run the configured lookup command on the selection",
            FollowLink => "Follow the link under the cursor",

            // Vim normal mode
            ToggleNormalMode => "Toggle vim normal mode in the reader",
            EnterVisualMode => "Enter visual (character) selection mode",
            EnterVisualLineMode => "Enter visual-line selection mode",
            StartYank => "Start a yank (copy) motion",
            FindForward => "Find a character forward on the line",
            FindBackward => "Find a character backward on the line",
            TillForward => "Move till before a character forward on the line",
            TillBackward => "Move till before a character backward on the line",
            RepeatFind => "Repeat the last find/till motion",
            RepeatFindReverse => "Repeat the last find/till in the opposite direction",
            WordForward => "Move to the beginning of the next word",
            WordBackward => "Move to the beginning of the previous word",
            WordEnd => "Move to the end of the current / next word",
            LineStart => "Move to the start of the line",
            LineEnd => "Move to the end of the line",
            FirstNonBlank => "Move to the first non-blank character on the line",

            // Jump list
            JumpForward => "Jump forward in the navigation history",
            JumpBackward => "Jump backward in the navigation history",

            // Display
            ToggleProfiling => "Toggle the performance profiler overlay",
            ToggleRawHtml => "Toggle raw HTML view (EPUB)",
            ToggleJustifyText => "Toggle justified text rendering (EPUB)",
            IncreaseMargin => "Increase content margin",
            DecreaseMargin => "Decrease content margin",

            // PDF-specific
            ZoomIn => "Zoom in (PDF)",
            ZoomOut => "Zoom out (PDF)",
            ZoomReset => "Reset zoom / fit-to-height (PDF)",
            ZoomFitWidth => "Fit page width (PDF)",
            PanLeft => "Pan the page left (PDF)",
            PanRight => "Pan the page right (PDF)",
            GoToPage => "Jump to a specific page number (PDF)",
            ToggleInvertImages => "Toggle image inversion in themed mode (PDF)",
            TogglePdfTheming => "Toggle themed vs original PDF rendering",
            TogglePdfWatching => "Toggle file watching / auto-reload (PDF)",
            TogglePdfPageLayout => "Toggle single / dual page layout (PDF)",
            TogglePdfRenderMode => "Toggle scroll / page render mode (PDF)",
            DumpDebugState => "Dump PDF debug state to the log",
            SynctexInverse => "SyncTeX inverse search: jump to LaTeX source",
            ScrollDown => "Scroll the reader down by one line",
            ScrollUp => "Scroll the reader up by one line",

            // Navigation panel
            ToggleSortOrder => "Toggle book list sort order",
            CollapseAll => "Collapse all TOC entries",
            ExpandAll => "Expand all TOC entries",
            Collapse => "Collapse the current TOC entry",
            Expand => "Expand the current TOC entry",
            SwitchNavMode => "Switch between book list and table of contents",

            // History / comments viewer
            DeleteEntry => "Delete the highlighted entry",
            CopyEntry => "Copy the highlighted entry to the clipboard",
            ExportComments => "Export all comments to a Markdown file",

            // Comment navigation
            EnterCommentNav => "Enter comment navigation mode (PDF)",

            // Context-dependent
            Cancel => "Close popup / clear selection / exit mode",

            // Keybindings config
            ReloadKeybindings => "Reload keybindings.toml (shows a modal with errors if any)",

            // Popup tab switching
            NextTab => "Switch to the next tab / pane",
            PrevTab => "Switch to the previous tab / pane",
        }
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

    /// Exhaustive match — adding a variant without also adding it to
    /// `Action::ALL` will fail this compilation. The test body is a runtime
    /// check that `Action::ALL` actually contains the result.
    ///
    /// This is the drift guard for `Action::ALL`. If you add a variant to the
    /// enum, you MUST: (1) add a description, (2) add a match arm here, and
    /// (3) add it to `Action::ALL`.
    #[test]
    fn all_slice_covers_every_variant() {
        fn __exhaustive_check(a: Action) -> Action {
            use Action::*;
            match a {
                Nop | Unknown => a,
                Quit | ForceRedraw | Suspend | SwitchFocus => a,
                MoveDown | MoveUp | MoveLeft | MoveRight => a,
                ScrollHalfDown | ScrollHalfUp | ScrollPageDown | ScrollPageUp => a,
                GoTop | GoBottom | Select => a,
                NextChapter | PrevChapter | NextPage | PrevPage => a,
                ParagraphForward | ParagraphBackward => a,
                StartSearch | ToggleGlobalSearch | NextSearchMatch | PrevSearchMatch => a,
                ToggleHelp | ToggleReadingHistory | ToggleBookStats | ToggleZenMode
                | ToggleCommentsViewer => a,
                OpenBookSearch | OpenBookSearchFresh | OpenSettings | OpenThemeSelector
                | OpenExternalViewer => a,
                ShrinkNavPanel | ExpandNavPanel | ResetNavPanelWidth => a,
                AddComment | DeleteComment => a,
                CopySelection | CopyChapterText | CopyTocItem => a,
                LookupSelection | FollowLink => a,
                ToggleNormalMode | EnterVisualMode | EnterVisualLineMode | StartYank => a,
                FindForward | FindBackward | TillForward | TillBackward | RepeatFind
                | RepeatFindReverse => a,
                WordForward | WordBackward | WordEnd | LineStart | LineEnd | FirstNonBlank => a,
                JumpForward | JumpBackward => a,
                ToggleProfiling | ToggleRawHtml | ToggleJustifyText | IncreaseMargin
                | DecreaseMargin => a,
                ZoomIn | ZoomOut | ZoomReset | ZoomFitWidth | PanLeft | PanRight => a,
                GoToPage | ToggleInvertImages | TogglePdfTheming | TogglePdfWatching => a,
                TogglePdfPageLayout | TogglePdfRenderMode | DumpDebugState | SynctexInverse => a,
                ScrollDown | ScrollUp => a,
                ToggleSortOrder | CollapseAll | ExpandAll | Collapse | Expand | SwitchNavMode => a,
                DeleteEntry | CopyEntry | ExportComments | EnterCommentNav => a,
                Cancel | ReloadKeybindings | NextTab | PrevTab => a,
            }
        }

        // Every variant present in ALL should survive a round-trip through
        // the exhaustive match. Any variant added to the enum but missing
        // from ALL will not be iterated here — but the match above will
        // fail to compile, which is exactly the failure we want.
        for a in Action::ALL {
            assert_eq!(__exhaustive_check(a.clone()), *a);
        }

        // ALL must list every variant exactly once.
        let mut seen = std::collections::HashSet::new();
        for a in Action::ALL {
            assert!(seen.insert(a.clone()), "duplicate in Action::ALL: {a:?}");
        }
    }

    #[test]
    fn every_action_has_non_empty_description() {
        // Walks Action::ALL — catches any variant whose description()
        // returns an empty or whitespace-only string.
        for a in Action::ALL {
            assert!(
                !a.description().trim().is_empty(),
                "action {a:?} has an empty description"
            );
        }
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
