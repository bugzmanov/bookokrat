# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BookRat is a terminal user interface (TUI) EPUB reader written in Rust. It allows users to browse and read EPUB files from the terminal with features like chapter navigation, scrolling, and bookmark persistence.

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

1. **main.rs** - Main application logic
   - `App` struct: Central state management for the TUI
   - `Mode` enum: Tracks UI mode (FileList vs Content)
   - Event loop handling keyboard input and rendering
   - EPUB file parsing and content display
   - Regex-based HTML to text conversion

2. **bookmark.rs** - Bookmark persistence
   - `Bookmark` struct: Stores chapter, scroll position, and timestamp
   - `Bookmarks` struct: Manages bookmarks for multiple books
   - JSON-based persistence to `bookmarks.json`

### Key Dependencies
- `ratatui`: Terminal UI framework
- `crossterm`: Cross-platform terminal manipulation
- `epub`: EPUB file parsing
- `regex`: HTML tag processing for content display
- `serde`/`serde_json`: Bookmark serialization

### State Management
The application maintains state through the `App` struct which includes:
- Current EPUB file list and selection
- Active EPUB document and chapter
- Scroll position and content length
- UI mode (file browser vs reader)
- Compiled regex patterns for HTML processing
- Bookmark management

### Content Processing
HTML content from EPUB files is processed using regex patterns to:
- Convert paragraph tags to newlines
- Preserve headers with formatting
- Remove remaining HTML tags
- Clean up whitespace

## Important Notes
- The application scans the current directory for EPUB files on startup
- Bookmarks are automatically saved when navigating between chapters or files
- Logging is written to `bookrat.log` for debugging
- The TUI uses vim-like keybindings (j/k for navigation, h/l for chapters)