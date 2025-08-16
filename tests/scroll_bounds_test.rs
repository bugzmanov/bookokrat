use bookrat::text_reader::TextReader;

#[test]
fn test_expected_behavior_scroll_bounds() {
    let mut reader = TextReader::new();

    // Create content that's longer than the screen
    let mut lines = Vec::new();
    for i in 1..=20 {
        lines.push(format!("Line {}", i));
    }
    let content = lines.join("\n");
    let visible_height = 5; // Screen shows 5 lines at once

    // With 20 lines of content and 5 visible lines,
    // the maximum scroll offset should be 20 - 5 = 15
    let expected_max_scroll = content.lines().count() - visible_height;

    // Scroll down many times
    for i in 0..25 {
        reader.scroll_down();
        println!(
            "Scroll {}: offset={}, max_should_be={}",
            i + 1,
            reader.scroll_offset,
            expected_max_scroll
        );

        // This assertion will fail, demonstrating the bug
        if reader.scroll_offset > expected_max_scroll {
            panic!(
                "BUG: After {} scrolls, offset ({}) exceeded max ({})",
                i + 1,
                reader.scroll_offset,
                expected_max_scroll
            );
        }
    }
}
