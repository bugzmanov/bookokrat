use crate::bookmarks::{Bookmark, Bookmarks};
use crate::inputs::KeySeq;
use crate::library;
use crate::main_app::VimNavMotions;
use crate::theme::current_theme;
use crate::widget::popup::Popup;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use log::{debug, warn};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};
use std::collections::HashMap;
use std::path::Path;

pub enum ReadingHistoryAction {
    OpenBook { path: String },
    OpenBookAbsolute { path: String },
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HistoryTab {
    CurrentLibrary,
    AllLibraries,
}

impl HistoryTab {
    fn toggle(self) -> Self {
        match self {
            HistoryTab::CurrentLibrary => HistoryTab::AllLibraries,
            HistoryTab::AllLibraries => HistoryTab::CurrentLibrary,
        }
    }
}

pub struct ReadingHistory {
    current_tab: HistoryTab,
    current_items: Vec<HistoryItem>,
    current_state: ListState,
    all_items: Option<Vec<HistoryItem>>,
    all_state: ListState,
    last_popup_area: Option<Rect>,
    last_list_area: Option<Rect>,
}

#[derive(Clone)]
struct HistoryItem {
    date: DateTime<Local>,
    title: String,
    path: String,
    book_progress: Option<f32>,
    exists: bool,
    absolute_path: Option<String>,
}

fn items_from_bookmarks(bookmarks: &Bookmarks) -> Vec<HistoryItem> {
    let mut latest_access: HashMap<String, (DateTime<Local>, &Bookmark, String)> = HashMap::new();

    for (path, bookmark) in bookmarks.iter() {
        let local_time = Local.from_utc_datetime(&bookmark.last_read.naive_utc());

        latest_access
            .entry(path.clone())
            .and_modify(|e| {
                if local_time > e.0 {
                    *e = (local_time, bookmark, path.clone());
                }
            })
            .or_insert((local_time, bookmark, path.clone()));
    }

    let mut items: Vec<HistoryItem> = latest_access
        .into_iter()
        .map(|(key, (date, bookmark, _))| {
            let title = bookmark
                .book_title
                .clone()
                .unwrap_or_else(|| title_from_path(&key));

            HistoryItem {
                date,
                title,
                path: key,
                book_progress: bookmark.book_progress,
                exists: true,
                absolute_path: bookmark.absolute_path.clone(),
            }
        })
        .collect();

    items.sort_by(|a, b| b.date.cmp(&a.date));
    items
}

fn collect_all_library_items() -> Vec<HistoryItem> {
    let libraries_dir = match library::libraries_data_dir() {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to resolve libraries dir: {e}");
            return Vec::new();
        }
    };
    if !libraries_dir.exists() {
        return Vec::new();
    }

    let mut by_path: HashMap<String, (DateTime<Local>, HistoryItem)> = HashMap::new();

    let dir_entries = match std::fs::read_dir(&libraries_dir) {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to read libraries dir: {e}");
            return Vec::new();
        }
    };

    for entry in dir_entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let bookmarks_file = entry.path().join("bookmarks.json");
        let bookmarks_path = bookmarks_file.to_string_lossy().to_string();
        let bookmarks = match Bookmarks::load_from_file(&bookmarks_path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        for (key, bookmark) in bookmarks.iter() {
            let resolved_path = bookmark
                .absolute_path
                .clone()
                .unwrap_or_else(|| key.clone());
            let local_time = Local.from_utc_datetime(&bookmark.last_read.naive_utc());

            if let Some((existing_time, _)) = by_path.get(&resolved_path) {
                if *existing_time >= local_time {
                    continue;
                }
            }

            let title = bookmark
                .book_title
                .clone()
                .unwrap_or_else(|| title_from_path(key));
            let exists = Path::new(&resolved_path).exists();

            by_path.insert(
                resolved_path.clone(),
                (
                    local_time,
                    HistoryItem {
                        date: local_time,
                        title,
                        path: resolved_path,
                        book_progress: bookmark.book_progress,
                        exists,
                        absolute_path: bookmark.absolute_path.clone(),
                    },
                ),
            );
        }
    }

    let mut items: Vec<HistoryItem> = by_path.into_values().map(|(_, item)| item).collect();
    items.sort_by(|a, b| b.date.cmp(&a.date));
    items
}

fn title_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn format_time(date: &DateTime<Local>) -> String {
    let hour = date.hour();
    let (h12, ampm) = if hour == 0 {
        (12, "am")
    } else if hour < 12 {
        (hour, "am")
    } else if hour == 12 {
        (12, "pm")
    } else {
        (hour - 12, "pm")
    };
    format!("{:>2}:{:02}{}", h12, date.minute(), ampm)
}

fn date_group_key(date: &DateTime<Local>) -> (i32, u32, u32) {
    (date.year(), date.month(), date.day())
}

fn format_date_group(date: &DateTime<Local>, now: &DateTime<Local>) -> String {
    if date.year() == now.year() && date.month() == now.month() && date.day() == now.day() {
        "Today".to_string()
    } else if date.year() == now.year() {
        format!("{} {:>2}", date.format("%b"), date.day())
    } else {
        format!("{} {:>2}, {}", date.format("%b"), date.day(), date.year())
    }
}

impl ReadingHistory {
    pub fn new(bookmarks: &Bookmarks) -> Self {
        let current_items = items_from_bookmarks(bookmarks);

        let mut current_state = ListState::default();
        if !current_items.is_empty() {
            current_state.select(Some(0));
        }

        ReadingHistory {
            current_tab: HistoryTab::CurrentLibrary,
            current_items,
            current_state,
            all_items: None,
            all_state: ListState::default(),
            last_popup_area: None,
            last_list_area: None,
        }
    }

    pub fn new_all_libraries(bookmarks: &Bookmarks) -> Self {
        let current_items = items_from_bookmarks(bookmarks);
        let all_items = collect_all_library_items();

        let mut current_state = ListState::default();
        if !current_items.is_empty() {
            current_state.select(Some(0));
        }
        let mut all_state = ListState::default();
        if !all_items.is_empty() {
            all_state.select(Some(0));
        }

        ReadingHistory {
            current_tab: HistoryTab::AllLibraries,
            current_items,
            current_state,
            all_items: Some(all_items),
            all_state,
            last_popup_area: None,
            last_list_area: None,
        }
    }

    fn active_items(&self) -> &[HistoryItem] {
        match self.current_tab {
            HistoryTab::CurrentLibrary => &self.current_items,
            HistoryTab::AllLibraries => self.all_items.as_deref().unwrap_or(&[]),
        }
    }

    fn active_state(&self) -> &ListState {
        match self.current_tab {
            HistoryTab::CurrentLibrary => &self.current_state,
            HistoryTab::AllLibraries => &self.all_state,
        }
    }

    fn active_state_mut(&mut self) -> &mut ListState {
        match self.current_tab {
            HistoryTab::CurrentLibrary => &mut self.current_state,
            HistoryTab::AllLibraries => &mut self.all_state,
        }
    }

    fn ensure_all_items_loaded(&mut self) {
        if self.all_items.is_none() {
            let items = collect_all_library_items();
            if !items.is_empty() && self.all_state.selected().is_none() {
                self.all_state.select(Some(0));
            }
            self.all_items = Some(items);
        }
    }

    fn switch_tab(&mut self) {
        self.current_tab = self.current_tab.toggle();
        if self.current_tab == HistoryTab::AllLibraries {
            self.ensure_all_items_loaded();
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let palette = current_theme();
        let popup_area = centered_rect(70, 80, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let hints = " Tab switch  j/k navigate  Enter select  Esc close ";

        let block = Block::default()
            .title(" Reading History ")
            .title_bottom(Line::from(hints).right_aligned())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette.popup_border_color()))
            .style(Style::default().bg(palette.base_00));
        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        let padded = Rect {
            x: inner.x + 2,
            y: inner.y + 1,
            width: inner.width.saturating_sub(4),
            height: inner.height.saturating_sub(2),
        };

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Tabs
                Constraint::Min(1),    // List
            ])
            .split(padded);

        self.render_tabs(f, main_chunks[0], &palette);
        self.render_list(f, main_chunks[1]);

        let item_count = self.active_items().len();
        let list_height = main_chunks[1].height as usize;
        if item_count > list_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(palette.base_04))
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));
            let mut scrollbar_state = ScrollbarState::new(item_count)
                .position(self.active_state().selected().unwrap_or(0));
            f.render_stateful_widget(
                scrollbar,
                popup_area.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }

    fn render_tabs(&self, f: &mut Frame, area: Rect, palette: &crate::theme::Base16Palette) {
        let tab_names = ["Current Library", "All Libraries"];

        let mut spans = vec![Span::raw(" ")];
        for (idx, name) in tab_names.iter().enumerate() {
            let is_selected = matches!(
                (idx, self.current_tab),
                (0, HistoryTab::CurrentLibrary) | (1, HistoryTab::AllLibraries)
            );
            let style = if is_selected {
                Style::default()
                    .fg(palette.base_06)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.base_03)
            };
            spans.push(Span::styled(*name, style));
            spans.push(Span::raw("   "));
        }

        f.render_widget(Paragraph::new(Line::from(spans)), area);

        let underline_y = area.y + 1;
        if underline_y < area.y + area.height {
            let underline_area = Rect {
                x: area.x,
                y: underline_y,
                width: area.width,
                height: 1,
            };

            let (underline_x, underline_len) = match self.current_tab {
                HistoryTab::CurrentLibrary => (1, tab_names[0].len()),
                HistoryTab::AllLibraries => (1 + tab_names[0].len() + 3, tab_names[1].len()),
            };

            let mut underline_spans = vec![Span::raw(" ".repeat(underline_x))];
            underline_spans.push(Span::styled(
                "─".repeat(underline_len),
                Style::default().fg(palette.base_0d),
            ));

            f.render_widget(Paragraph::new(Line::from(underline_spans)), underline_area);
        }
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect) {
        self.last_list_area = Some(area);
        let palette = current_theme();
        let now = Local::now();

        let list_widget = {
            let items = self.active_items();

            let date_labels: Vec<String> = items
                .iter()
                .map(|item| format_date_group(&item.date, &now))
                .collect();
            let max_date_width = date_labels.iter().map(|s| s.len()).max().unwrap_or(0);

            let mut prev_group: Option<(i32, u32, u32)> = None;
            let list_items: Vec<ListItem> = items
                .iter()
                .zip(date_labels.iter())
                .map(|(item, date_label)| {
                    let current_group = date_group_key(&item.date);
                    let date_str = if prev_group == Some(current_group) {
                        format!("{:width$}", "", width = max_date_width)
                    } else {
                        format!("{:<width$}", date_label, width = max_date_width)
                    };
                    prev_group = Some(current_group);

                    let time_str = format_time(&item.date);
                    let title_color = if item.exists {
                        palette.base_05
                    } else {
                        palette.base_03
                    };

                    let progress_str = match item.book_progress {
                        Some(p) => format!("{:>3}%", (p * 100.0).round() as u32),
                        None => "    ".to_string(),
                    };

                    let spans = vec![
                        Span::styled(date_str, Style::default().fg(palette.base_04)),
                        Span::raw(" "),
                        Span::styled(time_str, Style::default().fg(palette.base_03)),
                        Span::raw(" "),
                        Span::styled(progress_str, Style::default().fg(palette.base_0d)),
                        Span::raw(" "),
                        Span::styled(item.title.clone(), Style::default().fg(title_color)),
                    ];

                    ListItem::new(Line::from(spans))
                })
                .collect();

            List::new(list_items)
                .highlight_style(
                    Style::default()
                        .bg(palette.base_02)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("» ")
        };

        let state = self.active_state_mut();
        f.render_stateful_widget(list_widget, area, state);
    }

    pub fn next(&mut self) {
        let len = self.active_items().len();
        if len == 0 {
            return;
        }
        let state = self.active_state_mut();
        let i = match state.selected() {
            Some(i) => {
                if i >= len - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let len = self.active_items().len();
        if len == 0 {
            return;
        }
        let state = self.active_state_mut();
        let i = match state.selected() {
            Some(i) => {
                if i == 0 {
                    len - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        state.select(Some(i));
    }

    fn selected_item(&self) -> Option<&HistoryItem> {
        let items = self.active_items();
        self.active_state().selected().and_then(|i| items.get(i))
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.selected_item().map(|item| item.path.as_str())
    }

    fn selected_action(&self) -> Option<ReadingHistoryAction> {
        let item = self.selected_item()?;
        if !item.exists && self.current_tab == HistoryTab::AllLibraries {
            return None;
        }
        match self.current_tab {
            HistoryTab::CurrentLibrary => Some(ReadingHistoryAction::OpenBook {
                path: item.path.clone(),
            }),
            HistoryTab::AllLibraries => {
                let path = item
                    .absolute_path
                    .clone()
                    .unwrap_or_else(|| item.path.clone());
                Some(ReadingHistoryAction::OpenBookAbsolute { path })
            }
        }
    }

    pub fn handle_mouse_click(&mut self, x: u16, y: u16) -> bool {
        debug!("ReadingHistory: Mouse click at ({x}, {y})");

        if let Some(list_area) = self.last_list_area {
            if x >= list_area.x
                && x < list_area.x + list_area.width
                && y >= list_area.y
                && y < list_area.y + list_area.height
            {
                let relative_y = y.saturating_sub(list_area.y);
                let offset = self.active_state().offset();
                let new_index = offset + relative_y as usize;

                if new_index < self.active_items().len() {
                    self.active_state_mut().select(Some(new_index));
                    debug!("ReadingHistory: Selected item at index {new_index}");
                    return true;
                }
            }
        }
        false
    }
}

impl Popup for ReadingHistory {
    fn get_last_popup_area(&self) -> Option<Rect> {
        self.last_popup_area
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
    fn handle_h(&mut self) {}

    fn handle_j(&mut self) {
        self.next();
    }

    fn handle_k(&mut self) {
        self.previous();
    }

    fn handle_l(&mut self) {}

    fn handle_ctrl_d(&mut self) {
        let len = self.active_items().len();
        for _ in 0..10 {
            let current = self.active_state().selected().unwrap_or(0);
            if current < len.saturating_sub(1) {
                self.next();
            } else {
                break;
            }
        }
    }

    fn handle_ctrl_u(&mut self) {
        for _ in 0..10 {
            let current = self.active_state().selected().unwrap_or(0);
            if current > 0 {
                self.previous();
            } else {
                break;
            }
        }
    }

    fn handle_ctrl_f(&mut self) {
        let len = self.active_items().len();
        for _ in 0..20 {
            let current = self.active_state().selected().unwrap_or(0);
            if current < len.saturating_sub(1) {
                self.next();
            } else {
                break;
            }
        }
    }

    fn handle_ctrl_b(&mut self) {
        for _ in 0..20 {
            let current = self.active_state().selected().unwrap_or(0);
            if current > 0 {
                self.previous();
            } else {
                break;
            }
        }
    }

    fn handle_gg(&mut self) {
        if !self.active_items().is_empty() {
            self.active_state_mut().select(Some(0));
        }
    }

    fn handle_upper_g(&mut self) {
        let len = self.active_items().len();
        if len > 0 {
            self.active_state_mut().select(Some(len - 1));
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
            KeyCode::Tab | KeyCode::BackTab => {
                self.switch_tab();
                None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.handle_j();
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.handle_k();
                None
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.handle_h();
                None
            }
            KeyCode::Char('l') | KeyCode::Right => {
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
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_f();
                None
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_b();
                None
            }
            KeyCode::PageDown => {
                self.handle_ctrl_f();
                None
            }
            KeyCode::PageUp => {
                self.handle_ctrl_b();
                None
            }
            KeyCode::Esc => Some(ReadingHistoryAction::Close),
            KeyCode::Enter => self.selected_action(),
            _ => None,
        }
    }
}
