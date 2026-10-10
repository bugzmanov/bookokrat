use crate::theme::Base16Palette;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub(crate) struct PeekNote {
    pub(crate) updated_at: String,
    pub(crate) content: String,
}

pub(crate) const MAX_HEIGHT: u16 = 14;
/// Date, blank row, one line of text, the "+N lines" footer and padding.
pub(crate) const MIN_HEIGHT: u16 = 5;
const MIN_WIDTH: u16 = 24;
const MAX_WIDTH: u16 = 64;
const PAD_X: u16 = 1;
/// Columns from the card's left edge to its text: the accent bar plus padding.
pub(crate) const TEXT_INDENT: u16 = 1 + PAD_X;
const CHROME_WIDTH: u16 = TEXT_INDENT + PAD_X;
const PAD_BOTTOM: u16 = 1;
const ACCENT_BAR: &str = "▎";

pub(crate) fn width_for(notes: &[PeekNote], available: u16) -> u16 {
    let natural = notes
        .iter()
        .flat_map(|note| note.content.lines().chain([note.updated_at.as_str()]))
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        .min(u16::MAX as usize) as u16;
    natural
        .saturating_add(CHROME_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH)
        .min(available)
}

/// Each note: its date in gray, a blank row, then the text in the purple used
/// for inline notes. Several notes are separated by a blank row.
pub(crate) fn card_lines(
    notes: &[PeekNote],
    width: u16,
    palette: &Base16Palette,
) -> Vec<Line<'static>> {
    let text_width = width.saturating_sub(CHROME_WIDTH).max(1) as usize;
    let text_style = Style::default().fg(palette.base_0e);
    let date_style = Style::default().fg(palette.base_04);
    let mut lines = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        lines.push(Line::styled(note.updated_at.clone(), date_style));
        lines.push(Line::default());
        if note.content.trim().is_empty() {
            lines.push(Line::styled(
                "(empty note)",
                date_style.add_modifier(Modifier::ITALIC),
            ));
            continue;
        }
        for raw in note.content.lines() {
            if raw.trim().is_empty() {
                lines.push(Line::default());
                continue;
            }
            for wrapped in textwrap::wrap(raw, text_width) {
                lines.push(Line::styled(wrapped.into_owned(), text_style));
            }
        }
    }
    lines
}

pub(crate) fn height_for(line_count: usize) -> u16 {
    (line_count.min(u16::MAX as usize) as u16)
        .saturating_add(PAD_BOTTOM)
        .min(MAX_HEIGHT)
}

/// The card plus a one-column gap on each side, so it never butts against
/// the text it covers. Callers keep this inside the reader area.
pub(crate) fn with_gutter(card: Rect) -> Rect {
    Rect {
        x: card.x.saturating_sub(1),
        width: card.width + 2,
        ..card
    }
}

/// Draws the peek as a filled callout with a purple accent bar down the left
/// edge, echoing the `> ` of inline notes.
pub(crate) fn render(
    frame: &mut Frame,
    rect: Rect,
    mut body: Vec<Line<'static>>,
    palette: &Base16Palette,
    page_background: Color,
) {
    if rect.width <= CHROME_WIDTH || rect.height <= PAD_BOTTOM {
        return;
    }
    let rows = (rect.height - PAD_BOTTOM) as usize;
    if body.len() > rows {
        let hidden = body.len() - rows + 1;
        body.truncate(rows - 1);
        body.push(Line::styled(
            format!(
                "+{hidden} {} · Space+a",
                if hidden == 1 { "line" } else { "lines" }
            ),
            Style::default().fg(palette.base_04),
        ));
    }

    let bar = Span::styled(
        format!("{ACCENT_BAR}{}", " ".repeat(PAD_X as usize)),
        Style::default().fg(palette.base_0e),
    );
    let lines: Vec<Line<'static>> = body
        .into_iter()
        .chain((0..PAD_BOTTOM).map(|_| Line::default()))
        .map(|line| {
            let mut spans = vec![bar.clone()];
            spans.extend(line.spans);
            Line::from(spans).style(line.style)
        })
        .collect();

    let gutter = with_gutter(rect);
    frame.render_widget(Clear, gutter);
    frame.render_widget(
        Block::default().style(Style::default().bg(page_background)),
        gutter,
    );
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(palette.base_05).bg(palette.base_01)),
        rect,
    );
}
