//! # bookokrat
//!
//! A terminal-based EPUB, PDF, and DJVU book reader built with Rust.
//!
//! `bookokrat` provides a rich terminal UI for reading digital books, with
//! support for EPUB, PDF, and DJVU formats. It renders formatted text,
//! images, tables, and MathML directly in the terminal.
//!
//! ## Features
//!
//! - **EPUB** rendering with HTML-to-Markdown conversion and image display
//! - **PDF** rendering via the `pdf` feature flag (uses `pdfium`)
//! - **DJVU** support via external conversion
//! - Configurable keybindings, themes, and color modes
//! - Bookmarks, annotations, highlights, and reading history
//! - Full-text search across book content
//! - Table of contents navigation and jump list
//! - Image popups with kitty/sixel protocol support
//!
//! ## Feature Flags
//!
//! | Feature      | Description                                    |
//! |-------------|-----------------------------------------------|
//! | `pdf`       | Enables PDF rendering support (requires pdfium)| |
//! | `test-utils`| Exposes [`test_utils`] for integration tests   |
//!
//! ## Architecture
//!
//! The crate is organized into several top-level modules:
//!
//! - [`main_app`] — application state machine and event loop
//! - [`parsing`] — HTML/EPUB/Markdown parsing and rendering
//! - [`inputs`] — terminal input handling (keyboard, mouse, key sequences)
//! - [`keybindings`] — configurable key-action mapping
//! - [`book_manager`] — book loading, caching, and chapter management
//! - [`library`] — book library management
//! - [`theme`] — color themes and styling
//! - [`search`] / [`search_engine`] — full-text search
//! - [`annotations`], [`bookmarks`], [`marks`], [`comments`] — reading annotations
//! - [`widget`] — TUI widgets (table of contents, book list, search, etc.)
//! - [`images`] — image loading, storage, and display
//! - [`terminal`] / [`terminal_overlay`] — terminal setup and overlay management
//!
// Export modules for use in tests
pub mod annotations;
pub mod book_manager;
pub mod bookmarks;
pub mod clipboard;
pub mod color_mode;
pub mod comments;
pub mod config_migration;
pub mod export;
pub use inputs::event_source;
pub mod components;
pub mod images;
// Vendored ratatui-image
pub mod vendored;
pub use vendored::ratatui_image;
pub mod inputs;
pub mod jump_list;
pub mod keybindings;
pub mod library;
pub mod main_app;
pub mod markdown;
pub mod marks;
pub mod notification;
pub mod widget;
pub use components::mathml_renderer;
pub use widget::book_search;
pub use widget::book_stat;
pub use widget::navigation_panel;
pub use widget::navigation_panel::{book_list, table_of_contents};
pub use widget::reading_history;
pub use widget::text_reader as markdown_text_reader;
pub mod panic_handler;
pub mod parsing;
pub mod search;
pub mod search_engine;
pub mod settings;
pub mod system_command;
pub mod terminal;
pub mod terminal_overlay;
pub use components::table;
pub mod theme;
pub mod types;

// PDF rendering infrastructure - only available with pdf feature
#[cfg(feature = "pdf")]
pub mod pdf;

// Test utilities - only available when test-utils feature is enabled or during tests
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::simple_fake_books;

// Re-export main app components
pub use main_app::{App, FocusedPanel, MainPanel, PopupWindow, run_app_with_event_source};
