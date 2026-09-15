use crate::mcp::protocol::SnapshotFormat;
#[cfg(test)]
use crate::widget::text_reader::LineType;
use crate::widget::text_reader::RenderedLine;
use std::ops::RangeInclusive;

/// Whole current chapter: all rendered lines joined by newline.
pub fn epub_chapter_text(lines: &[RenderedLine]) -> String {
    lines
        .iter()
        .map(|l| l.raw_text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Current quantized viewport screenful.
///
/// `chunk_start = (scroll_offset / height) * height` so minor scrolling
/// within the same screen returns identical text.
pub fn epub_page_text(
    lines: &[RenderedLine],
    scroll_offset: usize,
    viewport_height: usize,
) -> String {
    if lines.is_empty() || viewport_height == 0 {
        return String::new();
    }
    let chunk_start = (scroll_offset / viewport_height) * viewport_height;
    let end = (chunk_start + viewport_height).min(lines.len());
    lines[chunk_start..end]
        .iter()
        .map(|l| l.raw_text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Inclusive page range `[start, end]` for current page ± `window`, clamped to `[0, total)`.
pub fn pdf_neighbor_window(current: usize, total: usize, window: usize) -> RangeInclusive<usize> {
    if total == 0 {
        return 0..=0;
    }
    let last = total - 1;
    let start = current.saturating_sub(window).min(last);
    let end = (current + window).min(last);
    start..=end
}

/// Cheap state key. The snapshot is rebuilt only when this changes.
/// (chapter_index, page_number, screen_index, generation) — whichever the
/// format uses. `generation` is the reader's render cache generation; it bumps
/// on any content change at the same position (comment edit, raw-HTML toggle,
/// image settle), so the snapshot refreshes even when position is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotKey {
    format: SnapshotFormat,
    chapter_index: usize,
    page_number: Option<usize>,
    screen_index: Option<usize>,
    generation: u64,
}

pub fn snapshot_key(
    format: SnapshotFormat,
    chapter_index: usize,
    page_number: Option<usize>,
    screen_index: Option<usize>,
    generation: u64,
) -> SnapshotKey {
    SnapshotKey {
        format,
        chapter_index,
        page_number,
        screen_index,
        generation,
    }
}

#[cfg(test)]
fn line(text: &str) -> RenderedLine {
    let mut l = RenderedLine::empty();
    l.raw_text = text.into();
    l.line_type = LineType::Text;
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_text_joins_all_lines() {
        let lines = vec![line("a"), line("b"), line("c")];
        assert_eq!(epub_chapter_text(&lines), "a\nb\nc");
    }

    #[test]
    fn page_text_quantizes_to_screen_top() {
        // 10 lines, viewport height 4, scrolled to offset 5.
        // quantized chunk_start = (5/4)*4 = 4 -> lines 4,5,6,7
        let lines: Vec<_> = (0..10).map(|i| line(&format!("L{i}"))).collect();
        let text = epub_page_text(&lines, 5, 4);
        assert_eq!(text, "L4\nL5\nL6\nL7");
    }

    #[test]
    fn page_text_same_chunk_for_nearby_scroll() {
        let lines: Vec<_> = (0..10).map(|i| line(&format!("L{i}"))).collect();
        // offsets 4,5,6,7 all quantize to chunk_start 4
        for off in [4, 5, 6, 7] {
            assert_eq!(
                epub_page_text(&lines, off, 4),
                "L4\nL5\nL6\nL7",
                "off={off}"
            );
        }
    }

    #[test]
    fn page_text_clamps_at_end_of_chapter() {
        let lines: Vec<_> = (0..6).map(|i| line(&format!("L{i}"))).collect();
        // 6 lines, height 4, offset 4 -> chunk_start 4 -> only L4,L5 remain
        assert_eq!(epub_page_text(&lines, 4, 4), "L4\nL5");
    }

    #[test]
    fn page_text_empty_chapter() {
        assert_eq!(epub_page_text(&[], 0, 4), "");
    }

    #[test]
    fn pdf_window_mid_book() {
        assert_eq!(pdf_neighbor_window(5, 10, 2), 3..=7);
    }

    #[test]
    fn pdf_window_clamps_at_start() {
        assert_eq!(pdf_neighbor_window(0, 10, 2), 0..=2);
        assert_eq!(pdf_neighbor_window(1, 10, 2), 0..=3);
    }

    #[test]
    fn pdf_window_clamps_at_end() {
        assert_eq!(pdf_neighbor_window(9, 10, 2), 7..=9);
        assert_eq!(pdf_neighbor_window(8, 10, 2), 6..=9);
    }

    #[test]
    fn pdf_window_when_window_exceeds_total() {
        assert_eq!(pdf_neighbor_window(1, 3, 5), 0..=2);
    }

    #[test]
    fn snapshot_key_differs_on_chapter_change() {
        use crate::mcp::protocol::SnapshotFormat;
        let k0 = snapshot_key(SnapshotFormat::Epub, 1, None, Some(2), 0);
        let k1 = snapshot_key(SnapshotFormat::Epub, 2, None, Some(2), 0);
        assert_ne!(k0, k1);
    }

    #[test]
    fn snapshot_key_same_within_screen_chunk() {
        use crate::mcp::protocol::SnapshotFormat;
        let k = snapshot_key(SnapshotFormat::Epub, 1, None, Some(2), 0);
        assert_eq!(k, snapshot_key(SnapshotFormat::Epub, 1, None, Some(2), 0));
    }

    #[test]
    fn snapshot_key_differs_on_generation_change() {
        use crate::mcp::protocol::SnapshotFormat;
        // Same position, but content re-rendered (e.g. a comment was added).
        let k0 = snapshot_key(SnapshotFormat::Epub, 1, None, Some(2), 0);
        let k1 = snapshot_key(SnapshotFormat::Epub, 1, None, Some(2), 1);
        assert_ne!(k0, k1);
    }
}
