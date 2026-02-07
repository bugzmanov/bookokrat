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
#[cfg(not(feature = "pdf"))]
use bookokrat::event_source::KeyboardEventSource;
#[cfg(feature = "pdf")]
use bookokrat::inputs::UnifiedEventSource;
use bookokrat::main_app::{App, run_app_with_event_source};
#[cfg(feature = "pdf")]
use bookokrat::main_app::{
    set_kitty_delete_range_support_override, set_kitty_shm_support_override,
};
use bookokrat::panic_handler;
#[cfg(feature = "pdf")]
use bookokrat::pdf::kittyv2::{delete_all_images, kgfx::cleanup_all_shms};
use bookokrat::settings;
#[cfg(feature = "pdf")]
use bookokrat::terminal;
use bookokrat::theme::load_custom_themes;

struct CliArgs {
    file_path: Option<String>,
    zen_mode: bool,
    test_mode: bool,
}

fn parse_args() -> Result<CliArgs> {
    let mut file_path = None;
    let mut zen_mode = false;
    let mut test_mode = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--zen-mode" => zen_mode = true,
            "--test-mode" => test_mode = true,
            "--help" | "-h" => {
                println!("Usage: bookokrat [FILE.epub] [--zen-mode] [--test-mode]");
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("bookokrat {}", env!("CARGO_PKG_VERSION"));
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
        test_mode,
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

    #[cfg(feature = "pdf")]
    {
        let caps = terminal::detect_terminal();
        if caps.env.tmux {
            bookokrat::pdf::kittyv2::set_tmux(true);
            terminal::enable_tmux_passthrough();
        }
        set_kitty_shm_support_override(terminal::probe_kitty_shm_support(&caps));
        set_kitty_delete_range_support_override(terminal::probe_kitty_delete_range_support(&caps));
    }
    // Terminal initialization
    enable_raw_mode()?;
    let mut stdout = stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load settings (skip in test mode for reproducible state)
    if !args.test_mode {
        settings::load_settings();
        load_custom_themes();
    }

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
    // In test mode: no auto-load, ephemeral bookmarks
    let auto_load_recent = args.file_path.is_none() && !args.test_mode;
    let bookmark_file = if args.test_mode {
        None
    } else {
        Some("bookmarks.json")
    };
    let mut app = App::new_with_config(book_directory, bookmark_file, auto_load_recent, None);
    app.set_zen_mode(args.zen_mode);
    app.set_test_mode(args.test_mode);
    if let Some(path) = args.file_path.as_deref() {
        if let Err(err) = app.open_book_for_reading_by_path(path) {
            error!("Failed to open requested book: {err}");
            app.show_error(format!("Failed to open requested book: {err}"));
        }
    }
    #[cfg(feature = "pdf")]
    let mut event_source = UnifiedEventSource::new();
    #[cfg(not(feature = "pdf"))]
    let mut event_source = KeyboardEventSource;
    let res = run_app_with_event_source(&mut terminal, &mut app, &mut event_source);

    // Delete all Kitty graphics images before leaving alternate screen
    #[cfg(feature = "pdf")]
    let _ = delete_all_images();

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

    // Cleanup any remaining SHM objects
    #[cfg(feature = "pdf")]
    cleanup_all_shms();

    info!("Shutting down Bookokrat");
    Ok(())
}
