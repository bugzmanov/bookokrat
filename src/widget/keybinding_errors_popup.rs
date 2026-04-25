use crate::keybindings::config::LoadError;
use crate::theme::current_theme;
use crate::widget::popup::Popup;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

pub struct KeybindingErrorsPopup {
    errors: Vec<LoadError>,
    scroll: u16,
    last_area: Option<Rect>,
}

impl KeybindingErrorsPopup {
    pub fn new(errors: Vec<LoadError>) -> Self {
        Self {
            errors,
            scroll: 0,
            last_area: None,
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_half_page_down(&mut self, visible_height: u16) {
        self.scroll = self.scroll.saturating_add(visible_height / 2);
    }

    pub fn scroll_half_page_up(&mut self, visible_height: u16) {
        self.scroll = self.scroll.saturating_sub(visible_height / 2);
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(82, 70, area);
        self.last_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let palette = current_theme();
        let block = Block::default()
            .title(" Keybindings config errors ")
            .title_bottom(Line::from(" j/k scroll  Esc close ").right_aligned())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette.base_08))
            .style(Style::default().bg(palette.base_00));

        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);

        self.render_header(f, chunks[0], palette);
        self.render_body(f, chunks[2], palette);
    }

    fn render_header(&self, f: &mut Frame, area: Rect, palette: &crate::theme::Base16Palette) {
        let count = self.errors.len();
        let summary = if count == 1 {
            "1 issue found in keybindings.toml.".to_string()
        } else {
            format!("{count} issues found in keybindings.toml.")
        };

        let lines = vec![
            Line::from(Span::styled(
                summary,
                Style::default()
                    .fg(palette.base_08)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Valid bindings in the file were still applied; listed entries were skipped.",
                Style::default().fg(palette.base_04),
            )),
            Line::from(Span::styled(
                "Fix the file, then press Ctrl+R to reload.",
                Style::default().fg(palette.base_04),
            )),
        ];

        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    fn render_body(&self, f: &mut Frame, area: Rect, palette: &crate::theme::Base16Palette) {
        let mut lines: Vec<Line> = Vec::new();
        for err in &self.errors {
            let location = match err.line {
                Some(n) => format!("line {n}"),
                None => "file".to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {location}: "),
                    Style::default()
                        .fg(palette.base_0d)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.message.clone(), Style::default().fg(palette.base_05)),
            ]));
            lines.push(Line::from(""));
        }

        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            area,
        );
    }
}

impl Popup for KeybindingErrorsPopup {
    fn get_last_popup_area(&self) -> Option<Rect> {
        self.last_area
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
