use bookokrat::simple_fake_books::FakeBookConfig;
use bookokrat::test_utils::test_helpers::{
    create_test_app_with_custom_fake_books, create_test_terminal,
};
// SVG snapshot tests using snapbox
use bookokrat::{App, main_app::FPSCounter};
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

// Helper function to create FPSCounter for tests
fn create_test_fps_counter() -> FPSCounter {
    FPSCounter::new()
}

/// Helper trait for simpler key event handling in tests
trait TestKeyEventHandler {
    fn press_key(&mut self, key: crossterm::event::KeyCode);
    fn press_char_times(&mut self, ch: char, times: usize);
}

impl TestKeyEventHandler for App {
    fn press_key(&mut self, key: crossterm::event::KeyCode) {
        self.handle_key_event_with_screen_height(
            crossterm::event::KeyEvent {
                code: key,
                modifiers: crossterm::event::KeyModifiers::empty(),
                kind: crossterm::event::KeyEventKind::Press,
                state: crossterm::event::KeyEventState::NONE,
            },
            None,
        );
    }

    fn press_char_times(&mut self, ch: char, times: usize) {
        for _ in 0..times {
            self.press_key(crossterm::event::KeyCode::Char(ch));
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

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
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
        std::path::Path::new("tests/snapshots/fake_books_file_list.svg"),
        "test_fake_books_file_list_svg",
        create_test_failure_handler("test_fake_books_file_list_svg"),
    );
}

#[test]
fn test_content_view_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);
    let mut app = App::new_with_config(Some("tests/testdata"), None, false);

    // Switch to content view

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_content_view.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/content_view.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    // Write to debug file
    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_content_scrolling.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/content_scrolling.svg"),
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
        app.focused_panel = bookokrat::FocusedPanel::Main(bookokrat::MainPanel::Content);
        // Force animation to complete for testing
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
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
        std::path::Path::new("tests/snapshots/chapter_title_normal_length.svg"),
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

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
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
        std::path::Path::new("tests/snapshots/chapter_title_narrow_terminal.svg"),
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
    app.handle_and_drain_mouse_events(mouse_event, None);

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_mouse_scroll_file_list.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/mouse_scroll_file_list.svg"),
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
        app.handle_and_drain_mouse_events(mouse_event, None);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_mouse_bounds_check.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/mouse_scroll_bounds_checking.svg"),
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
    use bookokrat::event_source::{EventSource, SimulatedEventSource};

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
            app.handle_and_drain_mouse_events(mouse_event, Some(&mut event_source));
        }
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_mouse_batching.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/mouse_event_batching.svg"),
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
    use bookokrat::event_source::{EventSource, SimulatedEventSource};

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
            app.handle_and_drain_mouse_events(mouse_event, Some(&mut event_source));
        }
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_horizontal_scroll.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/horizontal_scroll_handling.svg"),
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
    use bookokrat::event_source::{EventSource, SimulatedEventSource};

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
            app.handle_and_drain_mouse_events(mouse_event, Some(&mut event_source));
        }
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_edge_case_coordinates.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/edge_case_mouse_coordinates.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    // Simulate text selection: mouse down, drag, mouse up
    // Use coordinates starting from the left margin to test margin selection
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 30, // Click on left margin - should start from beginning of line
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
    app.handle_and_drain_mouse_events(mouse_down, None);
    app.handle_and_drain_mouse_events(mouse_drag, None);
    app.handle_and_drain_mouse_events(mouse_up, None);

    // Redraw to show the selection
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_text_selection.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/text_selection.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

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
    app.handle_and_drain_mouse_events(mouse_down, None);
    app.handle_and_drain_mouse_events(mouse_drag_beyond_bottom, None);
    app.handle_and_drain_mouse_events(mouse_up, None);

    // Redraw to show the selection and scroll state
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_text_selection_auto_scroll.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/text_selection_auto_scroll.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let initial_scroll_offset = app.get_scroll_offset();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_down, None);

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
        app.handle_and_drain_mouse_events(mouse_drag_beyond_bottom, None);
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
    app.handle_and_drain_mouse_events(mouse_up, None);

    // Redraw to show final state
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_continuous_auto_scroll_down.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/continuous_auto_scroll_down.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

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
    app.handle_and_drain_mouse_events(mouse_down, None);

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
        app.handle_and_drain_mouse_events(mouse_drag_above_top, None);
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
    app.handle_and_drain_mouse_events(mouse_up, None);

    // Redraw to show final state
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_continuous_auto_scroll_up.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/continuous_auto_scroll_up.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let initial_scroll_offset = app.get_scroll_offset();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_down, None);

    // Drag beyond bottom ONCE (simulating user holding mouse in position)
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_drag_beyond_bottom, None);

    // Now simulate multiple draw calls (which trigger auto-scroll updates)
    // This simulates the real-world scenario where the user holds the mouse
    // outside the content area and the auto-scroll timer continues scrolling
    let mut scroll_offsets = Vec::new();
    for _i in 0..10 {
        // Simulate a redraw happening (which calls update_auto_scroll)
        terminal
            .draw(|f| {
                let fps = create_test_fps_counter();
                app.draw(f, &fps)
            })
            .unwrap();
        scroll_offsets.push(app.get_scroll_offset());

        // Add a small delay to ensure the timer can trigger
        std::thread::sleep(std::time::Duration::from_millis(110));
    }

    // Verify that scrolling continued automatically without additional mouse events
    let final_scroll_offset = app.get_scroll_offset();
    assert!(
        final_scroll_offset > initial_scroll_offset,
        "Timer-based auto-scroll should have moved from initial offset {initial_scroll_offset} to {final_scroll_offset}"
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
    app.handle_and_drain_mouse_events(mouse_up, None);

    // Final redraw
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_timer_based_auto_scroll.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/timer_based_auto_scroll.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    // Start selection in the middle of the screen
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 45,
        row: 15,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_down, None);

    // Drag beyond bottom to trigger auto-scroll
    let mouse_drag_beyond_bottom = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 35, // Beyond the content area height
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_drag_beyond_bottom, None);
    let scroll_after_auto = app.get_scroll_offset();

    // Move cursor back to within content area - auto-scroll should stop
    let mouse_drag_back_in_area = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 70,
        row: 20, // Back within content area
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_drag_back_in_area, None);
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
    app.handle_and_drain_mouse_events(mouse_drag_within_area, None);

    // End selection
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 80,
        row: 25,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_up, None);

    // Redraw to show final state
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_auto_scroll_cursor_return.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/auto_scroll_stops_when_cursor_returns.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

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
    app.handle_and_drain_mouse_events(mouse_click1, None);
    app.handle_and_drain_mouse_events(mouse_up1, None);
    app.handle_and_drain_mouse_events(mouse_click2, None);
    app.handle_and_drain_mouse_events(mouse_up2, None);

    // Redraw to show the word selection
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_double_click_word_selection.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/double_click_word_selection.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

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
    app.handle_and_drain_mouse_events(mouse_click1, None);
    app.handle_and_drain_mouse_events(mouse_up1, None);
    app.handle_and_drain_mouse_events(mouse_click2, None);
    app.handle_and_drain_mouse_events(mouse_up2, None);
    app.handle_and_drain_mouse_events(mouse_click3, None);
    app.handle_and_drain_mouse_events(mouse_up3, None);

    // Redraw to show the paragraph selection
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_triple_click_paragraph_selection.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/triple_click_paragraph_selection.svg"),
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
    app.focused_panel = bookokrat::FocusedPanel::Main(bookokrat::MainPanel::Content);

    // Draw initial state
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    // Now simulate clicking on book text in the content area
    // According to the bug report: "when i click on a book text: nothing got selected,
    // but the status bar shows as if we are in text selection mode"
    let mouse_click_on_text = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50, // Click on book text in content area
        row: 12,    // Where book text should be displayed
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_click_on_text, None);

    // Complete the click with mouse up
    let mouse_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 50,
        row: 12,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    app.handle_and_drain_mouse_events(mouse_up, None);

    // Draw to see the current state
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
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
        std::path::Path::new("tests/snapshots/text_selection_click_on_book_text_bug.svg"),
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
    app.focused_panel = bookokrat::FocusedPanel::Main(bookokrat::MainPanel::NavigationList);

    // Draw initial state - should show book with expanded TOC
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    // Simulate pressing 'j' key 4 times to navigate down through TOC items
    app.press_char_times('j', 4);

    // Draw the state after navigation
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
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
        std::path::Path::new("tests/snapshots/toc_navigation_bug.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_toc_back_to_books_list.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/toc_back_to_books_list.svg"),
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
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_toc_chapter_navigation.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/toc_chapter_navigation.svg"),
        "test_toc_chapter_navigation_svg",
        create_test_failure_handler("test_toc_chapter_navigation_svg"),
    );
}

#[test]
fn test_mathml_content_rendering_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(120, 40);

    let mathml_content = r#"<!DOCTYPE html>
<html xml:lang="en" lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
    <title>AI Engineering - How to Use a Language Model to Compute a Text's Perplexity</title>
    <link rel="stylesheet" type="text/css" href="override_v1.css"/>
    <link rel="stylesheet" type="text/css" href="epub.css"/>
</head>
<body>
    <div id="book-content">
            <div class="sidebar" id="id902">
                <h1>How to Use a Language Model to Compute a Text's Perplexity</h1>

        <p><a contenteditable="false" data-primary="evaluation methodology" data-secondary="language model for computing text perplexity" data-type="indexterm" id="id903"></a><a contenteditable="false" data-primary="language models" data-type="indexterm" id="id904"></a>A model’s perplexity with respect to a text measures how difficult it is for the model to predict that text. Given a language model <em>X</em>, and a sequence of tokens <math xmlns="http://www.w3.org/1998/Math/MathML" alttext="left-bracket x 1 comma x 2 comma period period period comma x Subscript n Baseline right-bracket">
          <mrow>
            <mo>[</mo>
            <msub><mi>x</mi> <mn>1</mn> </msub>
            <mo>,</mo>
            <msub><mi>x</mi> <mn>2</mn> </msub>
            <mo>,</mo>
            <mo>.</mo>
            <mo>.</mo>
            <mo>.</mo>
            <mo>,</mo>
            <msub><mi>x</mi> <mi>n</mi> </msub>
            <mo>]</mo>
          </mrow>
        </math>, <em>X</em>’s perplexity for this sequence is:</p>
        <div data-type="equation">
                    <math xmlns="http://www.w3.org/1998/Math/MathML" alttext="upper P left-parenthesis x 1 comma x 2 comma period period period comma x Subscript n Baseline right-parenthesis Superscript minus StartFraction 1 Over n EndFraction Baseline equals left-parenthesis StartFraction 1 Over upper P left-parenthesis x 1 comma x 2 comma ellipsis comma x Subscript n Baseline right-parenthesis EndFraction right-parenthesis Superscript StartFraction 1 Over n EndFraction Baseline equals left-parenthesis product Underscript i equals 1 Overscript n Endscripts StartFraction 1 Over upper P left-parenthesis x Subscript i Baseline vertical-bar x 1 comma period period period comma x Subscript i minus 1 Baseline right-parenthesis EndFraction right-parenthesis Superscript StartFraction 1 Over n EndFraction">
          <mrow>
            <mi>P</mi>
            <msup><mrow><mo>(</mo><msub><mi>x</mi> <mn>1</mn> </msub><mo>,</mo><msub><mi>x</mi> <mn>2</mn> </msub><mo>,</mo><mo>.</mo><mo>.</mo><mo>.</mo><mo>,</mo><msub><mi>x</mi> <mi>n</mi> </msub><mo>)</mo></mrow> <mrow><mo>-</mo><mfrac><mn>1</mn> <mi>n</mi></mfrac></mrow> </msup>
            <mo>=</mo>
            <msup><mrow><mo>(</mo><mfrac><mn>1</mn> <mrow><mi>P</mi><mo>(</mo><msub><mi>x</mi> <mn>1</mn> </msub><mo>,</mo><msub><mi>x</mi> <mn>2</mn> </msub><mo>,</mo><mi>â</mi><mi></mi><mi>¦</mi><mo>,</mo><msub><mi>x</mi> <mi>n</mi> </msub><mo>)</mo></mrow></mfrac><mo>)</mo></mrow> <mfrac><mn>1</mn> <mi>n</mi></mfrac> </msup>
            <mo>=</mo>
            <msup><mrow><mo>(</mo><msubsup><mo>∏</mo> <mrow><mi>i</mi><mo>=</mo><mn>1</mn></mrow> <mi>n</mi> </msubsup><mfrac><mn>1</mn> <mrow><mi>P</mi><mo>(</mo><msub><mi>x</mi> <mi>i</mi> </msub><mo>|</mo><msub><mi>x</mi> <mn>1</mn> </msub><mo>,</mo><mo>.</mo><mo>.</mo><mo>.</mo><mo>,</mo><msub><mi>x</mi> <mrow><mi>i</mi><mo>-</mo><mn>1</mn></mrow> </msub><mo>)</mo></mrow></mfrac><mo>)</mo></mrow> <mfrac><mn>1</mn> <mi>n</mi></mfrac> </msup>
          </mrow>
        </math>
        </div>
        <p>where <math xmlns="http://www.w3.org/1998/Math/MathML" alttext="upper P left-parenthesis x Subscript i Baseline vertical-bar x 1 comma period period period comma x Subscript i minus 1 Baseline right-parenthesis">
          <mrow>
            <mi>P</mi>
            <mo>(</mo>
            <msub><mi>x</mi> <mi>i</mi> </msub>
            <mo>|</mo>
            <msub><mi>x</mi> <mn>1</mn> </msub>
            <mo>,</mo>
            <mo>.</mo>
            <mo>.</mo>
            <mo>.</mo>
            <mo>,</mo>
            <msub><mi>x</mi> <mrow><mi>i</mi><mo>-</mo><mn>1</mn></mrow> </msub>
            <mo>)</mo>
          </mrow>
        </math> denotes the probability that <em>X</em> assigns to the token <math xmlns="http://www.w3.org/1998/Math/MathML" alttext="x Subscript i">
          <msub><mi>x</mi> <mi>i</mi> </msub>
        </math> given the previous tokens <math xmlns="http://www.w3.org/1998/Math/MathML" alttext="x 1 comma period period period comma x Subscript i minus 1 Baseline">
          <mrow>
            <msub><mi>x</mi> <mn>1</mn> </msub>
            <mo>,</mo>
            <mo>.</mo>
            <mo>.</mo>
            <mo>.</mo>
            <mo>,</mo>
            <msub><mi>x</mi> <mrow><mi>i</mi><mo>-</mo><mn>1</mn></mrow> </msub>
          </mrow>
        </math>.</p>

        <p>To compute perplexity, you need access to the probabilities (or logprobs) the language model assigns to each next token. Unfortunately, not all commercial models expose their models’ logprobs, as discussed in <a data-type="xref" href="ch02.html#ch02_understanding_foundation_models_1730147895571359">Chapter 2</a>.</p>
                  </div>
        </body>
        </html>

        "#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("mathml_test.html");
    std::fs::write(&temp_html_path, mathml_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_mathml_content_rendering.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/mathml_content_rendering.svg"),
        "test_mathml_content_rendering_svg",
        create_test_failure_handler("test_mathml_content_rendering_svg"),
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
        bookokrat::test_utils::test_helpers::TempBookManager::new_with_configs(&book_configs)
            .expect("Failed to create temp books");

    // Create bookmarks using the production format by manually crafting valid JSON
    // This is the only way to create deterministic test data with specific timestamps
    use chrono::{Duration, TimeZone, Utc};
    use std::collections::HashMap;

    // Use a fixed date for deterministic test output
    let now = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();

    let mut books_map = HashMap::new();

    // Add books read today (most recent - should appear at top)
    for i in 0..10 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        let bookmark = bookokrat::bookmarks::Bookmark {
            chapter_href: format!("chapter_{}.html", i * 2),
            node_index: None,
            last_read: now - Duration::hours(i as i64),
            chapter_index: Some(i * 2),
            total_chapters: Some(10 + (i % 20)),
        };
        books_map.insert(book_path, bookmark);
    }

    // Add books read yesterday
    for i in 10..20 {
        let book_path = format!("{}/Test Book {}.epub", temp_manager.get_directory(), i);
        let bookmark = bookokrat::bookmarks::Bookmark {
            chapter_href: format!("chapter_{}.html", (i - 10) * 3),
            node_index: None,
            last_read: now - Duration::days(1) - Duration::hours((i - 10) as i64),
            chapter_index: Some((i - 10) * 3),
            total_chapters: Some(10 + (i % 20)),
        };
        books_map.insert(book_path, bookmark);
    }

    // Save using the production Bookmarks struct
    let mut prod_bookmarks =
        bookokrat::bookmarks::Bookmarks::with_file(&bookmark_path.to_string_lossy());

    // Add all the bookmarks using the production method
    for (path, bookmark) in books_map {
        prod_bookmarks.update_bookmark(
            &path,
            bookmark.chapter_href,
            bookmark.node_index,
            bookmark.chapter_index,
            bookmark.total_chapters,
        );
    }

    // Save using production code
    prod_bookmarks.save().unwrap();

    // Debug: print number of bookmarks created
    println!("Created {} bookmarks using production code", 100);

    // Now reload the app to pick up the bookmarks
    let mut app = bookokrat::App::new_with_config(
        Some(&temp_manager.get_directory()),
        Some(&bookmark_path.to_string_lossy()),
        false,
    );

    // Now show the reading history popup with capital H
    app.press_key(crossterm::event::KeyCode::Char('H'));

    // Draw the state with the reading history popup visible
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_book_reading_history_many_entries.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/book_reading_history_many_entries.svg"),
        "test_book_reading_history_with_many_entries_svg",
        |expected,
         actual,
         snapshot_path,
         expected_lines,
         actual_lines,
         diff_count,
         first_diff_line| {
            test_report::TestReport::add_failure(test_report::TestFailure {
                test_name: "test_book_reading_history_with_many_entries_svg".to_string(),
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
fn test_headings_h1_to_h6_rendering_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(120, 40);

    let headings_content = r#"<!DOCTYPE html>
<html xml:lang="en" lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
    <title>H1-H6 Headings Test</title>
    <link rel="stylesheet" type="text/css" href="override_v1.css"/>
    <link rel="stylesheet" type="text/css" href="epub.css"/>
</head>
<body>
    <div id="book-content">
        <h1>Level 1: Main Chapter Title</h1>
        <p>This is content under the main heading.</p>

        <h2>Level 2: Major Section</h2>
        <p>This is content under the major section.</p>

        <h3>Level 3: Subsection</h3>
        <p>This is content under the subsection.</p>

        <h4>Level 4: Minor Heading</h4>
        <p>This is content under the minor heading.</p>

        <h5>Level 5: Sub-minor Heading</h5>
        <p>This is content under the sub-minor heading.</p>

        <h6>Level 6: Smallest Heading</h6>
        <p>This is content under the smallest heading level. This test demonstrates the complete hierarchy of all heading levels from H1 through H6 and how they are visually distinguished in the terminal interface.</p>
    </div>
</body>
</html>
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("headings_test.html");
    std::fs::write(&temp_html_path, headings_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_headings_h1_to_h6_rendering.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/headings_h1_to_h6_rendering.svg"),
        "test_headings_h1_to_h6_rendering_svg",
        create_test_failure_handler("test_headings_h1_to_h6_rendering_svg"),
    );
}

#[test]
fn test_table_with_links_and_linebreaks_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(140, 50);

    let table_content = r#"<!DOCTYPE html>
<html xml:lang="en" lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
    <title>Table with Links and Line Breaks Test</title>
    <link rel="stylesheet" type="text/css" href="override_v1.css"/>
    <link rel="stylesheet" type="text/css" href="epub.css"/>
</head>
<body>
    <div id="book-content">
        <p class="pagebreak-before">When analyzing the use cases, I looked at both enterprise and consumer applications. To understand enterprise use cases, I interviewed 50 companies on their AI strategies and read over 100 case studies. To understand consumer applications, I examined 205 open source AI applications with at least 500 stars on GitHub.<sup><a data-type="noteref" id="id567-marker" href="ch01.html#id567">11</a></sup> I categorized applications into eight groups, as shown in <a data-type="xref" href="ch01_table_3_1730130814941550">Table 1-3</a>. The limited list here serves best as a reference. As you learn more about how to build foundation models in <a data-type="xref" href="ch02.html#ch02_understanding_foundation_models_1730147895571359">Chapter 2</a> and how to evaluate them in <a data-type="xref" href="ch03.html#ch03a_evaluation_methodology_1730150757064067">Chapter 3</a>, you'll also be able to form a better picture of what use cases foundation models can and should be used for.</p> <table id="ch01_table_3_1730130814941550"> <caption><span class="label">Table 1-3. </span>Common generative AI use cases across consumer and enterprise applications.</caption> <thead>
            <tr>
              <th>Category</th>
              <th>Examples of consumer use cases</th>
              <th>Examples of enterprise use cases</th>
            </tr>
          </thead>
          <tr>
            <td><strong>Coding</strong></td>
            <td>Coding on [localhost](http://localhost)</td>
            <td>Coding <i>again!</i></td>
          </tr>
          <tr>
            <td>Image and video <b>production</b></td>
            <td>Photo and video editing<br/> Design</td>
            <td>Presentation <br/> Ad generation</td>
          </tr>
          <tr>
            <td>Writing</td>
            <td>Email<br/> Social media and blog posts</td>
            <td>Copywriting, search engine optimization (SEO)<br/> Reports, memos, design docs</td>
          </tr>
          <tr>
            <td>Education</td>
            <td>Tutoring<br/> Essay grading</td>
            <td>Employee onboarding<br/> Employee upskill training</td>
          </tr>
          <tr>
            <td>Conversational bots</td>
            <td>General chatbot<br/> AI companion</td>
            <td>Customer support<br/> Product copilots</td>
          </tr>
          <tr>
            <td>Information aggregation</td>
            <td>Summarization<br/> Talk-to-your-docs</td>
            <td>Summarization<br/> Market research</td>
          </tr>
          <tr>
            <td>Data organization</td>
            <td>Image search<br/> <a class="orm:hideurl" href="https://en.wikipedia.org/wiki/Memex">Memex</a></td>
            <td>Knowledge management<br/> Document processing</td>
          </tr>
          <tr>
            <td>Workflow automation</td>
            <td>Travel planning<br/> Event planning</td>
            <td>Data extraction, entry, and annotation<br/> Lead generation</td>
          </tr>
        </table>

        <p>Because foundation models are general, applications built on top of them can solve many problems. This means that an application can belong to more than one category. For example, a bot can provide companionship and aggregate information. An application can help you extract structured data from a PDF and answer questions about that PDF.</p>
    </div>
</body>
</html>
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("table_with_links_test.html");
    std::fs::write(&temp_html_path, table_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_table_with_links_and_linebreaks.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/table_with_links_and_linebreaks.svg"),
        "test_table_with_links_and_linebreaks_svg",
        create_test_failure_handler("test_table_with_links_and_linebreaks_svg"),
    );
}

#[test]
fn test_basic_markdown_elements_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(120, 80);

    let basic_elements_content = r##"<!DOCTYPE html>
<html xml:lang="en" lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
    <title>Basic Markdown Elements Test</title>
    <link rel="stylesheet" type="text/css" href="override_v1.css"/>
    <link rel="stylesheet" type="text/css" href="epub.css"/>
</head>
<body>
    <div id="book-content">
        <h1>Supported Markdown Elements</h1>
        <p>This test demonstrates the basic markdown elements supported by BookRat.</p>

        <h2>Lists</h2>

        <h3>Unordered List</h3>
        <ul>
            <li>First item in unordered list</li>
            <li>Second item with <strong>bold text</strong></li>
            <li>Third item with <em>italic text</em>
                <ul>
                    <li>Nested list item one</li>
                    <li>Nested list item two with <code>inline code</code></li>
                    <li>Nested list item three
                        <ul>
                            <li>Deep nested item</li>
                            <li>Another deep nested item</li>
                        </ul>
                    </li>
                </ul>
            </li>
            <li>Fourth item with a <a href="https://example.com">link</a></li>
        </ul>

        <h3>Ordered List</h3>
        <ol>
            <li>First numbered item</li>
            <li>Second numbered item with formatting
                <ol>
                    <li><p>Nested numbered item</p></li>
                    <li><p>Another nested numbered item</p></li>
                </ol>
            </li>
            <li>Third numbered item</li>
        </ol>

        <h2>Definition Lists</h2>
        <dl>
            <dt>Term One</dt>
            <dd>Definition of term one with detailed explanation.</dd>

            <dt>Term Two</dt>
            <dd>Definition of term two with <strong>bold formatting</strong>.</dd>

            <dt>Technical Term</dt>
            <dd>A technical definition that includes <code>code snippets</code> and references to other concepts.</dd>
        </dl>

        <h2>Links</h2>
        <p>Various types of links are supported:</p>
        <ul>
            <li>External link: <a href="https://www.example.com">Visit Example.com</a></li>
            <li><strong>Internal reference</strong>: <a href="#section1">Go to Section 1</a></li>
            <li><i>Email link</i>: <a href="mailto:user@example.org">Contact Us</a></li>
            <li>Link with title: <a href="https://github.com" title="GitHub Homepage">GitHub</a></li>
        </ul>

        <!--
        <h2>Code Blocks</h2>
        <p>Code blocks with syntax highlighting:</p>

        <h3>Python Code</h3>
        <pre><code class="language-python">
def calculate_fibonacci(n):
    """Calculate the nth Fibonacci number."""
    if n <= 1:
        return n
    return calculate_fibonacci(n-1) + calculate_fibonacci(n-2)

# Example usage
result = calculate_fibonacci(10)
print("The 10th Fibonacci number is:", result)
        </code></pre>

        <h3>Rust Code</h3>
        <pre><code class="language-rust">
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];

    let sum: i32 = numbers
        .iter()
        .filter(|&x| x % 2 == 0)
        .sum();

    println!("Sum of even numbers: {}", sum);
}
        </code></pre>

        <h3>JavaScript Code</h3>
        <pre><code class="language-javascript">
const fetchData = async (url) => {
    try {
        const response = await fetch(url);
        const data = await response.json();
        return data;
    } catch (error) {
        console.error("Error fetching data:", error);
        throw error;
    }
};

// Usage example
fetchData('https://api.example.com/data')
    .then(data => console.log(data))
    .catch(err => console.error(err));
        </code></pre>

        <p>This demonstrates BookRat's comprehensive markdown support including nested lists, definition lists, various link types, and syntax-highlighted code blocks.</p>
       -->
        </div>
</body>
</html>
"##;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("basic_markdown_test.html");
    std::fs::write(&temp_html_path, basic_elements_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_basic_markdown_elements.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/basic_markdown_elements.svg"),
        "test_basic_markdown_elements_svg",
        create_test_failure_handler("test_basic_markdown_elements_svg"),
    );
}

#[test]
fn test_epub_type_attributes_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 30);

    let epub_content = r##"<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
    <title>EPUB Type Attributes Test</title>
</head>
<body>
    <section epub:type="chapter">
        <h1>Chapter 1: Introduction</h1>

        <p>This is a regular paragraph in the chapter.</p>

        <pre data-type="programlisting">p(I love food) = p(I) × p(I | love) × p(food | I, love)</pre>

        <aside epub:type="sidebar">
            <h2>Important Note</h2>
            <p>This is a sidebar with additional information that supplements the main content.</p>
        </aside>

        <section epub:type="bibliography">
            <h2>References</h2>
            <ol>
                <li>Smith, J. (2023). <cite>Digital Publishing Standards</cite>. Tech Press.</li>
                <li>Doe, A. (2022). "EPUB Structure Guidelines". <cite>Journal of Digital Media</cite>, 15(3), 45-62.</li>
            </ol>
        </section>

    </section>
</body>
</html>"##;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("epub_types_test.html");
    std::fs::write(&temp_html_path, epub_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_epub_type_attributes.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/epub_type_attributes.svg"),
        "test_epub_type_attributes_svg",
        create_test_failure_handler("test_epub_type_attributes_svg"),
    );
}

#[test]
fn test_complex_table_with_code_and_linebreaks_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(140, 50);

    let table_content = r#"<!DOCTYPE html>
<html xml:lang="en" lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
    <title>Complex Table with Code and Line Breaks Test</title>
    <link rel="stylesheet" type="text/css" href="override_v1.css"/>
    <link rel="stylesheet" type="text/css" href="epub.css"/>
</head>
<body>
    <div id="book-content">
        <table id="ch02_table_6_1730147895537582">
            <caption><span class="label">Table 2-6. </span>Examples of demonstration data used for <a href="https://arxiv.org/abs/2203.02155">InstructGPT</a>.</caption>
            <thead>
                <tr>
                    <th>Prompt</th>
                    <th>Labeler's response</th>
                </tr>
            </thead>
            <tbody>
                <tr>
                    <td><code>Serendipity means the occurrence and development of events by chance in a happy or beneficial way. Use the word in a sentence.</code></td>
                    <td><code>Running into Margaret and being introduced to Tom was a fortunate stroke of</code><br/><code>serendipity.</code></td>
                </tr>
                <tr>
                    <td><code>Read the following text and answer the three questions at the end.</code><br/>
                    <br/>
                    <code>[OMITTED FOR LENGTH - Article about Justice Ruth Bader Ginsburg,</code><br/><code>Champion Of Gender Equality, Dies At 87]</code><br/>
                    <br/>
                    <code>1. …</code><br/>
                    <code>2. …</code><br/>
                    <code>3. Why will Ginsburg's death have</code><br/><code>profound consequences for the court and the country?</code></td>
                    <td><code>1. …</code><br/>
                    <code>2. …</code><br/>
                    <code>3. Ruth Bader Ginsburg was the leader of the liberal wing, and because the court is about to open a new term, the chief</code><br/><code>justice no longer holds the controlling vote in contested cases.</code></td>
                </tr>
                <tr>
                    <td><code>ELI5: What's the cause of the</code><br/><code>"anxiety lump" in our chest during stressful or disheartening</code><br/><code>experiences?</code></td>
                    <td><code>The anxiety lump in your throat is caused by muscular tension keeping your glottis dilated to maximize airflow. The clenched chest or heartache feeling is caused by the vagus nerve which tells the organs to pump blood faster, stop digesting, and</code><br/><code>produce adrenaline and cortisol.</code></td>
                </tr>
            </tbody>
        </table>
    </div>
</body>
</html>
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("complex_table_test.html");
    std::fs::write(&temp_html_path, table_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_complex_table_with_code_and_linebreaks.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/complex_table_with_code_and_linebreaks.svg"),
        "test_complex_table_with_code_and_linebreaks_svg",
        create_test_failure_handler("test_complex_table_with_code_and_linebreaks_svg"),
    );
    assert_eq!(1, 2, "table caption is not shown!");
}

#[test]
fn test_html_subscript_rendering_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(120, 40);

    let subscript_content = r#"<!DOCTYPE html>
<html xml:lang="en" lang="en" xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>Subscript Rendering Test</title>
</head>
<body>
    <div id="book-content">
        <h1>Attention Function Mathematics</h1>

        <p>Let's look into how the attention function works. Given an input <code>x</code>, the key, value, and query vectors are computed by applying key, value, and query matrices to the input. Let <code>W</code><sub>K</sub><code>, W</code><sub>V</sub><code>, and W</code><sub>Q</sub> be the key, value, and query matrices. The key, value, and query vectors are computed as follows:</p>

        <pre data-type="programlisting">
K = xW<sub>K</sub>
V = xW<sub>V</sub>
Q = xW<sub>Q</sub></pre>

        <p>The query, key, and value matrices have dimensions corresponding to the model's hidden dimension. <a contenteditable="false" data-type="indexterm" data-primary="Llama" data-secondary="attention function" id="id726"></a>For example, in Llama 2-7B (<a href="https://arxiv.org/abs/2307.09288">Touvron et al., 2023</a>), the model's hidden dimension size is 4096, meaning that each of these matrices has a <code>4096 </code>×<code> 4096</code> dimension. Each resulting <code>K</code>, <code>V</code>, <code>Q</code> vector has the dimension of <code>4096</code>.<sup><a data-type="noteref" id="id727-marker" href="ch02.html#id727">8</a></sup></p>

        <p>Additional subscript examples: H<sub>2</sub>O, CO<sub>2</sub>, x<sub>i</sub>, x<sub>i-1</sub>, W<sub>key</sub></p>
    </div>
</body>
</html>
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("subscript_test.html");
    std::fs::write(&temp_html_path, subscript_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_html_subscript_rendering.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/html_subscript_rendering.svg"),
        "test_html_subscript_rendering_svg",
        create_test_failure_handler("test_html_subscript_rendering_svg"),
    );
}

#[test]
fn test_definition_list_with_complex_content_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(120, 40);

    // Create HTML content with definition list containing lists and images
    let dl_content = r#"<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>Complex Definition List Test</title>
</head>
<body>
    <h1>Definition List with Complex Content</h1>
    <p>This tests definition lists with nested content like lists and images.</p>

    <dl>
        <dt><strong>Programming Languages</strong></dt>
        <dd>
            <p>Popular programming languages include:</p>
            <ul>
                <li><em>Python</em> - High-level, interpreted language</li>
                <li><strong>Rust</strong> - Systems programming language</li>
                <li>JavaScript - Web development language</li>
            </ul>
            <ol>
                <li>First learn the basics</li>
                <li>Then practice with projects</li>
                <li>Finally, contribute to open source</li>
            </ol>
        </dd>

        <dt>Data Structures</dt>
        <dd>
            Fundamental computer science concepts with visual representations:
            <img src="datastructures.png" alt="Data structures diagram" width="400" height="300"/>
            <p>Including arrays, linked lists, trees, and graphs.</p>
        </dd>

        <dt><em>Algorithms</em></dt>
        <dd>
            <p>Step-by-step procedures for calculations:</p>
            <ol>
                <li><strong>Sorting algorithms</strong>
                    <ul>
                        <li>Quick sort</li>
                        <li>Merge sort</li>
                        <li>Heap sort</li>
                    </ul>
                </li>
                <li><em>Search algorithms</em>
                    <ul>
                        <li>Binary search</li>
                        <li>Linear search</li>
                    </ul>
                </li>
            </ol>
        </dd>

        <dt>Machine Learning</dt>
        <dd>
            <p>A subset of artificial intelligence that includes:</p>
            <ul>
                <li>Supervised learning with labeled data</li>
                <li>Unsupervised learning for pattern discovery</li>
                <li>Reinforcement learning through rewards</li>
            </ul>
            <img src="ml-workflow.jpg" alt="Machine learning workflow" width="500" height="350"/>
        </dd>
    </dl>

    <p>Definition lists are useful for glossaries and documentation.</p>
</body>
</html>
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("dl_complex_test.html");
    std::fs::write(&temp_html_path, dl_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_definition_list_complex.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/definition_list_complex_content.svg"),
        "test_definition_list_with_complex_content_svg",
        create_test_failure_handler("test_definition_list_with_complex_content_svg"),
    );
}

#[test]
fn test_lists_with_tables_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(140, 50);

    // Create HTML content with lists containing tables
    let list_table_content = r#"<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>Lists with Tables Test</title>
</head>
<body>
    <h1>Lists Containing Tables</h1>
    <p>This tests lists that contain tables as their content.</p>

    <h2>Unordered List with Tables</h2>
    <ul>
        <li>
            <p>Programming Language Comparison:</p>
            <table>
                <thead>
                    <tr>
                        <th>Language</th>
                        <th>Type</th>
                        <th>Performance</th>
                        <th>Use Cases</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>Python</td>
                        <td>Interpreted</td>
                        <td>Moderate</td>
                        <td>Data Science, Web</td>
                    </tr>
                    <tr>
                        <td>Rust</td>
                        <td>Compiled</td>
                        <td>High</td>
                        <td>Systems, WebAssembly</td>
                    </tr>
                    <tr>
                        <td>JavaScript</td>
                        <td>JIT Compiled</td>
                        <td>Good</td>
                        <td>Web, Node.js</td>
                    </tr>
                </tbody>
            </table>
        </li>
        <li>
            <p>Database Systems:</p>
            <table>
                <thead>
                    <tr>
                        <th>Database</th>
                        <th>Type</th>
                        <th>License</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>PostgreSQL</td>
                        <td>Relational</td>
                        <td>Open Source</td>
                    </tr>
                    <tr>
                        <td>MongoDB</td>
                        <td>NoSQL</td>
                        <td>SSPL</td>
                    </tr>
                </tbody>
            </table>
        </li>
    </ul>

    <h2>Ordered List with Mixed Content</h2>
    <ol>
        <li>
            <p>First, review the framework comparison:</p>
            <table>
                <thead>
                    <tr>
                        <th>Framework</th>
                        <th>Language</th>
                        <th>Learning Curve</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td><strong>React</strong></td>
                        <td>JavaScript</td>
                        <td>Moderate</td>
                    </tr>
                    <tr>
                        <td><em>Vue</em></td>
                        <td>JavaScript</td>
                        <td>Easy</td>
                    </tr>
                    <tr>
                        <td>Angular</td>
                        <td>TypeScript</td>
                        <td>Steep</td>
                    </tr>
                </tbody>
            </table>
            <p>Note the differences in learning curves.</p>
        </li>
        <li>
            <p>Next, consider the performance metrics:</p>
            <ul>
                <li>Bundle size comparison:
                    <table>
                        <thead>
                            <tr>
                                <th>Framework</th>
                                <th>Min Size (KB)</th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td>React</td>
                                <td>42.2</td>
                            </tr>
                            <tr>
                                <td>Vue</td>
                                <td>34.0</td>
                            </tr>
                        </tbody>
                    </table>
                </li>
                <li>Runtime performance varies by use case</li>
            </ul>
        </li>
        <li>
            <p>Finally, deployment options:</p>
            <table>
                <thead>
                    <tr>
                        <th>Platform</th>
                        <th>Free Tier</th>
                        <th>Auto-scaling</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>Vercel</td>
                        <td>Yes</td>
                        <td>Yes</td>
                    </tr>
                    <tr>
                        <td>Netlify</td>
                        <td>Yes</td>
                        <td>Limited</td>
                    </tr>
                    <tr>
                        <td>AWS</td>
                        <td>Limited</td>
                        <td>Yes</td>
                    </tr>
                </tbody>
            </table>
        </li>
    </ol>

    <h2>Nested Lists with Tables</h2>
    <ul>
        <li>Development Tools
            <ul>
                <li>IDEs and Editors:
                    <table>
                        <thead>
                            <tr>
                                <th>Editor</th>
                                <th>Price</th>
                                <th>Platform</th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td>VS Code</td>
                                <td>Free</td>
                                <td>Cross-platform</td>
                            </tr>
                            <tr>
                                <td>IntelliJ</td>
                                <td>Paid</td>
                                <td>Cross-platform</td>
                            </tr>
                        </tbody>
                    </table>
                </li>
                <li>Version Control Systems</li>
            </ul>
        </li>
    </ul>

    <p>Tables within lists provide structured data presentation.</p>
</body>
</html>
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("list_tables_test.html");
    std::fs::write(&temp_html_path, list_table_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();
    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_lists_with_tables.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/lists_with_tables.svg"),
        "test_lists_with_tables_svg",
        create_test_failure_handler("test_lists_with_tables_svg"),
    );
}

#[test]
fn test_content_search_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 35);

    // Create HTML content with searchable text - word "programming" appears multiple times
    let search_content = r#"<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>Search Test Document</title>
</head>
<body>
    <h1>Introduction to Programming</h1>

    <p>Programming is the process of creating a set of instructions that tell a computer how to perform a task.
    Programming can be done using a variety of computer programming languages, such as JavaScript, Python, and C++.</p>

    <h2>Popular Programming Languages</h2>

    <p>There are many programming languages available today. Some of the most popular programming languages include:</p>

    <ul>
        <li><strong>Python</strong>: Known for its simplicity and readability. Python is widely used in data science,
        machine learning, and web development.</li>
        <li><strong>JavaScript</strong>: The language of the web. JavaScript runs in browsers and enables interactive web pages.</li>
        <li><strong>Java</strong>: A robust, object-oriented language used for enterprise applications.</li>
        <li><strong>C++</strong>: A powerful systems programming language with fine control over hardware.</li>
        <li><strong>Rust</strong>: A modern systems programming language focused on safety and performance.</li>
    </ul>

    <h2>Getting Started with Programming</h2>

    <p>If you're new to programming, Python is often recommended as a first language. Python's syntax is clear and
    intuitive, making it an excellent choice for beginners. Here's a simple Python example:</p>

    <pre><code>def hello_world():
    print("Hello, World!")

hello_world()</code></pre>

    <p>This simple program demonstrates a function definition in Python. The function hello_world prints a greeting
    message when called.</p>

    <h2>Programming Paradigms</h2>

    <p>Different programming languages support different programming paradigms:</p>

    <ol>
        <li><em>Procedural Programming</em>: Programs are organized as procedures or functions.</li>
        <li><em>Object-Oriented Programming</em>: Programs are organized around objects and classes.</li>
        <li><em>Functional Programming</em>: Computation is treated as evaluation of mathematical functions.</li>
        <li><em>Declarative Programming</em>: Programs describe what should be done, not how.</li>
    </ol>

    <p>Understanding these paradigms helps in choosing the right approach for your programming projects.</p>

    <h2>The Future of Programming</h2>

    <p>As technology evolves, so does programming. New languages emerge, existing languages evolve, and programming
    practices continue to improve. Whether you're interested in web development, mobile apps, data science, or systems
    programming, there's a programming language and tools suited for your needs.</p>

    <p>Remember: the best programming language is the one that helps you solve your specific problem effectively.</p>
</body>
</html>
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_html_path = temp_dir.path().join("search_test.html");
    std::fs::write(&temp_html_path, search_content).unwrap();

    let mut app = App::new_with_config(Some(temp_dir.path().to_str().unwrap()), None, false);

    // Load the test document
    if let Some(book_info) = app.book_manager.get_book_info(0) {
        let path = book_info.path.clone();
        let _ = app.open_book_for_reading_by_path(&path);
    }

    // Initial draw to establish content
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    // Enter search mode with '/'
    app.press_key(crossterm::event::KeyCode::Char('/'));

    // Type search term "programming"
    for ch in "programming".chars() {
        app.press_key(crossterm::event::KeyCode::Char(ch));
    }

    // Press Enter to confirm search
    app.press_key(crossterm::event::KeyCode::Enter);

    // Draw to show search results with highlighting
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_content_search.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/content_search.svg"),
        "test_content_search_svg",
        create_test_failure_handler("test_content_search_svg"),
    );
}

#[test]
fn test_toc_search_svg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(100, 35);

    // Create test books with fake book helper - this creates proper EPUB structure with TOC
    let book_configs = vec![FakeBookConfig {
        title: "Digital Frontier".to_string(),
        chapter_count: 10,
        words_per_chapter: 50,
    }];

    let (mut app, _temp_manager) = create_test_app_with_custom_fake_books(&book_configs);

    // Select and open the book to show TOC
    app.press_key(crossterm::event::KeyCode::Enter);

    // Make sure we're focused on the TOC panel, not the content
    // After opening a book, focus typically goes to content, so we need to switch back
    app.focused_panel = bookokrat::FocusedPanel::Main(bookokrat::MainPanel::NavigationList);

    // Initial draw to show the TOC
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    // Enter search mode with '/' - this should search in the TOC
    app.press_key(crossterm::event::KeyCode::Char('/'));

    // Search for "chapter" which appears in TOC items
    for ch in "chapter".chars() {
        app.press_key(crossterm::event::KeyCode::Char(ch));
    }

    // Press Enter to confirm search
    app.press_key(crossterm::event::KeyCode::Enter);

    // Draw to show TOC with search results highlighted
    terminal
        .draw(|f| {
            let fps = create_test_fps_counter();
            app.draw(f, &fps)
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write("tests/snapshots/debug_toc_search.svg", &svg_output).unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/toc_search.svg"),
        "test_toc_search_svg",
        create_test_failure_handler("test_toc_search_svg"),
    );
}
