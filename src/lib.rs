// Export modules for use in tests
pub mod book_manager;
pub mod bookmarks;
pub mod color_mode;
pub mod comments;
pub mod export;
pub use inputs::event_source;
pub mod components;
pub mod images;
// Vendored ratatui-image
pub mod vendored;
pub use vendored::ratatui_image;
pub mod inputs;
pub mod jump_list;
pub mod main_app;
pub mod markdown;
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
