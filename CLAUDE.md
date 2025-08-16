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

BookRat is a terminal user interface (TUI) EPUB reader written in Rust. It provides a comprehensive reading experience with features including:

- Hierarchical table of contents navigation with expandable sections
- Text selection with mouse support (single, double, and triple-click)
- Reading history tracking with quick access popup
- External EPUB reader integration
- Vim-like keybindings throughout the interface
- Bookmark persistence and reading progress tracking
- Cross-platform support (macOS, Windows, Linux)

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
- **EPUB Inspector**: `cargo run --example epub_inspector <file.epub>` - Extracts and displays raw HTML content from EPUB chapters for debugging text processing issues
- **Panic Test**: `cargo run --example panic_test` - Interactive test to verify panic handler properly restores mouse functionality
- **Simulated Input Example**: `cargo run --example simulated_input_example` - Demonstrates running the app with simulated keyboard input for testing

## Architecture

### Core Components

1. **main.rs** - Entry point and terminal setup
   - Terminal initialization and panic handling
   - Main event loop bootstrapping
   - Application lifecycle management

2. **main_app.rs** - Core application logic (src/main_app.rs)
   - `App` struct: Central state management and component orchestration
   - `FocusedPanel` enum: Tracks which panel has keyboard focus
   - High-level action handling (open book, navigate chapters, switch modes)
   - Mouse event batching and processing
   - Vim-like keybinding support with multi-key sequences
   - Text selection and clipboard integration
   - Bookmark management with throttled saving
   - Reading history popup management

3. **bookmark.rs** - Bookmark persistence (src/bookmark.rs)
   - `Bookmark` struct: Stores chapter, scroll position, and timestamp
   - `Bookmarks` struct: Manages bookmarks for multiple books
   - JSON-based persistence to `bookmarks.json`
   - Tracks last read timestamp using chrono

4. **book_manager.rs** - Book discovery and management (src/book_manager.rs)
   - `BookManager` struct: Manages EPUB file discovery
   - `BookInfo` struct: Stores book path and display name
   - Automatic scanning of current directory for EPUB files
   - EPUB document loading and validation

5. **book_list.rs** - File browser UI component (src/book_list.rs)
   - `BookList` struct: Manages book selection UI
   - Displays books with last read timestamps
   - Integrated with bookmark system for showing reading history
   - Implements `VimNavMotions` for consistent navigation

6. **navigation_panel.rs** - Left panel navigation manager (src/navigation_panel.rs)
   - `NavigationPanel` struct: Manages mode switching between book list and TOC
   - `NavigationMode` enum: BookSelection vs TableOfContents
   - Renders appropriate sub-component based on mode
   - Handles mouse clicks and keyboard navigation
   - Extracts user actions for the main app

7. **table_of_contents.rs** - Hierarchical TOC display (src/table_of_contents.rs)
   - `TableOfContents` struct: Manages TOC rendering and interaction
   - `TocItem` enum: ADT for Chapter vs Section with children
   - Expandable/collapsible sections
   - Current chapter highlighting
   - Mouse and keyboard navigation support

8. **toc_parser.rs** - EPUB TOC extraction (src/toc_parser.rs)
   - `TocParser` struct: Parses NCX (EPUB2) and Nav (EPUB3) documents
   - Hierarchical structure extraction
   - Resource discovery and format detection
   - Robust regex-based content extraction

9. **text_reader.rs** - Reading view component (src/text_reader.rs)
   - `TextReader` struct: Manages text display and scrolling
   - Reading time calculation (250 WPM default)
   - Chapter progress percentage tracking
   - Smooth scrolling with acceleration
   - Half-screen scrolling with visual highlights
   - Text selection integration
   - Implements `VimNavMotions` for consistent navigation

10. **text_selection.rs** - Text selection system (src/text_selection.rs)
    - `TextSelection` struct: Manages selection state and rendering
    - Mouse-driven selection (drag, double-click for word, triple-click for paragraph)
    - Multi-line selection support
    - Clipboard integration via arboard
    - Visual highlighting with customizable colors
    - Coordinate validation and conversion

11. **text_generator.rs** - HTML to text conversion (src/text_generator.rs)
    - `TextGenerator` struct: Processes EPUB HTML content
    - Regex-based HTML cleaning
    - Chapter title extraction
    - Paragraph formatting with proper indentation
    - Entity decoding (e.g., &mdash;, &ldquo;)
    - Code block detection and formatting

12. **reading_history.rs** - Recent books popup (src/reading_history.rs)
    - `ReadingHistory` struct: Manages history display and interaction
    - Extracts recent books from bookmarks
    - Chronological sorting with deduplication
    - Popup overlay with centered layout
    - Mouse and keyboard navigation
    - Implements `VimNavMotions` for consistent navigation

13. **system_command.rs** - External application integration (src/system_command.rs)
    - `SystemCommandExecutor` trait: Abstraction for system commands
    - Cross-platform file opening (macOS, Windows, Linux)
    - EPUB reader detection (Calibre, ClearView, Skim, FBReader)
    - Chapter-specific navigation support
    - Mockable interface for testing

14. **event_source.rs** - Input event abstraction (src/event_source.rs)
    - `EventSource` trait: Abstraction for event polling/reading
    - `KeyboardEventSource`: Real crossterm-based implementation
    - `SimulatedEventSource`: Mock for testing
    - Helper methods for creating test events

15. **theme.rs** - Color theming (src/theme.rs)
    - `Base16Palette` struct: Color scheme definition
    - Oceanic Next theme implementation
    - Dynamic color selection based on UI mode

16. **panic_handler.rs** - Enhanced panic handling (src/panic_handler.rs)
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
- `arboard` (3.4): Clipboard integration
- `zip` (0.6): EPUB file handling
- `tempfile` (3.8): Temporary file management
- `textwrap` (0.16): Text wrapping utilities
- `pprof` (0.13): Performance profiling support

### State Management
The application maintains state through the `App` struct in `main_app.rs` which includes:
- Current EPUB document and chapter information
- Navigation panel with mode switching (book list vs TOC)
- Text reader with scroll position and content state
- Text selection state and clipboard integration
- Reading history popup state
- Bookmark management with throttled saves
- Book manager for file discovery
- Focus tracking between panels
- Multi-key sequence handling for vim motions
- Mouse event batching for smooth scrolling

### Content Processing Pipeline
1. EPUB file is opened and validated
2. Table of contents is parsed from NCX or Nav documents
3. Chapter HTML content is extracted via epub crate
4. Chapter title is extracted from h1/h2/title tags
5. HTML is cleaned (scripts, styles removed)
6. HTML entities are decoded
7. Code blocks are detected and preserved
8. Tags are converted to text formatting
9. Paragraphs are indented for readability
10. Text is wrapped to terminal width

### User Interface Features
- **Navigation Panel**: Switchable between book list and table of contents
- **File Browser Mode**: Lists all EPUB files with last read timestamps
- **Table of Contents**: Hierarchical view with expandable sections
- **Reading Mode**: Displays formatted text with chapter info
- **Text Selection**: Mouse-driven selection with clipboard support
- **Reading History**: Quick access popup for recently read books
- **Progress Tracking**: Shows chapter number, reading progress %, and time remaining
- **External Reader Integration**: Open books in GUI EPUB readers
- **Responsive Design**: Adjusts to terminal size changes
- **Vim Navigation**: Consistent vim-like keybindings throughout

### Keyboard Controls
- `j`/`k`: Navigate file list, TOC, or scroll content (line by line)
- `Ctrl+d`/`Ctrl+u`: Scroll half screen down/up with highlight
- `h`/`l`: Navigate between chapters
- `Tab`: Switch focus between navigation panel and content view
- `Enter`: Select a file/chapter to read or expand/collapse TOC sections
- `Space`: Expand/collapse TOC sections
- `b`: Toggle between book list and table of contents
- `Shift + h`: Show reading history popup
- `Ctrl+o`: Open current book in external EPUB reader
- `g`/`G`: Go to top/bottom (vim-style)
- `gg`: Go to beginning (vim-style multi-key)
- `q`: Quit the application
- `Esc`: Cancel text selection or close popups

### Mouse Controls
- **Click**: Select items in lists or TOC
- **Drag**: Select text in reading area
- **Double-click**: Select word
- **Triple-click**: Select paragraph
- **Scroll**: Scroll content or navigate lists

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

## Architecture Patterns

### Design Principles
- **Trait-based abstraction**: Key external dependencies (`EventSource`, `SystemCommandExecutor`) are abstracted behind traits for testability
- **Component delegation**: The `NavigationPanel` manages mode switching and delegates rendering to appropriate sub-components
- **ADT modeling**: The `TocItem` enum uses algebraic data types for type-safe hierarchical structures
- **Consistent navigation**: The `VimNavMotions` trait provides uniform vim-style navigation across all components
- **Mock-friendly design**: All external interactions are abstracted to enable comprehensive testing

### Component Communication
1. **Main App Orchestration**: `main_app.rs` coordinates all components and handles high-level application logic
2. **Event Flow**: Events flow from `event_source.rs` → `main_app.rs` → relevant components
3. **Panel Focus**: The `FocusedPanel` enum determines which component receives keyboard events
4. **Action Propagation**: Components return actions (e.g., `SelectedActionOwned`) that the main app processes
5. **State Updates**: State changes trigger re-renders through the main render loop

## Important Notes
- The application scans the current directory for EPUB files on startup
- Bookmarks are automatically saved when navigating between chapters or files
- The most recently read book is auto-loaded on startup
- Logging is written to `bookrat.log` for debugging
- The TUI uses vim-like keybindings throughout all components
- Reading speed is set to 250 words per minute for time calculations
- Scroll acceleration increases speed when holding down scroll keys
- Mouse events are batched to prevent flooding and ensure smooth scrolling
- Text selection automatically scrolls the view when dragging near edges
- The application supports both EPUB2 (NCX) and EPUB3 (Nav) table of contents formats
- External EPUB readers are detected based on the platform (macOS, Windows, Linux)

- one of the most important aspects of this project is perfomance. never make a significant change like switching a library if that wasn't instructed and if it's clearly would generate worse performance results