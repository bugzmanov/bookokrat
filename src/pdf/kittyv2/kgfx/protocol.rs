use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// Escape sequence bytes.
const ESC: u8 = 0x1B;
const APC_START: &[u8] = &[ESC, b'_', b'G'];
const APC_END: &[u8] = &[ESC, b'\\'];

// Tmux DCS passthrough state and sequences.
static IS_TMUX: AtomicBool = AtomicBool::new(false);

/// Enable tmux DCS passthrough wrapping for all Kitty graphics commands.
pub fn set_tmux_mode(tmux: bool) {
    IS_TMUX.store(tmux, Ordering::Relaxed);
}

/// Returns true if tmux DCS wrapping is active.
pub fn is_tmux() -> bool {
    IS_TMUX.load(Ordering::Relaxed)
}

const DCS_START: &[u8] = b"\x1bPtmux;";
const DCS_END: &[u8] = b"\x1b\\";
const TMUX_APC_START: &[u8] = &[ESC, ESC, b'_', b'G'];
const TMUX_APC_END: &[u8] = &[ESC, ESC, b'\\'];

fn write_apc_start<W: Write>(writer: &mut W) -> io::Result<()> {
    if is_tmux() {
        writer.write_all(DCS_START)?;
        writer.write_all(TMUX_APC_START)
    } else {
        writer.write_all(APC_START)
    }
}

fn write_apc_end<W: Write>(writer: &mut W) -> io::Result<()> {
    if is_tmux() {
        writer.write_all(TMUX_APC_END)?;
        writer.write_all(DCS_END)
    } else {
        writer.write_all(APC_END)
    }
}

/// Pixel format for image data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// RGB format (24-bit, 3 bytes per pixel).
    Rgb,
    /// RGBA format (32-bit, 4 bytes per pixel).
    Rgba,
}

impl Format {
    /// Returns the protocol format code.
    fn code(self) -> u8 {
        match self {
            Format::Rgb => 24,
            Format::Rgba => 32,
        }
    }

    /// Returns bytes per pixel.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Format::Rgb => 3,
            Format::Rgba => 4,
        }
    }
}

/// Quiet mode for terminal responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quiet {
    /// Terminal responds to all commands.
    #[default]
    Normal,
    /// Terminal only responds on errors.
    ErrorsOnly,
    /// Terminal never responds.
    Silent,
}

impl Quiet {
    /// Returns the protocol quiet code, if any.
    fn code(self) -> Option<u8> {
        match self {
            Quiet::Normal => None,
            Quiet::ErrorsOnly => Some(1),
            Quiet::Silent => Some(2),
        }
    }
}

/// Compression mode for direct transmissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    #[default]
    None,
    Zlib,
}

impl Compression {
    fn code(self) -> Option<&'static str> {
        match self {
            Compression::None => None,
            Compression::Zlib => Some("z"),
        }
    }
}

/// Source rectangle for cropping (REQ-P5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Destination cell size (REQ-P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestCells {
    pub columns: u16,
    pub rows: u16,
}

/// Builds and writes a Kitty graphics protocol transmit command.
///
/// Uses shared memory transmission (`t=s`) with the provided SHM path.
pub struct TransmitCommand {
    width: u32,
    height: u32,
    format: Format,
    image_id: Option<u32>,
    placement_id: Option<u32>,
    quiet: Quiet,
    no_cursor_move: bool,
    source_rect: Option<SourceRect>,
    dest_cells: Option<DestCells>,
}

impl TransmitCommand {
    /// Creates a new transmit command with required dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: Format::Rgba,
            image_id: None,
            placement_id: None,
            quiet: Quiet::default(),
            no_cursor_move: true,
            source_rect: None,
            dest_cells: None,
        }
    }

    /// Sets the pixel format.
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Sets the image ID.
    pub fn image_id(mut self, id: u32) -> Self {
        self.image_id = Some(id);
        self
    }

    /// Sets the placement ID (REQ-P4).
    pub fn placement_id(mut self, id: u32) -> Self {
        self.placement_id = Some(id);
        self
    }

    /// Sets the quiet mode.
    pub fn quiet(mut self, quiet: Quiet) -> Self {
        self.quiet = quiet;
        self
    }

    /// Sets whether to move the cursor after display.
    pub fn no_cursor_move(mut self, no_move: bool) -> Self {
        self.no_cursor_move = no_move;
        self
    }

    /// Sets the source rectangle for cropping (REQ-P5).
    ///
    /// Only the specified region of the image will be displayed.
    pub fn source_rect(mut self, x: u32, y: u32, width: u32, height: u32) -> Self {
        self.source_rect = Some(SourceRect {
            x,
            y,
            width,
            height,
        });
        self
    }

    /// Sets the destination cell size (REQ-P6).
    ///
    /// The image will be scaled to fit the specified terminal cells.
    pub fn dest_cells(mut self, columns: u16, rows: u16) -> Self {
        self.dest_cells = Some(DestCells { columns, rows });
        self
    }

    /// Writes the escape sequence to the given writer.
    ///
    /// The `shm_path` is the POSIX shared memory path (e.g., `/kgfx_12345`).
    /// It will be base64 encoded in the payload section.
    pub fn write_to<W: Write>(&self, writer: &mut W, shm_path: &str) -> io::Result<()> {
        // Build params string
        let mut params = format!(
            "a=T,t=s,f={},s={},v={}",
            self.format.code(),
            self.width,
            self.height
        );

        if let Some(id) = self.image_id {
            params.push_str(&format!(",i={id}"));
        }

        if let Some(id) = self.placement_id {
            params.push_str(&format!(",p={id}"));
        }

        if self.no_cursor_move {
            params.push_str(",C=1");
        }

        if let Some(q) = self.quiet.code() {
            params.push_str(&format!(",q={q}"));
        }

        // Source rectangle (cropping)
        if let Some(ref rect) = self.source_rect {
            params.push_str(&format!(
                ",x={},y={},w={},h={}",
                rect.x, rect.y, rect.width, rect.height
            ));
        }

        // Destination cells (scaling)
        if let Some(ref cells) = self.dest_cells {
            params.push_str(&format!(",c={},r={}", cells.columns, cells.rows));
        }

        // Base64 encode the path
        let payload = BASE64.encode(shm_path.as_bytes());

        // Write: ESC_G<params>;<payload>ESC\
        write_apc_start(writer)?;
        writer.write_all(params.as_bytes())?;
        writer.write_all(b";")?;
        writer.write_all(payload.as_bytes())?;
        write_apc_end(writer)?;

        Ok(())
    }
}

// ============================================================================
// REQ-C1: Direct Transmission
// ============================================================================

/// Maximum payload size per chunk (base64 characters).
pub const CHUNK_LIMIT: usize = 131072;

/// Builds and writes a direct (inline) pixel transmission.
///
/// Unlike `TransmitCommand` which uses SHM paths, this transmits
/// raw pixel data via base64 encoding, chunked as needed.
///
/// Use this as a fallback when:
/// - SHM capability probe fails at startup
/// - SHM creation fails at runtime
/// - Non-local terminal connections (remote, containers)
pub struct DirectTransmit {
    width: u32,
    height: u32,
    format: Format,
    image_id: Option<u32>,
    placement_id: Option<u32>,
    quiet: Quiet,
    no_cursor_move: bool,
    compression: Compression,
    source_rect: Option<SourceRect>,
    dest_cells: Option<DestCells>,
    chunk_limit: usize,
}

impl DirectTransmit {
    /// Creates a new direct transmission for the given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: Format::Rgba,
            image_id: None,
            placement_id: None,
            quiet: Quiet::default(),
            no_cursor_move: true,
            compression: Compression::default(),
            source_rect: None,
            dest_cells: None,
            chunk_limit: CHUNK_LIMIT,
        }
    }

    /// Sets the pixel format (default: Rgba).
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Sets the image ID.
    pub fn image_id(mut self, id: u32) -> Self {
        self.image_id = Some(id);
        self
    }

    /// Sets the placement ID.
    pub fn placement_id(mut self, id: u32) -> Self {
        self.placement_id = Some(id);
        self
    }

    /// Sets quiet mode.
    pub fn quiet(mut self, quiet: Quiet) -> Self {
        self.quiet = quiet;
        self
    }

    /// Sets cursor movement policy (default: true = don't move).
    pub fn no_cursor_move(mut self, no_move: bool) -> Self {
        self.no_cursor_move = no_move;
        self
    }

    /// Sets the compression mode for inline data.
    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// Sets the source rectangle for cropping.
    pub fn source_rect(mut self, x: u32, y: u32, width: u32, height: u32) -> Self {
        self.source_rect = Some(SourceRect {
            x,
            y,
            width,
            height,
        });
        self
    }

    /// Sets the destination cell size (REQ-P6).
    pub fn dest_cells(mut self, columns: u16, rows: u16) -> Self {
        self.dest_cells = Some(DestCells { columns, rows });
        self
    }

    /// Sets maximum chunk size in base64 characters.
    ///
    /// Will be clamped to [4, CHUNK_LIMIT] and aligned down to multiple of 4.
    pub fn chunk_limit(mut self, limit: usize) -> Self {
        self.chunk_limit = limit;
        self
    }

    /// Transmits pixel data to the writer.
    ///
    /// Returns the number of chunks written.
    pub fn send<W: Write>(self, writer: &mut W, pixels: &[u8]) -> io::Result<usize> {
        let encoded = BASE64.encode(pixels);
        self.send_encoded(writer, encoded.as_bytes())
    }

    /// Transmits pre-encoded base64 payload to the writer.
    ///
    /// The caller is responsible for any compression before encoding.
    pub fn send_encoded<W: Write>(self, writer: &mut W, encoded: &[u8]) -> io::Result<usize> {
        // Clamp and align chunk limit
        let limit = self.chunk_limit.clamp(4, CHUNK_LIMIT);
        let limit = limit - (limit % 4);

        // Calculate chunks
        let total_chunks = if encoded.is_empty() {
            1
        } else {
            encoded.len().div_ceil(limit)
        };

        // Build first chunk params
        let mut first_params = format!(
            "a=T,t=d,f={},s={},v={}",
            self.format.code(),
            self.width,
            self.height
        );

        if let Some(id) = self.image_id {
            first_params.push_str(&format!(",i={id}"));
        }

        if let Some(id) = self.placement_id {
            first_params.push_str(&format!(",p={id}"));
        }

        if self.no_cursor_move {
            first_params.push_str(",C=1");
        }

        if let Some(code) = self.compression.code() {
            first_params.push_str(&format!(",o={code}"));
        }

        if let Some(q) = self.quiet.code() {
            first_params.push_str(&format!(",q={q}"));
        }

        if let Some(ref rect) = self.source_rect {
            first_params.push_str(&format!(
                ",x={},y={},w={},h={}",
                rect.x, rect.y, rect.width, rect.height
            ));
        }

        if let Some(ref cells) = self.dest_cells {
            first_params.push_str(&format!(",c={},r={}", cells.columns, cells.rows));
        }

        // Write chunks
        let mut chunks_written = 0;

        for (i, chunk) in encoded.chunks(limit).enumerate() {
            let is_first = i == 0;
            let is_last = i == total_chunks - 1;

            write_apc_start(writer)?;

            if is_first {
                // First chunk: all params
                writer.write_all(first_params.as_bytes())?;
                if total_chunks > 1 {
                    writer.write_all(b",m=1")?;
                }
            } else if is_last {
                // Last chunk: m=0
                writer.write_all(b"m=0")?;
            } else {
                // Middle chunk: m=1
                writer.write_all(b"m=1")?;
            }

            writer.write_all(b";")?;
            writer.write_all(chunk)?;
            write_apc_end(writer)?;

            chunks_written += 1;
        }

        // Handle empty data case
        if chunks_written == 0 {
            write_apc_start(writer)?;
            writer.write_all(first_params.as_bytes())?;
            writer.write_all(b";")?;
            write_apc_end(writer)?;
            chunks_written = 1;
        }

        Ok(chunks_written)
    }
}

// ============================================================================
// REQ-P1: Delete Command
// ============================================================================

/// Delete mode: clear (hide) or delete (free memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeleteMode {
    /// Hide image but keep in memory (lowercase mode chars).
    #[default]
    Clear,
    /// Remove from memory entirely (uppercase mode chars).
    Delete,
}

/// Delete target: what to delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeleteTarget {
    All,
    ById(u32),
    ByRange { min: u32, max: u32 },
    ByPlacement { image_id: u32, placement_id: u32 },
}

/// Builds and writes a delete command.
///
/// Used to clear (hide) or delete (free memory) images from the terminal.
pub struct DeleteCommand {
    target: DeleteTarget,
    mode: DeleteMode,
    quiet: Quiet,
}

impl DeleteCommand {
    /// Delete all images.
    pub fn all() -> Self {
        Self {
            target: DeleteTarget::All,
            mode: DeleteMode::default(),
            quiet: Quiet::default(),
        }
    }

    /// Delete a single image by ID.
    pub fn by_id(id: u32) -> Self {
        Self {
            target: DeleteTarget::ById(id),
            mode: DeleteMode::default(),
            quiet: Quiet::default(),
        }
    }

    /// Delete a range of images by ID (inclusive).
    pub fn by_range(min: u32, max: u32) -> Self {
        Self {
            target: DeleteTarget::ByRange { min, max },
            mode: DeleteMode::default(),
            quiet: Quiet::default(),
        }
    }

    /// Delete a specific placement of an image.
    ///
    /// This targets only the specified placement, leaving other placements
    /// of the same image intact. Used for double-buffering during scroll.
    pub fn by_placement(image_id: u32, placement_id: u32) -> Self {
        Self {
            target: DeleteTarget::ByPlacement {
                image_id,
                placement_id,
            },
            mode: DeleteMode::default(),
            quiet: Quiet::default(),
        }
    }

    /// Set to clear mode (hide only, keep in memory).
    pub fn clear(mut self) -> Self {
        self.mode = DeleteMode::Clear;
        self
    }

    /// Set to delete mode (free memory).
    pub fn delete(mut self) -> Self {
        self.mode = DeleteMode::Delete;
        self
    }

    /// Sets quiet mode.
    pub fn quiet(mut self, quiet: Quiet) -> Self {
        self.quiet = quiet;
        self
    }

    /// Writes the escape sequence to the given writer.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mode_char = match (&self.target, &self.mode) {
            (DeleteTarget::All, DeleteMode::Clear) => 'a',
            (DeleteTarget::All, DeleteMode::Delete) => 'A',
            (DeleteTarget::ById(_), DeleteMode::Clear) => 'i',
            (DeleteTarget::ById(_), DeleteMode::Delete) => 'I',
            (DeleteTarget::ByRange { .. }, DeleteMode::Clear) => 'r',
            (DeleteTarget::ByRange { .. }, DeleteMode::Delete) => 'R',
            // For placement deletion, use 'i'/'I' with both image_id and placement_id
            (DeleteTarget::ByPlacement { .. }, DeleteMode::Clear) => 'i',
            (DeleteTarget::ByPlacement { .. }, DeleteMode::Delete) => 'I',
        };

        let mut params = format!("a=d,d={mode_char}");

        if let Some(q) = self.quiet.code() {
            params.push_str(&format!(",q={q}"));
        }

        match &self.target {
            DeleteTarget::All => {}
            DeleteTarget::ById(id) => {
                params.push_str(&format!(",i={id}"));
            }
            DeleteTarget::ByRange { min, max } => {
                params.push_str(&format!(",x={min},y={max}"));
            }
            DeleteTarget::ByPlacement {
                image_id,
                placement_id,
            } => {
                params.push_str(&format!(",i={image_id},p={placement_id}"));
            }
        }

        write_apc_start(writer)?;
        writer.write_all(params.as_bytes())?;
        writer.write_all(b";")?;
        write_apc_end(writer)?;

        Ok(())
    }
}

// ============================================================================
// REQ-P2: Query Command
// ============================================================================

/// Builds and writes a query command for capability detection.
///
/// Sends a minimal 1x1 pixel image via SHM with query action to detect
/// if the terminal supports shared memory transmission.
pub struct QueryCommand {
    image_id: Option<u32>,
}

impl QueryCommand {
    /// Creates a new query command.
    pub fn new() -> Self {
        Self { image_id: None }
    }

    /// Sets the image ID for tracking.
    pub fn image_id(mut self, id: u32) -> Self {
        self.image_id = Some(id);
        self
    }

    /// Writes the escape sequence to the given writer.
    ///
    /// The `shm_path` should point to a region containing at least 3 bytes
    /// (one RGB pixel). The caller is responsible for creating this region.
    pub fn write_to<W: Write>(&self, writer: &mut W, shm_path: &str) -> io::Result<()> {
        let mut params = String::from("a=q,t=s,f=24,s=1,v=1");

        if let Some(id) = self.image_id {
            params.push_str(&format!(",i={id}"));
        }

        let payload = BASE64.encode(shm_path.as_bytes());

        write_apc_start(writer)?;
        writer.write_all(params.as_bytes())?;
        writer.write_all(b";")?;
        writer.write_all(payload.as_bytes())?;
        write_apc_end(writer)?;

        Ok(())
    }
}

impl Default for QueryCommand {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// REQ-P3: Display Command
// ============================================================================

/// Builds and writes a display command.
///
/// Re-displays an already-transmitted image without re-sending pixel data.
pub struct DisplayCommand {
    image_id: u32,
    placement_id: Option<u32>,
    quiet: Quiet,
    source_rect: Option<SourceRect>,
    dest_cells: Option<DestCells>,
    no_cursor_move: bool,
}

impl DisplayCommand {
    /// Creates a new display command for the given image ID.
    pub fn new(image_id: u32) -> Self {
        Self {
            image_id,
            placement_id: None,
            quiet: Quiet::default(),
            source_rect: None,
            dest_cells: None,
            no_cursor_move: true,
        }
    }

    /// Sets the placement ID.
    pub fn placement_id(mut self, id: u32) -> Self {
        self.placement_id = Some(id);
        self
    }

    /// Sets quiet mode.
    pub fn quiet(mut self, quiet: Quiet) -> Self {
        self.quiet = quiet;
        self
    }

    /// Sets the source rectangle for cropping.
    pub fn source_rect(mut self, x: u32, y: u32, width: u32, height: u32) -> Self {
        self.source_rect = Some(SourceRect {
            x,
            y,
            width,
            height,
        });
        self
    }

    /// Sets the destination cell size (REQ-P6).
    pub fn dest_cells(mut self, columns: u16, rows: u16) -> Self {
        self.dest_cells = Some(DestCells { columns, rows });
        self
    }

    /// Sets whether to move the cursor after display.
    pub fn no_cursor_move(mut self, no_move: bool) -> Self {
        self.no_cursor_move = no_move;
        self
    }

    /// Writes the escape sequence to the given writer.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut params = format!("a=p,i={}", self.image_id);

        if let Some(id) = self.placement_id {
            params.push_str(&format!(",p={id}"));
        }

        if let Some(q) = self.quiet.code() {
            params.push_str(&format!(",q={q}"));
        }

        if let Some(ref rect) = self.source_rect {
            params.push_str(&format!(
                ",x={},y={},w={},h={}",
                rect.x, rect.y, rect.width, rect.height
            ));
        }

        if let Some(ref cells) = self.dest_cells {
            params.push_str(&format!(",c={},r={}", cells.columns, cells.rows));
        }

        if self.no_cursor_move {
            params.push_str(",C=1");
        }

        write_apc_start(writer)?;
        writer.write_all(params.as_bytes())?;
        writer.write_all(b";")?;
        write_apc_end(writer)?;

        Ok(())
    }
}

// ============================================================================
// Response Parsing
// ============================================================================

/// A parsed response from the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The image ID from the response.
    pub image_id: Option<u32>,
    /// The placement ID from the response (REQ-P4).
    pub placement_id: Option<u32>,
    /// The message (e.g., "OK" or error text).
    pub message: String,
}

impl Response {
    /// Returns true if this is a successful response.
    pub fn is_ok(&self) -> bool {
        self.message == "OK"
    }

    /// Returns the error message if this is not a success response.
    pub fn error(&self) -> Option<&str> {
        if self.is_ok() {
            None
        } else {
            Some(&self.message)
        }
    }
}

/// Parses a terminal graphics response.
///
/// Expected format: `ESC_Gi=<id>[,p=<placement>];<message>ESC\`
///
/// Returns `None` if the input doesn't contain a valid response.
pub fn parse_response(data: &[u8]) -> Option<Response> {
    // Find APC start: ESC_G
    let start = data.windows(3).position(|w| w == APC_START)?;
    let rest = &data[start + 3..];

    // Find APC end: ESC\
    let end = rest.windows(2).position(|w| w == APC_END)?;
    let content = &rest[..end];

    // Split on semicolon
    let semicolon = content.iter().position(|&b| b == b';')?;
    let params = &content[..semicolon];
    let message = &content[semicolon + 1..];

    // Parse params
    let params_str = std::str::from_utf8(params).ok()?;
    let mut image_id = None;
    let mut placement_id = None;

    for part in params_str.split(',') {
        if let Some(value) = part.strip_prefix("i=") {
            image_id = value.parse().ok();
        } else if let Some(value) = part.strip_prefix("p=") {
            placement_id = value.parse().ok();
        }
    }

    // Convert message to string
    let message = String::from_utf8_lossy(message).into_owned();

    Some(Response {
        image_id,
        placement_id,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transmit_command_basic() {
        let cmd = TransmitCommand::new(100, 50).format(Format::Rgb);

        let mut output = Vec::new();
        cmd.write_to(&mut output, "/kgfx_test").unwrap();

        let output_str = String::from_utf8_lossy(&output);

        assert!(output_str.starts_with("\x1b_G"));
        assert!(output_str.ends_with("\x1b\\"));
        assert!(output_str.contains("a=T"));
        assert!(output_str.contains("t=s"));
        assert!(output_str.contains("f=24"));
        assert!(output_str.contains("s=100"));
        assert!(output_str.contains("v=50"));
        assert!(output_str.contains("C=1"));

        let expected_payload = BASE64.encode("/kgfx_test");
        assert!(output_str.contains(&format!(";{expected_payload}\x1b")));
    }

    #[test]
    fn test_delete_all() {
        let mut output = Vec::new();
        DeleteCommand::all().delete().write_to(&mut output).unwrap();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("a=d"));
        assert!(output_str.contains("d=A"));
    }

    #[test]
    fn test_parse_response_ok() {
        let data = b"\x1b_Gi=42;OK\x1b\\";
        let response = parse_response(data).unwrap();

        assert_eq!(response.image_id, Some(42));
        assert!(response.is_ok());
    }

    #[test]
    fn test_parse_response_error() {
        let data = b"\x1b_Gi=7;ENOENT:No such file\x1b\\";
        let response = parse_response(data).unwrap();

        assert_eq!(response.image_id, Some(7));
        assert!(!response.is_ok());
        assert_eq!(response.error(), Some("ENOENT:No such file"));
    }

    #[test]
    fn test_format_bytes_per_pixel() {
        assert_eq!(Format::Rgb.bytes_per_pixel(), 3);
        assert_eq!(Format::Rgba.bytes_per_pixel(), 4);
    }
}
