//! PDF keybinding integration tests.
//!
//! Unlike `keybinding_actions.rs` which tests through the full App, these tests
//! construct a `PdfReaderState` directly and dispatch events to `handle_event()`.
//! This avoids the heavyweight render pipeline (MuPDF, worker threads, Kitty
//! graphics) while still exercising the full keymap → dispatch path.
//!
//! Only compiled with `--features pdf`.

#![cfg(feature = "pdf")]

use bookokrat::keybindings::context::KeyContext;
use bookokrat::keybindings::defaults::default_keymap;
use bookokrat::keybindings::notation::{format_key_binding, parse_key_binding};
use bookokrat::theme::current_theme;
use bookokrat::widget::pdf_reader::state::{InputAction, PdfReaderState};
use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyEventState};
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════════

fn create_pdf_state() -> PdfReaderState {
    PdfReaderState::new(
        "test.pdf".to_string(),
        true,  // is_kitty
        false, // is_iterm
        0,     // initial_page
        1.0,   // zoom_factor
        0,     // pan_from_left
        0,     // global_scroll_offset
        current_theme().clone(),
        0,             // theme_index
        false,         // comments_enabled
        false,         // supports_comments
        None,          // book_comments
        String::new(), // comments_doc_id
    )
}

/// Parse notation and simulate the full sequence against PdfReaderState.
/// Returns the action from the last key in the sequence.
fn simulate(state: &mut PdfReaderState, notation: &str) -> Option<InputAction> {
    let binding = parse_key_binding(notation).expect(notation);
    let mut last_action = None;
    for ki in binding.keys() {
        let key = KeyEvent {
            code: ki.code,
            modifiers: ki.modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let response = state.handle_event(&Event::Key(key));
        last_action = response.action;
    }
    last_action
}

// ═══════════════════════════════════════════════════════════════
// Macro: generates tests + tested-bindings list
// ═══════════════════════════════════════════════════════════════

macro_rules! pdf_binding_tests {
    ($(
        $test_name:ident : $ctx:expr, $notation:expr,
        setup = |$state:ident| $setup:expr,
        check = |$state2:ident, $action:ident| $check:expr
    );* $(;)?) => {
        $(
            #[test]
            fn $test_name() {
                let mut $state = create_pdf_state();
                $setup;
                let $action = simulate(&mut $state, $notation);
                let $state2 = &$state;
                assert!($check,
                    "binding check failed: {} {}",
                    stringify!($test_name), $notation
                );
            }
        )*

        fn tested_bindings() -> Vec<(KeyContext, String)> {
            vec![
                $(($ctx, $notation.to_string()),)*
            ]
        }
    };
}

// ═══════════════════════════════════════════════════════════════
// Table
// ═══════════════════════════════════════════════════════════════

pdf_binding_tests! {
    // ── PdfStandard ──────────────────────────────────
    // Scroll/zoom actions return Some(InputAction::Redraw) via update_zoom
    pdf_j: KeyContext::PdfStandard, "j",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_j_upper: KeyContext::PdfStandard, "J",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_down: KeyContext::PdfStandard, "<Down>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_k: KeyContext::PdfStandard, "k",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_k_upper: KeyContext::PdfStandard, "K",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_up: KeyContext::PdfStandard, "<Up>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    // next_page / prev_page at edges with empty rendered: return None (no-op)
    pdf_l: KeyContext::PdfStandard, "l",
        setup = |state| {},
        check = |_state, action| action.is_none();
    pdf_right: KeyContext::PdfStandard, "<Right>",
        setup = |state| {},
        check = |_state, action| action.is_none();
    pdf_h: KeyContext::PdfStandard, "h",
        setup = |state| {},
        check = |_state, action| action.is_none();
    pdf_left: KeyContext::PdfStandard, "<Left>",
        setup = |state| {},
        check = |_state, action| action.is_none();
    pdf_h_upper: KeyContext::PdfStandard, "H",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_l_upper: KeyContext::PdfStandard, "L",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_gg: KeyContext::PdfStandard, "gg",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_g_upper: KeyContext::PdfStandard, "G",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    // gd → synctex inverse; without anchor returns None
    pdf_gd: KeyContext::PdfStandard, "gd",
        setup = |state| {},
        check = |_state, action| action.is_none();
    pdf_space_g: KeyContext::PdfStandard, "<Space>g",
        setup = |state| {},
        check = |state, _action| state.go_to_page_input.is_some();
    pdf_ctrl_d: KeyContext::PdfStandard, "<C-d>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_ctrl_u: KeyContext::PdfStandard, "<C-u>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_ctrl_f: KeyContext::PdfStandard, "<C-f>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_ctrl_b: KeyContext::PdfStandard, "<C-b>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_pagedown: KeyContext::PdfStandard, "<PageDown>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_pageup: KeyContext::PdfStandard, "<PageUp>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    // Toggling normal mode ON with no rendered pages returns None (no-op)
    pdf_n: KeyContext::PdfStandard, "n",
        setup = |state| {},
        check = |state, action| action.is_none() && !state.normal_mode.active;
    // N = PrevSearchMatch; without matches shows HUD and returns Redraw
    pdf_n_upper: KeyContext::PdfStandard, "N",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_i: KeyContext::PdfStandard, "i",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::ToggleInvertImages));
    pdf_i_upper: KeyContext::PdfStandard, "I",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::TogglePdfTheming));
    pdf_p: KeyContext::PdfStandard, "p",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::ToggleProfiling));
    pdf_x: KeyContext::PdfStandard, "x",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::DumpDebugState));
    // 'a' starts comment input; without comments_enabled shows HUD and returns Redraw
    pdf_a: KeyContext::PdfStandard, "a",
        setup = |state| {},
        check = |state, action| matches!(action, Some(InputAction::Redraw))
            && !state.comment_input.is_active();
    pdf_z: KeyContext::PdfStandard, "z",
        setup = |state| {},
        check = |_state, action| action.is_some(); // zoom reset may return Redraw or RenderScale
    pdf_z_upper: KeyContext::PdfStandard, "Z",
        setup = |state| {},
        check = |_state, action| action.is_some();
    pdf_equals: KeyContext::PdfStandard, "=",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_plus: KeyContext::PdfStandard, "+",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_minus: KeyContext::PdfStandard, "-",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_underscore: KeyContext::PdfStandard, "_",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::Redraw));
    pdf_q: KeyContext::PdfStandard, "q",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::QuitApp));
    // Esc clears search highlight (default branch); returns SelectionChanged(empty)
    pdf_esc: KeyContext::PdfStandard, "<Esc>",
        setup = |state| {},
        check = |_state, action| matches!(action, Some(InputAction::SelectionChanged(rects)) if rects.is_empty());
    // S-Tab enters comment nav; without comments, start_comment_nav returns None
    pdf_backtab: KeyContext::PdfStandard, "<S-Tab>",
        setup = |state| {},
        check = |state, action| action.is_none() && !state.comment_nav_active;
    // '/' dispatched but main event loop handles; PdfReaderState returns None
    pdf_slash: KeyContext::PdfStandard, "/",
        setup = |state| {},
        check = |_state, action| action.is_none();
    // C-c calls copy_selection; without selection it returns None
    pdf_ctrl_c: KeyContext::PdfStandard, "<C-c>",
        setup = |state| {},
        check = |_state, action| action.is_none();

    // ── PdfNormal ────────────────────────────────────
    // Cursor motions return Some(InputAction::CursorChanged(...))
    pdf_normal_h: KeyContext::PdfNormal, "h",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_left: KeyContext::PdfNormal, "<Left>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_j: KeyContext::PdfNormal, "j",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_down: KeyContext::PdfNormal, "<Down>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_k: KeyContext::PdfNormal, "k",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_up: KeyContext::PdfNormal, "<Up>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_l: KeyContext::PdfNormal, "l",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_right: KeyContext::PdfNormal, "<Right>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_w: KeyContext::PdfNormal, "w",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_w_upper: KeyContext::PdfNormal, "W",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_b: KeyContext::PdfNormal, "b",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_b_upper: KeyContext::PdfNormal, "B",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_e: KeyContext::PdfNormal, "e",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_e_upper: KeyContext::PdfNormal, "E",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_0: KeyContext::PdfNormal, "0",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_caret: KeyContext::PdfNormal, "^",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_dollar: KeyContext::PdfNormal, "$",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    // ParagraphBackward/Forward are not implemented in PDF normal mode (returns None)
    pdf_normal_curly_open: KeyContext::PdfNormal, "{",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| action.is_none();
    pdf_normal_curly_close: KeyContext::PdfNormal, "}",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| action.is_none();
    // f/F/t/T set pending char motion, return Some(Redraw)
    pdf_normal_f: KeyContext::PdfNormal, "f",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::Redraw))
            && state.normal_mode.has_pending_char_motion();
    pdf_normal_f_upper: KeyContext::PdfNormal, "F",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::Redraw))
            && state.normal_mode.has_pending_char_motion();
    pdf_normal_t: KeyContext::PdfNormal, "t",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::Redraw))
            && state.normal_mode.has_pending_char_motion();
    pdf_normal_t_upper: KeyContext::PdfNormal, "T",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::Redraw))
            && state.normal_mode.has_pending_char_motion();
    // ; repeats last find; no prior find → cursor didn't move but action returns
    pdf_normal_semicolon: KeyContext::PdfNormal, ";",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    // , repeats last find reversed; same contract as ;
    pdf_normal_comma: KeyContext::PdfNormal, ",",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    // v/V toggle visual mode → returns VisualChanged
    pdf_normal_v: KeyContext::PdfNormal, "v",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::VisualChanged(_, _)))
            && state.normal_mode.is_visual_active();
    pdf_normal_v_upper: KeyContext::PdfNormal, "V",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::VisualChanged(_, _)))
            && state.normal_mode.is_visual_active();
    // y without visual selection = no-op (None)
    pdf_normal_y: KeyContext::PdfNormal, "y",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| action.is_none();
    // n toggles normal mode OFF, returns ExitNormalMode
    pdf_normal_n: KeyContext::PdfNormal, "n",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::ExitNormalMode))
            && !state.normal_mode.active;
    pdf_normal_n_upper: KeyContext::PdfNormal, "N",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::Redraw)); // no matches HUD
    // gg → cursor to page top; handled via pending_g state + 'g' char
    pdf_normal_gg: KeyContext::PdfNormal, "gg",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    pdf_normal_g_upper: KeyContext::PdfNormal, "G",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(action, Some(InputAction::CursorChanged(_, _)));
    // gd → synctex inverse; without anchor, returns None
    pdf_normal_gd: KeyContext::PdfNormal, "gd",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| action.is_none();
    // In normal mode, C-d/u/f/b move cursor → CursorChanged
    pdf_normal_ctrl_d: KeyContext::PdfNormal, "<C-d>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(
            action,
            Some(InputAction::CursorChanged(_, _)) | Some(InputAction::Redraw)
        );
    pdf_normal_ctrl_u: KeyContext::PdfNormal, "<C-u>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(
            action,
            Some(InputAction::CursorChanged(_, _)) | Some(InputAction::Redraw)
        );
    pdf_normal_ctrl_f: KeyContext::PdfNormal, "<C-f>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(
            action,
            Some(InputAction::CursorChanged(_, _)) | Some(InputAction::Redraw)
        );
    pdf_normal_ctrl_b: KeyContext::PdfNormal, "<C-b>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(
            action,
            Some(InputAction::CursorChanged(_, _)) | Some(InputAction::Redraw)
        );
    pdf_normal_pagedown: KeyContext::PdfNormal, "<PageDown>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(
            action,
            Some(InputAction::CursorChanged(_, _)) | Some(InputAction::Redraw)
        );
    pdf_normal_pageup: KeyContext::PdfNormal, "<PageUp>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| matches!(
            action,
            Some(InputAction::CursorChanged(_, _)) | Some(InputAction::Redraw)
        );
    // Esc exits normal mode → returns ExitNormalMode
    pdf_normal_esc: KeyContext::PdfNormal, "<Esc>",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| matches!(action, Some(InputAction::ExitNormalMode))
            && !state.normal_mode.active;
    // c = CopySelection; without selection, copy_selection returns None
    pdf_normal_c: KeyContext::PdfNormal, "c",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| action.is_none();
    // d = AddComment; without visual mode or selection, returns None
    pdf_normal_d: KeyContext::PdfNormal, "d",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| action.is_none();
    // S-Tab → EnterCommentNav; deactivates normal mode, start_comment_nav without comments = None
    pdf_normal_backtab: KeyContext::PdfNormal, "<S-Tab>",
        setup = |state| { state.normal_mode.active = true; },
        check = |state, action| action.is_none() && !state.normal_mode.active;
    // CR → FollowLink; without link at cursor, returns None
    pdf_normal_enter: KeyContext::PdfNormal, "<CR>",
        setup = |state| { state.normal_mode.active = true; },
        check = |_state, action| action.is_none()
}

// ═══════════════════════════════════════════════════════════════
// Coverage enforcement
// ═══════════════════════════════════════════════════════════════

#[test]
fn every_pdf_default_binding_has_a_test() {
    let keymap = default_keymap();
    let tested: HashSet<(KeyContext, String)> = tested_bindings().into_iter().collect();

    let pdf_contexts = [KeyContext::PdfStandard, KeyContext::PdfNormal];

    let mut missing = Vec::new();
    for ctx in &pdf_contexts {
        if let Some(ctx_map) = keymap.context(*ctx) {
            for (binding, action) in ctx_map.all_bindings() {
                let notation = format_key_binding(&binding);
                if !tested.contains(&(*ctx, notation.clone())) {
                    missing.push(format!(
                        "  {} {:>12} → {:?}",
                        ctx.config_key(),
                        notation,
                        action
                    ));
                }
            }
        }
    }

    if !missing.is_empty() {
        missing.sort();
        panic!(
            "Untested PDF keybindings ({}):\n{}",
            missing.len(),
            missing.join("\n")
        );
    }
}
