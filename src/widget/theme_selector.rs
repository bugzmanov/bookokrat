use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::theme::{all_theme_names, current_theme, current_theme_index, set_theme_by_index};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

pub enum ThemeSelectorAction {
    Close,
    ThemeChanged,
}

pub struct ThemeSelector {
    state: ListState,
    theme_names: Vec<String>,
    last_popup_area: Option<Rect>,
}

impl Default for ThemeSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeSelector {
    pub fn new() -> Self {
        let theme_names = all_theme_names();
        let current_idx = current_theme_index();

        let mut state = ListState::default();
        state.select(Some(current_idx));

        ThemeSelector {
            state,
            theme_names,
            last_popup_area: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(40, 50, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let palette = current_theme();
        let current_idx = current_theme_index();

        let items: Vec<ListItem> = self
            .theme_names
            .iter()
            .enumerate()
            .map(|(idx, name)| {
                let is_current = idx == current_idx;
                let marker = if is_current { " (current)" } else { "" };

                ListItem::new(Line::from(vec![
                    Span::styled(name, Style::default().fg(palette.base_06)),
                    Span::styled(marker, Style::default().fg(palette.base_03)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Select Theme ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(palette.base_0c))
                    .style(Style::default().bg(palette.base_00)),
            )
            .highlight_style(
                Style::default()
                    .bg(palette.base_02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("» ");

        f.render_stateful_widget(list, popup_area, &mut self.state);
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.theme_names.len() - 1 {
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
                    self.theme_names.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn apply_selected_theme(&self) -> bool {
        if let Some(idx) = self.state.selected() {
            if idx != current_theme_index() {
                set_theme_by_index(idx);
                return true;
            }
        }
        false
    }

    pub fn handle_mouse_click(&mut self, x: u16, y: u16) -> bool {
        if let Some(popup_area) = self.last_popup_area {
            if x >= popup_area.x
                && x < popup_area.x + popup_area.width
                && y > popup_area.y
                && y < popup_area.y + popup_area.height - 1
            {
                let relative_y = y.saturating_sub(popup_area.y).saturating_sub(1);
                let offset = self.state.offset();
                let new_index = offset + relative_y as usize;

                if new_index < self.theme_names.len() {
                    self.state.select(Some(new_index));
                    return true;
                }
            }
        }
        false
    }

    pub fn is_outside_popup_area(&self, x: u16, y: u16) -> bool {
        if let Some(popup_area) = self.last_popup_area {
            x < popup_area.x
                || x >= popup_area.x + popup_area.width
                || y < popup_area.y
                || y >= popup_area.y + popup_area.height
        } else {
            true
        }
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<ThemeSelectorAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('j') => {
                self.handle_j();
                None
            }
            KeyCode::Char('k') => {
                self.handle_k();
                None
            }
            KeyCode::Char('g') if key_seq.handle_key('g') == "gg" => {
                self.handle_gg();
                None
            }
            KeyCode::Char('G') => {
                self.handle_upper_g();
                None
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_d();
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_u();
                None
            }
            KeyCode::Esc => Some(ThemeSelectorAction::Close),
            KeyCode::Enter => {
                if self.apply_selected_theme() {
                    Some(ThemeSelectorAction::ThemeChanged)
                } else {
                    Some(ThemeSelectorAction::Close)
                }
            }
            _ => None,
        }
    }
}

impl VimNavMotions for ThemeSelector {
    fn handle_h(&mut self) {}

    fn handle_j(&mut self) {
        self.next();
    }

    fn handle_k(&mut self) {
        self.previous();
    }

    fn handle_l(&mut self) {}

    fn handle_ctrl_d(&mut self) {
        for _ in 0..5 {
            let current = self.state.selected().unwrap_or(0);
            if current < self.theme_names.len() - 1 {
                self.next();
            } else {
                break;
            }
        }
    }

    fn handle_ctrl_u(&mut self) {
        for _ in 0..5 {
            let current = self.state.selected().unwrap_or(0);
            if current > 0 {
                self.previous();
            } else {
                break;
            }
        }
    }

    fn handle_gg(&mut self) {
        if !self.theme_names.is_empty() {
            self.state.select(Some(0));
        }
    }

    fn handle_upper_g(&mut self) {
        if !self.theme_names.is_empty() {
            self.state.select(Some(self.theme_names.len() - 1));
        }
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
