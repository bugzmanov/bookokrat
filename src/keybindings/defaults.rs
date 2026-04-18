use super::action::Action;
use super::context::KeyContext;
use super::keymap::{ContextKeymap, Keymap};
use super::notation::parse_key_binding;

macro_rules! bind {
    ($ctx:expr, $notation:expr => $action:expr) => {
        $ctx.bind(
            parse_key_binding($notation)
                .unwrap_or_else(|e| panic!("bad default binding '{}': {}", $notation, e)),
            $action,
        );
    };
}

/// Build the complete default keymap using layered inheritance.
///
/// Layers (applied in order, later overrides earlier):
/// - `all`: shared vim navigation (j/k, gg/G, C-d/u/f/b, arrows, Esc)
/// - `normal`: vim cursor motions (w/b/e, 0/^/$, f/F/t/T, ;, v/V, y)
/// - `popup`: popup-specific (reserved)
/// - per-context specifics
pub fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new();

    for ctx_id in super::context::ALL_GROUP {
        let ctx = keymap.context_mut(*ctx_id);
        add_all_layer(ctx);
    }

    for ctx_id in super::context::NORMAL_GROUP {
        let ctx = keymap.context_mut(*ctx_id);
        add_normal_layer(ctx);
    }

    for ctx_id in super::context::POPUP_GROUP {
        let ctx = keymap.context_mut(*ctx_id);
        add_popup_layer(ctx);
    }

    global_specifics(&mut keymap);
    nav_specifics(&mut keymap);
    content_specifics(&mut keymap);
    epub_normal_specifics(&mut keymap);
    pdf_specifics(&mut keymap);
    pdf_normal_specifics(&mut keymap);
    popup_help_specifics(&mut keymap);
    popup_history_specifics(&mut keymap);
    popup_search_specifics(&mut keymap);
    popup_stats_specifics(&mut keymap);
    popup_comments_specifics(&mut keymap);
    popup_settings_specifics(&mut keymap);

    keymap
}

// ═══════════════════════════════════════════════════════════════
// Layers
// ═══════════════════════════════════════════════════════════════

fn add_all_layer(ctx: &mut ContextKeymap) {
    bind!(ctx, "j" => Action::MoveDown);
    bind!(ctx, "<Down>" => Action::MoveDown);
    bind!(ctx, "k" => Action::MoveUp);
    bind!(ctx, "<Up>" => Action::MoveUp);
    bind!(ctx, "gg" => Action::GoTop);
    bind!(ctx, "G" => Action::GoBottom);
    bind!(ctx, "<C-d>" => Action::ScrollHalfDown);
    bind!(ctx, "<C-u>" => Action::ScrollHalfUp);
    bind!(ctx, "<C-f>" => Action::ScrollPageDown);
    bind!(ctx, "<C-b>" => Action::ScrollPageUp);
    bind!(ctx, "<PageDown>" => Action::ScrollPageDown);
    bind!(ctx, "<PageUp>" => Action::ScrollPageUp);
    bind!(ctx, "<Esc>" => Action::Cancel);
}

fn add_normal_layer(ctx: &mut ContextKeymap) {
    bind!(ctx, "h" => Action::MoveLeft);
    bind!(ctx, "<Left>" => Action::MoveLeft);
    bind!(ctx, "l" => Action::MoveRight);
    bind!(ctx, "<Right>" => Action::MoveRight);
    bind!(ctx, "w" => Action::WordForward);
    bind!(ctx, "W" => Action::WordForward); // #5: uppercase alias
    bind!(ctx, "b" => Action::WordBackward);
    bind!(ctx, "B" => Action::WordBackward); // #5: uppercase alias
    bind!(ctx, "e" => Action::WordEnd);
    bind!(ctx, "E" => Action::WordEnd); // #5: uppercase alias
    bind!(ctx, "0" => Action::LineStart);
    bind!(ctx, "^" => Action::FirstNonBlank);
    bind!(ctx, "$" => Action::LineEnd);
    bind!(ctx, "{" => Action::ParagraphBackward);
    bind!(ctx, "}" => Action::ParagraphForward);
    bind!(ctx, "f" => Action::FindForward);
    bind!(ctx, "F" => Action::FindBackward);
    bind!(ctx, "t" => Action::TillForward);
    bind!(ctx, "T" => Action::TillBackward);
    bind!(ctx, ";" => Action::RepeatFind);
    bind!(ctx, "," => Action::RepeatFindReverse);
    bind!(ctx, "v" => Action::EnterVisualMode);
    bind!(ctx, "V" => Action::EnterVisualLineMode);
    bind!(ctx, "y" => Action::StartYank);
    bind!(ctx, "n" => Action::ToggleNormalMode);
}

fn add_popup_layer(_ctx: &mut ContextKeymap) {}

// ═══════════════════════════════════════════════════════════════
// Per-context specifics
// ═══════════════════════════════════════════════════════════════

fn global_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::Global);
    bind!(ctx, "?" => Action::ToggleHelp);
    bind!(ctx, "<Space>h" => Action::ToggleReadingHistory);
    bind!(ctx, "<Space>d" => Action::ToggleBookStats);
    bind!(ctx, "<Space>f" => Action::OpenBookSearch);
    bind!(ctx, "<Space>F" => Action::OpenBookSearchFresh);
    bind!(ctx, "<Space>o" => Action::OpenExternalViewer);
    bind!(ctx, "<Space>c" => Action::CopyChapterText);
    bind!(ctx, "<Space>C" => Action::CopyTocItem);
    bind!(ctx, "<Space>j" => Action::ToggleJustifyText);
    bind!(ctx, "<Space>a" => Action::ToggleCommentsViewer);
    bind!(ctx, "<Space>s" => Action::OpenSettings);
    bind!(ctx, "<Space>z" => Action::ToggleZenMode);
    bind!(ctx, "<Space>t" => Action::OpenThemeSelector);
    bind!(ctx, "<Space>w" => Action::TogglePdfWatching);
    bind!(ctx, "<Space>D" => Action::TogglePdfPageLayout);
    bind!(ctx, "<Space>S" => Action::TogglePdfRenderMode);
    bind!(ctx, "<Space>l" => Action::LookupSelection);
    bind!(ctx, "<Space><lt>" => Action::ResetNavPanelWidth);
    bind!(ctx, "<Space><gt>" => Action::ResetNavPanelWidth);
    bind!(ctx, "<C-l>" => Action::ForceRedraw);
    bind!(ctx, "<C-q>" => Action::Suspend);
    bind!(ctx, "<C-s>" => Action::OpenSettings);
    bind!(ctx, "<C-z>" => Action::ToggleZenMode);
    bind!(ctx, "<lt>" => Action::ShrinkNavPanel);
    bind!(ctx, "<gt>" => Action::ExpandNavPanel);
}

fn nav_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::Navigation);
    bind!(ctx, "h" => Action::Collapse);
    bind!(ctx, "<Left>" => Action::Collapse);
    bind!(ctx, "l" => Action::Expand);
    bind!(ctx, "<Right>" => Action::Expand);
    bind!(ctx, "H" => Action::CollapseAll);
    bind!(ctx, "L" => Action::ExpandAll);
    bind!(ctx, "<S-Left>" => Action::CollapseAll); // #9: Shift-arrow aliases
    bind!(ctx, "<S-Right>" => Action::ExpandAll); // #9: Shift-arrow aliases
    bind!(ctx, "<CR>" => Action::Select);
    bind!(ctx, "<Tab>" => Action::SwitchFocus);
    bind!(ctx, "/" => Action::StartSearch);
    bind!(ctx, "n" => Action::NextSearchMatch);
    bind!(ctx, "N" => Action::PrevSearchMatch);
    bind!(ctx, "S" => Action::ToggleSortOrder);
    bind!(ctx, "b" => Action::SwitchNavMode);
}

fn content_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::EpubContent);
    bind!(ctx, "j" => Action::ScrollDown);
    bind!(ctx, "<Down>" => Action::ScrollDown);
    bind!(ctx, "k" => Action::ScrollUp);
    bind!(ctx, "<Up>" => Action::ScrollUp);
    bind!(ctx, "h" => Action::PrevChapter);
    bind!(ctx, "<Left>" => Action::PrevChapter);
    bind!(ctx, "l" => Action::NextChapter);
    bind!(ctx, "<Right>" => Action::NextChapter);
    bind!(ctx, "{" => Action::ParagraphBackward);
    bind!(ctx, "}" => Action::ParagraphForward);
    bind!(ctx, "/" => Action::StartSearch);
    bind!(ctx, "n" => Action::ToggleNormalMode);
    bind!(ctx, "a" => Action::AddComment);
    bind!(ctx, "d" => Action::DeleteComment);
    bind!(ctx, "c" => Action::CopySelection);
    bind!(ctx, "<C-i>" => Action::JumpForward);
    bind!(ctx, "<C-o>" => Action::JumpBackward);
    bind!(ctx, "p" => Action::ToggleProfiling);
    bind!(ctx, "<Tab>" => Action::SwitchFocus);
    bind!(ctx, "-" => Action::IncreaseMargin);
    bind!(ctx, "=" => Action::DecreaseMargin);
    bind!(ctx, "+" => Action::DecreaseMargin);
    bind!(ctx, "v" => Action::EnterVisualMode);
    bind!(ctx, "V" => Action::EnterVisualLineMode);
    bind!(ctx, "y" => Action::StartYank);
    bind!(ctx, "q" => Action::Quit);
    bind!(ctx, "<CR>" => Action::FollowLink);
    bind!(ctx, "ss" => Action::ToggleRawHtml);
}

fn epub_normal_specifics(keymap: &mut Keymap) {
    let _ = keymap;
}

fn pdf_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PdfStandard);
    bind!(ctx, "j" => Action::ScrollDown);
    bind!(ctx, "J" => Action::ScrollDown);
    bind!(ctx, "<Down>" => Action::ScrollDown);
    bind!(ctx, "k" => Action::ScrollUp);
    bind!(ctx, "K" => Action::ScrollUp);
    bind!(ctx, "<Up>" => Action::ScrollUp);
    bind!(ctx, "l" => Action::NextPage);
    bind!(ctx, "<Right>" => Action::NextPage);
    bind!(ctx, "h" => Action::PrevPage);
    bind!(ctx, "<Left>" => Action::PrevPage);
    bind!(ctx, "H" => Action::PanLeft);
    bind!(ctx, "L" => Action::PanRight);
    bind!(ctx, "gd" => Action::SynctexInverse);
    bind!(ctx, "<Space>g" => Action::GoToPage);
    bind!(ctx, "n" => Action::ToggleNormalMode);
    bind!(ctx, "N" => Action::PrevSearchMatch);
    bind!(ctx, "i" => Action::ToggleInvertImages);
    bind!(ctx, "I" => Action::TogglePdfTheming);
    bind!(ctx, "p" => Action::ToggleProfiling);
    bind!(ctx, "x" => Action::DumpDebugState);
    bind!(ctx, "a" => Action::AddComment);
    bind!(ctx, "z" => Action::ZoomReset);
    bind!(ctx, "Z" => Action::ZoomFitWidth);
    bind!(ctx, "=" => Action::ZoomIn);
    bind!(ctx, "+" => Action::ZoomIn);
    bind!(ctx, "-" => Action::ZoomOut);
    bind!(ctx, "_" => Action::ZoomOut);
    bind!(ctx, "q" => Action::Quit);
    bind!(ctx, "<S-Tab>" => Action::EnterCommentNav);
    bind!(ctx, "/" => Action::StartSearch);
    bind!(ctx, "<C-c>" => Action::CopySelection);
}

fn pdf_normal_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PdfNormal);
    bind!(ctx, "c" => Action::CopySelection);
    bind!(ctx, "d" => Action::AddComment);
    bind!(ctx, "N" => Action::PrevSearchMatch);
    bind!(ctx, "gd" => Action::SynctexInverse); // #2: restore gd
    // Note: 'i' for pending_inner (text objects) is handled in handle_normal_mode_key,
    // not via the keymap, because it enters a pending state that consumes the next char.
    bind!(ctx, "<S-Tab>" => Action::EnterCommentNav);
    bind!(ctx, "<CR>" => Action::FollowLink);
}

fn popup_help_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PopupHelp);
    bind!(ctx, "?" => Action::Cancel);
    bind!(ctx, "/" => Action::StartSearch);
    bind!(ctx, "n" => Action::NextSearchMatch);
    bind!(ctx, "N" => Action::PrevSearchMatch);
}

fn popup_history_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PopupHistory);
    bind!(ctx, "h" => Action::MoveLeft);
    bind!(ctx, "<Left>" => Action::MoveLeft);
    bind!(ctx, "l" => Action::MoveRight);
    bind!(ctx, "<Right>" => Action::MoveRight);
    bind!(ctx, "<Tab>" => Action::NextTab);
    bind!(ctx, "<S-Tab>" => Action::PrevTab); // #8: restore BackTab
    bind!(ctx, "dd" => Action::DeleteEntry);
    bind!(ctx, "c" => Action::CopyEntry);
    bind!(ctx, "C" => Action::CopyEntry);
    bind!(ctx, "y" => Action::CopyEntry);
    bind!(ctx, "Y" => Action::CopyEntry);
    bind!(ctx, "/" => Action::StartSearch);
    bind!(ctx, "n" => Action::NextSearchMatch);
    bind!(ctx, "N" => Action::PrevSearchMatch);
    bind!(ctx, "<CR>" => Action::Select);
}

fn popup_search_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PopupSearch);
    bind!(ctx, "<CR>" => Action::Select);
    bind!(ctx, "/" => Action::StartSearch);
}

fn popup_stats_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PopupStats);
    bind!(ctx, "<CR>" => Action::Select);
}

fn popup_comments_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PopupComments);
    bind!(ctx, "h" => Action::MoveLeft);
    bind!(ctx, "<Left>" => Action::MoveLeft);
    bind!(ctx, "l" => Action::MoveRight);
    bind!(ctx, "<Right>" => Action::MoveRight);
    bind!(ctx, "<Tab>" => Action::NextTab);
    bind!(ctx, "dd" => Action::DeleteComment); // #4: restore dd (was single d)
    bind!(ctx, "<Space>e" => Action::ExportComments); // #4: restore Space+e (was single e)
    bind!(ctx, "<CR>" => Action::Select);
    bind!(ctx, "/" => Action::StartSearch);
    bind!(ctx, "?" => Action::ToggleGlobalSearch);
    bind!(ctx, "n" => Action::NextSearchMatch);
    bind!(ctx, "N" => Action::PrevSearchMatch);
}

fn popup_settings_specifics(keymap: &mut Keymap) {
    let ctx = keymap.context_mut(KeyContext::PopupSettings);
    bind!(ctx, "h" => Action::MoveLeft);
    bind!(ctx, "<Left>" => Action::MoveLeft);
    bind!(ctx, "l" => Action::MoveRight);
    bind!(ctx, "<Right>" => Action::MoveRight);
    bind!(ctx, "<Tab>" => Action::NextTab);
    bind!(ctx, "<S-Tab>" => Action::PrevTab);
    bind!(ctx, "<CR>" => Action::Select);
    bind!(ctx, "<Space>" => Action::Select);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybindings::keymap::LookupResult;
    use crate::keybindings::notation::parse_key_binding;

    fn lookup(keymap: &Keymap, context: KeyContext, notation: &str) -> LookupResult {
        let binding = parse_key_binding(notation).unwrap();
        keymap.lookup(context, binding.keys())
    }

    #[test]
    fn default_keymap_construction_does_not_panic() {
        let _keymap = default_keymap();
    }

    /// Every `Action` variant must be reachable: either bound in the default
    /// keymap, or explicitly allowlisted as unreachable-by-design.
    ///
    /// If this test fails, one of these is true:
    /// - You added an action variant and forgot to bind it in defaults.
    /// - You removed a default binding and left the action orphaned (like
    ///   the `Suspend` regression when `<C-z>`/`<C-q>` special-cases were
    ///   deleted).
    /// - The action was always dead code (e.g., `EditComment`) and should
    ///   either be wired up or deleted.
    ///
    /// To allowlist an action as intentionally unbound-by-default but still
    /// available for user overrides, add it to `UNBOUND_BY_DESIGN` below and
    /// document why.
    #[test]
    fn every_action_is_reachable_in_defaults() {
        use crate::keybindings::action::Action;
        use std::collections::HashSet;

        // Actions that cannot or should not appear in the default keymap.
        const UNREACHABLE_BY_DESIGN: &[Action] = &[
            // Special marker used in user config to disable a binding.
            Action::Nop,
            // Sentinel returned by serde when a config names an action the
            // current binary doesn't know about. Never dispatched.
            Action::Unknown,
        ];

        // Actions that have no default key but ARE available for users to
        // bind via `keybindings.yaml`. Each entry needs a justification —
        // otherwise bind it or delete it.
        const UNBOUND_BY_DESIGN: &[Action] = &[
            // Add entries as `(Action::Foo, "reason why unbound")` — kept
            // separate from UNREACHABLE_BY_DESIGN so the intent is clear.
        ];

        let keymap = default_keymap();
        let mut bound: HashSet<Action> = HashSet::new();
        for ctx in KeyContext::ALL {
            if let Some(ctx_map) = keymap.context(*ctx) {
                for (_, action) in ctx_map.all_bindings() {
                    bound.insert(action);
                }
            }
        }

        let unreachable: HashSet<&Action> = UNREACHABLE_BY_DESIGN.iter().collect();
        let unbound_ok: HashSet<&Action> = UNBOUND_BY_DESIGN.iter().collect();

        let missing: Vec<&Action> = Action::ALL
            .iter()
            .filter(|a| !bound.contains(a) && !unreachable.contains(a) && !unbound_ok.contains(a))
            .collect();

        assert!(
            missing.is_empty(),
            "the following actions have no default binding and are not allowlisted:\n{}\n\n\
             Each one is either a regression (action exists but no user can trigger it \
             out of the box) or dead code. Fix by:\n\
               - binding it in `defaults.rs`, OR\n\
               - adding it to `UNBOUND_BY_DESIGN` with a reason, OR\n\
               - deleting the variant from `Action` if it's dead.",
            missing
                .iter()
                .map(|a| format!("  - Action::{a:?}  ({})", a.description()))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    #[test]
    fn all_contexts_have_bindings() {
        let keymap = default_keymap();
        for ctx in KeyContext::ALL {
            let context_keymap = keymap.context(*ctx);
            assert!(context_keymap.is_some(), "context {ctx:?} has no keymap");
            assert!(
                !context_keymap.unwrap().all_bindings().is_empty(),
                "context {ctx:?} has no bindings"
            );
        }
    }

    // Layers
    #[test]
    fn all_layer_inherited() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "j"),
            LookupResult::Found(Action::MoveDown)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "gg"),
            LookupResult::Found(Action::GoTop)
        );
    }

    #[test]
    fn all_layer_not_in_global() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "j"),
            LookupResult::NoMatch
        );
    }

    #[test]
    fn normal_layer_inherited() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubNormal, "w"),
            LookupResult::Found(Action::WordForward)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::PdfNormal, "w"),
            LookupResult::Found(Action::WordForward)
        );
    }

    // #5: uppercase aliases
    #[test]
    fn uppercase_word_aliases() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubNormal, "W"),
            LookupResult::Found(Action::WordForward)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::EpubNormal, "B"),
            LookupResult::Found(Action::WordBackward)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::EpubNormal, "E"),
            LookupResult::Found(Action::WordEnd)
        );
    }

    // #2: gd in PDF normal
    #[test]
    fn pdf_normal_gd() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::PdfNormal, "gd"),
            LookupResult::Found(Action::SynctexInverse)
        );
    }

    // #4: comments dd and Space+e
    #[test]
    fn comments_dd_delete() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::PopupComments, "dd"),
            LookupResult::Found(Action::DeleteComment)
        );
        // Single d should be a prefix, not an action
        assert_eq!(
            lookup(&keymap, KeyContext::PopupComments, "d"),
            LookupResult::Prefix
        );
    }

    #[test]
    fn comments_space_e_export() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::PopupComments, "<Space>e"),
            LookupResult::Found(Action::ExportComments)
        );
    }

    // #8: BackTab in history
    #[test]
    fn history_backtab() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::PopupHistory, "<S-Tab>"),
            LookupResult::Found(Action::PrevTab)
        );
    }

    // #9: Shift-arrows in nav
    #[test]
    fn nav_shift_arrows() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "<S-Left>"),
            LookupResult::Found(Action::CollapseAll)
        );
        assert_eq!(
            lookup(&keymap, KeyContext::Navigation, "<S-Right>"),
            LookupResult::Found(Action::ExpandAll)
        );
    }

    // Spot checks
    #[test]
    fn global_help() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::Global, "?"),
            LookupResult::Found(Action::ToggleHelp)
        );
    }

    #[test]
    fn content_overrides_jk() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::EpubContent, "j"),
            LookupResult::Found(Action::ScrollDown)
        );
    }

    #[test]
    fn popup_help_question_closes() {
        let keymap = default_keymap();
        assert_eq!(
            lookup(&keymap, KeyContext::PopupHelp, "?"),
            LookupResult::Found(Action::Cancel)
        );
    }
}
