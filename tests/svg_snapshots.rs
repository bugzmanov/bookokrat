use bookrat::test_utils::test_helpers::create_test_terminal;
// SVG snapshot tests using snapbox
use bookrat::{App, Mode};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

mod visual_diff;
mod snapshot_assertions;
mod test_report;
use snapshot_assertions::assert_svg_snapshot;
use std::sync::Once;

static INIT: Once = Once::new();

fn ensure_test_report_initialized() {
    INIT.call_once(|| {
        test_report::init_test_report();
    });
}

// Convert terminal to SVG (directly in test file to access anstyle_svg)
fn terminal_to_svg(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut ansi_output = String::new();

    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = buffer.get(x, y);

            // Add ANSI escape codes for styling
            let mut styled_char = String::new();

            // Reset first
            styled_char.push_str("\u{1b}[0m");

            // Add colors
            if cell.fg != ratatui::style::Color::Reset {
                styled_char.push_str(&format_color(cell.fg, true));
            }
            if cell.bg != ratatui::style::Color::Reset {
                styled_char.push_str(&format_color(cell.bg, false));
            }

            // Add modifiers
            if cell.modifier.contains(ratatui::style::Modifier::BOLD) {
                styled_char.push_str("\u{1b}[1m");
            }
            if cell.modifier.contains(ratatui::style::Modifier::ITALIC) {
                styled_char.push_str("\u{1b}[3m");
            }
            if cell.modifier.contains(ratatui::style::Modifier::UNDERLINED) {
                styled_char.push_str("\u{1b}[4m");
            }

            // Add the character
            styled_char.push_str(&cell.symbol());

            ansi_output.push_str(&styled_char);
        }

        // Add newline and reset at end of line
        if y < buffer.area.height - 1 {
            ansi_output.push_str("\u{1b}[0m\n");
        }
    }

    // Final reset
    ansi_output.push_str("\u{1b}[0m");

    // Convert ANSI to SVG
    let term = anstyle_svg::Term::new();
    term.render_svg(&ansi_output)
}

fn format_color(color: ratatui::style::Color, is_foreground: bool) -> String {
    use ratatui::style::Color;

    let base = if is_foreground { 30 } else { 40 };

    match color {
        Color::Reset => "\u{1b}[0m".to_string(),
        Color::Black => format!("\u{1b}[{}m", base),
        Color::Red => format!("\u{1b}[{}m", base + 1),
        Color::Green => format!("\u{1b}[{}m", base + 2),
        Color::Yellow => format!("\u{1b}[{}m", base + 3),
        Color::Blue => format!("\u{1b}[{}m", base + 4),
        Color::Magenta => format!("\u{1b}[{}m", base + 5),
        Color::Cyan => format!("\u{1b}[{}m", base + 6),
        Color::Gray => format!("\u{1b}[{}m", base + 7),
        Color::DarkGray => format!("\u{1b}[{}m", base + 60),
        Color::LightRed => format!("\u{1b}[{}m", base + 61),
        Color::LightGreen => format!("\u{1b}[{}m", base + 62),
        Color::LightYellow => format!("\u{1b}[{}m", base + 63),
        Color::LightBlue => format!("\u{1b}[{}m", base + 64),
        Color::LightMagenta => format!("\u{1b}[{}m", base + 65),
        Color::LightCyan => format!("\u{1b}[{}m", base + 66),
        Color::White => format!("\u{1b}[{}m", base + 67),
        Color::Rgb(r, g, b) => {
            if is_foreground {
                format!("\u{1b}[38;2;{};{};{}m", r, g, b)
            } else {
                format!("\u{1b}[48;2;{};{};{}m", r, g, b)
            }
        }
        Color::Indexed(idx) => {
            if is_foreground {
                format!("\u{1b}[38;5;{}m", idx)
            } else {
                format!("\u{1b}[48;5;{}m", idx)
            }
        }
    }
}

#[test]
fn test_file_list_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_file_list.svg", &svg_output).unwrap();

    // Use our custom assertion with visual diff
    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/file_list.svg"),
        "test_file_list_svg",
        |expected, actual, snapshot_path, expected_lines, actual_lines, diff_count, first_diff_line| {
            // Add to test report
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_file_list_svg".to_string(),
                expected,
                actual,
                line_stats: test_report::LineStats {
                    expected_lines,
                    actual_lines,
                    diff_count,
                    first_diff_line,
                },
                snapshot_path,
            });
        }
    );
}

#[test]
fn test_content_view_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Switch to content view
    app.mode = Mode::Content;
    app.animation_progress = 1.0;

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_content_view.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/content_view.svg"),
        "test_content_view_svg",
        |expected, actual, snapshot_path, expected_lines, actual_lines, diff_count, first_diff_line| {
            // Add to test report
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_content_view_svg".to_string(),
                expected,
                actual,
                line_stats: test_report::LineStats {
                    expected_lines,
                    actual_lines,
                    diff_count,
                    first_diff_line,
                },
                snapshot_path,
            });
        }
    );
}

#[test]
fn test_content_scrolling_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
        // Force animation to complete for testing
        app.animation_progress = 1.0;
    }

    // Perform scrolling - 5 lines down
    for _ in 0..5 {
        app.scroll_down();
    }

    // Then half-screen scroll
    let visible_height = terminal.size().unwrap().height.saturating_sub(5) as usize;
    app.scroll_half_screen_down(visible_height);

    // Draw the final state
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_content_scrolling.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/content_scrolling.svg"),
        "test_content_scrolling_svg",
        |expected, actual, snapshot_path, expected_lines, actual_lines, diff_count, first_diff_line| {
            // Add to test report
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_content_scrolling_svg".to_string(),
                expected,
                actual,
                line_stats: test_report::LineStats {
                    expected_lines,
                    actual_lines,
                    diff_count,
                    first_diff_line,
                },
                snapshot_path,
            });
        }
    );
}
