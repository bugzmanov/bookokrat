// Export modules for use in tests
pub mod book_images;
pub mod book_list;
pub mod book_manager;
pub mod bookmark;
pub mod event_source;
pub mod image_placeholder;
pub mod image_storage;
pub mod main_app;
pub mod navigation_panel;
pub mod panic_handler;
pub mod reading_history;
pub mod simple_fake_books;
pub mod system_command;
pub mod table_of_contents;
pub mod text_generator;
pub mod text_reader;
pub mod text_selection;
pub mod theme;
pub mod toc_parser;

pub mod test_utils;

// Re-export main app components
pub use main_app::{App, FocusedPanel, run_app_with_event_source};
