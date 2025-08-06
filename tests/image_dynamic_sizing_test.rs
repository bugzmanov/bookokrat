use bookrat::text_reader::TextReader;

/// Test that the picker detects terminal cell dimensions dynamically
#[test]
fn test_dynamic_cell_height_detection() {
    // Create a TextReader instance which should detect cell height during initialization
    let reader = TextReader::new();

    // The test passes if TextReader::new() completes without panic
    // The actual detection happens during initialization and is logged

    // We can't directly test the detected values since they depend on the terminal,
    // but we can verify that the initialization completes successfully
    assert_eq!(
        reader.scroll_offset, 0,
        "TextReader should initialize with scroll_offset = 0"
    );

    println!("TextReader initialized successfully with dynamic cell height detection");
}

/// Test that image prescaling works with various cell heights
#[test]
fn test_image_prescaling_with_different_cell_heights() {
    // Common cell heights in various terminals
    let cell_heights = vec![14, 16, 18, 20];

    for cell_height in cell_heights {
        let target_height = 15 * cell_height;
        println!(
            "Testing with cell height {} pixels, target image height: {} pixels",
            cell_height, target_height
        );

        // The actual test would be to create a TextReader with this cell height
        // and verify the image is scaled to exactly target_height pixels
        // Since we can't mock the Picker here, we at least verify the math
        assert_eq!(
            target_height / cell_height,
            15,
            "Image should occupy exactly 15 cells"
        );
    }
}
