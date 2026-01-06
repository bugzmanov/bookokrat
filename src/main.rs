use std::{fs::File, io::stdout, path::Path};

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
use bookokrat::event_source::KeyboardEventSource;
use bookokrat::main_app::{App, run_app_with_event_source};
use bookokrat::panic_handler;
use bookokrat::settings;
use bookokrat::theme::load_custom_themes;

struct CliArgs {
    file_path: Option<String>,
    zen_mode: bool,
}

fn parse_args() -> Result<CliArgs> {
    let mut file_path = None;
    let mut zen_mode = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--zen-mode" => zen_mode = true,
            "--help" | "-h" => {
                println!("Usage: bookokrat [FILE.epub] [--zen-mode]");
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => {
                anyhow::bail!("Unknown option: {arg}");
            }
            _ => {
                if file_path.is_some() {
                    anyhow::bail!("Only one file path can be provided.");
                }
                file_path = Some(arg);
            }
        }
    }

    Ok(CliArgs {
        file_path,
        zen_mode,
    })
}

fn main() -> Result<()> {
    // Initialize panic handler first, before any other setup
    panic_handler::initialize_panic_handler();

    let args = parse_args()?;

    // Initialize logging with html5ever DEBUG logs filtered out
    WriteLogger::init(
        LevelFilter::Debug,
        simplelog::ConfigBuilder::new()
            .set_max_level(LevelFilter::Debug)
            .add_filter_ignore_str("html5ever")
            .build(),
        File::create("bookokrat.log")?,
    )?;

    info!("Starting Bookokrat EPUB reader");

    // Terminal initialization
    enable_raw_mode()?;
    let mut stdout = stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load settings from ~/.bookokrat_settings.yaml
    settings::load_settings();

    // Load custom themes from settings and apply saved theme
    load_custom_themes();

    // Create app and run it
    let book_directory = args
        .file_path
        .as_deref()
        .and_then(|path| Path::new(path).parent())
        .and_then(|parent| {
            if parent.as_os_str().is_empty() {
                None
            } else {
                parent.to_str()
            }
        });
    let auto_load_recent = args.file_path.is_none();
    let mut app = App::new_with_config(
        book_directory,
        Some("bookmarks.json"),
        auto_load_recent,
        None,
    );
    app.set_zen_mode(args.zen_mode);
    if let Some(path) = args.file_path.as_deref() {
        if let Err(err) = app.open_book_for_reading_by_path(path) {
            error!("Failed to open requested book: {err}");
            app.show_error(format!("Failed to open requested book: {err}"));
        }
    }
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
        error!("Application error: {err:?}");
        println!("{err:?}");
    }

    info!("Shutting down Bookokrat");
    Ok(())
}
