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
- Embedded image display with dynamic sizing and placeholder support
- Syntax highlighting for code blocks
- Link display and handling
- Background image loading and caching
- Image popup viewer for detailed image viewing
- Performance profiling support
- FPS monitoring for UI performance
- MathML rendering with ASCII art conversion
- Markdown AST-based text processing pipeline
- Multiple HTML parsing implementations (regex and html5ever)

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
- **MathML Test**: `cargo run --example test_mathml_rust` - Tests MathML parsing and ASCII rendering functionality

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
   - Image popup display and interaction
   - Performance profiling integration with pprof
   - FPS monitoring through `FPSCounter` struct

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
   - Embedded image display with dynamic sizing
   - Image placeholders with loading status
   - Link information extraction and display
   - Auto-scroll functionality during text selection
   - Raw HTML viewing mode toggle
   - Background image loading coordination

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
    - Table parsing and formatting
    - Image tag extraction and placeholder insertion
    - Link extraction and formatting
    - Syntax highlighting support for code blocks

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

17. **syntax_highlighter.rs** - Code syntax highlighting (src/syntax_highlighter.rs)
    - `SyntaxHighlighter` struct: Provides syntax highlighting for code blocks
    - Uses syntect library for highlighting
    - Multiple theme support (Monokai, base16 themes)
    - Language detection and appropriate syntax rules
    - Converts highlighted code to ratatui-compatible styled text

18. **mathml_renderer.rs** - MathML to ASCII conversion (src/mathml_renderer.rs)
    - `MathMLParser` struct: Converts MathML expressions to terminal-friendly ASCII art
    - `MathBox` struct: Represents rendered mathematical expressions with positioning
    - Unicode subscript/superscript support for improved readability
    - LaTeX notation fallback for complex expressions
    - Comprehensive fraction, square root, and summation rendering
    - Multi-line parentheses for complex expressions
    - Baseline alignment for proper mathematical layout

19. **markdown.rs** - Markdown AST definitions (src/markdown.rs)
    - `Document` struct: Root container for parsed content
    - `Node` struct: Individual content blocks with source tracking
    - `Block` enum: Different content types (heading, paragraph, code, table, etc.)
    - `Text` struct: Rich text with formatting and inline elements
    - `Style` enum: Text formatting options (emphasis, strong, code, strikethrough)
    - `Inline` enum: Inline elements (links, images, line breaks)
    - `HeadingLevel` enum: H1-H6 heading levels
    - Complete table support structures (rows, cells, alignment)

### Parsing Components (src/parsing/)

20. **html_to_markdown.rs** - HTML to Markdown AST conversion (src/parsing/html_to_markdown.rs)
    - `HtmlToMarkdownConverter` struct: Converts HTML content to clean Markdown AST
    - Uses html5ever for robust DOM parsing and traversal
    - Handles various HTML elements (headings, paragraphs, images, MathML)
    - Integrates MathML processing with mathml_to_ascii conversion
    - Preserves text formatting and inline elements during conversion
    - Entity decoding for proper text representation

21. **markdown_renderer.rs** - Markdown AST to string rendering (src/parsing/markdown_renderer.rs)
    - `MarkdownRenderer` struct: Converts Markdown AST to formatted text output
    - Simple AST traversal and string conversion without cleanup logic
    - Applies Markdown formatting syntax (headers, bold, italic, code)
    - Handles inline elements (links, images, line breaks)
    - H1 uppercase transformation for consistency
    - Proper spacing and formatting for terminal display

22. **html5ever_text_generator.rs** - Modern HTML parsing implementation (src/parsing/html5ever_text_generator.rs)
    - `TextGenerator` struct: Alternative to regex-based HTML processing
    - Uses html5ever for standards-compliant HTML parsing
    - Integration with Markdown AST pipeline for clean text processing
    - Improved handling of complex HTML structures and nested elements

23. **text_generator_wrapper.rs** - Implementation abstraction layer (src/parsing/text_generator_wrapper.rs)
    - `TextGeneratorWrapper` struct: Provides unified interface for different text generators
    - `TextGeneratorImpl` enum: Switches between regex and html5ever implementations
    - Allows easy switching between parsing approaches for testing and development
    - Delegates core functionality (chapter processing, TOC parsing) to chosen implementation

24. **text_generator.rs** - Legacy regex-based HTML processing (src/parsing/text_generator.rs)
    - Original regex-based implementation maintained for compatibility
    - Direct HTML tag processing and text extraction
    - Comprehensive entity decoding and content cleaning

### Image Components (src/images/)

25. **image_storage.rs** - Image extraction and caching (src/images/image_storage.rs)
    - `ImageStorage` struct: Manages extracted EPUB images
    - Automatic image extraction from EPUB files
    - Directory-based caching in `temp_images/`
    - Thread-safe storage with Arc<Mutex>
    - Deduplication of already extracted images

26. **book_images.rs** - Book-specific image management (src/images/book_images.rs)
    - `BookImages` struct: Manages images for current book
    - Image path resolution from EPUB resources
    - Integration with ImageStorage for caching
    - Support for various image formats (PNG, JPEG, etc.)

27. **image_placeholder.rs** - Image loading placeholders (src/images/image_placeholder.rs)
    - `ImagePlaceholder` struct: Displays loading/error states
    - `LoadingStatus` enum: NotStarted, Loading, Loaded, Failed
    - Visual feedback during image loading
    - Error message display for failed loads
    - Configurable styling and dimensions

28. **image_popup.rs** - Full-screen image viewer (src/images/image_popup.rs)
    - `ImagePopup` struct: Modal image display
    - Full-screen overlay with centered image
    - Keyboard controls (Esc to close, navigation)
    - Mouse interaction support
    - Image scaling and aspect ratio preservation

29. **background_image_loader.rs** - Async image loading (src/images/background_image_loader.rs)
    - `BackgroundImageLoader` struct: Non-blocking image loads
    - Thread-based background loading
    - Prevents UI freezing during image loading
    - Callback-based completion notification

### Key Dependencies (Cargo.toml)
- `ratatui` (0.29.0): Terminal UI framework
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
- `pprof` (0.15): Performance profiling support
- `ratatui-image` (local path): Terminal image rendering
- `image` (0.25): Image processing and manipulation
- `fast_image_resize` (3.0): Fast image resizing
- `imagesize` (0.13): Image dimension detection
- `open` (5.3): Cross-platform file opening
- `html2text` (0.2.1): HTML to plain text conversion
- `roxmltree` (0.18): XML parsing for MathML processing
- `once_cell` (1.19): Lazy static initialization for Unicode mappings
- `html5ever` (0.27): Modern HTML5 parsing
- `markup5ever_rcdom` (0.3): DOM representation for html5ever

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
- Image storage and caching system
- Book-specific image management
- Image popup display state
- Background image loading coordination
- Performance profiler state
- FPS counter for performance monitoring

### Content Processing Pipeline

**Modern HTML5ever-based Pipeline (default):**
1. EPUB file is opened and validated
2. Images are extracted and cached to `temp_images/` directory
3. Table of contents is parsed from NCX or Nav documents
4. Chapter HTML content is extracted via epub crate
5. HTML is parsed using html5ever into proper DOM structure
6. DOM is converted to clean Markdown AST with preserved formatting
7. MathML elements are converted to ASCII art using mathml_renderer
8. Markdown AST is rendered to formatted text output
9. HTML entities are decoded in the final text
10. Images are loaded asynchronously in background

**Legacy Regex-based Pipeline (available via wrapper):**
1. EPUB file is opened and validated
2. Images are extracted and cached to `temp_images/` directory
3. Table of contents is parsed from NCX or Nav documents
4. Chapter HTML content is extracted via epub crate
5. Chapter title is extracted from h1/h2/title tags
6. HTML is cleaned using regex (scripts, styles removed)
7. HTML entities are decoded
8. Code blocks are detected and preserved with syntax highlighting
9. Tables are parsed and formatted for terminal display
10. Image tags are replaced with placeholders
11. Links are extracted and formatted
12. Tags are converted to text formatting
13. Paragraphs are indented for readability
14. Text is wrapped to terminal width
15. Images are loaded asynchronously in background

**Text Generator Selection:**
The application uses `TextGeneratorWrapper` to switch between implementations. Currently defaults to html5ever-based processing for improved HTML handling and MathML support.

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
- **Embedded Images**: Display images inline with dynamic sizing
- **Image Placeholders**: Loading indicators for images
- **Image Popup**: Full-screen image viewer with keyboard controls
- **Syntax Highlighting**: Colored code blocks with language detection
- **Table Support**: Formatted table display in terminal
- **Link Display**: Clickable links with URL information
- **FPS Monitor**: Real-time performance monitoring
- **Raw HTML View**: Toggle to view original HTML content
- **MathML Support**: Mathematical expressions rendered as ASCII art
- **Unicode Math**: Subscripts and superscripts using Unicode characters
- **LaTeX Fallback**: LaTeX notation for complex mathematical expressions

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
- `i`: Open image popup when cursor is on an image
- `v`: Toggle raw HTML view
- `p`: Start/stop performance profiling
- `q`: Quit the application
- `Esc`: Cancel text selection, close popups, or exit image viewer

### Mouse Controls
- **Click**: Select items in lists or TOC, or click on images/links
- **Drag**: Select text in reading area
- **Double-click**: Select word
- **Triple-click**: Select paragraph
- **Scroll**: Scroll content or navigate lists
- **Click on image**: Open image in popup viewer
- **Click on link**: Display link URL information

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
- Images are extracted to `temp_images/` directory and cached for performance
- Image loading happens asynchronously to prevent UI blocking
- Syntax highlighting uses syntect with multiple theme options
- Tables are parsed and formatted for terminal display
- Performance profiling can be enabled with pprof integration
- FPS monitoring helps track UI performance in real-time
- The application uses a local fork of ratatui-image for terminal image rendering
- MathML expressions are converted to ASCII art with Unicode subscripts/superscripts when possible
- The text processing pipeline has been migrated from direct regex processing to a Markdown AST-based approach
- Multiple text generation implementations are available via TextGeneratorWrapper for flexibility
- Mathematical expressions support advanced layouts including fractions, square roots, and summations

## Performance Considerations
- **CRITICAL**: Performance is one of the most important aspects of this project
- Never make significant changes like switching libraries unless explicitly instructed
- Always consider performance implications of any changes
- Image loading is done asynchronously to maintain UI responsiveness
- Images are cached after extraction to avoid repeated disk I/O
- Text content is cached to avoid expensive re-parsing
- Mouse events are batched to prevent performance degradation

## Error Handling Guidelines
- When logging errors, the received error object should always be logged (when possible)
- Never log a guess of what might have happened - only actual errors
- Use proper error context with anyhow for better debugging
- Preserve error chains for proper error tracing