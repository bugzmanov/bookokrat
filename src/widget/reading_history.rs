use crate::bookmarks::Bookmarks;
use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::theme::OCEANIC_NEXT;
use chrono::{DateTime, Local, TimeZone};
use log::debug;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};
use std::collections::HashMap;

pub enum ReadingHistoryAction {
    OpenBook { path: String },
    Close,
}

pub struct ReadingHistory {
    items: Vec<HistoryItem>,
    state: ListState,
    last_popup_area: Option<Rect>,
}

#[derive(Clone)]
struct HistoryItem {
    date: DateTime<Local>,
    title: String,
    path: String,
    chapter: usize,
    total_chapters: usize,
}

impl ReadingHistory {
    pub fn new(bookmarks: &Bookmarks) -> Self {
        // Extract unique books with their most recent access time
        let mut latest_access: HashMap<String, (DateTime<Local>, String, usize, usize)> =
            HashMap::new();

        for (path, bookmark_entry) in bookmarks.iter() {
            let title = path
                .split('/')
                .next_back()
                .unwrap_or("Unknown")
                .trim_end_matches(".epub")
                .to_string();

            let local_time = Local.from_utc_datetime(&bookmark_entry.last_read.naive_utc());

            // Get chapter info from bookmark
            let chapter = bookmark_entry.chapter_index.unwrap_or(0);
            let total_chapters = bookmark_entry.total_chapters.unwrap_or(0);

            latest_access
                .entry(path.clone())
                .and_modify(|e| {
                    if local_time > e.0 {
                        *e = (local_time, title.clone(), chapter, total_chapters);
                    }
                })
                .or_insert((local_time, title, chapter, total_chapters));
        }

        // Convert to sorted list
        let mut items: Vec<HistoryItem> = latest_access
            .into_iter()
            .map(
                |(path, (date, title, chapter, total_chapters))| HistoryItem {
                    date,
                    title,
                    path,
                    chapter,
                    total_chapters,
                },
            )
            .collect();

        items.sort_by(|a, b| b.date.cmp(&a.date));

        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }

        ReadingHistory {
            items,
            state,
            last_popup_area: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        // Create centered popup area first
        let popup_area = centered_rect(60, 80, area);
        self.last_popup_area = Some(popup_area);

        // Clear the background for the popup area
        f.render_widget(Clear, popup_area);

        // Create list items with formatted dates
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|item| {
                let date_str = item.date.format("%Y-%m-%d").to_string();
                let progress_str = if item.total_chapters > 0 {
                    format!(" [ {} / {} ]", item.chapter + 1, item.total_chapters)
                } else {
                    String::new()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(date_str, Style::default().fg(OCEANIC_NEXT.base_03)),
                    Span::raw(" : "),
                    Span::styled(&item.title, Style::default().fg(OCEANIC_NEXT.base_05)),
                    Span::styled(progress_str, Style::default().fg(OCEANIC_NEXT.base_03)),
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
            .highlight_symbol("» ");

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

    /// Handle mouse click at the given position
    /// Returns true if an item was clicked (for double-click detection)
    pub fn handle_mouse_click(&mut self, x: u16, y: u16) -> bool {
        debug!("ReadingHistory: Mouse click at ({x}, {y})");

        if let Some(popup_area) = self.last_popup_area {
            debug!(
                "ReadingHistory: Popup area: x={}, y={}, w={}, h={}",
                popup_area.x, popup_area.y, popup_area.width, popup_area.height
            );

            // Check if click is within the popup area
            if x >= popup_area.x
                && x < popup_area.x + popup_area.width
                && y > popup_area.y
                && y < popup_area.y + popup_area.height - 1
            {
                // Calculate which item was clicked
                // Account for the border (1 line at top)
                let relative_y = y.saturating_sub(popup_area.y).saturating_sub(1);

                // Get the current scroll offset from the list state
                let offset = self.state.offset();

                // Calculate the actual index in the list
                let new_index = offset + relative_y as usize;

                debug!(
                    "ReadingHistory: relative_y={}, offset={}, new_index={}, items_len={}",
                    relative_y,
                    offset,
                    new_index,
                    self.items.len()
                );

                if new_index < self.items.len() {
                    self.state.select(Some(new_index));
                    debug!("ReadingHistory: Selected item at index {new_index}");
                    return true;
                }
            } else {
                debug!("ReadingHistory: Click outside popup area");
            }
        } else {
            debug!("ReadingHistory: No popup area set");
        }
        false
    }

    /// Check if the given coordinates are outside the popup area
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

impl VimNavMotions for ReadingHistory {
    fn handle_h(&mut self) {
        // Left movement - could be used to close history or go back
        // For now, we'll leave it empty as closing is handled by Esc/H
    }

    fn handle_j(&mut self) {
        // Down movement - move to next item
        self.next();
    }

    fn handle_k(&mut self) {
        // Up movement - move to previous item
        self.previous();
    }

    fn handle_l(&mut self) {
        // Right movement - could be used to select/enter
        // For now, Enter key handles selection
    }

    fn handle_ctrl_d(&mut self) {
        // Page down - move selection down by half page
        let page_size = 10; // Approximate half-page
        for _ in 0..page_size {
            let current = self.state.selected().unwrap_or(0);
            if current < self.items.len() - 1 {
                self.next();
            } else {
                break;
            }
        }
    }

    fn handle_ctrl_u(&mut self) {
        // Page up - move selection up by half page
        let page_size = 10; // Approximate half-page
        for _ in 0..page_size {
            let current = self.state.selected().unwrap_or(0);
            if current > 0 {
                self.previous();
            } else {
                break;
            }
        }
    }

    fn handle_gg(&mut self) {
        // Go to top - select first item
        if !self.items.is_empty() {
            self.state.select(Some(0));
        }
    }

    fn handle_upper_g(&mut self) {
        // Go to bottom - select last item
        if !self.items.is_empty() {
            let last_index = self.items.len() - 1;
            self.state.select(Some(last_index));
        }
    }
}

impl ReadingHistory {
    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<ReadingHistoryAction> {
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
            KeyCode::Char('h') => {
                self.handle_h();
                None
            }
            KeyCode::Char('l') => {
                self.handle_l();
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
            KeyCode::Esc => Some(ReadingHistoryAction::Close),
            KeyCode::Enter => self
                .selected_path()
                .map(|path| ReadingHistoryAction::OpenBook {
                    path: path.to_string(),
                }),
            _ => None,
        }
    }
}
