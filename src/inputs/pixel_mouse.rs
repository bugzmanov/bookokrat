//! Pixel-precision mouse coordinates via SGR-pixel mouse mode (DEC private mode 1016).
//!
//! Terminals that support `?1016` (Kitty, Ghostty) report mouse positions in
//! pixels instead of cells, using the exact same SGR wire format crossterm
//! already parses (`ESC[<Cb;Cx;Cy M/m`). crossterm is mode-agnostic, so it
//! happily fills `MouseEvent::column`/`row` with the pixel values.
//!
//! We convert those back to cell coordinates at event ingestion (so the rest of
//! the UI, which is entirely cell-based, is unaffected) while stashing the raw
//! pixel position. The PDF reader then uses the sub-cell fraction to hit-test
//! small links (e.g. citation numbers ~1 cell wide) precisely — a cell is
//! ~16px wide and ~28px tall, so cell quantization otherwise causes clicks on
//! tight clickboxes to miss.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// Whether this terminal supports SGR-pixel mouse (Kitty/Ghostty, not tmux).
// Determined once at startup; gates enabling the mode per PDF session.
static SUPPORTED: AtomicBool = AtomicBool::new(false);
static ENABLED: AtomicBool = AtomicBool::new(false);
// Cell size in pixels, stored as f32 bits. 0.0 means "unknown".
static CELL_W_BITS: AtomicU32 = AtomicU32::new(0);
static CELL_H_BITS: AtomicU32 = AtomicU32::new(0);
// Last observed raw pixel position of a mouse event.
static LAST_PX_X: AtomicU32 = AtomicU32::new(0);
static LAST_PX_Y: AtomicU32 = AtomicU32::new(0);

/// DEC private mode sequence enabling SGR-pixel mouse reporting.
pub const ENABLE_SEQ: &str = "\x1b[?1016h";
/// DEC private mode sequence disabling SGR-pixel mouse reporting.
pub const DISABLE_SEQ: &str = "\x1b[?1016l";

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether the terminal supports SGR-pixel mouse mode. Set once at startup.
pub fn is_supported() -> bool {
    SUPPORTED.load(Ordering::Relaxed)
}

pub fn set_supported(supported: bool) {
    SUPPORTED.store(supported, Ordering::Relaxed);
}

/// Enable SGR-pixel mouse mode by writing `?1016h` to stdout and caching the
/// renderer's cell size. No-op unless the terminal supports it. Confined to PDF
/// sessions — EPUB never emits this sequence, which on some embedded Ghostty
/// builds interferes with inline-image transmission.
pub fn enable_for_pdf(cell_w: u16, cell_h: u16) {
    if !is_supported() || is_enabled() {
        return;
    }
    use std::io::Write;
    let mut out = std::io::stdout();
    if write!(out, "{ENABLE_SEQ}").is_ok() {
        let _ = out.flush();
        set_cell_size(cell_w, cell_h);
        set_enabled(true);
    }
}

/// Disable SGR-pixel mouse mode by writing `?1016l`. No-op when not enabled.
pub fn disable() {
    if !is_enabled() {
        return;
    }
    use std::io::Write;
    let mut out = std::io::stdout();
    let _ = write!(out, "{DISABLE_SEQ}");
    let _ = out.flush();
    set_enabled(false);
}

fn cell_size() -> Option<(f32, f32)> {
    let w = f32::from_bits(CELL_W_BITS.load(Ordering::Relaxed));
    let h = f32::from_bits(CELL_H_BITS.load(Ordering::Relaxed));
    if w > 0.0 && h > 0.0 {
        Some((w, h))
    } else {
        None
    }
}

/// Set the cell size in pixels explicitly. Prefer this over [`refresh_cell_size`]
/// using the renderer's authoritative font size (the image picker's
/// `font_size()`): `window_size()` can round or include padding, and a cell
/// height off by a fraction accumulates down the screen into a whole-row error,
/// landing clicks on the line above the target.
pub fn set_cell_size(width: u16, height: u16) {
    if width > 0 && height > 0 {
        CELL_W_BITS.store(f32::from(width).to_bits(), Ordering::Relaxed);
        CELL_H_BITS.store(f32::from(height).to_bits(), Ordering::Relaxed);
    }
}

/// Query the terminal's cell size in pixels and cache it. Returns true on
/// success. Used only as a bootstrap before the renderer's font size is known;
/// [`set_cell_size`] supersedes it. The cell size is `window_pixels /
/// window_cells`.
pub fn refresh_cell_size() -> bool {
    match crossterm::terminal::window_size() {
        Ok(ws) if ws.columns > 0 && ws.rows > 0 && ws.width > 0 && ws.height > 0 => {
            let cw = f32::from(ws.width) / f32::from(ws.columns);
            let ch = f32::from(ws.height) / f32::from(ws.rows);
            CELL_W_BITS.store(cw.to_bits(), Ordering::Relaxed);
            CELL_H_BITS.store(ch.to_bits(), Ordering::Relaxed);
            true
        }
        _ => false,
    }
}

/// Cell index a pixel coordinate falls into.
fn pixel_to_cell(px: u16, cell_size: f32) -> u16 {
    (f32::from(px) / cell_size) as u16
}

/// Fraction (0.0..1.0) of a pixel coordinate within its cell.
fn pixel_fraction(px: u16, cell_idx: u16, cell_size: f32) -> f32 {
    ((f32::from(px) - f32::from(cell_idx) * cell_size) / cell_size).clamp(0.0, 0.999)
}

/// Convert a raw mouse event whose `column`/`row` carry pixel values (because
/// `?1016` is active) into cell coordinates, stashing the pixel position for
/// later sub-cell hit-testing. No-op when pixel mode is disabled or the cell
/// size is unknown (in which case the event is left untouched).
pub fn normalize_mouse_event(event: &mut crossterm::event::MouseEvent) {
    if !is_enabled() {
        return;
    }
    let Some((cw, ch)) = cell_size() else {
        return;
    };
    let px = event.column;
    let py = event.row;
    LAST_PX_X.store(u32::from(px), Ordering::Relaxed);
    LAST_PX_Y.store(u32::from(py), Ordering::Relaxed);
    event.column = pixel_to_cell(px, cw);
    event.row = pixel_to_cell(py, ch);
}

/// Fractional position (0.0..1.0) of the last mouse pixel within the given cell.
/// Returns (0.0, 0.0) when pixel mode is off or data is unavailable, preserving
/// the cell-quantized behavior (truncation to the cell's top-left). Callers that
/// need to compensate for that quantization (e.g. link hit-testing) check
/// [`is_enabled`] and widen their test accordingly.
pub fn subcell_fraction(cell_col: u16, cell_row: u16) -> (f32, f32) {
    if !is_enabled() {
        return (0.0, 0.0);
    }
    let Some((cw, ch)) = cell_size() else {
        return (0.0, 0.0);
    };
    let px = LAST_PX_X.load(Ordering::Relaxed) as u16;
    let py = LAST_PX_Y.load(Ordering::Relaxed) as u16;
    (
        pixel_fraction(px, cell_col, cw),
        pixel_fraction(py, cell_row, ch),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_index_and_fraction_reconstruct_pixel() {
        // A cell is ~16px wide here. A click at pixel 100 lands in cell 6
        // (96..112) at fraction 0.25.
        let cw = 16.0;
        let cell = pixel_to_cell(100, cw);
        assert_eq!(cell, 6);
        let frac = pixel_fraction(100, cell, cw);
        assert!((frac - 0.25).abs() < 1e-4, "frac={frac}");
        // Reconstructing the pixel from cell + fraction recovers the original.
        let reconstructed = (f32::from(cell) + frac) * cw;
        assert!((reconstructed - 100.0).abs() < 1e-2, "{reconstructed}");
    }

    #[test]
    fn fraction_is_always_within_a_cell() {
        let cw = 28.5;
        for px in 0u16..600 {
            let cell = pixel_to_cell(px, cw);
            let frac = pixel_fraction(px, cell, cw);
            assert!((0.0..1.0).contains(&frac), "px={px} frac={frac}");
        }
    }

    #[test]
    fn left_edge_of_cell_has_zero_fraction() {
        let cw = 20.0;
        // Pixel exactly on a cell boundary (cell 3 starts at 60).
        let cell = pixel_to_cell(60, cw);
        assert_eq!(cell, 3);
        assert_eq!(pixel_fraction(60, cell, cw), 0.0);
    }
}
