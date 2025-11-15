use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use log::error;
use std::cell::Cell;
use std::io::{self, Write};
use std::panic;

thread_local! {
    static SUPPRESS_EXIT: Cell<bool> = Cell::new(false);
}

pub fn initialize_panic_handler() {
    better_panic::install();

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let suppress = SUPPRESS_EXIT.with(|flag| flag.get());
        if suppress {
            if let Some(msg) = panic_info.payload().downcast_ref::<&str>() {
                error!("Suppressed panic: {}", msg);
            } else if let Some(msg) = panic_info.payload().downcast_ref::<String>() {
                error!("Suppressed panic: {}", msg);
            } else {
                error!("Suppressed panic with unknown payload");
            }
            // Do not restore terminal or exit; allow catch_unwind to handle it.
            return;
        }

        restore_terminal();
        default_hook(panic_info);
        std::process::exit(1);
    }));
}

pub fn with_panic_exit_suppressed<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    struct ExitGuard(bool);
    impl Drop for ExitGuard {
        fn drop(&mut self) {
            SUPPRESS_EXIT.with(|flag| flag.set(self.0));
        }
    }

    let previous = SUPPRESS_EXIT.with(|flag| {
        let prev = flag.get();
        flag.set(true);
        prev
    });
    let guard = ExitGuard(previous);
    let result = f();
    drop(guard);
    result
}

/// Restore terminal to a clean state
///
/// Specifically handles:
/// - Disabling raw mode
/// - Exiting alternate screen
/// - Disabling mouse capture (important for restoring mouse functionality)
/// - Disabling keyboard enhancement flags
/// - Showing the cursor
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    let _ = execute!(io::stderr(), crossterm::cursor::Show);
    let _ = writeln!(io::stderr());
}

/// Initialize human-panic metadata for release builds
#[cfg(not(debug_assertions))]
use human_panic::Metadata;
