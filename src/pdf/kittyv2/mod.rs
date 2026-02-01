//! Kitty graphics protocol v2 - clean room reimplementation.
//!
//! This module provides high-performance terminal graphics using the Kitty
//! graphics protocol with shared memory (SHM) transmission for 60fps PDF rendering.

#![cfg(feature = "pdf")]

pub mod image;
pub mod kgfx;
pub mod terminal_canvas;

use std::io::{self, Read, Write};
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{cursor::MoveTo, execute};
use ratatui::layout::Position;
use std::os::unix::io::AsRawFd;

pub use image::{Dimensions, Image, ImageId, ImageState, Transmission};
pub use kgfx::{
    DeleteCommand, DirectTransmit, DisplayCommand, Format, Quiet, TransmitCommand, tracker,
};
pub use terminal_canvas::{
    FrameHandle, FrameRegistry, FrameSpec, OperationBatch, PixelEncoder, RemovalTarget,
    ResponseMode, ScreenPlacement, SubmissionError, SubmissionOutcome, TerminalCanvas,
    TransferMode, probe_capabilities,
};

/// Display location configuration for an image.
#[derive(Debug, Clone, Copy, Default)]
pub struct DisplayLocation {
    /// Source rectangle X offset in pixels.
    pub x: u32,
    /// Source rectangle Y offset in pixels.
    pub y: u32,
    /// Source rectangle width in pixels (0 = full width).
    pub width: u32,
    /// Source rectangle height in pixels (0 = full height).
    pub height: u32,
    /// Destination width in terminal columns.
    pub columns: u16,
    /// Destination height in terminal rows.
    pub rows: u16,
}

/// Image display request.
#[derive(Debug)]
pub struct ImageRequest<'a> {
    /// The image state (queued or uploaded).
    pub image: &'a mut ImageState,
    /// Page number this image represents.
    pub page: usize,
    /// Terminal position to display at.
    pub position: Position,
    /// Display location configuration.
    pub location: DisplayLocation,
}

/// Batch of display operations.
#[derive(Debug)]
pub enum DisplayBatch<'a> {
    /// No changes needed.
    NoChange,
    /// Clear all images.
    Clear,
    /// Display a list of images.
    Display(Vec<ImageRequest<'a>>),
}

/// Execute a display batch directly using kittyv2 protocol.
pub fn execute_display_batch(batch: DisplayBatch) -> io::Result<()> {
    let _ = execute_display_batch_with_failures(batch)?;
    Ok(())
}

/// Execute a display batch and return pages that failed to display.
pub fn execute_display_batch_with_failures(batch: DisplayBatch) -> io::Result<Vec<usize>> {
    let mut stdout = io::stdout();
    let mut failed_pages = Vec::new();

    match batch {
        DisplayBatch::NoChange => Ok(failed_pages),
        DisplayBatch::Clear => {
            DeleteCommand::all()
                .clear()
                .quiet(Quiet::Silent)
                .write_to(&mut stdout)?;
            stdout.flush()?;
            Ok(failed_pages)
        }
        DisplayBatch::Display(requests) => {
            for request in requests {
                let page = request.page;
                if let Err(err) = display_image(request, &mut stdout) {
                    log::debug!("kittyv2 display failed for page {page}: {err}");
                    failed_pages.push(page);
                }
            }
            stdout.flush()?;
            Ok(failed_pages)
        }
    }
}

/// Display a single image using kittyv2 protocol.
fn display_image(request: ImageRequest, stdout: &mut io::Stdout) -> io::Result<()> {
    execute!(stdout, MoveTo(request.position.x, request.position.y))?;

    let loc = &request.location;
    let has_source_rect = loc.x > 0 || loc.y > 0 || loc.width > 0 || loc.height > 0;

    match request.image {
        ImageState::Queued(image) => {
            let dims = image.dimensions();
            let image_id = image.id.id.get();
            let new_id = ImageId::new(image.id.id);

            match &image.transmission {
                Transmission::SharedMemory { path, size } => {
                    let mut cmd = TransmitCommand::new(dims.width, dims.height)
                        .format(Format::Rgb)
                        .image_id(image_id)
                        .placement_id(image_id)
                        .quiet(Quiet::ErrorsOnly)
                        .no_cursor_move(true);

                    if loc.columns > 0 || loc.rows > 0 {
                        cmd = cmd.dest_cells(loc.columns, loc.rows);
                    }

                    if has_source_rect {
                        cmd = cmd.source_rect(loc.x, loc.y, loc.width, loc.height);
                    }

                    cmd.write_to(stdout, path)?;
                    tracker().lock().unwrap().register(
                        path.to_string(),
                        *size,
                        request.page as i64,
                    );
                }
                Transmission::Direct { data, .. } => {
                    let mut cmd = DirectTransmit::new(dims.width, dims.height)
                        .format(Format::Rgb)
                        .image_id(image_id)
                        .placement_id(image_id)
                        .quiet(Quiet::ErrorsOnly)
                        .no_cursor_move(true);

                    if loc.columns > 0 || loc.rows > 0 {
                        cmd = cmd.dest_cells(loc.columns, loc.rows);
                    }

                    if has_source_rect {
                        cmd = cmd.source_rect(loc.x, loc.y, loc.width, loc.height);
                    }

                    cmd.send(stdout, data)?;
                }
            }

            stdout.flush()?;

            // Update state to Uploaded
            *request.image = ImageState::Uploaded(new_id);
        }
        ImageState::Uploaded(image_id) => {
            let mut cmd = DisplayCommand::new(image_id.id.get())
                .placement_id(image_id.id.get())
                .quiet(Quiet::ErrorsOnly)
                .no_cursor_move(true);

            if loc.columns > 0 || loc.rows > 0 {
                cmd = cmd.dest_cells(loc.columns, loc.rows);
            }

            if has_source_rect {
                cmd = cmd.source_rect(loc.x, loc.y, loc.width, loc.height);
            }

            cmd.write_to(stdout)?;
        }
    }

    Ok(())
}

/// Delete all images.
pub fn delete_all_images() -> io::Result<()> {
    let mut stdout = io::stdout();
    DeleteCommand::all()
        .delete()
        .quiet(Quiet::Silent)
        .write_to(&mut stdout)?;
    stdout.flush()
}

/// Delete a single image by ID.
pub fn delete_image_by_id(id: NonZeroU32) -> io::Result<()> {
    let mut stdout = io::stdout();
    DeleteCommand::by_id(id.get())
        .delete()
        .quiet(Quiet::Silent)
        .write_to(&mut stdout)?;
    stdout.flush()
}

/// Delete images in an ID range.
pub fn delete_images_by_range(start: NonZeroU32, end: NonZeroU32) -> io::Result<()> {
    let mut stdout = io::stdout();
    DeleteCommand::by_range(start.get(), end.get())
        .delete()
        .quiet(Quiet::Silent)
        .write_to(&mut stdout)?;
    stdout.flush()
}

/// Clear images in an ID range (hide but keep in memory).
pub fn clear_images_by_range(start: NonZeroU32, end: NonZeroU32) -> io::Result<()> {
    let mut stdout = io::stdout();
    DeleteCommand::by_range(start.get(), end.get())
        .clear()
        .quiet(Quiet::Silent)
        .write_to(&mut stdout)?;
    stdout.flush()
}

/// Clear placements for an image ID (hide but keep in memory).
pub fn clear_placement(id: NonZeroU32) -> io::Result<()> {
    let mut stdout = io::stdout();
    // Clear ALL placements for this image ID (not a specific placement)
    DeleteCommand::by_id(id.get())
        .clear()
        .quiet(Quiet::Silent)
        .write_to(&mut stdout)?;
    stdout.flush()
}

/// Update viewport position for lifecycle tracking.
pub fn set_viewport_position(page: i64) {
    tracker().lock().unwrap().set_position(page);
}

/// Dump SHM state for debugging.
pub fn dump_shm_state() {
    let tracker = tracker().lock().unwrap();
    log::debug!("SHM tracker has {} registered regions", tracker.len());
}

/// Probes whether the terminal supports deleting images by ID range.
///
/// Modern Kitty terminals support this feature. This function returns true
/// by default as range deletion is well-supported and the fallback (individual
/// deletes) works if range deletion fails.
pub fn probe_delete_range_support() -> bool {
    const PROBE_TIMEOUT: Duration = Duration::from_millis(800);

    let _ = enable_raw_mode();

    let id_a = NonZeroU32::new(90001).unwrap();
    let id_b = NonZeroU32::new(90002).unwrap();

    let mut stdout = io::stdout();

    // Transmit two tiny images so we can verify display after delete-range.
    let red = [255u8, 0, 0];
    let blue = [0u8, 0, 255];

    let _ = DirectTransmit::new(1, 1)
        .format(Format::Rgb)
        .image_id(id_a.get())
        .placement_id(id_a.get())
        .quiet(Quiet::ErrorsOnly)
        .no_cursor_move(true)
        .send(&mut stdout, &red);

    let _ = DirectTransmit::new(1, 1)
        .format(Format::Rgb)
        .image_id(id_b.get())
        .placement_id(id_b.get())
        .quiet(Quiet::ErrorsOnly)
        .no_cursor_move(true)
        .send(&mut stdout, &blue);

    let _ = stdout.flush();

    // Issue a delete-by-range; unsupported terminals will respond with an error.
    let _ = DeleteCommand::by_range(1, 13)
        .delete()
        .write_to(&mut stdout);
    let _ = stdout.flush();

    let ok_a = display_and_check(&mut stdout, id_a.get(), PROBE_TIMEOUT);
    let ok_b = display_and_check(&mut stdout, id_b.get(), PROBE_TIMEOUT);

    let _ = DeleteCommand::by_id(id_a.get())
        .delete()
        .quiet(Quiet::Silent)
        .write_to(&mut stdout);
    let _ = DeleteCommand::by_id(id_b.get())
        .delete()
        .quiet(Quiet::Silent)
        .write_to(&mut stdout);
    let _ = stdout.flush();

    let _ = disable_raw_mode();

    ok_a && ok_b
}

fn display_and_check(stdout: &mut io::Stdout, id: u32, timeout: Duration) -> bool {
    let cmd = DisplayCommand::new(id)
        .placement_id(id)
        .dest_cells(1, 1)
        .no_cursor_move(true);

    if cmd.write_to(stdout).is_err() {
        return false;
    }
    let _ = stdout.flush();

    match read_response_with_timeout(timeout) {
        Ok(Some(resp)) => resp.is_ok(),
        _ => false,
    }
}

fn read_response_with_timeout(timeout: Duration) -> io::Result<Option<kgfx::Response>> {
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

        if let Some(response) = kgfx::parse_response(&buffer) {
            return Ok(Some(response));
        }
    }
}
