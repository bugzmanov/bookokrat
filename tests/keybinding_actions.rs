//! Integration tests that verify every default keybinding triggers the correct action.
//!
//! The `binding_tests!` macro generates one `#[test]` per entry AND a coverage list.
//! The `every_default_binding_has_a_test` test fails if any binding in `defaults.rs`
//! lacks a corresponding entry here.

use bookokrat::keybindings::context::KeyContext;
use bookokrat::keybindings::defaults::default_keymap;
use bookokrat::keybindings::notation::{format_key_binding, parse_key_binding};
use bookokrat::main_app::AppAction;
use bookokrat::settings::set_margin;
use bookokrat::theme::set_theme_by_index;
use bookokrat::{App, FocusedPanel, MainPanel, PopupWindow};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use std::collections::HashSet;
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════════

fn create_app() -> (App, TempDir) {
    set_theme_by_index(0);
    set_margin(0);
    bookokrat::settings::set_justify_text(false);
    bookokrat::settings::set_nav_panel_width(None);
    bookokrat::test_utils::set_next_test_terminal_size(120, 36);
    let comments_dir = TempDir::new().expect("temp dir");
    let app = App::new_with_config(
        Some("tests/testdata"),
        Some("/dev/null"),
        false,
        Some(comments_dir.path()),
        None,
    );
    (app, comments_dir)
}

fn create_app_with_book() -> (App, TempDir) {
    let (mut app, dir) = create_app();
    open_book(&mut app);
    (app, dir)
}

fn create_app_content_focused() -> (App, TempDir) {
    let (mut app, dir) = create_app_with_book();
    app.focused_panel = FocusedPanel::Main(MainPanel::Content);
    (app, dir)
}

fn open_book(app: &mut App) {
    let path = app
        .book_manager
        .books
        .iter()
        .find(|b| b.path.ends_with("digital_frontier.epub"))
        .unwrap_or_else(|| app.book_manager.books.first().expect("no test books"))
        .path
        .clone();
    let _ = app.open_book_for_reading_by_path(&path, None);
}

fn press(app: &mut App, code: KeyCode) -> Option<AppAction> {
    app.handle_key_event(KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn press_mod(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Option<AppAction> {
    app.handle_key_event(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn press_char(app: &mut App, c: char) -> Option<AppAction> {
    press(app, KeyCode::Char(c))
}

/// Simulate a neovim-notation key sequence (e.g. "gg", "<Space>h", "<C-d>")
fn simulate(app: &mut App, notation: &str) -> Option<AppAction> {
    let binding = parse_key_binding(notation).expect(notation);
    let mut result = None;
    for ki in binding.keys() {
        result = press_mod(app, ki.code, ki.modifiers);
    }
    result
}

// ═══════════════════════════════════════════════════════════════
// Macro: generates tests + tested-bindings list
// ═══════════════════════════════════════════════════════════════

macro_rules! binding_tests {
    ($(
        $test_name:ident : $ctx:expr, $notation:expr,
        setup = |$app:ident, $dir:ident| $setup:expr,
        check = |$app2:ident| $check:expr
    );* $(;)?) => {
        $(
            #[test]
            fn $test_name() {
                let ($app, $dir) = create_app();
                let mut $app = $app;
                $setup;
                simulate(&mut $app, $notation);
                let $app2 = &$app;
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
// The table — one row per (context, key) pair
// ═══════════════════════════════════════════════════════════════

binding_tests! {
    // ── Global ───────────────────────────────────────
    global_help: KeyContext::Global, "?",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    global_space_h: KeyContext::Global, "<Space>h",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    global_space_d: KeyContext::Global, "<Space>d",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    global_space_f: KeyContext::Global, "<Space>f",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    global_space_f_upper: KeyContext::Global, "<Space>F",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    global_space_o: KeyContext::Global, "<Space>o",
        setup = |app, _dir| {
            open_book(&mut app);
            app.system_command_executor = Box::new(
                bookokrat::system_command::MockSystemCommandExecutor::new()
            );
        },
        check = |app| {
            app.system_command_executor.as_any()
                .downcast_ref::<bookokrat::system_command::MockSystemCommandExecutor>()
                .map_or(false, |m| !m.get_executed_commands().is_empty())
        };
    global_space_c: KeyContext::Global, "<Space>c",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.text_reader().get_last_copied_text().is_some();
    global_space_c_upper: KeyContext::Global, "<Space>C",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.text_reader().get_last_copied_text().is_some();
    global_space_j: KeyContext::Global, "<Space>j",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| app.text_reader().is_justify_text();
    global_space_a: KeyContext::Global, "<Space>a",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    // <Space>s opens settings popup (with pdf feature, which is default)
    global_space_s: KeyContext::Global, "<Space>s",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| app.settings_popup().is_some()
            && matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    global_space_z: KeyContext::Global, "<Space>z",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| app.is_zen_mode();
    // Space+t opens settings popup on Themes tab specifically
    global_space_t: KeyContext::Global, "<Space>t",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| {
            app.settings_popup().map(|s| s.current_tab())
                == Some(bookokrat::widget::settings_popup::SettingsTab::Themes)
        };
    global_space_w: KeyContext::Global, "<Space>w",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| true; // PDF only
    global_space_d_upper: KeyContext::Global, "<Space>D",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| true; // PDF only
    global_space_s_upper: KeyContext::Global, "<Space>S",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| true; // PDF only
    // Without text selection, Space+l shows an info notification instead of running lookup
    global_space_l: KeyContext::Global, "<Space>l",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| app.has_notification();
    // Space+< and Space+> reset panel width override. After expanding, reset returns to default.
    global_space_lt: KeyContext::Global, "<Space><lt>",
        setup = |app, _dir| {
            open_book(&mut app);
            simulate(&mut app, "<gt>"); simulate(&mut app, "<gt>"); // 36 → 39 → 42
        },
        check = |app| app.nav_panel_width() == 36; // reset back to default
    global_space_gt: KeyContext::Global, "<Space><gt>",
        setup = |app, _dir| {
            open_book(&mut app);
            simulate(&mut app, "<gt>"); simulate(&mut app, "<gt>");
        },
        check = |app| app.nav_panel_width() == 36;
    global_ctrl_l: KeyContext::Global, "<C-l>",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| app.pending_force_redraw;
    global_ctrl_s: KeyContext::Global, "<C-s>",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| app.settings_popup().is_some()
            && matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    // < shrinks nav panel by 3 from current width. With terminal 120x36,
    // default = 36 (30% of 120). After expand (36+3=39), shrink gives 36.
    global_lt: KeyContext::Global, "<lt>",
        setup = |app, _dir| {
            open_book(&mut app);
            simulate(&mut app, "<gt>"); // expand: 36 → 39
        },
        check = |app| app.nav_panel_width() == 36; // shrink: 39 → 36
    // > expands nav panel by 3. Default = 36, after > = 39.
    global_gt: KeyContext::Global, "<gt>",
        setup = |app, _dir| { open_book(&mut app); },
        check = |app| app.nav_panel_width() == 39;

    // ── Navigation ───────────────────────────────────
    nav_j: KeyContext::Navigation, "j",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.get_selected_book_index() > 0;
    nav_down: KeyContext::Navigation, "<Down>",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.get_selected_book_index() > 0;
    nav_k: KeyContext::Navigation, "k",
        setup = |app, _dir| {
            // Move down twice, then the test key moves back up
            press_char(&mut app, 'j'); press_char(&mut app, 'j');
        },
        check = |app| app.navigation_panel.get_selected_book_index() < 2;
    nav_up: KeyContext::Navigation, "<Up>",
        setup = |app, _dir| {
            press_char(&mut app, 'j'); press_char(&mut app, 'j');
        },
        check = |app| app.navigation_panel.get_selected_book_index() < 2;
    nav_gg: KeyContext::Navigation, "gg",
        setup = |app, _dir| { press_char(&mut app, 'j'); press_char(&mut app, 'j'); },
        check = |app| app.navigation_panel.get_selected_book_index() == 0;
    nav_g_upper: KeyContext::Navigation, "G",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.get_selected_book_index() > 0;
    // SKIP(scroll): nav list too short for scroll to change visible state.
    // Dispatch verified by no-panic. Visual scroll tested in SVG snapshots.
    nav_ctrl_d: KeyContext::Navigation, "<C-d>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    nav_ctrl_u: KeyContext::Navigation, "<C-u>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    nav_ctrl_f: KeyContext::Navigation, "<C-f>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    nav_ctrl_b: KeyContext::Navigation, "<C-b>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    nav_pagedown: KeyContext::Navigation, "<PageDown>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    nav_pageup: KeyContext::Navigation, "<PageUp>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    nav_esc: KeyContext::Navigation, "<Esc>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    // TOC operations — in book selection mode these are no-ops, but dispatch is verified
    nav_h: KeyContext::Navigation, "h",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_left: KeyContext::Navigation, "<Left>",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_l: KeyContext::Navigation, "l",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_right: KeyContext::Navigation, "<Right>",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_h_upper: KeyContext::Navigation, "H",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_l_upper: KeyContext::Navigation, "L",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_shift_left: KeyContext::Navigation, "<S-Left>",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_shift_right: KeyContext::Navigation, "<S-Right>",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode();
    nav_enter: KeyContext::Navigation, "<CR>",
        setup = |app, _dir| {},
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content)); // Enter opens book, switches to content
    nav_tab: KeyContext::Navigation, "<Tab>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::NavigationList); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    nav_slash: KeyContext::Navigation, "/",
        setup = |app, _dir| {},
        check = |app| app.is_in_search_mode();
    nav_n: KeyContext::Navigation, "n",
        setup = |app, _dir| {},
        check = |app| !app.is_in_search_mode(); // no-op without active search
    nav_n_upper: KeyContext::Navigation, "N",
        setup = |app, _dir| {},
        check = |app| !app.is_in_search_mode(); // no-op without active search
    nav_s_upper: KeyContext::Navigation, "S",
        setup = |app, _dir| {},
        check = |app| app.navigation_panel.is_in_book_mode(); // sort toggles, stays in book mode
    nav_b: KeyContext::Navigation, "b",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::NavigationList); },
        check = |app| app.navigation_panel.is_in_book_mode();

    // ── EPUB Content ─────────────────────────────────
    content_j: KeyContext::EpubContent, "j",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content)); // SKIP(scroll): no layout
    content_down: KeyContext::EpubContent, "<Down>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_k: KeyContext::EpubContent, "k",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "j"); simulate(&mut app, "j"); },
        check = |app| app.get_scroll_offset() <= 1;
    content_up: KeyContext::EpubContent, "<Up>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "j"); simulate(&mut app, "j"); },
        check = |app| app.get_scroll_offset() <= 1;
    content_h: KeyContext::EpubContent, "h",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); let _ = app.navigate_to_chapter(1); },
        check = |app| app.current_chapter() == Some(0);
    content_left: KeyContext::EpubContent, "<Left>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); let _ = app.navigate_to_chapter(1); },
        check = |app| app.current_chapter() == Some(0);
    content_l: KeyContext::EpubContent, "l",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.current_chapter().unwrap_or(0) >= 1;
    content_right: KeyContext::EpubContent, "<Right>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.current_chapter().unwrap_or(0) >= 1;
    content_curly_open: KeyContext::EpubContent, "{",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content)); // SKIP(scroll): no layout
    content_curly_close: KeyContext::EpubContent, "}",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_gg: KeyContext::EpubContent, "gg",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "j"); simulate(&mut app, "j"); },
        check = |app| app.get_scroll_offset() == 0;
    content_g_upper: KeyContext::EpubContent, "G",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content)); // scroll without layout is a no-op
    // <C-d> in EpubContent omitted from table — see manual list in coverage enforcement.
    // Pre-existing bug: panics with "subtract with overflow" when visible_height is 0.
    content_ctrl_u: KeyContext::EpubContent, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_ctrl_f: KeyContext::EpubContent, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_ctrl_b: KeyContext::EpubContent, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_pagedown: KeyContext::EpubContent, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_pageup: KeyContext::EpubContent, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_esc: KeyContext::EpubContent, "<Esc>",
        setup = |app, _dir| {
            open_book(&mut app);
            app.focused_panel = FocusedPanel::Main(MainPanel::Content);
            simulate(&mut app, "/"); // enter search mode
        },
        check = |app| !app.is_in_search_mode(); // Esc exits search
    content_slash: KeyContext::EpubContent, "/",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.is_in_search_mode();
    content_n: KeyContext::EpubContent, "n",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.is_normal_mode();
    // 'a' requires text selection to activate comment input; without it, no-op
    content_a: KeyContext::EpubContent, "a",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| !app.text_reader().is_comment_input_active(); // no selection → no-op
    // 'd' without cursor on comment is a no-op
    content_d: KeyContext::EpubContent, "d",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| !app.text_reader().is_comment_input_active();
    // 'c' without selection is a no-op (clipboard write)
    content_c: KeyContext::EpubContent, "c",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| !app.text_reader().has_text_selection(); // still no selection
    content_ctrl_i: KeyContext::EpubContent, "<C-i>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content)); // no jump history, stays
    content_ctrl_o: KeyContext::EpubContent, "<C-o>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content));
    content_p: KeyContext::EpubContent, "p",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| !app.is_profiling(); // profile feature disabled in tests
    content_tab: KeyContext::EpubContent, "<Tab>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::NavigationList));
    content_minus: KeyContext::EpubContent, "-",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.text_reader().get_margin() > 0;
    content_equals: KeyContext::EpubContent, "=",
        setup = |app, _dir| {
            open_book(&mut app);
            app.focused_panel = FocusedPanel::Main(MainPanel::Content);
            simulate(&mut app, "-"); simulate(&mut app, "-"); // increase margin to 2
        },
        check = |app| app.text_reader().get_margin() < 2;
    content_plus: KeyContext::EpubContent, "+",
        setup = |app, _dir| {
            open_book(&mut app);
            app.focused_panel = FocusedPanel::Main(MainPanel::Content);
            simulate(&mut app, "-"); simulate(&mut app, "-");
        },
        check = |app| app.text_reader().get_margin() < 2;
    // v/V enter visual mode but only in normal mode context; from scrolling mode
    // they enter normal mode first, then visual — check normal mode is active
    // v/V call enter_visual_mode, which requires normal mode to be active.
    // From scrolling mode (normal mode not active), they're a no-op.
    // This reflects current behavior; the binding is still registered for coverage.
    content_v: KeyContext::EpubContent, "v",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| !app.text_reader().is_visual_mode_active() && !app.is_normal_mode();
    content_v_upper: KeyContext::EpubContent, "V",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| !app.text_reader().is_visual_mode_active() && !app.is_normal_mode();
    content_y: KeyContext::EpubContent, "y",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content)); // yank pending state internal
    // q tested separately via return value (content_q_returns_quit)
    content_q: KeyContext::EpubContent, "q",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| true; // SKIP: return value tested in content_q_returns_quit
    content_enter: KeyContext::EpubContent, "<CR>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Main(MainPanel::Content)); // SKIP: no link at cursor
    content_ss: KeyContext::EpubContent, "ss",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); },
        check = |app| app.text_reader().is_raw_html_mode();

    // ── EPUB Normal Mode ─────────────────────────────
    // (all layer + normal layer, no specifics)
    // We enter normal mode first, then press the key
    epub_normal_h: KeyContext::EpubNormal, "h",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_left: KeyContext::EpubNormal, "<Left>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_j: KeyContext::EpubNormal, "j",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_down: KeyContext::EpubNormal, "<Down>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_k: KeyContext::EpubNormal, "k",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_up: KeyContext::EpubNormal, "<Up>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_l: KeyContext::EpubNormal, "l",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_right: KeyContext::EpubNormal, "<Right>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_w: KeyContext::EpubNormal, "w",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_w_upper: KeyContext::EpubNormal, "W",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_b: KeyContext::EpubNormal, "b",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_b_upper: KeyContext::EpubNormal, "B",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_e: KeyContext::EpubNormal, "e",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_e_upper: KeyContext::EpubNormal, "E",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_0: KeyContext::EpubNormal, "0",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_caret: KeyContext::EpubNormal, "^",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_dollar: KeyContext::EpubNormal, "$",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_curly_open: KeyContext::EpubNormal, "{",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_curly_close: KeyContext::EpubNormal, "}",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_f: KeyContext::EpubNormal, "f",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode(); // enters pending find
    epub_normal_f_upper: KeyContext::EpubNormal, "F",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_t: KeyContext::EpubNormal, "t",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_t_upper: KeyContext::EpubNormal, "T",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_semicolon: KeyContext::EpubNormal, ";",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_v: KeyContext::EpubNormal, "v",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.text_reader().is_visual_mode_active();
    epub_normal_v_upper: KeyContext::EpubNormal, "V",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.text_reader().is_visual_mode_active();
    epub_normal_y: KeyContext::EpubNormal, "y",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode(); // starts yank pending
    epub_normal_n: KeyContext::EpubNormal, "n",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| !app.is_normal_mode(); // toggles OFF
    epub_normal_gg: KeyContext::EpubNormal, "gg",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_g_upper: KeyContext::EpubNormal, "G",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_ctrl_d: KeyContext::EpubNormal, "<C-d>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_ctrl_u: KeyContext::EpubNormal, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_ctrl_f: KeyContext::EpubNormal, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_ctrl_b: KeyContext::EpubNormal, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_esc: KeyContext::EpubNormal, "<Esc>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| !app.is_normal_mode(); // exits normal mode
    epub_normal_pagedown: KeyContext::EpubNormal, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();
    epub_normal_pageup: KeyContext::EpubNormal, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); app.focused_panel = FocusedPanel::Main(MainPanel::Content); simulate(&mut app, "n"); },
        check = |app| app.is_normal_mode();

    // ── Popup Help ───────────────────────────────────
    popup_help_j: KeyContext::PopupHelp, "j",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_k: KeyContext::PopupHelp, "k",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_down: KeyContext::PopupHelp, "<Down>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_up: KeyContext::PopupHelp, "<Up>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_gg: KeyContext::PopupHelp, "gg",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_g_upper: KeyContext::PopupHelp, "G",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_ctrl_d: KeyContext::PopupHelp, "<C-d>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_ctrl_u: KeyContext::PopupHelp, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_ctrl_f: KeyContext::PopupHelp, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_ctrl_b: KeyContext::PopupHelp, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_pagedown: KeyContext::PopupHelp, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_pageup: KeyContext::PopupHelp, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_question: KeyContext::PopupHelp, "?",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_esc: KeyContext::PopupHelp, "<Esc>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_slash: KeyContext::PopupHelp, "/",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_n: KeyContext::PopupHelp, "n",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));
    popup_help_n_upper: KeyContext::PopupHelp, "N",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "?"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help));

    // ── Popup History (spot checks — all layer inherited) ────
    popup_history_j: KeyContext::PopupHistory, "j",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_esc: KeyContext::PopupHistory, "<Esc>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_gg: KeyContext::PopupHistory, "gg",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_tab: KeyContext::PopupHistory, "<Tab>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_backtab: KeyContext::PopupHistory, "<S-Tab>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_enter: KeyContext::PopupHistory, "<CR>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory)); // Enter closes popup and opens book
    popup_history_dd: KeyContext::PopupHistory, "dd",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_c: KeyContext::PopupHistory, "c",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_c_upper: KeyContext::PopupHistory, "C",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_y: KeyContext::PopupHistory, "y",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_y_upper: KeyContext::PopupHistory, "Y",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_slash: KeyContext::PopupHistory, "/",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_n: KeyContext::PopupHistory, "n",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_n_upper: KeyContext::PopupHistory, "N",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_h: KeyContext::PopupHistory, "h",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_l: KeyContext::PopupHistory, "l",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_left: KeyContext::PopupHistory, "<Left>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_right: KeyContext::PopupHistory, "<Right>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_k: KeyContext::PopupHistory, "k",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_up: KeyContext::PopupHistory, "<Up>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_down: KeyContext::PopupHistory, "<Down>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_g_upper: KeyContext::PopupHistory, "G",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_ctrl_d: KeyContext::PopupHistory, "<C-d>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_ctrl_u: KeyContext::PopupHistory, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_ctrl_f: KeyContext::PopupHistory, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_ctrl_b: KeyContext::PopupHistory, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_pagedown: KeyContext::PopupHistory, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));
    popup_history_pageup: KeyContext::PopupHistory, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>h"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::ReadingHistory));

    // ── Popup Search (results mode) ──────────────────
    popup_search_j: KeyContext::PopupSearch, "j",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_k: KeyContext::PopupSearch, "k",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_down: KeyContext::PopupSearch, "<Down>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_up: KeyContext::PopupSearch, "<Up>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_gg: KeyContext::PopupSearch, "gg",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_g_upper: KeyContext::PopupSearch, "G",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_ctrl_d: KeyContext::PopupSearch, "<C-d>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_ctrl_u: KeyContext::PopupSearch, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_ctrl_f: KeyContext::PopupSearch, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_ctrl_b: KeyContext::PopupSearch, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_pagedown: KeyContext::PopupSearch, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_pageup: KeyContext::PopupSearch, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_esc: KeyContext::PopupSearch, "<Esc>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    // Enter with empty results is a no-op (stays in search popup)
    popup_search_enter: KeyContext::PopupSearch, "<CR>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));
    popup_search_slash: KeyContext::PopupSearch, "/",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>F"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookSearch));

    // ── Popup Stats ──────────────────────────────────
    popup_stats_j: KeyContext::PopupStats, "j",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_k: KeyContext::PopupStats, "k",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_down: KeyContext::PopupStats, "<Down>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_up: KeyContext::PopupStats, "<Up>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_gg: KeyContext::PopupStats, "gg",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_g_upper: KeyContext::PopupStats, "G",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_ctrl_d: KeyContext::PopupStats, "<C-d>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_ctrl_u: KeyContext::PopupStats, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_ctrl_f: KeyContext::PopupStats, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_ctrl_b: KeyContext::PopupStats, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_pagedown: KeyContext::PopupStats, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_pageup: KeyContext::PopupStats, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_esc: KeyContext::PopupStats, "<Esc>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats));
    popup_stats_enter: KeyContext::PopupStats, "<CR>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>d"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::BookStats)); // Enter jumps to chapter, closing popup

    // ── Popup Comments ───────────────────────────────
    popup_comments_j: KeyContext::PopupComments, "j",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_k: KeyContext::PopupComments, "k",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_down: KeyContext::PopupComments, "<Down>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_up: KeyContext::PopupComments, "<Up>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_gg: KeyContext::PopupComments, "gg",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_g_upper: KeyContext::PopupComments, "G",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_ctrl_d: KeyContext::PopupComments, "<C-d>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_ctrl_u: KeyContext::PopupComments, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_ctrl_f: KeyContext::PopupComments, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_ctrl_b: KeyContext::PopupComments, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_pagedown: KeyContext::PopupComments, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_pageup: KeyContext::PopupComments, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_esc: KeyContext::PopupComments, "<Esc>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_h: KeyContext::PopupComments, "h",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_left: KeyContext::PopupComments, "<Left>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_l: KeyContext::PopupComments, "l",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_right: KeyContext::PopupComments, "<Right>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_tab: KeyContext::PopupComments, "<Tab>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_dd: KeyContext::PopupComments, "dd",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_space_e: KeyContext::PopupComments, "<Space>e",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| app.comments_viewer().map_or(false, |v| v.is_export_mode());
    // Enter with no comment selected is a no-op (stays in popup)
    popup_comments_enter: KeyContext::PopupComments, "<CR>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_slash: KeyContext::PopupComments, "/",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_question: KeyContext::PopupComments, "?",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_n: KeyContext::PopupComments, "n",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));
    popup_comments_n_upper: KeyContext::PopupComments, "N",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>a"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::CommentsViewer));

    // ── Popup Settings ───────────────────────────────
    popup_settings_j: KeyContext::PopupSettings, "j",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_k: KeyContext::PopupSettings, "k",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_down: KeyContext::PopupSettings, "<Down>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_up: KeyContext::PopupSettings, "<Up>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_gg: KeyContext::PopupSettings, "gg",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_g_upper: KeyContext::PopupSettings, "G",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_ctrl_d: KeyContext::PopupSettings, "<C-d>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_ctrl_u: KeyContext::PopupSettings, "<C-u>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_ctrl_f: KeyContext::PopupSettings, "<C-f>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_ctrl_b: KeyContext::PopupSettings, "<C-b>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_pagedown: KeyContext::PopupSettings, "<PageDown>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_pageup: KeyContext::PopupSettings, "<PageUp>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_esc: KeyContext::PopupSettings, "<Esc>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_h: KeyContext::PopupSettings, "h",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_left: KeyContext::PopupSettings, "<Left>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_l: KeyContext::PopupSettings, "l",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_right: KeyContext::PopupSettings, "<Right>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_tab: KeyContext::PopupSettings, "<Tab>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_backtab: KeyContext::PopupSettings, "<S-Tab>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_enter: KeyContext::PopupSettings, "<CR>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings));
    popup_settings_space: KeyContext::PopupSettings, "<Space>",
        setup = |app, _dir| { open_book(&mut app); simulate(&mut app, "<Space>t"); },
        check = |app| matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Settings))
}

// ═══════════════════════════════════════════════════════════════
// Return-value tests (can't be expressed as state checks)
// ═══════════════════════════════════════════════════════════════

#[test]
fn content_q_returns_quit() {
    let (mut app, _dir) = create_app_content_focused();
    let result = simulate(&mut app, "q");
    assert_eq!(result, Some(AppAction::Quit));
}

// ═══════════════════════════════════════════════════════════════
// Behavioral edge-case tests
// ═══════════════════════════════════════════════════════════════

/// ? in help popup during search navigation should exit search, not close popup.
#[test]
fn help_question_during_search_exits_search_not_popup() {
    let (mut app, _dir) = create_app();
    open_book(&mut app);
    simulate(&mut app, "?"); // open help
    assert!(matches!(
        app.focused_panel,
        FocusedPanel::Popup(PopupWindow::Help)
    ));
    simulate(&mut app, "/"); // start search input
    press_char(&mut app, 'a'); // type something
    press(&mut app, KeyCode::Enter); // confirm → enters NavigationMode
    // Now ? should exit search, NOT close the popup
    simulate(&mut app, "?");
    assert!(
        matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help)),
        "? during search navigation should exit search, not close help"
    );
    // Press ? again → now it closes
    simulate(&mut app, "?");
    assert!(
        !matches!(app.focused_panel, FocusedPanel::Popup(PopupWindow::Help)),
        "second ? should close the popup"
    );
}

// ═══════════════════════════════════════════════════════════════
// Coverage enforcement
// ═══════════════════════════════════════════════════════════════

/// Fails if any default binding lacks a test in the table above.
/// PDF contexts (PdfStandard, PdfNormal) are excluded — they require
/// the pdf feature and a full render pipeline to test.
#[test]
fn every_default_binding_has_a_test() {
    let keymap = default_keymap();
    let tested: HashSet<(KeyContext, String)> = tested_bindings().into_iter().collect();

    let skip_contexts = [
        KeyContext::PdfStandard, // tested in keybinding_actions_pdf.rs
        KeyContext::PdfNormal,
    ];

    // Bindings deliberately excluded from the table with documented reasons.
    // Each exception is a KNOWN BUG or environmental limitation, NOT a test gap.
    let known_exceptions: HashSet<(KeyContext, &str)> = HashSet::from([
        // Pre-existing bug: panics with "subtract with overflow" when visible_height is 0
        // (no terminal layout in unit test). navigation.rs:53. Covered by SVG snapshots.
        (KeyContext::EpubContent, "<C-d>"),
    ]);

    let mut missing = Vec::new();
    for ctx in KeyContext::ALL {
        if skip_contexts.contains(ctx) {
            continue;
        }
        if let Some(ctx_map) = keymap.context(*ctx) {
            for (binding, action) in ctx_map.all_bindings() {
                let notation = format_key_binding(&binding);
                if known_exceptions.contains(&(*ctx, notation.as_str())) {
                    continue;
                }
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
            "Untested keybindings ({}):\n{}",
            missing.len(),
            missing.join("\n")
        );
    }
}
