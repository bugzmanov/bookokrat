// Export modules for use in tests
pub mod bookmark;
pub mod text_generator;
pub mod book_list;
pub mod text_reader;
pub mod theme;
pub mod book_manager;
pub mod event_source;
pub mod main_app;

pub mod test_utils;

// Re-export main app components
pub use main_app::{App, Mode, run_app_with_event_source};