use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use std::io::{self, Write};
use std::panic;

#[cfg(feature = "pdf")]
use crate::pdf::kittyv2::kgfx::cleanup_all_shms;

pub fn initialize_panic_handler() {
    better_panic::install();

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore_terminal();

        // Clean up SHM objects before exiting to prevent resource leaks in /dev/shm
        #[cfg(feature = "pdf")]
        cleanup_all_shms();

        default_hook(panic_info);

        std::process::exit(1);
    }));
}

/// Restore terminal to a clean state
///
/// Specifically handles:
/// - Disabling raw mode
/// - Exiting alternate screen
/// - Disabling mouse capture (important for restoring mouse functionality)
/// - Disabling keyboard enhancement flags
/// - Showing the cursor
pub fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    let _ = execute!(io::stderr(), crossterm::cursor::Show);
    let _ = writeln!(io::stderr());
}
