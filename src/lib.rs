// Export modules for use in tests
pub mod book_list;
pub mod book_manager;
pub mod bookmark;
pub mod event_source;
pub mod html5ever_text_generator;
pub mod html_to_markdown;
pub mod images;
pub mod main_app;
pub mod markdown;
pub mod markdown_renderer;
pub mod mathml_renderer;
pub mod navigation_panel;
pub mod panic_handler;
pub mod reading_history;
pub mod simple_fake_books;
pub mod system_command;
pub mod table_of_contents;
pub mod text_generator;
pub mod text_generator_wrapper;
pub mod text_reader;
pub mod text_selection;
pub mod theme;
pub mod toc_parser;

pub mod test_utils;

// Re-export main app components
pub use main_app::{App, FocusedPanel, run_app_with_event_source};
