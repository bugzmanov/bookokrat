use std::ffi::CString;
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::pdf::kittyv2::kgfx::{MemoryRegion, QueryCommand, Response, parse_response};
use crate::pdf::kittyv2::terminal_canvas::types::TransferMode;

const PROBE_TIMEOUT: Duration = Duration::from_millis(800);
const PROBE_IMAGE_ID: u32 = 1;

pub fn probe_capabilities() -> TransferMode {
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

fn probe_shared_memory() -> io::Result<bool> {
    let mut region = MemoryRegion::create_with_pattern("probev2-*", 4)?;
    region.write(&[0, 0, 0, 255])?;
    let shm_path = region.path().to_string();
    region.close_fd();

    let mut stdout = io::stdout();
    QueryCommand::new()
        .image_id(PROBE_IMAGE_ID)
        .write_to(&mut stdout, &shm_path)?;
    stdout.flush()?;

    let response = read_response_with_timeout(PROBE_TIMEOUT)?;

    // Clean up probe SHM - terminal has already read it by now
    if let Ok(c_path) = CString::new(shm_path.as_str()) {
        unsafe {
            libc::shm_unlink(c_path.as_ptr());
        }
    }

    match response {
        Some(response) => Ok(response.is_ok()),
        None => Ok(false),
    }
}

fn read_response_with_timeout(timeout: Duration) -> io::Result<Option<Response>> {
    let mut stdin = io::stdin();
    let fd = stdin.as_raw_fd();
    let start = Instant::now();
    let mut buffer = Vec::new();

    loop {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Ok(None);
        }
        let remaining = timeout - elapsed;
        let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;

        let mut poll_fd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };

        let ready = unsafe { libc::poll(&mut poll_fd, 1, timeout_ms) };
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        if ready == 0 {
            return Ok(None);
        }

        let mut chunk = [0u8; 1024];
        let read = stdin.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);

        if let Some(response) = parse_response(&buffer) {
            return Ok(Some(response));
        }
    }
}
