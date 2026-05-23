use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

#[cfg(feature = "pdf")]
use crate::annotations::highlight_accent_color;
use crate::annotations::{HighlightColor, highlight_background_color};
use crate::theme::Base16Palette;

/// What the palette key handler should do in response to a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HighlightPaletteAction {
    Apply(HighlightColor),
    Remove,
    Cancel,
    ShowHelp,
    UnknownKey,
}

pub(crate) fn classify_palette_key(code: &KeyCode) -> HighlightPaletteAction {
    match code {
        KeyCode::Esc => HighlightPaletteAction::Cancel,
        KeyCode::Char('?') => HighlightPaletteAction::ShowHelp,
        KeyCode::Char('x') | KeyCode::Char('X') => HighlightPaletteAction::Remove,
        KeyCode::Char(ch) => match HighlightColor::from_shortcut(*ch) {
            Some(color) => HighlightPaletteAction::Apply(color),
            None => HighlightPaletteAction::UnknownKey,
        },
        _ => HighlightPaletteAction::UnknownKey,
    }
}

pub(crate) fn palette_hud_message() -> String {
    let choices = HighlightColor::ALL
        .iter()
        .map(|color| format!("{} {}", color.shortcut(), color.label()))
        .collect::<Vec<_>>()
        .join("  ");
    format!("Highlight: {choices}")
}

pub(crate) fn palette_edit_hud_message() -> String {
    let choices = HighlightColor::ALL
        .iter()
        .map(|color| format!("{} {}", color.shortcut(), color.label()))
        .collect::<Vec<_>>()
        .join("  ");
    format!("Edit highlight: {choices}  x Remove  (same color clears)")
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HighlightPaletteTheme {
    pub fg: Color,
    pub accent: Color,
    pub panel_bg: Color,
    pub header_bg: Color,
    pub swatch_style: HighlightPaletteSwatchStyle,
    /// When true the palette is editing an existing highlight: the title and a
    /// hint line make the recolor / remove options explicit.
    pub show_remove: bool,
    /// The color of the highlight being edited, marked in the swatch row.
    pub current_color: Option<HighlightColor>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HighlightPaletteSwatchStyle {
    Background,
    #[cfg(feature = "pdf")]
    ForegroundBlocks,
}

pub(crate) fn render_centered_highlight_palette(
    frame: &mut Frame<'_>,
    area: Rect,
    palette: &Base16Palette,
    theme: HighlightPaletteTheme,
) -> Option<(u16, u16, u16, u16)> {
    if area.width < 12 || area.height < 5 {
        return None;
    }

    let viewport_cap = area.width.saturating_sub(2).max(1);
    let width = 58u16.min(viewport_cap).max(20u16.min(viewport_cap));
    let height = 5u16.min(area.height);
    let modal_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let pad = 1u16;
    let backing_x = modal_area.x.saturating_sub(pad);
    let backing_y = modal_area.y.saturating_sub(pad);
    let backing_w = (modal_area.width + pad * 2).min(area.x + area.width - backing_x);
    let backing_h = (modal_area.height + pad * 2).min(area.y + area.height - backing_y);

    frame.render_widget(Clear, modal_area);

    let title = if theme.show_remove {
        " Edit highlight "
    } else {
        " Highlight "
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(theme.accent).bg(theme.header_bg),
        ))
        .borders(Borders::ALL)
        .border_set(border::PLAIN)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.panel_bg).fg(theme.fg));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let mut lines = vec![Line::raw(""), highlight_palette_swatches(palette, theme)];
    if theme.show_remove {
        lines.push(Line::styled(
            "x or the current color  →  remove",
            Style::default().fg(theme.fg),
        ));
    }
    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(theme.fg).bg(theme.panel_bg))
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);

    Some((backing_x, backing_y, backing_w, backing_h))
}

fn highlight_palette_swatches(
    palette: &Base16Palette,
    theme: HighlightPaletteTheme,
) -> Line<'static> {
    let mut swatches = Vec::new();
    for (idx, color) in HighlightColor::ALL.iter().enumerate() {
        if idx > 0 {
            swatches.push(Span::raw("  "));
        }

        let is_current = theme.current_color == Some(*color);
        let label = if is_current {
            format!(" {} {} \u{2713} ", color.shortcut(), color.label())
        } else {
            format!(" {} {} ", color.shortcut(), color.label())
        };

        match theme.swatch_style {
            HighlightPaletteSwatchStyle::Background => {
                swatches.push(Span::styled(
                    label,
                    Style::default()
                        .fg(theme.fg)
                        .bg(highlight_background_color(*color, palette))
                        .add_modifier(Modifier::BOLD),
                ));
            }
            #[cfg(feature = "pdf")]
            HighlightPaletteSwatchStyle::ForegroundBlocks => {
                swatches.push(Span::styled(
                    "█",
                    Style::default()
                        .fg(highlight_accent_color(*color, palette))
                        .add_modifier(Modifier::BOLD),
                ));
                swatches.push(Span::raw(" "));
                swatches.push(Span::styled(
                    label.trim().to_string(),
                    Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                ));
            }
        }
    }
    Line::from(swatches)
}
