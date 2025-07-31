use std::{fs::File, io::stdout};

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{error, info};
use ratatui::{backend::CrosstermBackend, Terminal};
use simplelog::{Config, LevelFilter, WriteLogger};

mod book_list;
mod book_manager;
mod bookmark;
mod event_source;
mod main_app;
mod panic_handler;
mod simple_fake_books;
mod system_command;
mod table_of_contents;
mod text_generator;
mod text_reader;
mod text_selection;
mod theme;
mod toc_parser;

#[cfg(test)]
mod test_utils;

use crate::event_source::KeyboardEventSource;
use crate::main_app::{run_app_with_event_source, App};

fn main() -> Result<()> {
    // Initialize panic handler first, before any other setup
    panic_handler::initialize_panic_handler();

    // Initialize logging
    WriteLogger::init(
        LevelFilter::Debug,
        Config::default(),
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
