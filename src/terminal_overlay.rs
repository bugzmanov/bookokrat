use std::io::{Write, stdout};
use std::sync::OnceLock;

use ratatui::layout::Rect;

pub fn overlay_force_clear_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("BOOKOKRAT_OVERLAY_FORCE_CLEAR")
            .map(|v| v != "0")
            .unwrap_or(false)
    })
}

pub fn kitty_delete_overlay_hack_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("KONSOLE_VERSION").is_ok()
            || std::env::var("TERM_PROGRAM")
                .is_ok_and(|v| v.to_ascii_lowercase().contains("konsole"))
            || crate::terminal::is_warp_terminal()
    })
}

pub fn emit_kitty_delete_all() {
    let _ = stdout().write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\");
}

pub fn clear_overlay_images_if_needed() {
    if kitty_delete_overlay_hack_enabled() {
        emit_kitty_delete_all();
    }
}

pub fn clear_rect_direct(rect: Rect) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    clear_rects_direct([rect]);
}

pub fn clear_rects_direct(rects: impl IntoIterator<Item = Rect>) {
    clear_rects_direct_bg(rects, None);
}

/// Like [`clear_rects_direct`] but erases with ECH (`CSI n X`) under an
/// explicit background color. Two reasons over printing spaces: ECH is the
/// erase op that reliably detaches inline-image fragments from cells (printing
/// spaces does not on all terminals), and the bg keeps the cleared strip from
/// flashing in whatever color the SGR state happened to be.
pub fn clear_rects_direct_bg(rects: impl IntoIterator<Item = Rect>, bg: Option<(u8, u8, u8)>) {
    let mut out = stdout();
    let _ = write!(out, "\x1b7");
    if let Some((r, g, b)) = bg {
        let _ = write!(out, "\x1b[48;2;{r};{g};{b}m");
    }
    for rect in rects {
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        for dy in 0..rect.height {
            let _ = write!(
                out,
                "\x1b[{};{}H\x1b[{}X",
                rect.y.saturating_add(dy).saturating_add(1),
                rect.x.saturating_add(1),
                rect.width
            );
        }
    }
    if bg.is_some() {
        let _ = write!(out, "\x1b[49m");
    }
    let _ = write!(out, "\x1b8");
    let _ = out.flush();
}
