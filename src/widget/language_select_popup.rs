use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use crate::theme::Base16Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    English,
    Chinese,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Chinese => "中文",
        }
    }

    pub fn prompt_instruction(&self) -> &'static str {
        match self {
            Language::English => {
                "Please summarize the following text in English in a concise manner (around 3-5 bullet points):\n\n"
            }
            Language::Chinese => "请用中文简洁地总结以下文本（大约3-5个要点）：\n\n",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LanguageSelectAction {
    Selected(Language),
    Close,
}

#[derive(Clone)]
pub struct LanguageSelectPopup {
    items: Vec<Language>,
    state: ListState,
    last_popup_area: Option<Rect>,
}

impl LanguageSelectPopup {
    pub fn new() -> Self {
        let items = vec![Language::English, Language::Chinese];
        let mut state = ListState::default();
        state.select(Some(0));

        LanguageSelectPopup {
            items,
            state,
            last_popup_area: None,
        }
    }

    pub fn with_selected(selected: Language) -> Self {
        let items = vec![Language::English, Language::Chinese];
        let index = items.iter().position(|&l| l == selected).unwrap_or(0);
        let mut state = ListState::default();
        state.select(Some(index));

        LanguageSelectPopup {
            items,
            state,
            last_popup_area: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, _palette: &Base16Palette) {
        let popup_area = self.centered_rect(40, 30, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|lang| {
                ListItem::new(Line::from(Span::styled(
                    lang.as_str(),
                    Style::default().fg(Color::White),
                )))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Select Language ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White))
                    .style(Style::default().bg(Color::Rgb(64, 64, 64))),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(64, 64, 64))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("» ");

        f.render_stateful_widget(list, popup_area, &mut self.state);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<LanguageSelectAction> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.next();
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.previous();
                None
            }
            KeyCode::Enter => {
                if let Some(selected) = self.state.selected() {
                    Some(LanguageSelectAction::Selected(self.items[selected]))
                } else {
                    None
                }
            }
            KeyCode::Esc => Some(LanguageSelectAction::Close),
            _ => None,
        }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
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

impl Default for LanguageSelectPopup {
    fn default() -> Self {
        Self::new()
    }
}
