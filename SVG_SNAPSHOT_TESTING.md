# SVG Snapshot Testing with snapbox

This document explains how SVG-based snapshot testing is implemented in BookRat using snapbox and anstyle-svg.

## Overview

SVG snapshot testing captures the terminal output as styled SVG images, providing visual regression testing that preserves colors, formatting, and layout. This is superior to plain text snapshots because it captures the complete visual appearance of the TUI.

## Setup

### Dependencies

Added to `Cargo.toml`:
```toml
[dev-dependencies]
snapbox = { version = "0.6", features = ["term-svg"] }
anstyle = "1.0"
anstyle-svg = "0.1.5"
```

### Key Components

1. **SVG Conversion** (`tests/svg_snapshots.rs`):
   - Converts ratatui terminal buffer to ANSI escape sequences
   - Uses `anstyle-svg::Term` to render ANSI as SVG
   - Preserves colors, styles, and formatting

2. **Snapshot Testing**:
   - Uses `snapbox::assert_data_eq!` for comparisons
   - Stores SVG files in `tests/snapshots/`
   - Supports overwrite mode for generating new snapshots

## Writing SVG Snapshot Tests

### Basic Test Structure

```rust
#[test]
fn test_file_list_svg() {
    let mut terminal = create_test_terminal(80, 24);
    let mut app = App::new();
    
    terminal.draw(|f| app.draw(f)).unwrap();
    let svg_output = terminal_to_svg(&terminal);
    
    snapbox::assert_data_eq!(
        svg_output,
        snapbox::Data::read_from(&std::path::Path::new("tests/snapshots/file_list.svg"), None)
    );
}
```

### Available Tests

1. **File List View** (`test_file_list_svg`):
   - Tests the initial book selection interface
   - Captures file list with timestamps and styling

2. **Content View** (`test_content_view_svg`):
   - Tests the reading interface layout
   - Shows the expanded content area

## Running SVG Snapshot Tests

### Generate New Snapshots
```bash
SNAPSHOTS=overwrite cargo test test_file_list_svg
SNAPSHOTS=overwrite cargo test test_content_view_svg
```

### Run All SVG Tests
```bash
cargo test svg_snapshots
```

### View Generated SVGs
The SVG files can be opened in any web browser or SVG viewer to inspect the visual output.

## SVG Conversion Process

1. **Terminal Buffer Extraction**: Reads each cell from the ratatui TestBackend
2. **ANSI Generation**: Converts ratatui colors and styles to ANSI escape sequences
3. **SVG Rendering**: Uses `anstyle-svg::Term::render_svg()` to create SVG output

### Color Mapping

The system maps ratatui colors to ANSI equivalents:
- **Basic colors**: Black, Red, Green, Yellow, Blue, Magenta, Cyan, White
- **Bright colors**: Light variants of basic colors  
- **RGB colors**: True color support with `\u{1b}[38;2;r;g;b;m`
- **Indexed colors**: 256-color palette support

### Style Support

- **Bold**: `\u{1b}[1m`
- **Italic**: `\u{1b}[3m`  
- **Underlined**: `\u{1b}[4m`
- **Background colors**: Mapped to appropriate ANSI background codes

## Generated SVG Features

- **Proper dimensions**: Matches terminal size (80x24, 100x30, etc.)
- **Monospace font**: Uses `SFMono-Regular, Consolas, Liberation Mono, Menlo`
- **Color preservation**: Maintains the Oceanic Next theme colors
- **Scalable**: SVG format scales cleanly for documentation

## Benefits

1. **Visual Regression Testing**: Catches layout and styling changes
2. **Documentation**: SVG files serve as visual documentation
3. **Cross-platform**: Consistent rendering regardless of terminal
4. **Version Control Friendly**: Text-based SVG format diffs well
5. **Debugging**: Easy to inspect visual output during development

## Workflow

1. **Write Test**: Create test with expected terminal state
2. **Generate Snapshot**: Run with `SNAPSHOTS=overwrite` to create initial SVG
3. **Verify Visual**: Open SVG file to confirm it looks correct
4. **Commit**: Add both test code and SVG snapshot to version control
5. **Continuous Testing**: Future runs will compare against the stored SVG

## Example Output

The generated SVG files show:
- Proper terminal borders and layout
- Color-coded text (file names, timestamps, borders)
- Correct spacing and alignment
- Theme-appropriate styling

## Debugging

- **Debug files**: Tests create `debug_*.svg` files for inspection
- **Terminal capture**: Can capture intermediate states during user interactions
- **Size consistency**: Use fixed terminal dimensions for reproducible results

This approach provides comprehensive visual testing for the TUI application, ensuring that UI changes are intentional and properly styled.