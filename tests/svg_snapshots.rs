use bookrat::simple_fake_books::FakeBookConfig;
use bookrat::test_utils::test_helpers::{
    create_test_app_with_custom_fake_books, create_test_terminal,
};
// SVG snapshot tests using snapbox
use bookrat::App;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

mod snapshot_assertions;
mod svg_generation;
mod test_report;
mod visual_diff;
use snapshot_assertions::assert_svg_snapshot;
use std::sync::Once;
use svg_generation::terminal_to_svg;

static INIT: Once = Once::new();

fn ensure_test_report_initialized() {
    INIT.call_once(|| {
        test_report::init_test_report();
    });
}

/// Helper trait for simpler key event handling in tests
trait TestKeyEventHandler {
    fn press_key(&mut self, key: crossterm::event::KeyCode);
    fn press_keys(&mut self, keys: &[crossterm::event::KeyCode]);
    fn press_char_times(&mut self, ch: char, times: usize);
    fn press_sequence(&mut self, sequence: &[KeyAction]);
}

/// Represents different types of key press actions for test sequences
#[derive(Clone)]
enum KeyAction {
    /// Single key press
    Key(crossterm::event::KeyCode),
    /// Character repeated multiple times
    CharTimes(char, usize),
    /// Multiple key presses
    Keys(Vec<crossterm::event::KeyCode>),
}

impl TestKeyEventHandler for App {
    fn press_key(&mut self, key: crossterm::event::KeyCode) {
        self.handle_key_event(crossterm::event::KeyEvent {
            code: key,
            modifiers: crossterm::event::KeyModifiers::empty(),
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
    }

    fn press_keys(&mut self, keys: &[crossterm::event::KeyCode]) {
        for key in keys {
            self.press_key(*key);
        }
    }

    fn press_char_times(&mut self, ch: char, times: usize) {
        for _ in 0..times {
            self.press_key(crossterm::event::KeyCode::Char(ch));
        }
    }

    /// Execute a sequence of different key actions
    fn press_sequence(&mut self, sequence: &[KeyAction]) {
        for action in sequence {
            match action {
                KeyAction::Key(key) => self.press_key(*key),
                KeyAction::CharTimes(ch, times) => self.press_char_times(*ch, *times),
                KeyAction::Keys(keys) => self.press_keys(keys),
            }
        }
    }
}

/// Helper function to create standard test failure handler
fn create_test_failure_handler(
    test_name: &str,
) -> impl FnOnce(String, String, String, usize, usize, usize, Option<usize>) + '_ {
    move |expected,
          actual,
          snapshot_path,
          expected_lines,
          actual_lines,
          diff_count,
          first_diff_line| {
        test_report::TestReport::add_failure(test_report::TestFailure {
            test_name: test_name.to_string(),
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
}

#[test]
fn test_fake_books_file_list_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(80, 24);

    // Test setup constants - make the test parameters visible
    const DIGITAL_FRONTIER_CHAPTERS: usize = 33;

    // Create test books with explicit configuration
    let book_configs = vec![
        FakeBookConfig {
            title: "Digital Frontier".to_string(),
            chapter_count: DIGITAL_FRONTIER_CHAPTERS,
            words_per_chapter: 150,
        },
        FakeBookConfig {
            title: "Seven Chapter Book".to_string(),
            chapter_count: 7,
            words_per_chapter: 200,
        },
    ];

    let (mut app, _temp_manager) = create_test_app_with_custom_fake_books(&book_configs);

    app.press_key(crossterm::event::KeyCode::Enter); // Select first book (Digital Frontier)
    app.press_key(crossterm::event::KeyCode::Tab); // Switch to content view

    app.press_char_times('j', DIGITAL_FRONTIER_CHAPTERS + 1);

    app.press_key(crossterm::event::KeyCode::Enter); // Select first book (Digital Frontier)

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_fake_books_file_list.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/fake_books_file_list.svg"),
        "test_fake_books_file_list_svg",
        create_test_failure_handler("test_fake_books_file_list_svg"),
    );
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

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/file_list.svg"),
        "test_file_list_svg",
        create_test_failure_handler("test_file_list_svg"),
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
        create_test_failure_handler("test_content_view_svg"),
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
        let _ = app.open_book_for_reading_by_path(&path);
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
        let _ = app.open_book_for_reading_by_path(&path);
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
        let _ = app.open_book_for_reading_by_path(&path);
    }

    app.press_key(crossterm::event::KeyCode::Tab); // Switch to content view

    app.press_char_times('j', 1);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_event, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
        app.handle_mouse_event(mouse_event, None);
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
        let _ = app.open_book_for_reading_by_path(&path);
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
            app.handle_mouse_event(mouse_event, Some(&mut event_source));
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
        let _ = app.open_book_for_reading_by_path(&path);
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
            app.handle_mouse_event(mouse_event, Some(&mut event_source));
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
        let _ = app.open_book_for_reading_by_path(&path);
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
            app.handle_mouse_event(mouse_event, Some(&mut event_source));
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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_down, None);
    app.handle_mouse_event(mouse_drag, None);
    app.handle_mouse_event(mouse_up, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_down, None);
    app.handle_mouse_event(mouse_drag_beyond_bottom, None);
    app.handle_mouse_event(mouse_up, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_down, None);

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
        app.handle_mouse_event(mouse_drag_beyond_bottom, None);
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
    app.handle_mouse_event(mouse_up, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_down, None);

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
        app.handle_mouse_event(mouse_drag_above_top, None);
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
    app.handle_mouse_event(mouse_up, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_down, None);

    // Drag beyond bottom ONCE (simulating user holding mouse in position)
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_drag_beyond_bottom, None);

    // Now simulate multiple draw calls (which trigger auto-scroll updates)
    // This simulates the real-world scenario where the user holds the mouse
    // outside the content area and the auto-scroll timer continues scrolling
    let mut scroll_offsets = Vec::new();
    for _i in 0..10 {
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
    app.handle_mouse_event(mouse_up, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_down, None);

    // Drag beyond bottom to trigger auto-scroll
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_drag_beyond_bottom, None);
    let scroll_after_auto = app.get_scroll_offset();

    // Move cursor back to within content area - auto-scroll should stop
    let mouse_drag_back_in_area = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 70,
        row: 20, // Back within content area
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_drag_back_in_area, None);
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
    app.handle_mouse_event(mouse_drag_within_area, None);
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
    app.handle_mouse_event(mouse_up, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_click1, None);
    app.handle_mouse_event(mouse_up1, None);
    app.handle_mouse_event(mouse_click2, None);
    app.handle_mouse_event(mouse_up2, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_click1, None);
    app.handle_mouse_event(mouse_up1, None);
    app.handle_mouse_event(mouse_click2, None);
    app.handle_mouse_event(mouse_up2, None);
    app.handle_mouse_event(mouse_click3, None);
    app.handle_mouse_event(mouse_up3, None);

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
        let _ = app.open_book_for_reading_by_path(&path);
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
    app.handle_mouse_event(mouse_click_on_text, None);

    // Complete the click with mouse up
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 50,
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_mouse_event(mouse_up, None);

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

#[test]
fn test_toc_navigation_bug_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load a book that has hierarchical TOC structure
    if let Some(book_info) = app.book_manager.get_book_info(1) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    // Start with file list panel focused to show the TOC
    app.focused_panel = bookrat::main_app::FocusedPanel::FileList;

    // Draw initial state - should show book with expanded TOC
    terminal.draw(|f| app.draw(f)).unwrap();

    // Simulate pressing 'j' key 4 times to navigate down through TOC items
    app.press_char_times('j', 4);

    // Draw the state after navigation
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_toc_navigation_bug.svg", &svg_output).unwrap();

    // This test captures the TOC navigation bug:
    // When a book is loaded with TOC visible in the left panel,
    // the user should be able to navigate through the TOC items with j/k keys
    // and select specific chapters with Enter key.
    // Currently, only book selection works, not individual chapter selection.
    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/toc_navigation_bug.svg"),
        "test_toc_navigation_bug_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_toc_navigation_bug_svg".to_string(),
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
fn test_book_list_to_toc_transition_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Start with book list
    app.focused_panel = bookrat::main_app::FocusedPanel::FileList;
    terminal.draw(|f| app.draw(f)).unwrap();

    // Select a book and open it - this should transition to TOC mode
    app.press_key(crossterm::event::KeyCode::Enter);

    // Draw the state after transition - should show TOC
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_book_list_to_toc_transition.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/book_list_to_toc_transition.svg"),
        "test_book_list_to_toc_transition_svg",
        create_test_failure_handler("test_book_list_to_toc_transition_svg"),
    );
}

#[test]
fn test_toc_back_to_books_list_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load a book to enter TOC mode
    app.press_key(crossterm::event::KeyCode::Enter);

    // Navigate to "<< Books List" (first item)
    // Since we're already at the top, just press Enter
    app.press_key(crossterm::event::KeyCode::Enter);

    // Draw the state - should be back to book list with the open book highlighted in red
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_toc_back_to_books_list.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/toc_back_to_books_list.svg"),
        "test_toc_back_to_books_list_svg",
        create_test_failure_handler("test_toc_back_to_books_list_svg"),
    );
}

#[test]
fn test_toc_chapter_navigation_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load a book to enter TOC mode
    app.press_key(crossterm::event::KeyCode::Enter);

    // Navigate down to a chapter (skip "<< Books List")
    app.press_char_times('j', 3); // Move to 3rd chapter

    // Select the chapter
    app.press_key(crossterm::event::KeyCode::Enter);

    // Draw the state - should show content view with the selected chapter
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_toc_chapter_navigation.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/toc_chapter_navigation.svg"),
        "test_toc_chapter_navigation_svg",
        create_test_failure_handler("test_toc_chapter_navigation_svg"),
    );
}

#[test]
fn test_book_reading_history_with_many_entries_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(120, 40); // Larger terminal for better visibility

    // Create app with custom fake books - 120 books for reading history
    let mut book_configs = Vec::new();
    for i in 0..120 {
        book_configs.push(FakeBookConfig {
            title: format!(
                "Book {} - {}",
                i + 1,
                match i % 10 {
                    0 => "Science Fiction Classic",
                    1 => "Mystery Thriller",
                    2 => "Fantasy Epic",
                    3 => "Historical Fiction",
                    4 => "Biography",
                    5 => "Technical Manual",
                    6 => "Romance Novel",
                    7 => "Horror Story",
                    8 => "Philosophy Text",
                    _ => "Adventure Tale",
                }
            ),
            chapter_count: 10 + (i % 20), // Varying chapter counts
            words_per_chapter: 1000,
        });
    }

    // Create a temporary bookmark file for this test
    let temp_dir = tempfile::tempdir().unwrap();
    let bookmark_path = temp_dir.path().join("test_bookmarks.json");

    // Create app with real bookmark file
    let temp_manager =
        bookrat::test_utils::test_helpers::TempBookManager::new_with_configs(&book_configs)
            .expect("Failed to create temp books");

    let mut app = bookrat::App::new_with_config(
        Some(&temp_manager.get_directory()),
        Some(&bookmark_path.to_string_lossy()),
        false,
    );

    // Create bookmarks with interesting dates to show sorting
    // We'll manually create a bookmarks file with specific timestamps
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use std::collections::HashMap;

    #[derive(serde::Serialize)]
    struct TestBookmark {
        chapter: usize,
        scroll_offset: usize,
        last_read: DateTime<Utc>,
        total_chapters: usize,
    }

    #[derive(serde::Serialize)]
    struct TestBookmarks {
        books: HashMap<String, TestBookmark>,
    }

    let mut bookmarks = TestBookmarks {
        books: HashMap::new(),
    };

    // Use a fixed date for deterministic test output
    let now = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();

    // Add books read today (most recent - should appear at top)
    for i in 0..10 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: i * 2, // Varying progress
                scroll_offset: 0,
                last_read: now - Duration::hours(i as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Add books read yesterday
    for i in 10..20 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: (i - 10) * 3, // Varying progress
                scroll_offset: 0,
                last_read: now - Duration::days(1) - Duration::hours((i - 10) as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Add books read last week
    for i in 20..30 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: (i - 20) + 5, // Varying progress
                scroll_offset: 0,
                last_read: now - Duration::days(7) - Duration::hours((i - 20) as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Add books read last month
    for i in 30..40 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: i % 15, // Varying progress
                scroll_offset: 0,
                last_read: now - Duration::days(30) - Duration::hours((i - 30) as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Add books read 6 months ago
    for i in 40..50 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: (i - 40) * 2, // Varying progress
                scroll_offset: 0,
                last_read: now - Duration::days(180) - Duration::hours((i - 40) as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Add books read 1 year ago
    for i in 50..60 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: i % 20, // Varying progress
                scroll_offset: 0,
                last_read: now - Duration::days(365) - Duration::hours((i - 50) as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Add books read 2 years ago
    for i in 60..70 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: (i - 60), // Varying progress
                scroll_offset: 0,
                last_read: now - Duration::days(730) - Duration::hours((i - 60) as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Add some very old books (5+ years)
    for i in 70..100 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        let years_ago = 5 + ((i - 70) / 10); // 5, 6, 7 years
        bookmarks.books.insert(
            book_path,
            TestBookmark {
                chapter: i % 25, // Varying progress
                scroll_offset: 0,
                last_read: now
                    - Duration::days(365 * years_ago as i64)
                    - Duration::hours((i - 70) as i64),
                total_chapters: 10 + (i % 20), // Match the book_configs chapter counts
            },
        );
    }

    // Write the bookmarks to the file
    let bookmarks_json = serde_json::to_string_pretty(&bookmarks).unwrap();
    std::fs::write(&bookmark_path, bookmarks_json).unwrap();

    // Debug: print number of bookmarks created
    println!("Created {} bookmarks", bookmarks.books.len());

    // Now reload the app to pick up the bookmarks
    app = bookrat::App::new_with_config(
        Some(&temp_manager.get_directory()),
        Some(&bookmark_path.to_string_lossy()),
        false,
    );

    // Now show the reading history popup with capital H
    app.press_key(crossterm::event::KeyCode::Char('H'));

    // Draw the state with the reading history popup visible
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_book_reading_history_many_entries.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/book_reading_history_many_entries.svg"),
        "test_book_reading_history_with_many_entries_svg",
        create_test_failure_handler("test_book_reading_history_with_many_entries_svg"),
    );
}

#[test]
fn test_image_scrolling_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 40);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Load the first book and switch to content view
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    // Navigate to a chapter with content
    app.press_key(crossterm::event::KeyCode::Tab); // Focus text reader

    // Initial state - image should be fully visible at scroll_offset = 0
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output_initial = terminal_to_svg(&terminal);
    std::fs::write(
        "tests/snapshots/debug_image_scrolling_initial.svg",
        &svg_output_initial,
    )
    .unwrap();

    // Scroll down a few lines with 'j' - image should start to be cropped from top
    app.press_char_times('j', 5);

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output_partial = terminal_to_svg(&terminal);
    std::fs::write(
        "tests/snapshots/debug_image_scrolling_partial.svg",
        &svg_output_partial,
    )
    .unwrap();

    // Scroll down more - image should be almost hidden
    app.press_char_times('j', 10);

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output_hidden = terminal_to_svg(&terminal);
    std::fs::write(
        "tests/snapshots/debug_image_scrolling_hidden.svg",
        &svg_output_hidden,
    )
    .unwrap();

    // Scroll back up - image should reappear gradually
    app.press_char_times('k', 10);

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output_reappear = terminal_to_svg(&terminal);
    std::fs::write(
        "tests/snapshots/debug_image_scrolling_reappear.svg",
        &svg_output_reappear,
    )
    .unwrap();

    // Test mouse scroll as well
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 20,
            modifiers: crossterm::event::KeyModifiers::empty(),
        },
        None,
    );
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 20,
            modifiers: crossterm::event::KeyModifiers::empty(),
        },
        None,
    );

    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output_mouse_scroll = terminal_to_svg(&terminal);

    assert_svg_snapshot(
        svg_output_mouse_scroll.clone(),
        &std::path::Path::new("tests/snapshots/image_scrolling_test.svg"),
        "test_image_scrolling_svg",
        create_test_failure_handler("test_image_scrolling_svg"),
    );
}
