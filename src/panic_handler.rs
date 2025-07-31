use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use std::io::{self, Write};
use std::panic;

/// Initialize panic handler for the application
///
/// This sets up different panic handling behavior for debug vs release builds:
/// - Debug: Uses better-panic for detailed backtraces
/// - Release: Uses human-panic for user-friendly crash reports
pub fn initialize_panic_handler() {
    // #[cfg(debug_assertions)]
    // {
    // Install better-panic for debug builds
    better_panic::install();
    // }

    // #[cfg(not(debug_assertions))]
    // {
    //     human_panic::setup_panic!(Metadata {
    //         name: env!("CARGO_PKG_NAME").into(),
    //         version: env!("CARGO_PKG_VERSION").into(),
    //         authors: env!("CARGO_PKG_AUTHORS").replace(':', ", ").into(),
    //         homepage: Some("https://github.com/user/bookrat".into()),
    //     });
    // }

    // Set custom panic hook that restores terminal state
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Try to restore terminal state
        restore_terminal();

        // Call the default panic handler (better-panic or human-panic)
        default_hook(panic_info);

        // Exit with error code
        std::process::exit(1);
    }));
}

/// Restore terminal to a clean state
///
/// This function attempts to restore the terminal to its original state
/// when a panic occurs, ensuring the terminal doesn't remain in a broken state.
/// Specifically handles:
/// - Disabling raw mode
/// - Exiting alternate screen
/// - Disabling mouse capture (important for restoring mouse functionality)
/// - Disabling keyboard enhancement flags
/// - Showing the cursor
fn restore_terminal() {
    // Attempt to restore terminal state
    // We ignore errors here because we're already in a panic situation
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    let _ = execute!(io::stderr(), crossterm::cursor::Show);

    // Print a newline to ensure clean output
    let _ = writeln!(io::stderr());
}

/// Initialize human-panic metadata for release builds
#[cfg(not(debug_assertions))]
use human_panic::Metadata;
