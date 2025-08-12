use bookrat::book_manager::{BookInfo, BookManager};
use bookrat::bookmark::Bookmarks;
use bookrat::main_app::VimNavMotions;
use bookrat::navigation_panel::{CurrentBookInfo, NavigationMode, NavigationPanel};
use bookrat::table_of_contents::TocItem;
use bookrat::test_utils::test_helpers::create_test_terminal;
use bookrat::text_reader::TextReader;
use bookrat::theme::Base16Palette;

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

// Create a mock book manager with test books
fn create_test_book_manager() -> BookManager {
    let mut book_manager = BookManager::new();
    let mut books = Vec::new();
    for i in 1..=100 {
        books.push(BookInfo {
            display_name: format!("Book {}", i),
            path: "book1.epub".to_string(),
        })
    }
    book_manager.books = books;
    book_manager
}

// Get default theme palette
fn get_test_palette() -> Base16Palette {
    bookrat::theme::OCEANIC_NEXT
}

#[test]
fn test_book_list_vim_motion_g() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(30, 10); // Small terminal to focus on component

    let book_manager = create_test_book_manager();
    let mut nav_panel = NavigationPanel::new(&book_manager);

    // Ensure we're in book selection mode
    assert_eq!(nav_panel.mode, NavigationMode::BookSelection);

    nav_panel.handle_G();
    terminal
        .draw(|f| {
            let area = f.size();
            let palette = get_test_palette();
            let bookmarks = Bookmarks::ephemeral();
            nav_panel.render(f, area, false, &palette, &bookmarks, &book_manager);
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_book_list_vim_g_component.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/book_list_vim_g_component.svg"),
        "test_book_list_vim_motion_g",
        create_test_failure_handler("test_book_list_vim_motion_g"),
    );
}

#[test]
fn test_book_list_vim_motion_gg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(30, 10);

    let book_manager = create_test_book_manager();
    let mut nav_panel = NavigationPanel::new(&book_manager);

    // Move down a few times to test gg from a non-top position
    for _ in 0..4 {
        nav_panel.move_selection_down();
    }

    nav_panel.handle_gg();
    // Render only the navigation panel in book list mode
    terminal
        .draw(|f| {
            let area = f.size();
            let palette = get_test_palette();
            let bookmarks = Bookmarks::ephemeral();
            nav_panel.render(f, area, false, &palette, &bookmarks, &book_manager);
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_book_list_vim_gg_component.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/book_list_vim_gg_component.svg"),
        "test_book_list_vim_motion_gg",
        create_test_failure_handler("test_book_list_vim_motion_gg"),
    );
}

// Helper function to create a book with many chapters for TOC testing
fn create_test_book_info_with_toc() -> CurrentBookInfo {
    let mut toc_items = vec![];

    // Create 25 chapters to test scrolling
    for i in 1..=25 {
        toc_items.push(TocItem::Chapter {
            title: format!("Chapter {}", i),
            href: format!("chapter{}.xhtml", i),
            index: i - 1,
        });
    }

    CurrentBookInfo {
        path: "test_book.epub".to_string(),
        toc_items,
        current_chapter: 0,
    }
}

#[test]
fn test_navigation_panel_vim_motion_g() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(40, 15);

    let book_manager = create_test_book_manager();
    let mut nav_panel = NavigationPanel::new(&book_manager);

    // Switch to TOC mode with our test book
    let book_info = create_test_book_info_with_toc();
    nav_panel.switch_to_toc_mode(0, book_info);

    // Test G (go to bottom)
    nav_panel.handle_G();

    // Render only the navigation panel
    terminal
        .draw(|f| {
            let area = f.size();
            let palette = get_test_palette();
            let bookmarks = Bookmarks::ephemeral();
            nav_panel.render(f, area, false, &palette, &bookmarks, &book_manager);
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_nav_panel_vim_g_component.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/nav_panel_vim_g_component.svg"),
        "test_navigation_panel_vim_motion_g",
        create_test_failure_handler("test_navigation_panel_vim_motion_g"),
    );
}

#[test]
fn test_navigation_panel_vim_motion_gg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(40, 15);

    let book_manager = create_test_book_manager();
    let mut nav_panel = NavigationPanel::new(&book_manager);

    // Switch to TOC mode with our test book
    let book_info = create_test_book_info_with_toc();
    nav_panel.switch_to_toc_mode(0, book_info);

    // Move down to middle to test gg from a non-top position
    for _ in 0..10 {
        nav_panel.move_selection_down();
    }

    // Test gg (go to top)
    nav_panel.handle_gg();

    // Render only the navigation panel
    terminal
        .draw(|f| {
            let area = f.size();
            let palette = get_test_palette();
            let bookmarks = Bookmarks::ephemeral();
            nav_panel.render(f, area, false, &palette, &bookmarks, &book_manager);
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_nav_panel_vim_gg_component.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/nav_panel_vim_gg_component.svg"),
        "test_navigation_panel_vim_motion_gg",
        create_test_failure_handler("test_navigation_panel_vim_motion_gg"),
    );
}

#[test]
fn test_text_reader_vim_motion_g() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(50, 20);

    let mut text_reader = TextReader::new();

    // Create test content with many lines
    let test_content = (0..=100)
        .map(|i| {
            format!(
                "This is line {}. Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
                i
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    println!("Created {} input lines", test_content.lines().count());

    // Let the text reader calculate its own dimensions based on the area
    let area = terminal.get_frame().size();
    text_reader.update_wrapped_lines_if_needed(&test_content, area);

    println!("Total wrapped lines: {}", text_reader.total_wrapped_lines);
    println!("Visible height: {}", text_reader.visible_height);
    println!("Scroll offset before G: {}", text_reader.scroll_offset);

    // Test G (go to bottom)
    text_reader.handle_G();

    println!("Scroll offset after G: {}", text_reader.scroll_offset);
    println!(
        "Should show lines {} to {}",
        text_reader.scroll_offset,
        text_reader.scroll_offset + text_reader.visible_height - 1
    );

    // Debug: Check the last few lines of content and actual wrapping
    let lines: Vec<&str> = test_content.lines().collect();
    for i in 98..=100 {
        println!("Line {}: len={}", i, lines[i].len());
    }

    // Check what width is actually being used for text
    println!("Area width: {}, height: {}", area.width, area.height);
    let text_width = area.width.saturating_sub(12) as usize;
    println!("Text width for wrapping: {}", text_width);

    // Render only the text reader
    terminal
        .draw(|f| {
            let area = f.size();
            let palette = get_test_palette();
            let chapter_title = Some("Chapter 1".to_string());
            text_reader.render(
                f,
                area,
                &test_content,
                &chapter_title,
                1,
                5,
                &palette,
                true,
                None,
                None,
            );
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    // Debug: print what we expect to see
    println!("\nExpected to see at bottom (accounting for wrapping):");
    println!(
        "Line 100 wraps to: 'This is line 100. Lorem ipsum dolor sit amet,' + 'consectetur adipiscing elit.'"
    );

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_text_reader_vim_g_component.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/text_reader_vim_g_component.svg"),
        "test_text_reader_vim_motion_g",
        create_test_failure_handler("test_text_reader_vim_motion_g"),
    );
}

#[test]
fn test_text_reader_vim_motion_gg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(50, 20);

    let mut text_reader = TextReader::new();

    // Create test content
    let test_content = (0..=100)
        .map(|i| {
            format!(
                "This is line {}. Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
                i
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Update wrapped lines
    text_reader.update_wrapped_lines(&test_content, 48, 18);

    // Scroll down first
    text_reader.scroll_offset = 50;

    // Test gg (go to top)
    text_reader.handle_gg();
    text_reader.handle_j();
    // Render only the text reader
    terminal
        .draw(|f| {
            let area = f.size();
            let palette = get_test_palette();
            let chapter_title = Some("Chapter 1".to_string());
            text_reader.render(
                f,
                area,
                &test_content,
                &chapter_title,
                1,
                5,
                &palette,
                true,
                None,
                None,
            );
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_text_reader_vim_gg_component.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        &std::path::Path::new("tests/snapshots/text_reader_vim_gg_component.svg"),
        "test_text_reader_vim_motion_gg",
        create_test_failure_handler("test_text_reader_vim_motion_gg"),
    );
}
