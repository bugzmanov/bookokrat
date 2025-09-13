use std::{fs::File, io::stdout};

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use log::{error, info};
use ratatui::{Terminal, backend::CrosstermBackend};
use simplelog::{LevelFilter, WriteLogger};

// Use modules from the library crate
use bookrat::event_source::KeyboardEventSource;
use bookrat::main_app::{App, run_app_with_event_source};
use bookrat::panic_handler;

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
