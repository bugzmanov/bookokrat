use std::io::{self, Write};

use crate::pdf::kittyv2::kgfx::{
    Compression, DeleteCommand, DeleteMode, DirectTransmit, DisplayCommand, Quiet, TransmitCommand,
    pool, tracker,
};
use crate::pdf::kittyv2::terminal_canvas::encoder::PixelEncoder;
use crate::pdf::kittyv2::terminal_canvas::probe;
use crate::pdf::kittyv2::terminal_canvas::registry::FrameRegistry;
use crate::pdf::kittyv2::terminal_canvas::types::{
    FrameHandle, FrameSpec, OperationBatch, RemovalTarget, ResponseMode, ScreenPlacement,
    SubmissionError, SubmissionOutcome, TransferMode,
};

pub struct TerminalCanvas {
    mode: TransferMode,
    registry: FrameRegistry,
    response_mode: ResponseMode,
    next_image_id: u32,
    stdout: io::Stdout,
}

impl TerminalCanvas {
    pub fn new(mode: TransferMode, response_mode: ResponseMode) -> Self {
        Self {
            mode,
            registry: FrameRegistry::new(),
            response_mode,
            next_image_id: 2,
            stdout: io::stdout(),
        }
    }

    pub fn probe_capabilities(&mut self) -> TransferMode {
        let mode = probe::probe_capabilities();
        self.mode = mode;
        mode
    }

    pub fn submit_frame(
        &mut self,
        frame: FrameSpec,
        placement: ScreenPlacement,
    ) -> Result<FrameHandle, SubmissionError> {
        let image_id = self.allocate_image_id();
        let handle = FrameHandle {
            image_id,
            page: frame.page,
        };

        self.write_cursor_move(placement)
            .map_err(SubmissionError::IoFailure)?;

        match self.mode {
            TransferMode::SharedMemory => {
                let shm_path = pool()
                    .lock()
                    .unwrap()
                    .write_and_get_path(&frame.pixels)
                    .map_err(|err| {
                        if err.kind() == io::ErrorKind::WouldBlock {
                            SubmissionError::PoolExhausted
                        } else {
                            SubmissionError::IoFailure(err)
                        }
                    })?;

                let mut cmd = TransmitCommand::new(frame.width, frame.height)
                    .format(frame.format)
                    .image_id(image_id)
                    .placement_id(image_id)
                    .quiet(self.quiet_mode())
                    .no_cursor_move(true)
                    .dest_cells(placement.cell_width, placement.cell_height);

                if placement.has_source_rect() {
                    cmd = cmd.source_rect(
                        placement.source_x,
                        placement.source_y,
                        placement.source_width,
                        placement.source_height,
                    );
                }

                cmd.write_to(&mut self.stdout, &shm_path)
                    .map_err(SubmissionError::IoFailure)?;
                self.stdout.flush().map_err(SubmissionError::IoFailure)?;

                tracker()
                    .lock()
                    .unwrap()
                    .register(shm_path, frame.pixels.len(), frame.page);
            }
            TransferMode::Chunked => {
                let encoded = PixelEncoder::compress_and_encode(&frame.pixels)
                    .map_err(|_| SubmissionError::EncodingFailed)?;

                let mut cmd = DirectTransmit::new(frame.width, frame.height)
                    .format(frame.format)
                    .image_id(image_id)
                    .placement_id(image_id)
                    .quiet(self.quiet_mode())
                    .no_cursor_move(true)
                    .compression(Compression::Zlib)
                    .dest_cells(placement.cell_width, placement.cell_height);

                if placement.has_source_rect() {
                    cmd = cmd.source_rect(
                        placement.source_x,
                        placement.source_y,
                        placement.source_width,
                        placement.source_height,
                    );
                }

                cmd.send_encoded(&mut self.stdout, &encoded)
                    .map_err(SubmissionError::IoFailure)?;
                self.stdout.flush().map_err(SubmissionError::IoFailure)?;
            }
        }

        self.registry.record(frame.page, image_id);
        Ok(handle)
    }

    pub fn show_cached(
        &mut self,
        handle: FrameHandle,
        placement: ScreenPlacement,
    ) -> io::Result<()> {
        let cached = self.registry.lookup(handle.page);
        if cached != Some(handle.image_id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "frame not found in cache",
            ));
        }

        self.write_cursor_move(placement)?;

        let mut cmd = DisplayCommand::new(handle.image_id)
            .placement_id(handle.image_id)
            .quiet(self.quiet_mode())
            .dest_cells(placement.cell_width, placement.cell_height)
            .no_cursor_move(true);

        if placement.has_source_rect() {
            cmd = cmd.source_rect(
                placement.source_x,
                placement.source_y,
                placement.source_width,
                placement.source_height,
            );
        }

        cmd.write_to(&mut self.stdout)?;
        self.stdout.flush()
    }

    pub fn remove(&mut self, target: RemovalTarget) -> io::Result<()> {
        self.remove_with_mode(target, DeleteMode::Delete, true)
    }

    pub fn clear(&mut self, target: RemovalTarget) -> io::Result<()> {
        self.remove_with_mode(target, DeleteMode::Clear, false)
    }

    pub fn set_viewport_position(&mut self, page: i64) {
        tracker().lock().unwrap().set_position(page);
    }

    pub fn flush(&mut self, batch: OperationBatch) -> SubmissionOutcome {
        match batch {
            OperationBatch::Idle => SubmissionOutcome::empty(),
            OperationBatch::ClearScreen => {
                let _ = self.remove(RemovalTarget::Everything);
                SubmissionOutcome::empty()
            }
            OperationBatch::Render(frames) => {
                let mut outcome = SubmissionOutcome::empty();
                for (frame, placement) in frames {
                    let page = frame.page;
                    match self.submit_frame(frame, placement) {
                        Ok(handle) => outcome.successful.push(handle),
                        Err(err) => outcome.failed.push((page, err)),
                    }
                }
                outcome
            }
        }
    }

    pub fn flush_all(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }

    pub fn shutdown(&mut self) {
        let _ = self.remove(RemovalTarget::Everything);
        tracker().lock().unwrap().cleanup_all();
    }

    fn write_cursor_move(&mut self, placement: ScreenPlacement) -> io::Result<()> {
        write!(
            &mut self.stdout,
            "\x1b[{};{}H",
            placement.row, placement.column
        )
    }

    fn quiet_mode(&self) -> Quiet {
        match self.response_mode {
            ResponseMode::Silent => Quiet::Silent,
            ResponseMode::ErrorsOnly => Quiet::ErrorsOnly,
            ResponseMode::Full => Quiet::Normal,
        }
    }

    fn allocate_image_id(&mut self) -> u32 {
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(2);
        id
    }

    fn remove_with_mode(
        &mut self,
        target: RemovalTarget,
        mode: DeleteMode,
        invalidate_registry: bool,
    ) -> io::Result<()> {
        match target {
            RemovalTarget::Everything => {
                let mut cmd = DeleteCommand::all();
                cmd = match mode {
                    DeleteMode::Clear => cmd.clear(),
                    DeleteMode::Delete => cmd.delete(),
                };
                cmd = cmd.quiet(self.quiet_mode());
                cmd.write_to(&mut self.stdout)?;
                self.stdout.flush()?;
                if invalidate_registry {
                    self.registry.clear();
                }
            }
            RemovalTarget::Single(handle) => {
                let mut cmd = DeleteCommand::by_id(handle.image_id);
                cmd = match mode {
                    DeleteMode::Clear => cmd.clear(),
                    DeleteMode::Delete => cmd.delete(),
                };
                cmd = cmd.quiet(self.quiet_mode());
                cmd.write_to(&mut self.stdout)?;
                self.stdout.flush()?;
                if invalidate_registry {
                    self.registry.invalidate(handle.page);
                }
            }
            RemovalTarget::PageRange { min, max } => {
                let ids = self.registry.frames_in_range(min, max);

                for (page, id) in ids {
                    let mut cmd = DeleteCommand::by_id(id);
                    cmd = match mode {
                        DeleteMode::Clear => cmd.clear(),
                        DeleteMode::Delete => cmd.delete(),
                    };
                    cmd = cmd.quiet(self.quiet_mode());
                    cmd.write_to(&mut self.stdout)?;
                    self.stdout.flush()?;
                    if invalidate_registry {
                        self.registry.invalidate(page);
                    }
                }
            }
        }

        Ok(())
    }
}
