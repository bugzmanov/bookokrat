// Export modules for use in tests
pub mod book_list;
pub mod book_manager;
pub mod book_search;
pub mod book_stat;
pub mod bookmarks;
pub mod comments;
pub mod event_source;
pub mod images;
pub mod jump_list;
pub mod main_app;
pub mod markdown;
pub mod markdown_text_reader;
pub mod mathml_renderer;
pub mod navigation_panel;
pub mod panic_handler;
pub mod parsing;
pub mod reading_history;
pub mod search;
pub mod search_engine;
pub mod system_command;
pub mod table;
pub mod table_of_contents;
pub mod text_reader_trait;
pub mod text_selection;
pub mod theme;

// Test utilities - only available when test-utils feature is enabled or during tests
#[cfg(any(test, feature = "test-utils"))]
pub mod simple_fake_books;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

// Re-export main app components
pub use main_app::{App, FocusedPanel, MainPanel, PopupWindow, run_app_with_event_source};
