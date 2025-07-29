// Export modules for use in tests
pub mod book_list;
pub mod book_manager;
pub mod bookmark;
pub mod event_source;
pub mod main_app;
pub mod panic_handler;
pub mod text_generator;
pub mod text_reader;
pub mod text_selection;
pub mod theme;
pub mod toc_parser;

pub mod test_utils;

// Re-export main app components
pub use main_app::{run_app_with_event_source, App, FocusedPanel};
