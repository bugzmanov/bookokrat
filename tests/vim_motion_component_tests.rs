use bookokrat::book_manager::{BookFormat, BookInfo, BookManager};
use bookokrat::main_app::VimNavMotions;
use bookokrat::markdown_text_reader::{ActiveSection, MarkdownTextReader};
use bookokrat::navigation_panel::{CurrentBookInfo, NavigationMode, NavigationPanel};
use bookokrat::table_of_contents::TocItem;
use bookokrat::test_utils::test_helpers::create_test_terminal;
use bookokrat::theme::Base16Palette;

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
            display_name: format!("Book {i}"),
            path: "book1.epub".to_string(),
            format: BookFormat::Epub,
        })
    }
    book_manager.books = books;
    book_manager
}

// Get default theme palette
fn get_test_palette() -> &'static Base16Palette {
    bookokrat::theme::OCEANIC_NEXT
}

#[test]
fn test_book_list_vim_motion_g() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(30, 10); // Small terminal to focus on component

    let book_manager = create_test_book_manager();
    let mut nav_panel = NavigationPanel::new(&book_manager);

    // Ensure we're in book selection mode
    assert_eq!(nav_panel.mode, NavigationMode::BookSelection);

    nav_panel.handle_upper_g();
    terminal
        .draw(|f| {
            let area = f.area();
            let palette = get_test_palette();
            nav_panel.render(f, area, false, palette, &book_manager);
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
        std::path::Path::new("tests/snapshots/book_list_vim_g_component.svg"),
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
    terminal
        .draw(|f| {
            let area = f.area();
            let palette = get_test_palette();
            nav_panel.render(f, area, false, palette, &book_manager);
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
        std::path::Path::new("tests/snapshots/book_list_vim_gg_component.svg"),
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
            title: format!("Chapter {i}"),
            href: format!("chapter{i}.xhtml"),
            anchor: None,
        });
    }

    CurrentBookInfo {
        path: "test_book.epub".to_string(),
        toc_items,
        current_chapter: 0,
        current_chapter_href: Some("chapter1.xhtml".to_string()),
        active_section: ActiveSection::new(0, "chapter1.xhtml".to_string(), None),
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
    nav_panel.switch_to_toc_mode(book_info);

    // Test G (go to bottom)
    nav_panel.handle_upper_g();

    terminal
        .draw(|f| {
            let area = f.area();
            let palette = get_test_palette();
            nav_panel.render(f, area, false, palette, &book_manager);
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
        std::path::Path::new("tests/snapshots/nav_panel_vim_g_component.svg"),
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
    nav_panel.switch_to_toc_mode(book_info);

    // Move down to middle to test gg from a non-top position
    for _ in 0..10 {
        nav_panel.move_selection_down();
    }

    // Test gg (go to top)
    nav_panel.handle_gg();

    terminal
        .draw(|f| {
            let area = f.area();
            let palette = get_test_palette();
            nav_panel.render(f, area, false, palette, &book_manager);
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
        std::path::Path::new("tests/snapshots/nav_panel_vim_gg_component.svg"),
        "test_navigation_panel_vim_motion_gg",
        create_test_failure_handler("test_navigation_panel_vim_motion_gg"),
    );
}

#[test]
fn test_text_reader_vim_motion_g() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(50, 20);

    let mut text_reader = MarkdownTextReader::new();

    // Create test content with many lines
    let test_content = (0..=100)
        .map(|i| {
            format!("This is line {i}. Lorem ipsum dolor sit amet, consectetur adipiscing elit.")
        })
        .collect::<Vec<_>>()
        .join("\n");

    text_reader.set_content_from_string(&test_content, None);

    // Test G (go to bottom)
    text_reader.handle_upper_g();

    terminal
        .draw(|f| {
            let area = f.area();
            let palette = get_test_palette();
            text_reader.render(f, area, 1, 5, palette, true, false, false);
        })
        .unwrap();

    let svg_output = terminal_to_svg(&terminal);

    std::fs::create_dir_all("tests/snapshots").unwrap();
    std::fs::write(
        "tests/snapshots/debug_text_reader_vim_g_component.svg",
        &svg_output,
    )
    .unwrap();

    assert_svg_snapshot(
        svg_output.clone(),
        std::path::Path::new("tests/snapshots/text_reader_vim_g_component.svg"),
        "test_text_reader_vim_motion_g",
        create_test_failure_handler("test_text_reader_vim_motion_g"),
    );
}

#[test]
fn test_text_reader_vim_motion_gg() {
    ensure_test_report_initialized();
    let mut terminal = create_test_terminal(50, 20);

    let mut text_reader = MarkdownTextReader::new();

    // Create test content
    let test_content = (0..=100)
        .map(|i| {
            format!("This is line {i}. Lorem ipsum dolor sit amet, consectetur adipiscing elit.")
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Update wrapped lines
    text_reader.set_content_from_string(&test_content, None);

    // Scroll down first
    text_reader.scroll_down();

    // Test gg (go to top)
    text_reader.handle_j();
    text_reader.handle_j();
    text_reader.handle_gg();
    terminal
        .draw(|f| {
            let area = f.area();
            let palette = get_test_palette();
            text_reader.render(f, area, 1, 5, palette, true, false, false);
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
        std::path::Path::new("tests/snapshots/text_reader_vim_gg_component.svg"),
        "test_text_reader_vim_motion_gg",
        create_test_failure_handler("test_text_reader_vim_motion_gg"),
    );
}
