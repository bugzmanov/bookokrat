#[cfg(unix)]
use std::io::{self, Write};
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

#[cfg(unix)]
use crate::pdf::kittyv2::kgfx::{MemoryRegion, QueryCommand, Response, parse_response};
use crate::pdf::kittyv2::terminal_canvas::types::TransferMode;
#[cfg(unix)]
use crate::pdf::kittyv2::terminal_io;

#[cfg(unix)]
const PROBE_TIMEOUT: Duration = Duration::from_millis(800);
#[cfg(unix)]
const PROBE_IMAGE_ID: u32 = 1;

pub fn probe_capabilities() -> TransferMode {
    #[cfg(not(unix))]
    {
        return TransferMode::Chunked;
    }

    #[cfg(unix)]
    {
        // Enable raw mode to read terminal responses
        let raw_mode_was_enabled = enable_raw_mode().is_ok();

        let result = match probe_shared_memory() {
            Ok(true) => TransferMode::SharedMemory,
            _ => TransferMode::Chunked,
        };

        // Restore terminal state - raw mode will be re-enabled by main.rs
        if raw_mode_was_enabled {
            let _ = disable_raw_mode();
        }

        result
    }
}

#[cfg(unix)]
fn probe_shared_memory() -> io::Result<bool> {
    let mut region = MemoryRegion::create_with_pattern("probev2-*", 4)?;
    region.write(&[0, 0, 0, 255])?;
    let shm_path = region.path().to_string();
    region.close_fd();

    let mut stdout = io::stdout();
    QueryCommand::new().image_id(PROBE_IMAGE_ID).write_to(
        &mut stdout,
        &shm_path,
        crate::pdf::kittyv2::is_tmux_mode(),
    )?;
    stdout.flush()?;

    let response = read_response_with_timeout(PROBE_TIMEOUT)?;

    // Clean up probe SHM - terminal has already read it by now
    region.unlink()?;

    match response {
        Some(response) => Ok(response.is_ok()),
        None => Ok(false),
    }
}

#[cfg(unix)]
fn read_response_with_timeout(timeout: Duration) -> io::Result<Option<Response>> {
    terminal_io::read_response_with_timeout(timeout, parse_response)
}
