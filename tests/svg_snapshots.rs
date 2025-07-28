use bookrat::test_utils::test_helpers::create_test_terminal;
// SVG snapshot tests using snapbox
use bookrat::App;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

mod snapshot_assertions;
mod test_report;
mod visual_diff;
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
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
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
        },
    );
}

#[test]
fn test_content_view_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Switch to content view

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_content_view.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/content_view.svg"),
        "test_content_view_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
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
        },
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
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
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
        },
    );
}

#[test]
fn test_chapter_title_normal_length_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the 7-chapter test book to get chapter with title
    if let Some(book_info) = app.book_manager.get_book_info(1) {
        let path = book_info.path.clone();
        app.load_epub(&path);
        // Switch to content focus like runtime behavior after loading
        app.focused_panel = bookrat::main_app::FocusedPanel::Content;
        // Force animation to complete for testing
    }

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_chapter_title_normal.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/chapter_title_normal_length.svg"),
        "test_chapter_title_normal_length_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            // Add to test report
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_chapter_title_normal_length_svg".to_string(),
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
        },
    );
}

#[test]
fn test_chapter_title_narrow_terminal_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(50, 24); // Narrow terminal
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the 7-chapter test book to get chapter with title
    if let Some(book_info) = app.book_manager.get_book_info(1) {
        let path = book_info.path.clone();
        app.load_epub(&path);
        // Force animation to complete for testing
    }

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_chapter_title_narrow.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/chapter_title_narrow_terminal.svg"),
        "test_chapter_title_narrow_terminal_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            // Add to test report
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_chapter_title_narrow_terminal_svg".to_string(),
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
        },
    );
}

#[test]
fn test_chapter_title_no_title_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the digital frontier book (which may not have chapter titles)
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
        // Force animation to complete for testing
    }

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_chapter_title_no_title.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/chapter_title_no_title.svg"),
        "test_chapter_title_no_title_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            // Add to test report
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_chapter_title_no_title_svg".to_string(),
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
        },
    );
}

#[test]
fn test_mouse_scroll_file_list_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Ensure we're in file list mode

    // Simulate mouse scroll down in file list - should move selection down
    let mouse_event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 40,
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply mouse scroll event in file list
    app.handle_mouse_event(mouse_event);

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_mouse_scroll_file_list.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/mouse_scroll_file_list.svg"),
        "test_mouse_scroll_file_list_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_mouse_scroll_file_list_svg".to_string(),
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
        },
    );
}

#[test]
fn test_mouse_scroll_bounds_checking_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // Scroll to the bottom first using keyboard
    for _ in 0..50 {
        app.scroll_down();
    }

    // Now try excessive mouse scrolling at the bottom - this used to cause CPU spike
    let mouse_event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 50,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply many scroll down events to test bounds checking
    for _ in 0..20 {
        app.handle_mouse_event(mouse_event);
    }

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_mouse_bounds_check.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/mouse_scroll_bounds_checking.svg"),
        "test_mouse_scroll_bounds_checking_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_mouse_scroll_bounds_checking_svg".to_string(),
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
        },
    );
}

#[test]
fn test_mouse_event_batching_svg() {
    use bookrat::event_source::{EventSource, SimulatedEventSource};

    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // Create a simulated event source with many rapid scroll events
    let events = vec![
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
    ];

    let mut event_source = SimulatedEventSource::new(events);

    // Test batching - read first event and let it batch the rest
    if event_source
        .poll(std::time::Duration::from_millis(0))
        .unwrap()
    {
        let first_event = event_source.read().unwrap();
        if let crossterm::event::Event::Mouse(mouse_event) = first_event {
            app.handle_mouse_event_with_batching(mouse_event, &mut event_source);
        }
    }

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_mouse_batching.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/mouse_event_batching.svg"),
        "test_mouse_event_batching_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_mouse_event_batching_svg".to_string(),
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
        },
    );
}

#[test]
fn test_horizontal_scroll_handling_svg() {
    use bookrat::event_source::{EventSource, SimulatedEventSource};

    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // Create a simulated event source with many rapid horizontal scroll events
    // This simulates the "5 log scrolls" that cause freezing
    let events = vec![
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
    ];

    let mut event_source = SimulatedEventSource::new(events);

    // Test horizontal scroll handling - should not cause freezing
    while event_source
        .poll(std::time::Duration::from_millis(0))
        .unwrap()
    {
        let event = event_source.read().unwrap();
        if let crossterm::event::Event::Mouse(mouse_event) = event {
            app.handle_mouse_event_with_batching(mouse_event, &mut event_source);
        }
    }

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_horizontal_scroll.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/horizontal_scroll_handling.svg"),
        "test_horizontal_scroll_handling_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_horizontal_scroll_handling_svg".to_string(),
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
        },
    );
}

#[test]
fn test_edge_case_mouse_coordinates_svg() {
    use bookrat::event_source::{EventSource, SimulatedEventSource};

    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // Create a simulated event source with edge case coordinates that would trigger crossterm overflow bug
    let events = vec![
        // Edge case coordinates that trigger the crossterm overflow bug
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 0, // This causes the overflow in crossterm
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 50,
            row: 0, // This also causes the overflow in crossterm
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollLeft,
            column: 65535, // Max u16 value
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
        // Valid coordinates that should work
        crossterm::event::Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 50,
            row: 15,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }),
    ];

    let mut event_source = SimulatedEventSource::new(events);

    // Test edge case coordinate handling - should not panic or freeze
    while event_source
        .poll(std::time::Duration::from_millis(0))
        .unwrap()
    {
        let event = event_source.read().unwrap();
        if let crossterm::event::Event::Mouse(mouse_event) = event {
            app.handle_mouse_event_with_batching(mouse_event, &mut event_source);
        }
    }

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_edge_case_coordinates.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/edge_case_mouse_coordinates.svg"),
        "test_edge_case_mouse_coordinates_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_edge_case_mouse_coordinates_svg".to_string(),
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
        },
    );
}

#[test]
fn test_text_selection_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();

    // Simulate text selection: mouse down, drag, mouse up
    // Use coordinates starting from the left margin to test margin selection
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10, // Click on left margin - should start from beginning of line
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 70, // Drag to select text
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 70,
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply the mouse events
    app.handle_mouse_event(mouse_down);
    app.handle_mouse_event(mouse_drag);
    app.handle_mouse_event(mouse_up);

    // Redraw to show the selection
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_text_selection.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/text_selection.svg"),
        "test_text_selection_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_text_selection_svg".to_string(),
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
        },
    );
}

#[test]
fn test_text_selection_with_auto_scroll_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Drag beyond the bottom of the content area to trigger auto-scroll
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 60,
        row: 35,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply the mouse events to test auto-scroll
    app.handle_mouse_event(mouse_down);
    app.handle_mouse_event(mouse_drag_beyond_bottom);
    app.handle_mouse_event(mouse_up);

    // Redraw to show the selection and scroll state
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_text_selection_auto_scroll.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/text_selection_auto_scroll.svg"),
        "test_text_selection_with_auto_scroll_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_text_selection_with_auto_scroll_svg".to_string(),
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
        },
    );
}

#[test]
fn test_continuous_auto_scroll_down_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();
    let initial_scroll_offset = app.get_scroll_offset();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_down);

    // Simulate continuous dragging beyond bottom - should keep scrolling
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply multiple drag events to simulate continuous scrolling
    let mut scroll_offsets = Vec::new();
    for i in 0..10 {
        app.handle_mouse_event(mouse_drag_beyond_bottom);
        scroll_offsets.push(app.get_scroll_offset());
        // Each drag should continue scrolling until we hit the bottom
        if i > 0 {
            // Verify that scrolling continues (offset increases or stays at max)
            assert!(
                scroll_offsets[i] >= scroll_offsets[i - 1],
                "Auto-scroll stopped prematurely at iteration {}: offset {} -> {}",
                i,
                scroll_offsets[i - 1],
                scroll_offsets[i]
            );
        }
    }

    // The scroll offset should have increased significantly from initial
    assert!(
        app.get_scroll_offset() > initial_scroll_offset,
        "Auto-scroll should have moved from initial offset {} to {}",
        initial_scroll_offset,
        app.get_scroll_offset()
    );

    // End selection
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 60,
        row: 35,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_up);

    // Redraw to show final state
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_continuous_auto_scroll_down.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/continuous_auto_scroll_down.svg"),
        "test_continuous_auto_scroll_down_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_continuous_auto_scroll_down_svg".to_string(),
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
        },
    );
}

#[test]
fn test_continuous_auto_scroll_up_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();

    // Scroll down first to create room for upward auto-scroll
    // Only scroll a small amount to ensure we don't hit max
    for _ in 0..3 {
        app.scroll_down();
    }
    let initial_scroll_offset = app.get_scroll_offset();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_down);

    // Simulate continuous dragging above top - should keep scrolling up
    let mouse_drag_above_top = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 0, // Definitely above the content area (top of terminal)
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply multiple drag events to simulate continuous scrolling
    let mut scroll_offsets = Vec::new();
    for i in 0..10 {
        app.handle_mouse_event(mouse_drag_above_top);
        scroll_offsets.push(app.get_scroll_offset());
        // Each drag should continue scrolling until we hit the top
        if i > 0 {
            // Verify that scrolling continues (offset decreases or stays at 0)
            assert!(
                scroll_offsets[i] <= scroll_offsets[i - 1],
                "Auto-scroll up stopped prematurely at iteration {}: offset {} -> {}",
                i,
                scroll_offsets[i - 1],
                scroll_offsets[i]
            );
        }
    }

    // The scroll offset should have decreased significantly from initial
    assert!(
        app.get_scroll_offset() < initial_scroll_offset,
        "Auto-scroll up should have moved from initial offset {} to {}",
        initial_scroll_offset,
        app.get_scroll_offset()
    );

    // End selection
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 60,
        row: 2,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_up);

    // Redraw to show final state
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_continuous_auto_scroll_up.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/continuous_auto_scroll_up.svg"),
        "test_continuous_auto_scroll_up_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_continuous_auto_scroll_up_svg".to_string(),
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
        },
    );
}

#[test]
fn test_timer_based_auto_scroll_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();
    let initial_scroll_offset = app.get_scroll_offset();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_down);

    // Drag beyond bottom ONCE (simulating user holding mouse in position)
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_drag_beyond_bottom);

    // Now simulate multiple draw calls (which trigger auto-scroll updates)
    // This simulates the real-world scenario where the user holds the mouse
    // outside the content area and the auto-scroll timer continues scrolling
    let mut scroll_offsets = Vec::new();
    for i in 0..10 {
        // Simulate a redraw happening (which calls update_auto_scroll)
        terminal.draw(|f| app.draw(f)).unwrap();
        scroll_offsets.push(app.get_scroll_offset());

        // Add a small delay to ensure the timer can trigger
        std::thread::sleep(std::time::Duration::from_millis(110));
    }

    // Verify that scrolling continued automatically without additional mouse events
    let final_scroll_offset = app.get_scroll_offset();
    assert!(
        final_scroll_offset > initial_scroll_offset,
        "Timer-based auto-scroll should have moved from initial offset {} to {}",
        initial_scroll_offset,
        final_scroll_offset
    );

    // Verify progressive scrolling occurred
    for i in 1..scroll_offsets.len() {
        assert!(
            scroll_offsets[i] >= scroll_offsets[i - 1],
            "Auto-scroll should continue progressing: iteration {} went from {} to {}",
            i,
            scroll_offsets[i - 1],
            scroll_offsets[i]
        );
    }

    // End selection
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 60,
        row: 35,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_up);

    // Final redraw
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_timer_based_auto_scroll.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/timer_based_auto_scroll.svg"),
        "test_timer_based_auto_scroll_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_timer_based_auto_scroll_svg".to_string(),
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
        },
    );
}

#[test]
fn test_auto_scroll_stops_when_cursor_returns_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_down);

    // Drag beyond bottom to trigger auto-scroll
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_drag_beyond_bottom);
    let scroll_after_auto = app.get_scroll_offset();

    // Move cursor back to within content area - auto-scroll should stop
    let mouse_drag_back_in_area = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 70,
        row: 20, // Back within content area
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_drag_back_in_area);
    let scroll_after_return = app.get_scroll_offset();

    // Scroll should stop when cursor returns to content area
    assert_eq!(
        scroll_after_auto, scroll_after_return,
        "Auto-scroll should stop when cursor returns to content area"
    );

    // Another drag within area should not cause more scrolling
    let mouse_drag_within_area = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 80,
        row: 25, // Still within content area
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_drag_within_area);
    let final_scroll = app.get_scroll_offset();

    assert_eq!(
        scroll_after_return, final_scroll,
        "No additional scrolling should occur when dragging within content area"
    );

    // End selection
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 80,
        row: 25,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_up);

    // Redraw to show final state
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_auto_scroll_cursor_return.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/auto_scroll_stops_when_cursor_returns.svg"),
        "test_auto_scroll_stops_when_cursor_returns_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_auto_scroll_stops_when_cursor_returns_svg".to_string(),
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
        },
    );
}

#[test]
fn test_double_click_word_selection_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();

    // Simulate double-click to select a word
    // Click on a word in the middle of the content
    let mouse_click1 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45, // Click on a word
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_up1 = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 45,
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_click2 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45, // Second click on same position
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_up2 = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 45,
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply the double-click sequence
    app.handle_mouse_event(mouse_click1);
    app.handle_mouse_event(mouse_up1);
    app.handle_mouse_event(mouse_click2);
    app.handle_mouse_event(mouse_up2);

    // Redraw to show the word selection
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_double_click_word_selection.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/double_click_word_selection.svg"),
        "test_double_click_word_selection_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_double_click_word_selection_svg".to_string(),
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
        },
    );
}

#[test]
fn test_triple_click_paragraph_selection_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // First draw to initialize the content area
    terminal.draw(|f| app.draw(f)).unwrap();

    // Simulate triple-click to select a paragraph
    // Click on a paragraph in the middle of the content
    let mouse_click1 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50, // Click on a paragraph
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_up1 = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 50,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_click2 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50, // Second click on same position
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_up2 = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 50,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_click3 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50, // Third click on same position
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let mouse_up3 = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 50,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Apply the triple-click sequence
    app.handle_mouse_event(mouse_click1);
    app.handle_mouse_event(mouse_up1);
    app.handle_mouse_event(mouse_click2);
    app.handle_mouse_event(mouse_up2);
    app.handle_mouse_event(mouse_click3);
    app.handle_mouse_event(mouse_up3);

    // Redraw to show the paragraph selection
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_triple_click_paragraph_selection.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/triple_click_paragraph_selection.svg"),
        "test_triple_click_paragraph_selection_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_triple_click_paragraph_selection_svg".to_string(),
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
        },
    );
}

#[test]
fn test_text_selection_click_on_book_text_bug_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and ensure we're in content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        app.load_epub(&path);
    }

    // Ensure content panel has focus
    app.focused_panel = bookrat::main_app::FocusedPanel::Content;

    // Draw initial state
    terminal.draw(|f| app.draw(f)).unwrap();

    // Now simulate clicking on book text in the content area
    // According to the bug report: "when i click on a book text: nothing got selected,
    // but the status bar shows as if we are in text selection mode"
    let mouse_click_on_text = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50, // Click on book text in content area
        row: 12,    // Where book text should be displayed
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_click_on_text);

    // Complete the click with mouse up
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 50,
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_up);

    // Draw to see the current state
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_text_selection_click_on_book_text_bug.svg",
        &svg_output,
    )
    .unwrap();

    // This test should capture the bug: if the status bar shows text selection mode
    // but no actual text is selected, we'll see it in the snapshot
    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/text_selection_click_on_book_text_bug.svg"),
        "test_text_selection_click_on_book_text_bug_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_text_selection_click_on_book_text_bug_svg".to_string(),
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
        },
    );
}
