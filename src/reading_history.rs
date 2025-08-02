use crate::bookmark::Bookmarks;
use crate::theme::OCEANIC_NEXT;
use chrono::{DateTime, Local, TimeZone};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
    Frame,
};
use std::collections::HashMap;

pub struct ReadingHistory {
    items: Vec<HistoryItem>,
    state: ListState,
}

#[derive(Clone)]
struct HistoryItem {
    date: DateTime<Local>,
    title: String,
    path: String,
}

impl ReadingHistory {
    pub fn new(bookmarks: &Bookmarks) -> Self {
        // Extract unique books with their most recent access time
        let mut latest_access: HashMap<String, (DateTime<Local>, String)> = HashMap::new();

        for (path, bookmark) in bookmarks.iter() {
            let title = path
                .split('/')
                .last()
                .unwrap_or("Unknown")
                .trim_end_matches(".epub")
                .to_string();

            let local_time = Local.from_utc_datetime(&bookmark.last_read.naive_utc());

            latest_access
                .entry(path.clone())
                .and_modify(|e| {
                    if local_time > e.0 {
                        *e = (local_time, title.clone());
                    }
                })
                .or_insert((local_time, title));
        }

        // Convert to sorted list
        let mut items: Vec<HistoryItem> = latest_access
            .into_iter()
            .map(|(path, (date, title))| HistoryItem { date, title, path })
            .collect();

        // Sort by date descending (most recent first)
        items.sort_by(|a, b| b.date.cmp(&a.date));

        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }

        ReadingHistory { items, state }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        // Create centered popup area first
        let popup_area = centered_rect(60, 80, area);

        // Clear the background for the popup area
        f.render_widget(Clear, popup_area);

        // Create list items with formatted dates
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|item| {
                let date_str = item.date.format("%Y-%m-%d %H:%M").to_string();
                ListItem::new(Line::from(vec![
                    Span::styled(date_str, Style::default().fg(OCEANIC_NEXT.base_03)),
                    Span::raw(" : "),
                    Span::styled(&item.title, Style::default().fg(OCEANIC_NEXT.base_05)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Reading History ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(OCEANIC_NEXT.base_0c))
                    .style(Style::default().bg(OCEANIC_NEXT.base_00)), // Use theme background
            )
            .highlight_style(
                Style::default()
                    .bg(OCEANIC_NEXT.base_02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(list, popup_area, &mut self.state);
    }

    pub fn next(&mut self) {
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

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.state
            .selected()
            .and_then(|i| self.items.get(i))
            .map(|item| item.path.as_str())
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
