use std::{fs::OpenOptions, io::stdout, path::Path, path::PathBuf};

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::EnableMouseCapture,
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use log::{error, info, warn};
use ratatui::{Terminal, backend::CrosstermBackend};
use simplelog::{LevelFilter, WriteLogger};

mod cli;
mod extract;
mod piped_input;
mod print;

// Use modules from the library crate
use bookokrat::config_migration::{self, FileMoveMigration, Migration};
#[cfg(not(feature = "pdf"))]
use bookokrat::event_source::KeyboardEventSource;
#[cfg(feature = "pdf")]
use bookokrat::inputs::UnifiedEventSource;
use bookokrat::library;
use bookokrat::main_app::{App, OpenPosition, run_app_with_event_source, should_auto_load_recent};
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

fn find_most_recent_book() -> Option<library::MostRecentBook> {
    let libraries_dir = library::libraries_data_dir().ok()?;
    library::find_most_recent_book_in(&libraries_dir)
}

/// Build the list of config-file migrations to apply at startup.
/// Paths are resolved from the user's real environment (home dir, etc.)
/// so the migration module itself stays pure.
fn build_config_migrations() -> Vec<Box<dyn Migration>> {
    let mut out: Vec<Box<dyn Migration>> = Vec::new();
    let Some(target_dir) = settings::preferred_config_dir() else {
        return out;
    };
    let home = std::env::home_dir();

    // Legacy locations for config.yaml, in priority order
    // (first present becomes the source; others become stale cleanup).
    let mut config_legacies: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    if let Some(ref h) = home {
        config_legacies.push(
            h.join("Library")
                .join("Application Support")
                .join("bookokrat")
                .join("config.yaml"),
        );
    }
    if let Some(ref h) = home {
        config_legacies.push(h.join(".bookokrat_settings.yaml"));
    }
    if !config_legacies.is_empty() {
        out.push(Box::new(FileMoveMigration::new(
            "config",
            target_dir.join("config.yaml"),
            config_legacies,
        )));
    }

    // No keybindings migration: the feature ships for the first time as
    // `keybindings.toml` in this release, so there's no prior location on any
    // platform that could hold a compatible file. First launch writes the
    // stub via `write_template_if_missing()`.

    out
}

/// Path to open for a FILE argument: itself, or a temp copy when it is stdin or a pipe.
fn resolve_input_path(arg: &str) -> String {
    if !piped_input::is_stream_arg(arg) {
        return arg.to_string();
    }
    match piped_input::materialize(arg) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(e) => {
            eprintln!(
                "Error: failed to read document from {}: {e:#}",
                piped_input::describe(arg)
            );
            std::process::exit(1);
        }
    }
}

/// Read a y/n answer from stdin. Anything other than "y"/"yes" (case-insensitive) counts as no.
fn ask_user_confirm(prompt: &str) -> bool {
    use std::io::{self, BufRead, Write};
    println!("\n{prompt}");
    print!(" [y/N]: ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    let answer = line.trim().to_lowercase();
    matches!(answer.as_str(), "y" | "yes")
}

/// Handle --synctex-forward: connect to a running instance's socket and send a forward search command.
///
/// Format: "LINE:COLUMN:FILE" (matches the Zathura/SumatraPDF convention)
#[cfg(feature = "pdf")]
fn handle_synctex_forward(spec: &str, pdf_file: Option<&str>) -> Result<()> {
    use bookokrat::pdf::synctex;

    // Parse "line:column:file"
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid --synctex-forward format: expected LINE:COLUMN:FILE, got '{spec}'");
    }
    let line: u32 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid line number: '{}'", parts[0]))?;
    let column: u32 = parts[1]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid column number: '{}'", parts[1]))?;
    let file = parts[2];

    // Determine socket path from the PDF file argument
    let pdf_path = pdf_file
        .ok_or_else(|| anyhow::anyhow!("--synctex-forward requires a PDF file argument"))?;
    let socket_path = synctex::synctex_socket_path(Path::new(pdf_path));

    if !socket_path.exists() {
        anyhow::bail!(
            "No running Bookokrat instance found for '{}' (expected socket at {})",
            pdf_path,
            socket_path.display()
        );
    }

    synctex::send_forward_command(&socket_path, file, line, column)
        .map_err(|e| anyhow::anyhow!("SyncTeX forward search failed: {e}"))
}

fn main() -> Result<()> {
    // Initialize panic handler first, before any other setup
    panic_handler::initialize_panic_handler();

    let args = cli::Cli::parse();

    // Print defaults and exit — no TUI setup needed.
    if args.print_default_keybindings {
        print!(
            "{}",
            bookokrat::keybindings::config::print_default_keybindings()
        );
        return Ok(());
    }
    if args.print_default_keybindings_grouped {
        print!(
            "{}",
            bookokrat::keybindings::config::print_default_keybindings_grouped()
        );
        return Ok(());
    }

    // Handle subcommands before TUI setup
    if let Some(ref command) = args.command {
        match command {
            cli::Command::Print {
                file,
                toc,
                info,
                chapter,
                pages,
            } => {
                let file = resolve_input_path(file);
                return print::cmd_print(&file, *toc, *info, *chapter, *pages);
            }
            cli::Command::Extract { file, output } => {
                let file = resolve_input_path(file);
                return extract::cmd_extract(&file, output);
            }
        }
    }

    // SyncTeX forward search client mode: send command to running instance and exit
    #[cfg(feature = "pdf")]
    if let Some(ref fwd) = args.synctex_forward {
        return handle_synctex_forward(fwd, args.file.as_deref());
    }

    // Before any terminal detection, which caches its results.
    if args.no_images {
        bookokrat::terminal::disable_images();
    }

    // A piped document is read from its own library, not from where it came from.
    let piped = args.file.as_deref().is_some_and(piped_input::is_stream_arg);
    let library_file = if piped { None } else { args.file.as_deref() };

    // Layer user's keybindings.toml overrides on top of the built-in defaults.
    // Tests deliberately skip this so they stay hermetic with respect to
    // `~/.config/bookokrat/`. Any issues are surfaced to the app below so it
    // can open the error popup on first draw.
    let keybinding_load_errors = bookokrat::keybindings::reload_keymap();

    // Resolve library directory from file argument or CWD
    let library_dir = library_file
        .and_then(|p| Path::new(p).parent())
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .or_else(|| args.directory.as_deref().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let lib_paths = library::resolve_library_paths(&library_dir);
    let log_path = library::resolve_log_path();

    // Run migration before logging is initialized (prints to stdout)
    if !args.test_mode
        && let Ok(ref paths) = lib_paths
        && let Err(e) = library::migrate_if_needed(&library_dir, paths)
    {
        eprintln!("Warning: migration failed: {e}");
    }

    // Initialize logging with html5ever DEBUG logs filtered out
    let log_file = match log_path {
        Ok(ref p) => OpenOptions::new().create(true).append(true).open(p)?,
        Err(_) => OpenOptions::new()
            .create(true)
            .append(true)
            .open("bookokrat.log")?,
    };
    WriteLogger::init(
        LevelFilter::Debug,
        simplelog::ConfigBuilder::new()
            .set_max_level(LevelFilter::Debug)
            .add_filter_ignore_str("html5ever")
            .build(),
        log_file,
    )?;

    if let Err(ref e) = lib_paths {
        warn!("Failed to resolve XDG library paths, falling back to CWD: {e}");
    }
    info!(
        "=== Starting bookokrat {} pid={} ===",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    );
    if args.no_images {
        info!("Images disabled (--no-images)");
    }

    // A piped document is copied into the cache before anything else touches stdin.
    let file = args.file.as_deref().map(resolve_input_path);

    // Validate file argument before entering TUI
    if let Some(ref path) = file {
        if !Path::new(path).exists() {
            eprintln!("Error: file not found: {path}");
            std::process::exit(1);
        }
        if bookokrat::book_manager::BookManager::detect_format(path).is_none() {
            eprintln!("Error: unsupported file format: {path}");
            std::process::exit(1);
        }
        #[cfg(feature = "pdf")]
        if args.no_images
            && matches!(
                bookokrat::book_manager::BookManager::detect_format(path),
                Some(
                    bookokrat::book_manager::BookFormat::Pdf
                        | bookokrat::book_manager::BookFormat::Djvu
                )
            )
        {
            let shown = args
                .file
                .as_deref()
                .map_or(path.as_str(), piped_input::describe);
            eprintln!(
                "Error: {shown}: {}",
                bookokrat::terminal::NO_IMAGES_PDF_MESSAGE
            );
            std::process::exit(1);
        }
    }

    // Must come before the migration prompt and terminal probes, which read stdin.
    #[cfg(unix)]
    if let Err(e) = piped_input::reattach_stdin_to_tty() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }

    info!("Starting Bookokrat EPUB reader");

    // Config-file migration: must run before the TUI steals stdin/stdout
    // (we need a y/n prompt) and before settings::load_settings() so that
    // the settings loader looks at the post-migration target path.
    if !args.test_mode {
        let migrations = build_config_migrations();
        match config_migration::run(&migrations, ask_user_confirm) {
            config_migration::Outcome::NothingToDo => {}
            config_migration::Outcome::Completed { applied } => {
                println!("Migrated {} config file(s): {applied:?}", applied.len());
            }
            config_migration::Outcome::Declined => {
                eprintln!(
                    "Config migration declined. Bookokrat cannot start until legacy config \
                     files are migrated. Relaunch to try again."
                );
                std::process::exit(1);
            }
            config_migration::Outcome::Blocked { .. } => {
                eprintln!("{}", config_migration::format_conflict_error(&migrations));
                std::process::exit(2);
            }
            config_migration::Outcome::Failed {
                applied,
                failed_id,
                error,
            } => {
                eprintln!("Config migration failed on '{failed_id}': {error}");
                if !applied.is_empty() {
                    eprintln!("Already migrated before failure: {applied:?}");
                }
                eprintln!("Resolve the issue above and restart.");
                std::process::exit(3);
            }
        }
    }

    bookokrat::clipboard::init();

    #[cfg(feature = "pdf")]
    let pixel_mouse_supported = {
        let caps = terminal::detect_terminal_with_probe();
        info!(
            "Startup terminal caps: kind={:?}, tmux={}, truecolor={}, graphics={}, \
             underline_color={}, \
             protocol={:?}, pdf_supported={}, pdf_scroll_mode={}, pdf_comments={}, \
             TERM_PROGRAM={:?}, TERM={:?}, kitty_window={}, kitty_pid={}, wezterm_executable={}",
            caps.kind,
            caps.env.tmux,
            caps.supports_true_color,
            caps.supports_graphics,
            caps.supports_underline_color,
            caps.protocol,
            caps.pdf.supported,
            caps.pdf.supports_scroll_mode,
            caps.pdf.supports_comments,
            caps.env.term_program,
            caps.env.term,
            caps.env.kitty_window,
            caps.env.kitty_pid,
            caps.env.wezterm_executable,
        );
        if caps.env.tmux && !terminal::images_disabled() {
            bookokrat::pdf::kittyv2::set_tmux(true);
            terminal::enable_tmux_passthrough();
        }
        set_kitty_shm_support_override(terminal::probe_kitty_shm_support(&caps));
        set_kitty_delete_range_support_override(terminal::probe_kitty_delete_range_support(&caps));
        let _ = terminal::probe_kitty_relative_placement_support(&caps);
        let tmux_anchors = terminal::supports_tmux_placeholder_anchors(&caps);
        if caps.env.tmux {
            info!("tmux placeholder anchors: {tmux_anchors}");
        }
        bookokrat::pdf::kittyv2::set_kitty_tmux_placeholder_anchors(tmux_anchors);

        // SGR-pixel mouse (?1016) is supported by Kitty and Ghostty. Skip under
        // tmux: it intercepts mouse reporting and forwards cell coordinates,
        // which would otherwise be misread as pixels.
        !caps.env.tmux
            && matches!(
                caps.kind,
                terminal::TerminalKind::Kitty | terminal::TerminalKind::Ghostty
            )
    };
    // Terminal initialization
    enable_raw_mode()?;
    let mut stdout = stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Record SGR-pixel mouse support (Kitty/Ghostty, not under tmux which
    // reports cells). The mode itself is only enabled per-PDF-session (see
    // pixel_mouse::enable_for_pdf): EPUB never emits ?1016, since on some
    // embedded Ghostty builds it interferes with inline-image transmission.
    #[cfg(feature = "pdf")]
    {
        use bookokrat::inputs::pixel_mouse;
        pixel_mouse::set_supported(pixel_mouse_supported);
        if pixel_mouse_supported {
            pixel_mouse::refresh_cell_size();
        }
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Test mode runs on default settings that never touch the user's config.
    let runtime_settings = if args.test_mode {
        settings::RuntimeSettings::in_memory(settings::Settings::default())
    } else {
        settings::load_settings();
        load_custom_themes();
        settings::RuntimeSettings::global()
    };

    // Create app and run it
    let book_directory = library_file
        .and_then(|path| Path::new(path).parent())
        .and_then(|parent| {
            if parent.as_os_str().is_empty() {
                None
            } else {
                parent.to_str()
            }
        })
        .or(args.directory.as_deref());

    // In test mode: no auto-load, ephemeral bookmarks
    let auto_load_recent =
        should_auto_load_recent(file.as_deref(), args.test_mode, args.continue_reading);
    let (bookmark_file, comments_dir) = if args.test_mode {
        (None, None)
    } else if let Ok(ref paths) = lib_paths {
        (
            Some(paths.bookmarks_file.to_string_lossy().into_owned()),
            Some(paths.comments_dir.clone()),
        )
    } else {
        (Some("bookmarks.json".to_string()), None)
    };
    let image_cache_dir = lib_paths.as_ref().ok().map(|p| p.image_cache_dir.clone());
    let mut app = App::new_with_background_library_scan(
        book_directory,
        bookmark_file.as_deref(),
        auto_load_recent,
        comments_dir.as_deref(),
        image_cache_dir,
        runtime_settings,
    );
    app.set_zen_mode(args.zen_mode);
    app.set_test_mode(args.test_mode);
    if !keybinding_load_errors.is_empty() {
        app.open_keybinding_errors_popup(keybinding_load_errors);
    }
    if args.continue_reading {
        if let Some(recent) = find_most_recent_book() {
            let result = app.open_book_for_reading_with_source_bookmarks(
                &recent.path,
                &recent.source_bookmarks,
            );
            if let Err(err) = result {
                error!("Failed to open most recent book: {err}");
                panic_handler::restore_terminal();
                eprintln!("Error: failed to open {}: {err}", recent.path);
                std::process::exit(1);
            }
        }
    } else if let Some(path) = file.as_deref() {
        let position = if let Some(ch) = args.chapter {
            if ch == 0 {
                eprintln!("Error: chapter number must be 1 or greater");
                std::process::exit(1);
            }
            Some(OpenPosition::Chapter(ch - 1))
        } else if let Some(pg) = args.page {
            if pg == 0 {
                eprintln!("Error: page number must be 1 or greater");
                std::process::exit(1);
            }
            Some(OpenPosition::Page(pg - 1))
        } else {
            None
        };
        let piped_library = Path::new(path)
            .parent()
            .filter(|_| piped && !args.test_mode)
            .and_then(|dir| match library::resolve_library_paths(dir) {
                Ok(paths) => Some(paths.bookmarks_file.to_string_lossy().into_owned()),
                Err(e) => {
                    warn!("Failed to resolve the piped documents library: {e}");
                    None
                }
            });
        let opened = match piped_library {
            Some(bookmarks) => app.open_book_for_reading_in_library(path, &bookmarks, position),
            None => app.open_book_for_reading_by_path(path, position),
        };
        if let Err(err) = opened {
            error!("Failed to open requested book: {err}");
            panic_handler::restore_terminal();
            let shown = args.file.as_deref().map_or(path, piped_input::describe);
            eprintln!("Error: failed to open {shown}: {err}");
            std::process::exit(1);
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
    panic_handler::restore_terminal();

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
