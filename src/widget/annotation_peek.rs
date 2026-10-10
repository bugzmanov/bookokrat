use crate::theme::current_theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub(crate) fn render(frame: &mut Frame, area: Rect, x: u16, y: u16, title: &str, content: &str) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let theme = current_theme();
    let width = content
        .lines()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        .saturating_add(2)
        .clamp(24, 60)
        .min(area.width as usize) as u16;
    let mut lines: Vec<Line> = content
        .lines()
        .flat_map(|line| textwrap::wrap(line, width.saturating_sub(2) as usize))
        .map(|line| Line::from(line.into_owned()))
        .collect();
    if lines.is_empty() {
        lines.push(Line::from("(empty annotation)"));
    }
    let max_height = area.height.saturating_sub(1).min(12);
    let height = (lines.len().saturating_add(2).min(max_height as usize)) as u16;
    if lines.len() > height.saturating_sub(2) as usize {
        lines.truncate(height.saturating_sub(2) as usize);
        if let Some(last) = lines.last_mut() {
            *last = Line::from("… More in Comments Viewer");
        }
    }
    let right = area.x.saturating_add(area.width);
    let bottom = area.y.saturating_add(area.height);
    let below = y.saturating_add(1);
    let popup_y = if below.saturating_add(height) <= bottom {
        below
    } else {
        y.saturating_sub(height).max(area.y)
    };
    let popup = Rect::new(
        x.clamp(area.x, right.saturating_sub(width)),
        popup_y,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(" {title} "))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.popup_border_color())),
            )
            .style(Style::default().fg(theme.base_05).bg(theme.base_00)),
        popup,
    );
}
