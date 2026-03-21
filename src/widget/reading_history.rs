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
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
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
}

#[derive(Clone)]
struct HistoryItem {
    date: DateTime<Local>,
    title: String,
    path: String,
    chapter: usize,
    total_chapters: usize,
    book_progress: Option<f32>,
    is_pdf: bool,
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
            let is_pdf = bookmark.pdf_page.is_some();
            let chapter = bookmark.pdf_page.or(bookmark.chapter_index).unwrap_or(0);
            let total_chapters = bookmark.total_chapters.unwrap_or(0);

            HistoryItem {
                date,
                title,
                path: key,
                chapter,
                total_chapters,
                book_progress: bookmark.book_progress,
                is_pdf,
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
            let is_pdf = bookmark.pdf_page.is_some();
            let chapter = bookmark.pdf_page.or(bookmark.chapter_index).unwrap_or(0);
            let total_chapters = bookmark.total_chapters.unwrap_or(0);
            let exists = Path::new(&resolved_path).exists();

            by_path.insert(
                resolved_path.clone(),
                (
                    local_time,
                    HistoryItem {
                        date: local_time,
                        title,
                        path: resolved_path,
                        chapter,
                        total_chapters,
                        book_progress: bookmark.book_progress,
                        is_pdf,
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

fn format_date(date: &DateTime<Local>, now: &DateTime<Local>) -> String {
    if date.year() == now.year() {
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
        format!(
            "{:>2}:{:02}{} {} {:>2}",
            h12,
            date.minute(),
            ampm,
            date.format("%b"),
            date.day()
        )
    } else {
        format!("{} {:>2}, {}", date.format("%b"), date.day(), date.year())
    }
}

fn format_chapter(item: &HistoryItem) -> String {
    if item.total_chapters > 0 {
        if item.is_pdf {
            format!("[p{}/{}]", item.chapter + 1, item.total_chapters)
        } else {
            format!("[{}/{}]", item.chapter + 1, item.total_chapters)
        }
    } else {
        String::new()
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

        let active_style = Style::default()
            .fg(palette.base_05)
            .bg(palette.base_02)
            .add_modifier(Modifier::BOLD);
        let inactive_style = Style::default().fg(palette.base_04);

        let (current_style, all_style) = if self.current_tab == HistoryTab::CurrentLibrary {
            (active_style, inactive_style)
        } else {
            (inactive_style, active_style)
        };

        let title_line = Line::from(vec![
            Span::styled(" Current Library ", current_style),
            Span::raw("  "),
            Span::styled(" All Libraries ", all_style),
            Span::raw("  "),
            Span::styled("[Tab] switch", Style::default().fg(palette.base_03)),
        ]);

        let block = Block::default()
            .title(title_line)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette.popup_border_color()))
            .style(Style::default().bg(palette.base_00));
        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        self.render_list(f, inner);
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect) {
        let palette = current_theme();
        let now = Local::now();

        let list_widget = {
            let items = self.active_items();

            let chapter_strs: Vec<String> = items.iter().map(|item| format_chapter(item)).collect();
            let max_chapter_width = chapter_strs.iter().map(|s| s.len()).max().unwrap_or(0);

            let list_items: Vec<ListItem> = items
                .iter()
                .zip(chapter_strs.iter())
                .map(|(item, chapter_str)| {
                    let date_str = format_date(&item.date, &now);
                    let progress_str = if let Some(progress) = item.book_progress {
                        format!("{:>3}%", (progress * 100.0).round() as u32)
                    } else {
                        "    ".to_string()
                    };
                    let padded_chapter =
                        format!("{:>width$}", chapter_str, width = max_chapter_width);

                    let title_color = if item.exists {
                        palette.base_05
                    } else {
                        palette.base_03
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(date_str, Style::default().fg(palette.base_03)),
                        Span::raw(" "),
                        Span::styled(progress_str, Style::default().fg(palette.base_0d)),
                        Span::raw(" "),
                        Span::styled(padded_chapter, Style::default().fg(palette.base_03)),
                        Span::raw(" : "),
                        Span::styled(item.title.clone(), Style::default().fg(title_color)),
                    ]))
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

        if let Some(popup_area) = self.last_popup_area {
            if x >= popup_area.x
                && x < popup_area.x + popup_area.width
                && y > popup_area.y
                && y < popup_area.y + popup_area.height - 1
            {
                let relative_y = y.saturating_sub(popup_area.y).saturating_sub(1);
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
