use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::theme::Base16Palette;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChatGPTPopupAction {
    Close,
}

#[derive(Debug, Clone)]
pub enum SummaryState {
    Loading,
    Success(String),
    Error(String),
}

#[derive(Clone)]
pub struct ChatGPTPopup {
    state: SummaryState,
    last_popup_area: Option<Rect>,
}

impl ChatGPTPopup {
    pub fn new() -> Self {
        ChatGPTPopup {
            state: SummaryState::Loading,
            last_popup_area: None,
        }
    }

    pub fn set_summary(&mut self, summary: String) {
        self.state = SummaryState::Success(summary);
    }

    pub fn set_error(&mut self, error: String) {
        self.state = SummaryState::Error(error);
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, _palette: &Base16Palette) {
        let popup_area = self.centered_rect(60, 80, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let (title, content) = match &self.state {
            SummaryState::Loading => (
                " ChatGPT Summary - Loading... ",
                vec![Line::from(Span::styled(
                    "Sending to ChatGPT for summarization...",
                    Style::default().fg(Color::White),
                ))],
            ),
            SummaryState::Success(summary) => (
                " ChatGPT Summary ",
                summary
                    .lines()
                    .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::White))))
                    .collect(),
            ),
            SummaryState::Error(error) => (
                " ChatGPT Summary - Error ",
                vec![Line::from(Span::styled(
                    format!("Error: {}", error),
                    Style::default().fg(Color::White),
                ))],
            ),
        };

        let paragraph = Paragraph::new(content)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White))
                    .style(Style::default().bg(Color::Rgb(64, 64, 64))),
            )
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        f.render_widget(paragraph, popup_area);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<ChatGPTPopupAction> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => Some(ChatGPTPopupAction::Close),
            _ => None,
        }
    }

    fn centered_rect(&self, percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
}
