# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## CRITICAL RULES FOR AI ASSISTANTS

1. **Testing**: ALWAYS use the existing SVG-based snapshot testing in `tests/svg_snapshots.rs`. NEVER introduce new testing frameworks or approaches.
2. **Golden Snapshots**: NEVER update golden snapshot files with `SNAPSHOTS=overwrite` unless explicitly requested by the user. This is critical for test integrity.
3. **Test Updates**: NEVER update any test files or test expectations unless explicitly requested by the user. This includes unit tests, integration tests, and snapshot tests.
4. **File Creation**: Prefer editing existing files over creating new ones. Only create new files when absolutely necessary.
5. **Code Formatting**: NEVER manually reformat code or change indentation/line breaks. ONLY use `cargo fmt` for all formatting. When editing code, preserve the existing formatting exactly and let `cargo fmt` handle any formatting changes.
6. **Final Formatting**: ALWAYS run `cargo fmt` before reporting task completion if any code changes were made. This ensures consistent code formatting and prevents formatting-related changes in future edits.

## Project Overview

BookRat is a terminal user interface (TUI) EPUB reader written in Rust. It allows users to browse and read EPUB files from the terminal with features like chapter navigation, scrolling, bookmark persistence, and reading progress tracking.

## Key Commands

### Development
- Build: `cargo build --release`
- Run: `cargo run`
- Check code: `cargo check`
- Run linter: `cargo clippy`
- Run tests: `cargo test`
- Format code: `cargo fmt`

### Testing
- Run all tests: `cargo test`
- Run specific test: `cargo test <test_name>`
- Run tests with output: `cargo test -- --nocapture`

### Development Tools
- EPUB Inspector: `cargo run --example epub_inspector <file.epub>` - Extracts and displays raw HTML content from EPUB chapters for debugging text processing issues
- Panic Test: `cargo run --example panic_test` - Interactive test to verify panic handler properly restores mouse functionality

## Architecture

### Core Components

1. **main.rs** - Main application logic (src/main.rs:1-450+)
   - `App` struct: Central state management for the TUI
   - `Mode` enum: Tracks UI mode (FileList vs Content)
   - Event loop handling keyboard input and rendering
   - Animation system for smooth transitions
   - Auto-loading of most recently read book

2. **bookmark.rs** - Bookmark persistence (src/bookmark.rs:1-68)
   - `Bookmark` struct: Stores chapter, scroll position, and timestamp
   - `Bookmarks` struct: Manages bookmarks for multiple books
   - JSON-based persistence to `bookmarks.json`
   - Tracks last read timestamp using chrono

3. **book_manager.rs** - Book discovery and management (src/book_manager.rs:1-101)
   - `BookManager` struct: Manages EPUB file discovery
   - `BookInfo` struct: Stores book path and display name
   - Automatic scanning of current directory for EPUB files
   - EPUB document loading and validation

4. **book_list.rs** - File browser UI component (src/book_list.rs:1-92)
   - `BookList` struct: Manages book selection UI
   - Displays books with last read timestamps
   - Integrated with bookmark system for showing reading history

5. **text_reader.rs** - Reading view component (src/text_reader.rs:1-369)
   - `TextReader` struct: Manages text display and scrolling
   - Reading time calculation (250 WPM default)
   - Chapter progress percentage tracking
   - Styled text parsing (bold, quotes, etc.)
   - Smooth scrolling with acceleration
   - Half-screen scrolling with visual highlights
   - Responsive text wrapping

6. **text_generator.rs** - HTML to text conversion (src/text_generator.rs:1-171)
   - `TextGenerator` struct: Processes EPUB HTML content
   - Regex-based HTML cleaning
   - Chapter title extraction
   - Paragraph formatting with proper indentation
   - Entity decoding (e.g., &mdash;, &ldquo;)

7. **theme.rs** - Color theming (src/theme.rs:1-53)
   - `Base16Palette` struct: Color scheme definition
   - Oceanic Next theme implementation
   - Dynamic color selection based on UI mode

8. **panic_handler.rs** - Enhanced panic handling (src/panic_handler.rs:1-66)
   - `initialize_panic_handler()`: Sets up panic hooks based on build type
   - Debug builds: Uses `better-panic` for detailed backtraces
   - Release builds: Uses `human-panic` for user-friendly crash reports
   - Terminal state restoration on panic to prevent broken terminal
   - Proper mouse capture restoration to maintain mouse functionality post-panic

### Key Dependencies (Cargo.toml)
- `ratatui` (0.26.1): Terminal UI framework
- `crossterm` (0.27.0): Cross-platform terminal manipulation
- `epub` (1.1.0): EPUB file parsing
- `regex` (1.10.3): HTML tag processing
- `serde`/`serde_json`: Bookmark serialization
- `chrono` (0.4): Timestamp handling
- `anyhow` (1.0.79): Error handling
- `simplelog` (0.12.1): Logging framework
- `log` (0.4): Logging facade
- `better-panic` (0.3): Enhanced panic handling with backtraces (debug builds)
- `human-panic` (2.0): User-friendly crash reports (release builds)
- `libc` (0.2): System interface for exit codes

### State Management
The application maintains state through the `App` struct which includes:
- Current EPUB document and chapter information
- Scroll position and content length
- UI mode (file browser vs reader)
- Animation state for smooth transitions
- Bookmark management
- Book manager for file discovery
- Theme-aware rendering

### Content Processing Pipeline
1. EPUB HTML content is extracted via epub crate
2. Chapter title is extracted from h1/h2/title tags
3. HTML is cleaned (scripts, styles removed)
4. HTML entities are decoded
5. Tags are converted to text formatting
6. Paragraphs are indented for readability
7. Text is wrapped to terminal width

### User Interface Features
- **File Browser Mode**: Lists all EPUB files with last read timestamps
- **Reading Mode**: Displays formatted text with chapter info
- **Progress Tracking**: Shows chapter number, reading progress %, and time remaining
- **Smooth Animations**: Transitions between modes
- **Responsive Design**: Adjusts to terminal size changes

### Keyboard Controls
- `j`/`k`: Navigate file list or scroll content (line by line)
- `Ctrl+d`/`Ctrl+u`: Scroll half screen down/up with highlight
- `h`/`l`: Navigate between chapters
- `Tab`: Switch between file list and content view
- `Enter`: Select a file to read
- `q`: Quit the application

## Snapshot Testing

**IMPORTANT FOR AI ASSISTANTS:**
1. **ALWAYS use SVG-based snapshot tests** - All UI tests MUST use the existing SVG snapshot testing infrastructure in `tests/svg_snapshots.rs`. DO NOT introduce any new testing approaches or frameworks.
2. **NEVER update golden snapshots without explicit permission** - Golden snapshot files in `tests/snapshots/` should NEVER be updated with `SNAPSHOTS=overwrite` unless the user explicitly asks for it. This is critical for maintaining test integrity.

BookRat uses visual snapshot testing for its terminal UI to ensure the rendering remains consistent across changes.

### Running Snapshot Tests

```bash
# Run snapshot tests
cargo test --test svg_snapshots

# Run with automatic browser report opening
OPEN_REPORT=1 cargo test --test svg_snapshots
```

### When Tests Fail

When snapshot tests fail, the system generates a comprehensive HTML report showing:
- Side-by-side visual comparison (Expected vs Actual)
- Line statistics and diff information
- Buttons to copy update commands to clipboard

The report is saved to: `target/test-reports/svg_snapshot_report.html`

### Updating Snapshots

After reviewing the visual differences, you can update snapshots in two ways:

1. **Update individual test**: Click "📋 Copy Update Command" button in the report
   ```bash
   SNAPSHOTS=overwrite cargo test test_file_list_svg
   ```

2. **Update all snapshots**: Click "📋 Copy Update All Command" button
   ```bash
   SNAPSHOTS=overwrite cargo test --test svg_snapshots
   ```

The `SNAPSHOTS=overwrite` environment variable tells snapbox to update the snapshot files with the current test output instead of failing when differences are found.

### Test Architecture

The snapshot testing system consists of:

1. **svg_snapshots.rs** - Main test file that renders the TUI and captures SVG output. ALL NEW UI TESTS MUST BE ADDED HERE.
2. **snapshot_assertions.rs** - Custom assertion function that compares snapshots
3. **test_report.rs** - Generates the HTML visual diff report
4. **visual_diff.rs** - Creates visual comparisons (no longer used directly)

When adding new tests:
- Add them to `tests/svg_snapshots.rs` following the existing pattern
- Use `terminal_to_svg()` to convert terminal output to SVG
- Use `assert_svg_snapshot()` for assertions
- Never create new test files or testing approaches

### Working with New Snapshot Tests

**CRITICAL FOR AI ASSISTANTS:**
When adding a new snapshot test, it is **expected and normal** for the test to fail initially because there is no saved golden snapshot file yet. This is not an error - it's the intended workflow.

**Key Points:**
1. **Test failure is expected** - New snapshot tests will always fail on first run since no golden snapshot exists
2. **Focus on the generated snapshot** - When a new test fails, examine the debug SVG file (e.g., `tests/snapshots/debug_test_name.svg`) to verify it shows what the test scenario should display
3. **Analyze the visual output** - Check that the generated snapshot accurately represents the UI state being tested
4. **Verify test correctness** - Ensure the snapshot captures the intended behavior, UI elements, status messages, etc.
5. **Only then consider updating** - If the generated snapshot looks correct for the test scenario, then it may be appropriate to create the golden snapshot

**Example Workflow:**
1. Add new test to `tests/svg_snapshots.rs`
2. Run the test - it will fail (this is expected)
3. Examine the debug SVG file to see the actual rendered output
4. Verify the output matches what the test scenario should produce
5. If correct, the golden snapshot can be created; if incorrect, fix the test logic first

This approach ensures that snapshot tests accurately capture the intended UI behavior rather than just making tests pass.

### Environment Variables

- `OPEN_REPORT=1` - Automatically opens the HTML report in your default browser
- `SNAPSHOTS=overwrite` - Updates snapshot files with current test output

### Workflow

1. Make changes to the TUI code
2. Run `cargo test --test svg_snapshots`
3. If tests fail, review the HTML report (saved to `target/test-reports/`)
4. Click to copy the update command for accepted changes
5. Paste and run the command to update snapshots
6. Commit the updated snapshot files

### Tips

- Always review visual changes before updating snapshots
- The report uses synchronized scrolling for easy comparison
- Each test can be updated individually or all at once
- Snapshot files are stored in `tests/snapshots/`

## Important Notes
- The application scans the current directory for EPUB files on startup
- Bookmarks are automatically saved when navigating between chapters or files
- The most recently read book is auto-loaded on startup
- Logging is written to `bookrat.log` for debugging
- The TUI uses vim-like keybindings
- Reading speed is set to 250 words per minute for time calculations
- Scroll acceleration increases speed when holding down scroll keys