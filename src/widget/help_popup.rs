use crate::inputs::KeySeq;
use crate::theme::OCEANIC_NEXT;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

pub enum HelpPopupAction {
    Close,
}

pub struct HelpPopup {
    content: String,
    scroll_offset: usize,
    last_popup_area: Option<Rect>,
}

impl HelpPopup {
    pub fn new() -> Self {
        let content = include_str!("../../readme.txt").to_string();

        HelpPopup {
            content,
            scroll_offset: 0,
            last_popup_area: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        // Calculate the maximum line width in the content
        let max_content_width = self
            .content
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(80);

        // Add 4 characters for left and right margins (2 chars each side)
        // Add 2 more for borders
        let desired_width = (max_content_width + 6).min(area.width as usize);

        let popup_area = content_sized_rect(desired_width as u16, 90, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let lines: Vec<Line> = self
            .content
            .lines()
            .skip(self.scroll_offset)
            .map(|line| {
                Line::from(Span::styled(
                    format!("  {}", line),
                    Style::default().fg(OCEANIC_NEXT.base_05),
                ))
            })
            .collect();

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Help - Press ? or ESC to close ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(OCEANIC_NEXT.base_0c))
                    .style(Style::default().bg(OCEANIC_NEXT.base_00)),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, popup_area);
    }

    pub fn scroll_down(&mut self) {
        let max_lines = self.content.lines().count();
        if self.scroll_offset < max_lines.saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_page_down(&mut self, page_size: usize) {
        let max_lines = self.content.lines().count();
        self.scroll_offset = (self.scroll_offset + page_size).min(max_lines.saturating_sub(1));
    }

    fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    fn scroll_to_bottom(&mut self) {
        let max_lines = self.content.lines().count();
        self.scroll_offset = max_lines.saturating_sub(1);
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<HelpPopupAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('j') => {
                self.scroll_down();
                None
            }
            KeyCode::Char('k') => {
                self.scroll_up();
                None
            }
            KeyCode::Char('g') if key_seq.handle_key('g') == "gg" => {
                self.scroll_to_top();
                key_seq.clear();
                None
            }
            KeyCode::Char('G') => {
                self.scroll_to_bottom();
                None
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page_size = if let Some(area) = self.last_popup_area {
                    (area.height as usize / 2).max(1)
                } else {
                    10
                };
                self.scroll_page_down(page_size);
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page_size = if let Some(area) = self.last_popup_area {
                    (area.height as usize / 2).max(1)
                } else {
                    10
                };
                self.scroll_page_up(page_size);
                None
            }
            KeyCode::Esc | KeyCode::Char('?') => Some(HelpPopupAction::Close),
            _ => None,
        }
    }
}

fn content_sized_rect(width: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    // Calculate centering based on fixed width
    let available_width = r.width;
    let width = width.min(available_width);
    let margin = (available_width.saturating_sub(width)) / 2;

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(margin),
            Constraint::Length(width),
            Constraint::Length(margin),
        ])
        .split(popup_layout[1])[1]
}
