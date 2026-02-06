# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## CRITICAL RULES FOR AI ASSISTANTS

1. **Testing**: ALWAYS use the existing SVG-based snapshot testing in `tests/svg_snapshots.rs`. NEVER introduce new testing frameworks or approaches.
2. **Golden Snapshots**: NEVER update golden snapshot files with `SNAPSHOTS=overwrite` unless explicitly requested by the user. This is critical for test integrity.
3. **Test Updates**: NEVER update any test files or test expectations unless explicitly requested by the user. This includes unit tests, integration tests, and snapshot tests.
4. **File Creation**: Prefer editing existing files over creating new ones. Only create new files when absolutely necessary.
5. **Code Formatting**: NEVER manually reformat code or change indentation/line breaks. ONLY use `cargo fmt` for all formatting. When editing code, preserve the existing formatting exactly and let `cargo fmt` handle any formatting changes.
6. **Final Formatting**: ALWAYS run `cargo fmt` before reporting task completion if any code changes were made. This ensures consistent code formatting and prevents formatting-related changes in future edits.
7. **Comments/Annotations**: NEVER modify the comment storage format or location (`.bookokrat_comments/`) without explicit user request. The YAML-based persistence is critical.
8. **ANSI Art**: The `readme.ans` file contains binary CP437-encoded art. NEVER modify this file.
9. **Vendored Code**: The `src/vendored/` directory contains vendored ratatui-image code. This is NOT a crates.io dependency - it's vendored for customization.
10. **Kitty Graphics Protocol**: For Kitty terminals, ALWAYS use SHM (shared memory) transmission for images. NEVER use base64/direct transmission - it's too slow for 60fps PDF rendering.
11. **Backward Compatibility for Persistent Data**: When modifying any persistent data structures (bookmarks, settings, comments, or any files stored on the user's machine), ALWAYS ensure backward compatibility:
    - New fields in serialized structs MUST be `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]` or have `#[serde(default)]`
    - Old app versions must be able to read new data files (serde ignores unknown fields by default - do NOT add `deny_unknown_fields`)
    - New app versions must be able to read old data files (missing fields default to `None` or sensible defaults)
    - If a breaking change is unavoidable, ASK the user how to design a migration path before implementing
    - Test both directions: old data → new app, and consider new data → old app
12. **VHS Test Tapes and Keyboard Shortcuts**: When changing keyboard shortcut mappings, ALWAYS check and update all VHS test tapes in `vhs_tests/tapes/` to use the new keybindings. Test tapes simulate user input, so outdated keybindings will cause test failures.
13. **Keyboard Mapping Collisions**: When adding a new keyboard shortcut, ALWAYS check for collisions with existing mappings. Search the codebase for existing uses of the proposed key/key combination. If a collision is found, notify the user about the conflict and ask for a decision on how to proceed (use a different key, override existing mapping, or make it context-dependent).

## Core Principles

**This is NOT a toy project.** Bookokrat aims to replace GUI PDF/EPUB readers with a terminal-based alternative that is equally usable or better.

### Two Main Pillars

1. **User Experience**
   - Every user interaction must feel polished and intentional
   - No hacks, workarounds, or shortcuts for UX issues
   - If something doesn't work correctly, fix it properly - don't paper over it
   - The application should "just work" - users should never need to perform extra actions to trigger proper rendering or behavior
   - Visual feedback must be immediate and accurate

2. **Performance**
   - 60fps rendering is the target, not a nice-to-have
   - Background operations must never block the UI
   - Memory and CPU usage should be minimal
   - Startup time should be fast
   - Page navigation and scrolling must be instant

### Quality Standards

- **UX bugs are critical bugs** - treat them with the same urgency as crashes
- **Never accept "good enough"** - if a GUI app does something better, match or exceed it
- **Test on real hardware** - VHS visual tests exist to catch rendering issues across terminals
- **Responsiveness matters** - perceived performance is as important as actual performance

## Project Overview

Bookokrat is a terminal user interface (TUI) document reader written in Rust (version 0.3.0). It supports **EPUB** and **PDF** formats with comprehensive reading features including:

**Document Format Support:**
- **EPUB Reader**: Full EPUB2/EPUB3 support with Markdown AST-based rendering
- **PDF Reader** (optional `pdf` feature): High-performance PDF rendering using MuPDF with Kitty graphics protocol

**Core Features:**
- **Inline Comments/Annotations**: Add, edit, and delete comments on selected text passages with persistent YAML storage (supports both EPUB and PDF)
- **Help System**: Beautiful ANSI art help popup with full keyboard reference using CP437 encoding
- **Hierarchical Navigation**: Table of contents with expandable sections and vim-style keybindings
- **Text Selection**: Mouse support (single, double, and triple-click) with clipboard integration
- **Reading History**: Quick access popup showing recently read books
- **Book Statistics**: Popup displaying chapter and screen counts
- **Search Functionality**: Book-wide text search with result navigation and highlighting
- **Jump List Navigation**: Vim-style forward/backward navigation (Ctrl+o/Ctrl+i)
- **Bookmarks**: Automatic bookmark persistence and reading progress tracking
- **External Integration**: Open books in system readers
- **Image Support**: Embedded images with dynamic sizing, placeholders, and full-screen popup viewer
- **MathML Rendering**: Mathematical expressions converted to ASCII art with Unicode support (EPUB)
- **Syntax Highlighting**: Colored code blocks with language detection (EPUB)
- **Link Handling**: Display and follow hyperlinks
- **Color Adaptation**: True color (24-bit) detection with smart fallback to 256-color palette
- **Theme System**: Multiple Base16 color themes with custom theme support
- **Notification System**: Timed toast notifications with severity levels
- **Performance Tools**: Profiling support with pprof and FPS monitoring
- **Markdown AST Pipeline**: Modern HTML5ever-based text processing with preserved formatting (EPUB)
- **Cross-platform**: macOS, Windows, and Linux support

**PDF-Specific Features** (requires `pdf` feature):
- **High-performance rendering**: Background worker pool with page caching
- **Kitty Graphics Protocol**: SHM-based image transfer for 60fps rendering
- **Zoom levels**: Multiple zoom levels with smooth navigation
- **PDF text selection**: Pixel-based selection with text extraction
- **PDF comments**: Annotations on PDF pages with pixel coordinates

## Key Commands

### Development
- Build (EPUB only): `cargo build --release`
- Build (with PDF support): `cargo build --release --features pdf`
- Run: `cargo run`
- Run (with PDF): `cargo run --features pdf`
- Check code: `cargo check`
- Check (with PDF): `cargo check --features pdf`
- Run linter: `cargo clippy`
- Lint (with PDF): `cargo clippy --features pdf`
- Run tests: `cargo test`
- Format code: `cargo fmt`

### Testing
- Run all tests: `cargo test`
- Run specific test: `cargo test <test_name>`
- Run tests with output: `cargo test -- --nocapture`

### Development Tools
- **EPUB Inspector**: `cargo run --example epub_inspector <file.epub>` - Extracts and displays raw HTML content from EPUB chapters for debugging text processing issues
- **MathML Test**: `cargo run --example test_mathml_rust` - Tests MathML parsing and ASCII rendering functionality
- **Debug Bug Dump**: `cargo run --example dump_bug` - Debugging tool for lists and AST structures

### Feature Flags
- `pdf` - Enables PDF support (requires MuPDF, adds tokio async runtime)
- `test-utils` - Enables test utilities for creating fake books

## Architecture

### Core Components

1. **main.rs** - Entry point and terminal setup
   - Terminal initialization and panic handling
   - Main event loop bootstrapping
   - Application lifecycle management

2. **main_app.rs** - Core application logic (src/main_app.rs)
   - `App` struct: Central state management and component orchestration
   - `FocusedPanel` enum: Tracks which panel has keyboard focus
   - `PopupWindow` enum: Manages popups (ReadingHistory, BookStats, ImagePopup, HelpPopup)
   - High-level action handling (open book, navigate chapters, switch modes)
   - Mouse event batching and processing
   - Vim-like keybinding support with multi-key sequences and Space-prefixed commands
   - Text selection and clipboard integration
   - Comment/annotation management with Arc<Mutex<>> sharing
   - Bookmark management with throttled saving
   - Reading history popup management
   - Book statistics popup display
   - Help popup display with ANSI art
   - Image popup display and interaction
   - Notification system integration
   - Jump list navigation support
   - Search mode integration
   - Performance profiling integration with pprof
   - FPS monitoring through `FPSCounter` struct (defined inline in main_app.rs)

3. **bookmark.rs** - Bookmark persistence (src/bookmark.rs)
   - `Bookmark` struct: Stores chapter, scroll position, and timestamp
   - `Bookmarks` struct: Manages bookmarks for multiple books
   - JSON-based persistence to `bookmarks.json`
   - Tracks last read timestamp using chrono

4. **book_manager.rs** - Book discovery and management (src/book_manager.rs)
   - `BookManager` struct: Manages book file discovery
   - `BookInfo` struct: Stores book path and display name
   - `BookFormat` enum: Epub, Html, Pdf - detects format from extension
   - Automatic scanning of current directory for EPUB and PDF files
   - Document loading and validation for all supported formats

5. **book_list.rs** - File browser UI component (src/book_list.rs)
   - `BookList` struct: Manages book selection UI
   - Displays books with last read timestamps
   - Integrated with bookmark system for showing reading history
   - Implements `VimNavMotions` for consistent navigation

6. **navigation_panel.rs** - Left panel navigation manager (src/navigation_panel.rs)
   - `NavigationPanel` struct: Manages mode switching between book list and TOC
   - `NavigationMode` enum: BookSelection vs TableOfContents vs BookSearch
   - Renders appropriate sub-component based on mode
   - Handles mouse clicks and keyboard navigation
   - Extracts user actions for the main app

7. **table_of_contents.rs** - Hierarchical TOC display (src/table_of_contents.rs)
   - `TableOfContents` struct: Manages TOC rendering and interaction
   - `TocItem` enum: ADT for Chapter vs Section with children
   - Expandable/collapsible sections
   - Current chapter highlighting
   - Mouse and keyboard navigation support

8. **widget/text_reader/** - Main EPUB reading view component (MODULARIZED into src/widget/text_reader/)
   - `MarkdownTextReader` struct: Manages text display and scrolling using Markdown AST
   - Implements `TextReaderTrait` for abstraction
   - **Split across multiple files** (see components #44-51 for details):
     - `mod.rs` - Main struct and coordination (30KB)
     - `rendering.rs` - Content rendering to spans (122KB - largest file)
     - `normal_mode.rs` - Text navigation and vim motions (68KB)
     - `navigation.rs` - Scrolling and movement (11KB)
     - `selection.rs` - Mouse selection handling (12KB)
     - `text_selection.rs` - Selection state (14KB)
     - `images.rs` - Image loading and display (10KB)
     - `search.rs` - Search highlighting (11KB)
     - `comments.rs` - Comment rendering and editing (37KB)
     - `types.rs` - Type definitions (7KB)
   - Reading time calculation (250 WPM default)
   - Chapter progress percentage tracking
   - Smooth scrolling with acceleration
   - Half-screen scrolling with visual highlights
   - Text selection with clipboard integration
   - Implements `VimNavMotions` for consistent navigation
   - Embedded image display with dynamic sizing
   - Image placeholders with loading status
   - Link information extraction and display
   - Auto-scroll functionality during text selection
   - Raw HTML viewing mode toggle
   - Background image loading coordination
   - Rich text rendering with preserved formatting
   - Search highlighting support
   - Jump position tracking
   - **Comment rendering and editing with textarea overlay**

9. **text_reader_trait.rs** - Text reader abstraction (src/text_reader_trait.rs)
   - `TextReaderTrait`: Common interface for different text reader implementations
   - Unified API for scrolling, navigation, and content access
   - Enables swapping between different rendering implementations

10. **text_selection.rs** - Text selection system (src/text_selection.rs)
    - `TextSelection` struct: Manages selection state and rendering
    - Mouse-driven selection (drag, double-click for word, triple-click for paragraph)
    - Multi-line selection support
    - Clipboard integration via arboard
    - Visual highlighting with customizable colors
    - Coordinate validation and conversion

11. **reading_history.rs** - Recent books popup (src/reading_history.rs)
    - `ReadingHistory` struct: Manages history display and interaction
    - Extracts recent books from bookmarks
    - Chronological sorting with deduplication
    - Popup overlay with centered layout
    - Mouse and keyboard navigation
    - Implements `VimNavMotions` for consistent navigation

12. **system_command.rs** - External application integration (src/system_command.rs)
    - `SystemCommandExecutor` trait: Abstraction for system commands
    - Cross-platform file opening (macOS, Windows, Linux)
    - EPUB reader detection (Calibre, ClearView, Skim, FBReader)
    - Chapter-specific navigation support
    - Mockable interface for testing

13. **event_source.rs** - Input event abstraction (src/event_source.rs)
    - `EventSource` trait: Abstraction for event polling/reading
    - `KeyboardEventSource`: Real crossterm-based implementation
    - `SimulatedEventSource`: Mock for testing
    - Helper methods for creating test events

14. **theme.rs** - Color theming (src/theme.rs)
    - `Base16Palette` struct: Color scheme definition
    - Oceanic Next theme implementation
    - Dynamic color selection based on UI mode

15. **panic_handler.rs** - Enhanced panic handling (src/panic_handler.rs)
    - `initialize_panic_handler()`: Sets up panic hooks based on build type
    - Debug builds: Uses `better-panic` for detailed backtraces
    - Release builds: Uses `human-panic` for user-friendly crash reports
    - Terminal state restoration on panic to prevent broken terminal
    - Proper mouse capture restoration to maintain mouse functionality post-panic


16. **mathml_renderer.rs** - MathML to ASCII conversion (src/mathml_renderer.rs)
    - `MathMLParser` struct: Converts MathML expressions to terminal-friendly ASCII art
    - `MathBox` struct: Represents rendered mathematical expressions with positioning
    - Unicode subscript/superscript support for improved readability
    - LaTeX notation fallback for complex expressions
    - Comprehensive fraction, square root, and summation rendering
    - Multi-line parentheses for complex expressions
    - Baseline alignment for proper mathematical layout

17. **markdown.rs** - Markdown AST definitions (src/markdown.rs)
    - `Document` struct: Root container for parsed content
    - `Node` struct: Individual content blocks with source tracking
    - `Block` enum: Different content types (heading, paragraph, code, table, etc.)
    - `Text` struct: Rich text with formatting and inline elements
    - `Style` enum: Text formatting options (emphasis, strong, code, strikethrough)
    - `Inline` enum: Inline elements (links, images, line breaks)
    - `HeadingLevel` enum: H1-H6 heading levels
    - Complete table support structures (rows, cells, alignment)

### Comments and Annotations System

18. **comments.rs** - Comment persistence and management (src/comments.rs)
    - `BookComments` struct: Manages all comments for a single book
    - `Comment` struct: Individual comment with text, timestamp, and target
    - `CommentTarget` enum: Unified targeting for both EPUB and PDF
      - `Text { node_index, subtarget }` - EPUB text-based targeting
      - `Pdf { page, rects }` - PDF pixel-based targeting
    - `BlockSubtarget` enum: Fine-grained EPUB targeting (word ranges, etc.)
    - `PdfSelectionRect` struct: PDF pixel coordinates for selection
    - YAML-based persistence to `.bookokrat_comments/book_<md5hash>.yaml`
    - MD5 hashing of book filenames for unique identification
    - Efficient indexing by chapter/page
    - Auto-saving on modifications
    - Chronological sorting and duplicate prevention
    - Thread-safe access via Arc<Mutex<>>

19. **widget/text_reader/comments.rs** - Comment UI integration (src/widget/text_reader/comments.rs)
    - Comment rendering as purple quote-style blocks
    - Timestamp display format: "Note // MM-DD-YY HH:MM"
    - Textarea overlay for comment editing using tui-textarea
    - Comment deletion at cursor position
    - Visual styling with borders and proper coloring
    - Auto-scrolling to keep textarea visible
    - Minimum 3-line height for input area
    - Integration with text selection for comment creation

### Help and Notification Systems

20. **widget/help_popup.rs** - Help popup with ANSI art (src/widget/help_popup.rs)
    - Beautiful ANSI art header from `readme.ans` (CP437 encoding)
    - Full keyboard reference from `readme.txt`
    - vt100 parser for ANSI sequence rendering
    - SAUCE metadata stripping for proper display
    - Custom ANSI preprocessing (ESC[1;R;G;Bt conversion)
    - Vim-style navigation (j/k, gg/G, Ctrl+d/u)
    - Scrollbar support for long content
    - 90 columns wide, 94% vertical screen coverage
    - Toggled with `?` key

21. **notification.rs** - Toast notification system (src/notification.rs)
    - `Notification` struct: Individual notification with message and severity
    - `NotificationManager` struct: Global notification state
    - `NotificationLevel` enum: Info, Warning, Error
    - 5-second default timeout with automatic expiration
    - Bottom-right corner rendering
    - Color-coded by severity level
    - Interactive dismissal on click
    - Time-based auto-dismissal tracking

### Color and Terminal Capabilities

22. **color_mode.rs** - Terminal color detection (src/color_mode.rs)
    - `supports_true_color()`: Detects 24-bit RGB terminal support
    - `smart_color()`: Adaptive color selection based on terminal capabilities
    - `rgb_to_256color()`: Smart RGB to 256-color palette conversion
    - Environment variable checking (COLORTERM, TERM)
    - Grayscale palette detection and optimization
    - Distance-based color matching algorithm
    - Affects image protocol selection (Kitty/Sixel/Halfblocks)

23. **terminal.rs** - Terminal capability detection and protocol selection (src/terminal.rs)
    - `TerminalCapabilities`: Unified terminal kind/protocol/capability model
    - `detect_terminal()` / `detect_terminal_with_picker()`: Centralized detection and protocol overrides
    - `PdfCapabilities`: Derived PDF feature gating (scroll mode, comments, normal mode, iTerm version blocks)
    - Kitty SHM and delete-range probes for PDF rendering

### Type Definitions and Utilities

24. **types.rs** - Common type definitions (src/types.rs)
    - `LinkInfo` struct: Link information with URL and type
    - Link classification helpers
    - Shared type definitions across modules

### Input Handling Components (src/inputs/)

25. **inputs/event_source.rs** - Event abstraction (src/inputs/event_source.rs)
    - `EventSource` trait: Abstraction for event polling/reading
    - `KeyboardEventSource`: Real crossterm-based implementation
    - `SimulatedEventSource`: Mock for testing
    - Helper methods for creating test events

26. **inputs/terminal_input.rs** - PDF Kitty event handling (src/inputs/terminal_input.rs) - PDF FEATURE
    - `UnifiedEventSource` struct: Combines keyboard and Kitty protocol events
    - `KittyResponse` enum: Parses Kitty graphics protocol responses
    - Handles asynchronous image placement confirmations
    - Required for SHM-based image transfer coordination
    - Uses tokio for async event handling

27. **inputs/key_seq.rs** - Multi-key sequence tracking (src/inputs/key_seq.rs)
    - `KeySeqTracker` struct: Manages vim-style multi-key sequences
    - 1-second timeout for sequence completion
    - Tracks sequences like "gg" for vim motions
    - Automatic timeout and reset handling

27. **inputs/mouse_tracker.rs** - Enhanced mouse handling (src/inputs/mouse_tracker.rs)
    - `MouseTracker` struct: Tracks mouse events for multi-click detection
    - `ClickType` enum: Single, Double, Triple click detection
    - Distance threshold (3 cells) for multi-click validation
    - Time-based click grouping
    - Position tracking for drag operations

28. **inputs/text_area_utils.rs** - Textarea input mapping (src/inputs/text_area_utils.rs)
    - Crossterm to tui-textarea input conversion
    - Keyboard event mapping for textarea widget
    - Handles special keys and modifiers

### Settings and Configuration

29. **settings.rs** - Settings persistence (src/settings.rs)
    - `Settings` struct: Application settings with persistence
    - Theme selection and custom theme support
    - Margin configuration
    - YAML-based persistence to config directory
    - Auto-loading on startup

### Search and Navigation Components

30. **search.rs** - General search state and functionality (src/search.rs)
    - `SearchState` struct: Manages search state across the application
    - Tracks current search query and mode
    - Integrates with main app for search coordination

31. **search_engine.rs** - Search engine implementation (src/search_engine.rs)
    - `SearchEngine` struct: Core search functionality
    - Case-insensitive search with result ranking
    - Search result scoring
    - Multi-chapter search support
    - Note: fuzzy-matcher dependency currently commented out

32. **widget/book_search.rs** - Book-wide search UI (src/widget/book_search.rs)
    - `BookSearch` struct: Full-text search across entire book
    - Search result navigation with chapter context
    - Visual search result highlighting
    - Implements `VimNavMotions` for consistent navigation
    - Search result list with context preview

33. **jump_list.rs** - Vim-like jump list navigation (src/jump_list.rs)
    - `JumpList` struct: Maintains navigation history
    - Forward/backward navigation (Ctrl+o/Ctrl+i)
    - Chapter and position tracking
    - Circular buffer implementation
    - Integrates with main navigation flow

34. **widget/book_stat.rs** - Book statistics popup (src/widget/book_stat.rs)
    - `BookStat` struct: Displays book statistics
    - Chapter count and screen count per chapter
    - Total screens calculation
    - Centered popup display
    - Quick overview of book structure

35. **widget/comments_viewer.rs** - Comments browser UI (src/widget/comments_viewer.rs)
    - `CommentsViewer` struct: Browse and manage all comments
    - Lists comments across chapters/pages
    - Navigation to comment locations
    - Comment editing and deletion
    - Supports both EPUB and PDF comments

36. **widget/theme_selector.rs** - Theme picker UI (src/widget/theme_selector.rs)
    - `ThemeSelector` struct: Interactive theme selection
    - Preview of available Base16 themes
    - Custom theme support
    - Live preview of theme changes

### UI Component Modules (src/components/)

37. **components/table.rs** - Custom table widget (src/components/table.rs)
    - `Table` struct: Enhanced table rendering
    - Column alignment support
    - Header and content separation
    - Responsive width calculation
    - Used by book statistics and search results

38. **components/mathml_renderer.rs** - MathML rendering (src/components/mathml_renderer.rs)
    - MathML to ASCII conversion functionality
    - See component #16 for detailed description

### Parsing Components (src/parsing/)

39. **parsing/html_to_markdown.rs** - HTML to Markdown AST conversion (src/parsing/html_to_markdown.rs)
    - `HtmlToMarkdownConverter` struct: Converts HTML content to clean Markdown AST
    - Uses html5ever for robust DOM parsing and traversal
    - Handles various HTML elements (headings, paragraphs, images, MathML)
    - Integrates MathML processing with mathml_to_ascii conversion
    - Preserves text formatting and inline elements during conversion
    - Entity decoding for proper text representation

40. **parsing/markdown_renderer.rs** - Markdown AST to string rendering (src/parsing/markdown_renderer.rs)
    - `MarkdownRenderer` struct: Converts Markdown AST to formatted text output
    - Simple AST traversal and string conversion without cleanup logic
    - Applies Markdown formatting syntax (headers, bold, italic, code)
    - Handles inline elements (links, images, line breaks)
    - H1 uppercase transformation for consistency
    - Proper spacing and formatting for terminal display

41. **parsing/text_generator.rs** - Legacy regex-based HTML processing (src/parsing/text_generator.rs)
    - Original regex-based implementation maintained for compatibility
    - Direct HTML tag processing and text extraction
    - Comprehensive entity decoding and content cleaning
    - Used as fallback for certain parsing scenarios

42. **parsing/toc_parser.rs** - TOC parsing implementation (src/parsing/toc_parser.rs)
    - Parses NCX (EPUB2) and Nav (EPUB3) documents
    - Hierarchical structure extraction
    - Resource discovery and format detection
    - Robust regex-based content extraction

### Image Components (src/images/)

43. **images/image_storage.rs** - Image extraction and caching (src/images/image_storage.rs)
    - `ImageStorage` struct: Manages extracted EPUB images
    - Automatic image extraction from EPUB files
    - Directory-based caching in `.bookokrat_temp_images/` or `temp_images/`
    - Thread-safe storage with Arc<Mutex>
    - Deduplication of already extracted images

44. **images/book_images.rs** - Book-specific image management (src/images/book_images.rs)
    - `BookImages` struct: Manages images for current book
    - Image path resolution from EPUB resources
    - Integration with ImageStorage for caching
    - Support for various image formats (PNG, JPEG, etc.)

45. **images/image_placeholder.rs** - Image loading placeholders (src/images/image_placeholder.rs)
    - `ImagePlaceholder` struct: Displays loading/error states
    - `LoadingStatus` enum: NotStarted, Loading, Loaded, Failed
    - Visual feedback during image loading
    - Error message display for failed loads
    - Configurable styling and dimensions

46. **images/image_popup.rs** - Full-screen image viewer (src/images/image_popup.rs)
    - `ImagePopup` struct: Modal image display
    - Full-screen overlay with centered image
    - Keyboard controls (Esc to close, navigation)
    - Mouse interaction support
    - Image scaling and aspect ratio preservation

47. **images/background_image_loader.rs** - Async image loading (src/images/background_image_loader.rs)
    - `BackgroundImageLoader` struct: Non-blocking image loads
    - Thread-based background loading
    - Prevents UI freezing during image loading
    - Callback-based completion notification

### Widget Components (src/widget/)

The EPUB text reader has been modularized into multiple files under `src/widget/text_reader/`:

48. **widget/text_reader/mod.rs** - Main EPUB text reader module
    - `MarkdownTextReader` struct: Main reading view using Markdown AST
    - Implements `TextReaderTrait` for abstraction
    - Coordinates all text reader submodules
    - See component #8 for high-level features

49. **widget/text_reader/rendering.rs** - Content rendering logic (122KB)
    - Rich text span generation from Markdown AST
    - Syntax highlighting for code blocks
    - Table rendering
    - Image placeholder rendering
    - Link visualization
    - Search highlight integration

50. **widget/text_reader/normal_mode.rs** - Text navigation and vim motions (68KB)
    - Vim-style cursor movement
    - Visual mode selection
    - Word/paragraph navigation
    - Search result navigation

51. **widget/text_reader/navigation.rs** - Scrolling and navigation
    - Smooth scrolling with acceleration
    - Half-screen scrolling with visual highlights
    - Jump to top/bottom
    - Chapter navigation
    - Implements `VimNavMotions` trait

52. **widget/text_reader/selection.rs** - Mouse selection handling
    - Click-to-position cursor
    - Drag selection
    - Double-click word selection
    - Triple-click paragraph selection
    - Auto-scroll during selection

53. **widget/text_reader/text_selection.rs** - Text selection state
    - Selection range tracking
    - Multi-line selection support
    - Visual highlighting
    - Clipboard integration
    - Coordinate validation

54. **widget/text_reader/images.rs** - Image loading and display
    - Background image loading coordination
    - Image placeholder management
    - Image popup triggering
    - Dynamic image sizing
    - Loading status tracking

55. **widget/text_reader/search.rs** - Search functionality
    - Search highlighting in rendered content
    - Match position tracking
    - Next/previous match navigation
    - Search state integration

56. **widget/text_reader/comments.rs** - Comment rendering and editing (37KB)
    - Comment rendering as styled blocks
    - Timestamp display
    - Textarea overlay for editing
    - Integration with text selection

57. **widget/text_reader/types.rs** - Type definitions
    - `RenderedLine` struct: Line data with metadata
    - `LineType` enum: Content, Image, Link, etc.
    - `NodeReference`: AST node tracking
    - Internal type definitions for text reader

### PDF Reader Widget (src/widget/pdf_reader/) - PDF FEATURE

The PDF reader widget provides a complete PDF viewing experience:

58. **widget/pdf_reader/mod.rs** - PDF reader module exports
    - Exports all PDF reader components
    - Feature-gated behind `pdf` feature

59. **widget/pdf_reader/state.rs** - PDF reader state management
    - `PdfReaderState` struct: Complete reader state
    - Current page and zoom level tracking
    - Selection and navigation state
    - Display plan coordination

60. **widget/pdf_reader/rendering.rs** - PDF rendering
    - Display plan execution
    - Viewport updates
    - Image tiling and positioning

61. **widget/pdf_reader/navigation.rs** - PDF navigation
    - Page navigation (next/previous/goto)
    - Zoom level changes
    - Scroll position management

62. **widget/pdf_reader/comments.rs** - PDF comment handling
    - Comment rendering on PDF pages
    - Pixel-based comment positioning

63. **widget/pdf_reader/region.rs** - Content regions
    - `ImageRegion` struct: Image areas on page
    - `TextRegion` struct: Text areas with bounds

64. **widget/pdf_reader/types.rs** - PDF type definitions
    - `PdfDisplayRequest` struct: Rendering request
    - `PdfDisplayPlan` struct: Layout plan
    - `ChangeAmount` enum: Navigation amounts

### PDF Infrastructure (src/pdf/) - PDF FEATURE

The PDF module provides high-performance PDF rendering infrastructure:

65. **pdf/mod.rs** - PDF module exports and constants
    - Worker count, cache size, prefetch radius configuration
    - Module coordination

66. **pdf/service.rs** - Render service coordination
    - `RenderService` struct: Manages worker pool
    - Request/response handling
    - Page caching coordination

67. **pdf/worker.rs** - Background rendering workers
    - Worker pool for parallel rendering
    - MuPDF integration for page rendering
    - Async task coordination

68. **pdf/request.rs** - Render request types
    - `RenderRequest` struct: Page render request
    - `RenderResponse` struct: Rendered page data
    - `WorkerFault` enum: Worker error types

69. **pdf/state.rs** - Render state machine
    - `RenderState` enum: Rendering lifecycle states
    - State transitions and error handling

70. **pdf/types.rs** - Core PDF data structures
    - `CharInfo` struct: Character positioning
    - `LineBounds` struct: Line boundaries
    - `PageData` struct: Page content and text
    - `LinkRect` struct: Clickable link areas

71. **pdf/cache.rs** - Page caching system
    - `PageCache` struct: LRU page cache
    - Memory management (5-15MB per page)
    - Prefetching support

72. **pdf/converter.rs** - Image protocol conversion (200KB)
    - `ConvertedImage` struct: Protocol-ready image
    - `RenderedFrame` struct: Tiled image frames
    - Kitty/Sixel format conversion

73. **pdf/selection.rs** - PDF text selection
    - `SelectionRect` struct: Selection coordinates
    - `TextSelection` struct: Selected text extraction
    - Pixel-to-text coordinate mapping

74. **pdf/normal_mode.rs** - PDF navigation modes
    - `CursorPosition` struct: Cursor tracking
    - `VisualMode` enum: Selection modes

75. **pdf/toc.rs** - PDF table of contents
    - `TocEntry` struct: TOC item
    - PDF outline parsing

76. **pdf/zoom.rs** - Zoom level management
    - Zoom levels and calculations
    - Page dimension adjustments

77. **pdf/page_numbers.rs** - Page number detection
    - Page numbering extraction from content

### Kitty Graphics Protocol (src/pdf/kitty/) - PDF FEATURE

Low-level Kitty terminal graphics protocol implementation for high-performance image display:

78. **pdf/kitty/mod.rs** - Kitty protocol module
    - Capability negotiation
    - Image state management
    - Protocol initialization

79. **pdf/kitty/protocol.rs** - Protocol implementation (13KB)
    - Kitty graphics command encoding
    - Response parsing
    - Image placement commands

80. **pdf/kitty/shm.rs** - Shared memory support (7.7KB)
    - SHM-based image transfer (critical for 60fps)
    - Memory-mapped file handling
    - POSIX shared memory integration

81. **pdf/kitty/shm_pool.rs** - SHM pool management (5.5KB)
    - SHM region lifecycle
    - Pool allocation and cleanup

82. **pdf/kitty/image.rs** - Image handling
    - Image data formatting
    - Pixel format conversion

83. **pdf/kitty/medium.rs** - Image media types (6KB)
    - Image format handling
    - Transfer medium selection

84. **pdf/kitty/action.rs** - Kitty actions (9.4KB)
    - Image placement actions
    - Delete and clear commands

85. **pdf/kitty/async_io.rs** - Async I/O handling
    - Non-blocking response reading
    - Tokio integration

86. **pdf/kitty/display.rs** - Display configuration
    - Cell dimension calculations
    - Viewport sizing

87. **pdf/kitty/error.rs** - Error types
    - Protocol error definitions
    - Error handling

88. **pdf/kitty/delete.rs** - Image deletion
    - Image cleanup commands
    - Resource management

89. **pdf/kitty/types.rs** - Protocol types
    - Command structures
    - Response types

### Vendored Dependencies (src/vendored/)

The application vendors the ratatui-image library for customization:

90. **vendored/ratatui_image/** - Terminal image rendering (VENDORED, not crates.io)
    - Complete ratatui-image implementation vendored for customization
    - Protocol implementations: Kitty, Sixel, iTerm2, Halfblocks
    - Image resizing and protocol selection
    - Color mode-aware protocol selection
    - ~10 files with image protocol handling
    - Base64 encoding for image data
    - Sixel compression with flate2

### Test Utilities (src/test_utils/)

91. **test_utils/simple_fake_books.rs** - Test book creation (src/test_utils/simple_fake_books.rs)
    - Helper functions for creating test EPUB files
    - Generates sample books with various content types
    - Used in unit and integration tests

92. **test_utils/mod.rs** - Test helper module
    - Common test utilities and fixtures
    - Mock data generation
    - Test environment setup

### Key Dependencies (Cargo.toml)

**Version:** 0.3.0
**Edition:** Rust 2024
**Rust Version:** 1.86

**Core UI & Terminal:**
- `ratatui` (0.30.0): Terminal UI framework (with underline-color feature)
- `crossterm` (0.29.0): Cross-platform terminal manipulation (with event-stream)

**EPUB Handling:**
- `epub` (2.1.5): EPUB file parsing
- `zip` (0.6): EPUB file handling
- `walkdir` (2.4): Directory traversal for book discovery

**PDF Rendering (optional, `pdf` feature):**
- `mupdf`: PDF parsing and rendering (git custom build with svg, system-fonts, img)
- `tokio` (1.37.0): Async runtime (rt, macros features)
- `futures-util` (0.3.30): Async utilities
- `flume` (0.11.0): Async channels for worker communication
- `wide` (0.7): SIMD operations
- `lru` (0.12): LRU cache for page caching
- `rayon` (1): Parallel processing
- `memmap2` (0): Memory-mapped file access
- `base64` (0.22): Base64 encoding for Kitty protocol

**Parsing & Text Processing:**
- `regex` (1.10.3): HTML tag processing
- `html5ever` (0.27): Modern HTML5 parsing
- `markup5ever_rcdom` (0.3): DOM representation for html5ever
- `roxmltree` (0.18): XML parsing for MathML processing
- `textwrap` (0.16): Text wrapping utilities

**Serialization & Persistence:**
- `serde` (1.0): Serialization framework with derive support
- `serde_json` (1.0): JSON serialization for bookmarks
- `serde_yaml` (0.9): YAML serialization for comments and settings
- `md5` (0.7): Book hashing for comment file identification

**Date/Time & Utilities:**
- `chrono` (0.4): Timestamp handling with serde support
- `once_cell` (1.19): Lazy static initialization
- `dirs` (5.0): Configuration directory paths
- `home` (0.5.11): Home directory (pinned for Rust 1.86)

**Error Handling & Logging:**
- `anyhow` (1.0.79): Error handling
- `simplelog` (0.12.1): Logging framework
- `log` (0.4): Logging facade
- `thiserror` (1.0.59): Error types (for vendored code)

**Panic Handling:**
- `better-panic` (0.3): Enhanced panic handling with backtraces (debug builds)
- `human-panic` (2.0): User-friendly crash reports (release builds)
- `libc` (0.2): System interface for exit codes

**Clipboard & File Operations:**
- `arboard` (3.4): Clipboard integration
- `tempfile` (3.8): Temporary file management
- `open` (5.3): Cross-platform file opening

**Image Processing:**
- `image` (0.25): Image processing and manipulation
- `fast_image_resize` (3.0): Fast image resizing
- `imagesize` (0.13): Image dimension detection
- `png` (0.17): PNG encoding

**Image Protocols (Vendored Dependencies):**
- `icy_sixel` (0.1.1): Sixel protocol support
- `base64-simd` (0.8): Fast base64 encoding
- `rand` (0.8.5): Random utilities
- `flate2` (1.0): Compression for Sixel

**UI Widgets:**
- `tui-textarea`: Textarea widget for comment editing (VENDORED in `vendor/tui-textarea`; do not modify unless explicitly asked)

**ANSI Processing:**
- `vt100` (0.15): ANSI parsing for help popup
- `codepage-437` (0.1.0): CP437 to UTF-8 conversion

**Performance:**
- `pprof` (0.15): Performance profiling support with flamegraph and protobuf-codec

**Platform-Specific:**
- `rustix` (0.38.4): Unix-like systems (stdio, termios, fs) - non-Windows only
- `windows` (0.58.0): Windows API (console, filesystem, security) - Windows only

**Dev Dependencies:**
- `snapbox` (0.6): SVG snapshot testing
- `anstyle` (1.0): ANSI styling for snapshots
- `anstyle-svg` (0.1.5): SVG output
- `tempfile` (3.8): Test temp files
- `serial_test` (3.2): Test synchronization

**Note:** The ratatui-image library is VENDORED in `src/vendored/`, not a crates.io dependency.
**Note:** `tui-textarea` is VENDORED in `vendor/tui-textarea` and should not be modified unless explicitly requested.

### State Management
The application maintains state through the `App` struct in `main_app.rs` which includes:

**Document State:**
- Current document (EPUB or PDF) and chapter/page information
- `BookFormat` enum tracking current document type
- Navigation panel with mode switching (book list vs TOC vs search)

**Reader State:**
- Text reader (MarkdownTextReader) for EPUB with scroll position and content state
- **PDF reader state** (PdfReaderState) for PDF documents (PDF feature)
- Text selection state and clipboard integration

**Comments & Annotations:**
- **Comments system** with Arc<Mutex<BookComments>> for shared state
- Supports both EPUB text-based and PDF pixel-based targeting

**UI Components:**
- Popup management (reading history, book stats, image viewer, help popup, theme selector, comments viewer)
- **Notification manager** for toast notifications
- **Theme selector** for Base16 theme switching
- **Comments viewer** for browsing all comments

**Navigation & Search:**
- Search state and search mode tracking
- Jump list for navigation history
- Bookmark management with throttled saves
- Book manager for file discovery (EPUB and PDF)
- Focus tracking between panels

**Input Handling:**
- **Multi-key sequence tracker** for vim motions (gg, etc.)
- **Mouse tracker** for double/triple-click detection
- Mouse event batching for smooth scrolling

**Image & Media:**
- Image storage and caching system
- Book-specific image management
- Image popup display state
- Background image loading coordination

**PDF-Specific State** (PDF feature):
- `RenderService` for background PDF rendering
- Page cache with LRU eviction
- Kitty graphics protocol state
- Conversion channels (flume) for async image processing
- SHM pool management for image transfer

**Performance & System:**
- Performance profiler state
- FPS counter for performance monitoring
- **Color mode detection** for terminal capabilities
- **Settings persistence** for themes and preferences

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

**Legacy Regex-based Pipeline (available as fallback):**
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
The application primarily uses the Markdown AST-based pipeline through MarkdownTextReader for rendering, with html_to_markdown.rs handling the HTML to AST conversion.

### User Interface Features

**Navigation & Organization:**
- **Navigation Panel**: Switchable between book list, table of contents, and search results
- **File Browser Mode**: Lists all EPUB files with last read timestamps
- **Table of Contents**: Hierarchical view with expandable sections (H/L to collapse/expand all)
- **Reading History**: Quick access popup for recently read books (Space+h)
- **Book Statistics**: Popup showing chapter and screen counts (Space+d)
- **Help System**: Beautiful ANSI art help popup with full keyboard reference (? key)

**Reading & Annotation:**
- **Reading Mode**: Displays formatted text with chapter info using Markdown AST
- **Comments/Annotations**: Add, edit, and delete inline comments on selected text (a/d keys)
- **Text Selection**: Mouse-driven selection with clipboard support (drag, double/triple-click)
- **Progress Tracking**: Shows chapter number, reading progress %, and time remaining
- **Raw HTML View**: Toggle to view original HTML content (Space+s)
- **Copy Functions**: Copy selection (c), entire chapter (Space+c), or debug transcript (Space+z)

**Search & Navigation:**
- **Search Functionality**: Book-wide search with result navigation (/ to search, Space+f/F for book search)
- **Jump List Navigation**: Vim-style forward/backward navigation history (Ctrl+o/Ctrl+i)
- **Vim Navigation**: Consistent vim-like keybindings throughout including multi-key sequences
- **Smart Notifications**: 5-second toast notifications for user feedback

**Content Rendering:**
- **Embedded Images**: Display images inline with dynamic sizing
- **Image Placeholders**: Loading indicators with status feedback
- **Image Popup**: Full-screen image viewer with keyboard controls (Enter on image)
- **Syntax Highlighting**: Colored code blocks with language detection
- **Table Support**: Formatted table display in terminal
- **Link Display**: Hyperlinks with URL information
- **MathML Support**: Mathematical expressions rendered as ASCII art
- **Unicode Math**: Subscripts and superscripts using Unicode characters
- **LaTeX Fallback**: LaTeX notation for complex mathematical expressions

**System Integration:**
- **External Reader Integration**: Open books in GUI EPUB readers (Space+o)
- **Color Adaptation**: True color (24-bit) with smart fallback to 256-color palette
- **Cross-Platform**: macOS, Windows, and Linux support
- **Responsive Design**: Adjusts to terminal size changes

**Performance & Debugging:**
- **FPS Monitor**: Real-time performance monitoring overlay
- **Performance Profiler**: pprof integration for profiling (p key)
- **Mouse Event Batching**: Smooth scrolling with flood prevention

### Keyboard Controls

**Vim-Style Navigation:**
- `j`/`k`: Navigate down/up (works in all lists and reader)
- `h`/`l`: Previous/next chapter in reader; collapse/expand in TOC
- `Ctrl+d`/`Ctrl+u`: Scroll half screen down/up with highlight
- `gg`: Jump to top (vim-style multi-key sequence, 1-second timeout)
- `G`: Jump to bottom
- `Ctrl+o`/`Ctrl+i`: Navigate backward/forward in jump list

**Search:**
- `/`: Enter search mode (vim-style search)
- `n`/`N`: Navigate to next/previous search result

**Global Commands:**
- `Tab`: Switch focus between navigation panel and content view
- `Enter`: Select file/chapter/search result or expand/collapse TOC sections
- `?`: Toggle help popup with ANSI art
- `q`: Quit the application
- `Esc`: Cancel selection, close popups, exit search mode, or exit image viewer

**Comments/Annotations:**
- `a`: Add or edit comment on selected text
- `d`: Delete comment at cursor position (when on comment line)
- `c` or `Ctrl+C`: Copy selection to clipboard

**Space-Prefixed Commands (Modal):**
- `Space+h`: Toggle reading history popup
- `Space+d`: Show book statistics popup
- `Space+o`: Open current book in external system EPUB reader
- `Space+s`: Toggle raw HTML view
- `Space+c`: Copy entire chapter (EPUB) / extract page text (PDF)
- `Space+C`: Copy selected TOC item (PDF, requires TOC focus)
- `Space+z`: Copy debug transcript
- `Space+f`: Reopen last book-wide search
- `Space+F`: Start fresh book-wide search

**Navigation Panel:**
- `b`: Toggle between book list and table of contents
- `H`/`L`: Collapse/expand all TOC entries

**Reader Panel:**
- `Enter`: Open image popup when cursor is on an image
- `p`: Toggle performance profiler overlay

**Note:** All popups (help, history, stats, search results) support vim navigation (j/k, gg/G, Ctrl+d/u, Enter to select, Esc to close).

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

## VHS Terminal Screenshot Testing (PDF Rendering)

**WARNING: This is a heavyweight testing approach.** It launches actual terminal emulators, runs the application, and captures real screenshots. Use this **primarily for PDF rendering validation** where the Kitty graphics protocol and image rendering cannot be tested with SVG snapshots.

**IMPORTANT FOR AI ASSISTANTS:** Do NOT run VHS tests unless explicitly requested by the user. These tests are slow, require a display, and should only be used when specifically testing PDF rendering changes.

**CRITICAL: NEVER update VHS golden snapshots (`--update` flag).** Golden snapshots are the source of truth for visual regression testing. Only a human can decide when to update them. If VHS tests fail:
1. Read the generated HTML report to understand the differences
2. Analyze whether the differences are actual bugs or minor rendering variations
3. Report findings to the user - describe what changed and whether it appears to be a real issue
4. Let the user decide whether to update the golden snapshots
5. NEVER run `./vhs_tests/run.sh --update` - this decision belongs to the user

For most UI testing, use the SVG-based snapshot tests described above.

### When to Use VHS Tests

- **PDF rendering validation** - Testing that PDF pages render correctly via Kitty graphics protocol
- **Terminal-specific rendering** - Verifying rendering differences between terminals (Kitty, Ghostty)
- **Full integration testing** - End-to-end tests requiring actual terminal graphics

### When NOT to Use VHS Tests

- Text-based UI components (use SVG snapshots instead)
- EPUB rendering (use SVG snapshots instead)
- Unit tests or component tests
- Any test that doesn't require actual terminal graphics

### Running VHS Tests

```bash
# Run ALL tapes (full test suite)
./vhs_tests/run.sh --terminal kitty

# Run specific tape only
./vhs_tests/run.sh --terminal kitty --tape pdf_smoke

# Run with verbose output (shows all commands being executed)
./vhs_tests/run.sh --terminal kitty --verbose

# Update golden snapshots (HUMANS ONLY - AI must never run this)
./vhs_tests/run.sh --terminal kitty --update
```

**Running the full suite** executes all `.tape` files in `vhs_tests/tapes/` sequentially. Each tape runs in its own Kitty window, and the harness automatically manages window creation/cleanup between tapes. The test harness launches a self-managed Kitty instance with remote control enabled, so no manual terminal setup is required.

### Directory Structure

```
vhs_tests/
├── tapes/                    # Test tape files (.tape)
├── golden/                   # Golden screenshots (by terminal)
│   └── kitty/
│       └── pdf_smoke/
│           ├── initial.png
│           ├── page_2.png
│           └── ...
├── output/
│   ├── screenshots/          # Actual screenshots from test runs
│   └── reports/              # HTML comparison reports
└── lib/                      # Test harness scripts
    ├── tape_runner.sh        # Tape parser and executor
    └── kitty_manager.sh      # Kitty terminal management
```

### Tape File Format

Tape files (`.tape`) define test sequences with simple commands:

```tape
# pdf_smoke.tape - Example tape file

# Specify PDF to test (relative to project root)
pdf tests/testdata/vhs_test.pdf

# Wait for app to load
wait 500

# Capture screenshot
screenshot initial

# Navigate (l = next page, h = previous)
key l
wait 300
screenshot page_2

# Zen mode toggle
ctrl z
wait 300
screenshot zen_mode

# Help popup
key ?
wait 500
screenshot help_popup

# Close popup
escape

# Quit
key q
```

### Tape Commands

| Command | Arguments | Description |
|---------|-----------|-------------|
| `pdf` | `<path>` | PDF file to test (relative to project root) |
| `screenshot` | `<name>` | Capture screenshot with given name |
| `key` | `<char>` | Send single key press |
| `ctrl` | `<char>` | Send Ctrl+key combination |
| `escape` | - | Send Escape key |
| `return` | - | Send Return/Enter key |
| `wait` | `<ms>` | Wait specified milliseconds (default: 500) |

**IMPORTANT: Wait Times** - Kitty terminal is very fast. Never use wait times longer than 500ms in tape files. Most operations complete in 200-300ms. Only use 500ms for initial app load or page navigation that requires rendering.

### PDF Navigation Keys

For PDF documents:
- `l` / `h` - Next/previous page
- `j` / `k` - Scroll within page
- `Ctrl+z` - Toggle zen mode (hide sidebar)
- `?` - Help popup
- `q` - Quit

### Test Reports

When tests fail, an HTML report is generated showing:
- Side-by-side comparison of expected vs actual screenshots
- Pass/fail status for each screenshot
- Missing golden snapshots

Reports are saved to: `vhs_tests/output/reports/<terminal>_<tape>_report.html`

### Adding New VHS Tests

1. Create a new tape file in `vhs_tests/tapes/`:
   ```bash
   # vhs_tests/tapes/my_test.tape
   pdf tests/testdata/my_test.pdf
   wait 500
   screenshot initial
   # ... add test steps
   key q
   ```

2. Run the test (will fail initially - no golden snapshots):
   ```bash
   ./vhs_tests/run.sh --terminal kitty --tape my_test --verbose
   ```

3. Review generated screenshots in `vhs_tests/output/screenshots/kitty/my_test/`

4. If screenshots look correct, create golden snapshots:
   ```bash
   ./vhs_tests/run.sh --terminal kitty --tape my_test --update
   ```

### Terminal Support

Currently supported:
- **Kitty** - Self-managed instance with remote control (recommended for CI)

The test harness automatically launches and manages its own Kitty instance with the required settings (`allow_remote_control`, `listen_on`). No manual terminal configuration is needed.

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

**File Management & Persistence:**
- The application scans the current directory for EPUB and PDF files on startup
- Bookmarks are automatically saved to `bookmarks.json` when navigating between chapters/pages or files
- **Comments are persisted to `.bookokrat_comments/book_<md5hash>.yaml` per book** (supports both EPUB and PDF)
- **Settings are persisted to config directory** via `settings.rs` (themes, margins, etc.)
- Images are extracted to `.bookokrat_temp_images/` or `temp_images/` and cached for performance
- The most recently read book is auto-loaded on startup
- Logging is written to `bookokrat.log` for debugging

**UI & Navigation:**
- The TUI uses vim-like keybindings throughout all components with Space-prefixed modal commands
- Multi-key sequences (like "gg") have a 1-second timeout
- **Help popup (`?` key) displays ANSI art with CP437 encoding and full keyboard reference**
- Mouse events are batched to prevent flooding and ensure smooth scrolling
- **Mouse tracker detects double/triple-clicks with 3-cell distance threshold**
- Text selection automatically scrolls the view when dragging near edges
- **Notification system shows toast messages for 5 seconds in bottom-right corner**
- **Theme selector** allows switching between Base16 color themes

**Reading Features:**
- Reading speed is set to 250 words per minute for time calculations (EPUB)
- Scroll acceleration increases speed when holding down scroll keys
- **Inline comments/annotations can be added, edited, and deleted on selected text** (EPUB and PDF)
- Jump list maintains a navigation history for easy backward/forward navigation (Ctrl+o/Ctrl+i)
- Book statistics provide a quick overview of book structure and size
- **Comments viewer** allows browsing all comments across the document

**EPUB Content Processing:**
- The application supports both EPUB2 (NCX) and EPUB3 (Nav) table of contents formats
- The text processing pipeline uses a Markdown AST-based approach with html5ever parser
- The main text reader is MarkdownTextReader, which uses the Markdown AST pipeline
- MathML expressions are converted to ASCII art with Unicode subscripts/superscripts when possible
- Mathematical expressions support advanced layouts including fractions, square roots, and summations
- Code blocks support syntax highlighting integrated into the rendering pipeline
- Tables are parsed and formatted for terminal display

**PDF Rendering** (PDF feature):
- **High-performance rendering** using MuPDF library with background worker pool
- **Page caching** with LRU eviction (5-15MB per page, configurable cache size)
- **Prefetching** of adjacent pages for smooth navigation
- **Kitty graphics protocol** with SHM-based image transfer (critical for 60fps)
- **Text extraction** for selection and search
- **PDF table of contents** parsing from document outline
- **Zoom levels** with smooth viewport updates
- **Terminal capability gates** are centralized in `src/terminal.rs` (protocol selection, comments/scroll/normal mode, iTerm version checks)

**Images & Media:**
- Image loading happens asynchronously to prevent UI blocking
- **Color mode detection adapts between true color (24-bit) and 256-color palettes**
- **The application VENDORS ratatui-image in `src/vendored/` - it's not a crates.io dependency**
- Image protocols (Kitty/Sixel/iTerm2/Halfblocks) are selected based on terminal capabilities
- **PDF uses Kitty SHM for optimal performance** - never use base64 for PDF images

**Performance & Integration:**
- Performance profiling can be enabled with pprof integration (`p` key)
- FPS monitoring helps track UI performance in real-time
- External readers are detected based on the platform (macOS, Windows, Linux)
- **PDF rendering uses worker pool** with configurable worker count

**Search:**
- Search functionality supports case-insensitive matching
- Note: fuzzy-matcher dependency is currently commented out

**Code Organization:**
- **MarkdownTextReader is modularized across multiple files in `src/widget/text_reader/`**
- **PDF reader is modularized in `src/widget/pdf_reader/`** (PDF feature)
- **PDF infrastructure is in `src/pdf/`** with Kitty protocol in `src/pdf/kitty/` (PDF feature)
- **Input handling is organized in `src/inputs/` module**
- **UI components are in `src/components/` and `src/widget/`**
- **Parsing logic is in `src/parsing/`**
- **Image handling is in `src/images/`**
- **Rust edition 2024, version 1.86 is used**

## Performance Considerations
- **CRITICAL**: Performance is one of the most important aspects of this project
- Never make significant changes like switching libraries unless explicitly instructed
- Always consider performance implications of any changes
- Image loading is done asynchronously to maintain UI responsiveness
- Images are cached after extraction to avoid repeated disk I/O
- Text content is cached to avoid expensive re-parsing
- Mouse events are batched to prevent performance degradation
- **PDF pages are cached** with LRU eviction and prefetching
- **PDF rendering uses background workers** to avoid blocking the UI
- **Kitty SHM is required** for PDF images - never use base64 transmission

## Error Handling Guidelines
- When logging errors, the received error object should always be logged (when possible)
- Never log a guess of what might have happened - only actual errors
- Use proper error context with anyhow for better debugging
- Preserve error chains for proper error tracing
- When introducing new regexes they should always be cached to avoid recompilation cycles
- Rendering of items in markdown_text_reader.rs should always use Base16Palette and should avoid relying on default ratatui style

## User Feedback for Unavailable Features (PDF Reader)

When a user attempts to use a feature that is not available in their current context (e.g., terminal limitations, mode restrictions), use the **error HUD** pattern instead of toast notifications.

### Error HUD Pattern
Use `self.set_error_hud(message)` followed by `return Some(InputAction::Redraw)` to display inline error messages. This provides immediate, contextual feedback that appears in the PDF viewer area.

**When to use error HUD:**
- User tries to activate a feature that's not supported in their terminal (e.g., normal mode in iTerm)
- User tries to use a feature outside its required mode (e.g., comments outside zen mode)
- Any direct user interaction that cannot be fulfilled due to context restrictions

**Examples:**
```rust
// Comments only available in zen mode
if !self.comments_enabled {
    self.set_error_hud("Comments are only available in zen mode for PDFs".to_string());
    return Some(InputAction::Redraw);
}

// PDF normal mode not supported in iTerm
if self.is_iterm {
    self.set_error_hud("PDF normal mode is not supported in iTerm".to_string());
    return Some(InputAction::Redraw);
}
```

**When to use toast notifications instead:**
- Background operation results (e.g., "Saved comment", "Copied 123 chars")
- Non-blocking informational messages
- Success/failure of async operations

## Rich Text Rendering Architecture (MarkdownTextReader)

### Core Design Principle
All markdown elements (lists, quotes, definition lists, tables, etc.) must preserve rich text formatting (bold, italic, links, etc.) rather than converting to plain text. This ensures consistent formatting behavior across all content types.

### render_text_spans API
The central method for rendering rich text content is `render_text_spans()`:

```rust
fn render_text_spans(
    &mut self,
    spans: &[Span<'static>],          // Pre-styled spans with formatting
    prefix: Option<&str>,             // Optional prefix (bullets, "> ", etc.)
    node_ref: NodeReference,
    lines: &mut Vec<RenderedLine>,
    total_height: &mut usize,
    width: usize,
    indent: usize,                    // Proper indentation support
    add_empty_line_after: bool,
)
```

**Key Features:**
- **Prefix Support**: Automatically adds prefixes like "• ", "> ", or numbered bullets
- **Indentation**: Properly handles indentation levels (2 spaces per level)
- **Rich Text Preservation**: Maintains all styling from `render_text_or_inline()`
- **Text Wrapping**: Handles text wrapping while preserving formatting
- **Link Coordinates**: Automatically fixes link coordinates after wrapping

### Standard Rendering Pattern
For any markdown element containing text:

1. **Generate styled spans** using `render_text_or_inline()`
2. **Apply element-specific styling** (e.g., quote color, bold for definitions)
3. **Call render_text_spans** with appropriate prefix and indentation
4. **Update line types** if needed for specific elements

```rust
// Example: List item rendering
let mut content_spans = Vec::new();
for item in content.iter() {
    content_spans.extend(self.render_text_or_inline(item, palette, is_focused, *total_height));
}

self.render_text_spans(
    &content_spans,
    Some(&prefix),           // "• " or "1. "
    node_ref.clone(),
    lines,
    total_height,
    width,
    indent,                  // Proper indentation
    false,                   // Don't add empty line
);
```

### CRITICAL: Avoid text_to_string()
**NEVER** use `text_to_string()` for rendering content as it strips all formatting:
- ❌ `let text_str = self.text_to_string(content);` (loses bold, italic, links)
- ✅ `content_spans.extend(self.render_text_or_inline(item, ...)` (preserves formatting)

### Updated Elements
The following elements now properly support rich text:
- **Lists**: Bullets/numbers + rich text content with proper indentation
- **Quotes**: "> " prefix + italic styling + rich text content
- **Definition Lists**: Bold terms + indented definitions with rich text
- **Future elements**: Should follow the same pattern

This architecture ensures that bold text, italic text, links, and other formatting work consistently across all markdown elements without hardcoding support for each element type.

# important-instruction-reminders
- Don't use eprintln if you need logging. This is TUI application. eprintln breaks UI. Use log crate to do proper logging
- Always log actual error that happened when creating "failed" branch logging
- **Git Operations**: Be very careful when modifying git state. Any operation that mutates git history (commit, reset, rebase, amend, push, etc.) must be explicitly requested by the user. If not sure, ask for confirmation before executing.

- **UTF-8 Text Handling**: All document content (EPUB, HTML, PDF) is UTF-8 encoded and may contain multi-byte characters (e.g., `–`, `é`, `中`). NEVER use byte-based string operations (`.len()`, `[start..end]` on `&str`) for text manipulation. Always use character-based operations:
  - Use `.chars().count()` instead of `.len()` for character count
  - Use `.chars().collect::<Vec<char>>()` then slice the Vec for substring extraction
  - Use `.char_indices()` when you need both character and byte positions
  - This applies to: search highlighting, text selection, cursor positioning, text wrapping

Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.
ALWAYS prefer editing an existing file to creating a new one.
NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.
- Do not put useless comments. Comments should be only for code that does something unusual or tricky
