use std::{fs::File, io::stdout};

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use log::{error, info};
use ratatui::{Terminal, backend::CrosstermBackend};
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

mod book_list;
mod book_manager;
mod book_stat;
mod bookmark;
mod event_source;
mod images;
mod main_app;
mod markdown;
mod markdown_text_reader;
mod mathml_renderer;
mod navigation_panel;
mod panic_handler;
mod parsing;
mod reading_history;
mod simple_fake_books;
mod system_command;
mod table;
mod table_of_contents;
mod text_reader;
mod text_reader_trait;
mod text_selection;
mod theme;
mod toc_parser;

#[cfg(test)]
mod test_utils;

use crate::event_source::KeyboardEventSource;
use crate::main_app::{App, run_app_with_event_source};

fn main() -> Result<()> {
    // Initialize panic handler first, before any other setup
    panic_handler::initialize_panic_handler();

    // Initialize logging with html5ever DEBUG logs filtered out
    WriteLogger::init(
        LevelFilter::Debug,
        simplelog::ConfigBuilder::new()
            .set_max_level(LevelFilter::Debug)
            .add_filter_ignore_str("html5ever")
            .build(),
        File::create("bookrat.log")?,
    )?;

    info!("Starting BookRat EPUB reader");

    // Terminal initialization
    enable_raw_mode()?;
    let mut stdout = stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let mut app = App::new();
    let mut event_source = KeyboardEventSource;
    let res = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);

    // Restore terminal state
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        error!("Application error: {:?}", err);
        println!("{err:?}");
    }

    info!("Shutting down BookRat");
    Ok(())
}
