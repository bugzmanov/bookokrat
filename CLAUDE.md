# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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

## Important Notes
- The application scans the current directory for EPUB files on startup
- Bookmarks are automatically saved when navigating between chapters or files
- The most recently read book is auto-loaded on startup
- Logging is written to `bookrat.log` for debugging
- The TUI uses vim-like keybindings
- Reading speed is set to 250 words per minute for time calculations
- Scroll acceleration increases speed when holding down scroll keys