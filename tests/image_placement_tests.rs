use bookrat::test_utils::test_helpers::create_test_terminal;
use bookrat::App;
use crossterm::event::KeyCode;

// Import the necessary modules
mod snapshot_assertions;
mod svg_generation;
mod test_report;
use snapshot_assertions::assert_svg_snapshot;
use svg_generation::terminal_to_svg;

/// Test that the image is placed after the first paragraph
#[test]
fn test_image_placement_after_first_paragraph() {
    test_report::init_test_report();
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Navigate to a chapter with content
    app.press_key(KeyCode::Enter); // Select first book
    app.press_key(KeyCode::Enter); // Select first chapter

    // Let the app render
    terminal.draw(|f| app.draw(f)).unwrap();

    // Convert to SVG and check snapshot
    let svg = terminal_to_svg(&terminal);
    assert_svg_snapshot(
        svg,
        &std::path::Path::new("tests/snapshots/image_after_first_paragraph.svg"),
        "test_image_placement_after_first_paragraph",
        |expected, actual, path, exp_lines, act_lines, diff_count, first_diff| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_image_placement_after_first_paragraph".to_string(),
                expected,
                actual,
                snapshot_path: path,
                line_stats: test_report::LineStats {
                    expected_lines: exp_lines,
                    actual_lines: act_lines,
                    diff_count,
                    first_diff_line: first_diff,
                },
            });
        },
    );
}

/// Test that the image is exactly 10 cells high in the rendered output
#[test]
fn test_image_height_exactly_10_cells() {
    test_report::init_test_report();
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Navigate to content
    app.press_key(KeyCode::Enter); // Select book
    app.press_key(KeyCode::Enter); // Select chapter

    // Render
    terminal.draw(|f| app.draw(f)).unwrap();

    // The snapshot should show the image taking exactly 15 lines
    let svg = terminal_to_svg(&terminal);
    assert_svg_snapshot(
        svg,
        &std::path::Path::new("tests/snapshots/image_height_10_cells.svg"),
        "test_image_height_exactly_10_cells",
        |expected, actual, path, exp_lines, act_lines, diff_count, first_diff| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_image_height_exactly_10_cells".to_string(),
                expected,
                actual,
                snapshot_path: path,
                line_stats: test_report::LineStats {
                    expected_lines: exp_lines,
                    actual_lines: act_lines,
                    diff_count,
                    first_diff_line: first_diff,
                },
            });
        },
    );
}

/// Test scrolling behavior with the image
#[test]
fn test_image_scrolling_behavior() {
    test_report::init_test_report();
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Navigate to content
    app.press_key(KeyCode::Enter);
    app.press_key(KeyCode::Enter);

    // Initial state - image should be visible
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_initial = terminal_to_svg(&terminal);
    assert_svg_snapshot(
        svg_initial,
        &std::path::Path::new("tests/snapshots/image_scrolling_initial.svg"),
        "test_image_scrolling_behavior_initial",
        |expected, actual, path, exp_lines, act_lines, diff_count, first_diff| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_image_scrolling_behavior_initial".to_string(),
                expected,
                actual,
                snapshot_path: path,
                line_stats: test_report::LineStats {
                    expected_lines: exp_lines,
                    actual_lines: act_lines,
                    diff_count,
                    first_diff_line: first_diff,
                },
            });
        },
    );

    // Scroll down a few times
    for _ in 0..5 {
        app.press_key(KeyCode::Down);
    }
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_partial = terminal_to_svg(&terminal);
    assert_svg_snapshot(
        svg_partial,
        &std::path::Path::new("tests/snapshots/image_scrolling_partial.svg"),
        "test_image_scrolling_behavior_partial",
        |expected, actual, path, exp_lines, act_lines, diff_count, first_diff| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_image_scrolling_behavior_partial".to_string(),
                expected,
                actual,
                snapshot_path: path,
                line_stats: test_report::LineStats {
                    expected_lines: exp_lines,
                    actual_lines: act_lines,
                    diff_count,
                    first_diff_line: first_diff,
                },
            });
        },
    );

    // Scroll down more until image is hidden
    for _ in 0..10 {
        app.press_key(KeyCode::Down);
    }
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_hidden = terminal_to_svg(&terminal);
    assert_svg_snapshot(
        svg_hidden,
        &std::path::Path::new("tests/snapshots/image_scrolling_hidden.svg"),
        "test_image_scrolling_behavior_hidden",
        |expected, actual, path, exp_lines, act_lines, diff_count, first_diff| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_image_scrolling_behavior_hidden".to_string(),
                expected,
                actual,
                snapshot_path: path,
                line_stats: test_report::LineStats {
                    expected_lines: exp_lines,
                    actual_lines: act_lines,
                    diff_count,
                    first_diff_line: first_diff,
                },
            });
        },
    );

    // Scroll back up to see image reappear
    for _ in 0..10 {
        app.press_key(KeyCode::Up);
    }
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_reappear = terminal_to_svg(&terminal);
    assert_svg_snapshot(
        svg_reappear,
        &std::path::Path::new("tests/snapshots/image_scrolling_reappear.svg"),
        "test_image_scrolling_behavior_reappear",
        |expected, actual, path, exp_lines, act_lines, diff_count, first_diff| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_image_scrolling_behavior_reappear".to_string(),
                expected,
                actual,
                snapshot_path: path,
                line_stats: test_report::LineStats {
                    expected_lines: exp_lines,
                    actual_lines: act_lines,
                    diff_count,
                    first_diff_line: first_diff,
                },
            });
        },
    );
}

/// Helper trait for simpler key event handling in tests
trait TestKeyEventHandler {
    fn press_key(&mut self, key: KeyCode);
}

impl TestKeyEventHandler for App {
    fn press_key(&mut self, key: KeyCode) {
        self.handle_key_event(crossterm::event::KeyEvent {
            code: key,
            modifiers: crossterm::event::KeyModifiers::empty(),
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
    }
}
