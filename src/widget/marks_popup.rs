use crate::bookmarks::Bookmarks;
use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::marks::{GlobalMarks, MarkLocation};
use crate::theme::current_theme;
use crate::widget::popup::Popup;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use std::path::Path;

#[derive(Debug, Clone)]
pub enum MarksPopupAction {
    Close,
    Jump(MarkLocation),
    Delete(MarkScopeKey),
}

#[derive(Debug, Clone)]
pub enum MarkScopeKey {
    Local { book_path: String, ch: char },
    Global { ch: char },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarksTab {
    CurrentBook,
    Global,
}

impl MarksTab {
    fn toggle(self) -> Self {
        match self {
            MarksTab::CurrentBook => MarksTab::Global,
            MarksTab::Global => MarksTab::CurrentBook,
        }
    }
}

#[derive(Debug, Clone)]
struct MarkRow {
    ch: char,
    is_global: bool,
    location: MarkLocation,
    /// Path the local mark belongs to (for delete routing). For global marks
    /// this is the same as `location.path()`, kept here for symmetry.
    book_path: String,
    book_label: String,
    location_label: String,
    snippet: Option<String>,
}

pub struct MarksPopup {
    current_tab: MarksTab,
    current_book_rows: Vec<MarkRow>,
    global_rows: Vec<MarkRow>,
    current_state: ListState,
    global_state: ListState,
    visible: bool,
    last_popup_area: Option<Rect>,
}

impl Default for MarksPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl MarksPopup {
    pub fn new() -> Self {
        Self {
            current_tab: MarksTab::CurrentBook,
            current_book_rows: Vec::new(),
            global_rows: Vec::new(),
            current_state: ListState::default(),
            global_state: ListState::default(),
            visible: false,
            last_popup_area: None,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Rebuild the row lists. Call before `show()`. `current_book_path` is the
    /// path of the open document and is used both to scope local marks and to
    /// label them.
    pub fn rebuild(
        &mut self,
        current_book_path: Option<&str>,
        local_bookmarks: &Bookmarks,
        global_marks: &GlobalMarks,
    ) {
        let mut current_rows: Vec<MarkRow> = Vec::new();
        if let Some(path) = current_book_path {
            if let Some(bookmark) = local_bookmarks.get_bookmark(path) {
                if let Some(map) = bookmark.marks.as_ref() {
                    let book_label = bookmark
                        .book_title
                        .clone()
                        .unwrap_or_else(|| short_book_name(path));
                    let mut keys: Vec<char> = map.keys().copied().collect();
                    keys.sort();
                    for ch in keys {
                        if let Some(loc) = map.get(&ch) {
                            current_rows.push(MarkRow {
                                ch,
                                is_global: false,
                                location: loc.clone().retarget_path(path.to_string()),
                                book_path: path.to_string(),
                                book_label: book_label.clone(),
                                location_label: location_label(loc),
                                snippet: loc.snippet().map(|s| s.to_string()),
                            });
                        }
                    }
                }
            }
        }

        let mut global_rows: Vec<MarkRow> = Vec::new();
        for ch in ('A'..='Z').collect::<Vec<_>>() {
            if let Some(loc) = global_marks.get(ch) {
                let book_label = local_bookmarks
                    .get_bookmark(loc.path())
                    .and_then(|b| b.book_title.clone())
                    .unwrap_or_else(|| short_book_name(loc.path()));
                global_rows.push(MarkRow {
                    ch,
                    is_global: true,
                    location: loc.clone(),
                    book_path: loc.path().to_string(),
                    book_label,
                    location_label: location_label(loc),
                    snippet: loc.snippet().map(|s| s.to_string()),
                });
            }
        }

        self.current_book_rows = current_rows;
        self.global_rows = global_rows;
        clamp_selection(&mut self.current_state, self.current_book_rows.len());
        clamp_selection(&mut self.global_state, self.global_rows.len());
    }

    fn active_rows(&self) -> &[MarkRow] {
        match self.current_tab {
            MarksTab::CurrentBook => &self.current_book_rows,
            MarksTab::Global => &self.global_rows,
        }
    }

    fn active_state(&self) -> &ListState {
        match self.current_tab {
            MarksTab::CurrentBook => &self.current_state,
            MarksTab::Global => &self.global_state,
        }
    }

    fn active_state_mut(&mut self) -> &mut ListState {
        match self.current_tab {
            MarksTab::CurrentBook => &mut self.current_state,
            MarksTab::Global => &mut self.global_state,
        }
    }

    fn switch_tab(&mut self) {
        self.current_tab = self.current_tab.toggle();
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let palette = current_theme();
        let popup_width = area.width.saturating_sub(10).min(100);
        let popup_height = area.height.saturating_sub(4).min(24);
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };
        self.last_popup_area = Some(popup_area);

        frame.render_widget(Clear, popup_area);

        let bottom_hint =
            " j/k: Navigate | Tab: Switch | Enter: Jump | dd: Delete | Esc/'/`: Close ";
        let block = Block::default()
            .title(" Marks ")
            .title_bottom(Line::from(bottom_hint).right_aligned())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette.popup_border_color()))
            .style(Style::default().bg(palette.base_00));
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let padded = Rect {
            x: inner.x + 2,
            y: inner.y + 1,
            width: inner.width.saturating_sub(4),
            height: inner.height.saturating_sub(1),
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(padded);

        self.render_tabs(frame, chunks[0], &palette);
        self.render_list(frame, chunks[1]);
    }

    fn render_tabs(&self, f: &mut Frame, area: Rect, palette: &crate::theme::Base16Palette) {
        let tab_names = ["Current Book", "Global"];
        let counts = [self.current_book_rows.len(), self.global_rows.len()];

        let mut spans = vec![Span::raw(" ")];
        for (idx, (name, count)) in tab_names.iter().zip(counts.iter()).enumerate() {
            let is_selected = matches!(
                (idx, self.current_tab),
                (0, MarksTab::CurrentBook) | (1, MarksTab::Global)
            );
            let style = if is_selected {
                Style::default()
                    .fg(palette.base_06)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.base_03)
            };
            spans.push(Span::styled(format!("{name} ({count})"), style));
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
            let label_lens: Vec<usize> = tab_names
                .iter()
                .zip(counts.iter())
                .map(|(n, c)| format!("{n} ({c})").chars().count())
                .collect();
            let (underline_x, underline_len) = match self.current_tab {
                MarksTab::CurrentBook => (1, label_lens[0]),
                MarksTab::Global => (1 + label_lens[0] + 3, label_lens[1]),
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
        let palette = current_theme();
        let inner_width = area.width.saturating_sub(3) as usize;
        let show_book_label = self.current_tab == MarksTab::Global;

        let items: Vec<ListItem> = if self.active_rows().is_empty() {
            let msg = match self.current_tab {
                MarksTab::CurrentBook => "No local marks. Set with m<a-z> while reading.",
                MarksTab::Global => "No global marks. Set with m<A-Z> from any book.",
            };
            vec![ListItem::new(vec![Line::from(vec![Span::styled(
                msg,
                Style::default().fg(palette.base_03),
            )])])]
        } else {
            self.active_rows()
                .iter()
                .map(|row| build_row_lines(row, inner_width, show_book_label, &palette))
                .collect()
        };

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(palette.base_02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("» ");

        let mut state = self.active_state().clone();
        f.render_stateful_widget(list, area, &mut state);
        // ListState may have updated `offset` for scrolling; persist it.
        *self.active_state_mut() = state;
    }

    fn selected(&self) -> Option<&MarkRow> {
        let rows = self.active_rows();
        self.active_state().selected().and_then(|i| rows.get(i))
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<MarksPopupAction> {
        use crate::keybindings::action::Action;
        use crate::keybindings::context::KeyContext;
        use crate::keybindings::keymap::LookupResult;
        use crate::keybindings::notation::key_event_to_input;

        let input = key_event_to_input(&key);
        let km = crate::keybindings::keymap();

        let mut prospective: Vec<_> = key_seq.keys().iter().map(key_event_to_input).collect();
        prospective.push(input);

        match km.lookup(KeyContext::PopupMarks, &prospective) {
            LookupResult::Found(action) => {
                key_seq.clear();
                match action {
                    Action::MoveDown => {
                        self.handle_j();
                        None
                    }
                    Action::MoveUp => {
                        self.handle_k();
                        None
                    }
                    Action::GoTop => {
                        self.handle_gg();
                        None
                    }
                    Action::GoBottom => {
                        self.handle_upper_g();
                        None
                    }
                    Action::ScrollHalfDown => {
                        self.handle_ctrl_d();
                        None
                    }
                    Action::ScrollHalfUp => {
                        self.handle_ctrl_u();
                        None
                    }
                    Action::NextTab | Action::PrevTab => {
                        self.switch_tab();
                        None
                    }
                    Action::Cancel => Some(MarksPopupAction::Close),
                    Action::Select => self
                        .selected()
                        .map(|r| MarksPopupAction::Jump(r.location.clone())),
                    Action::DeleteEntry => self.selected().map(|r| {
                        if r.is_global {
                            MarksPopupAction::Delete(MarkScopeKey::Global { ch: r.ch })
                        } else {
                            MarksPopupAction::Delete(MarkScopeKey::Local {
                                book_path: r.book_path.clone(),
                                ch: r.ch,
                            })
                        }
                    }),
                    _ => None,
                }
            }
            LookupResult::Prefix => {
                key_seq.push(key);
                None
            }
            LookupResult::NoMatch => {
                if !key_seq.is_empty() {
                    key_seq.clear();
                    return self.handle_key(key, key_seq);
                }
                None
            }
        }
    }
}

impl Popup for MarksPopup {
    fn get_last_popup_area(&self) -> Option<Rect> {
        self.last_popup_area
    }
}

impl VimNavMotions for MarksPopup {
    fn handle_h(&mut self) {}
    fn handle_l(&mut self) {}
    fn handle_j(&mut self) {
        let len = self.active_rows().len();
        if len == 0 {
            return;
        }
        let next = match self.active_state().selected() {
            Some(i) if i + 1 < len => i + 1,
            Some(i) => i,
            None => 0,
        };
        self.active_state_mut().select(Some(next));
    }
    fn handle_k(&mut self) {
        if self.active_rows().is_empty() {
            return;
        }
        let next = match self.active_state().selected() {
            Some(0) | None => 0,
            Some(i) => i - 1,
        };
        self.active_state_mut().select(Some(next));
    }
    fn handle_gg(&mut self) {
        if !self.active_rows().is_empty() {
            self.active_state_mut().select(Some(0));
        }
    }
    fn handle_upper_g(&mut self) {
        let last = self.active_rows().len().saturating_sub(1);
        if !self.active_rows().is_empty() {
            self.active_state_mut().select(Some(last));
        }
    }
    fn handle_ctrl_d(&mut self) {
        let len = self.active_rows().len();
        if len == 0 {
            return;
        }
        let step = (len / 2).max(1);
        let next = self
            .active_state()
            .selected()
            .map(|i| (i + step).min(len - 1))
            .unwrap_or(0);
        self.active_state_mut().select(Some(next));
    }
    fn handle_ctrl_u(&mut self) {
        let len = self.active_rows().len();
        if len == 0 {
            return;
        }
        let step = (len / 2).max(1);
        let next = self
            .active_state()
            .selected()
            .map(|i| i.saturating_sub(step))
            .unwrap_or(0);
        self.active_state_mut().select(Some(next));
    }
    fn handle_ctrl_f(&mut self) {
        self.handle_upper_g();
    }
    fn handle_ctrl_b(&mut self) {
        self.handle_gg();
    }
}

fn clamp_selection(state: &mut ListState, len: usize) {
    if len == 0 {
        state.select(None);
    } else if state.selected().is_none_or(|i| i >= len) {
        state.select(Some(0));
    }
}

fn short_book_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn location_label(loc: &MarkLocation) -> String {
    match loc {
        MarkLocation::Epub {
            chapter,
            chapter_title,
            ..
        } => match chapter_title {
            Some(title) if !title.trim().is_empty() => title.trim().to_string(),
            _ => format!("ch {}", chapter + 1),
        },
        MarkLocation::Pdf { page, .. } => format!("p. {}", page + 1),
    }
}

/// Build a 2-line entry for a mark: a header line with key/location/(book)
/// and a snippet line beneath it.
fn build_row_lines(
    row: &MarkRow,
    inner_width: usize,
    show_book_label: bool,
    palette: &crate::theme::Base16Palette,
) -> ListItem<'static> {
    let mut header = vec![
        Span::styled(
            format!("{}  ", row.ch),
            Style::default()
                .fg(palette.base_0a)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}]", row.location_label),
            Style::default().fg(palette.base_03),
        ),
    ];
    if show_book_label {
        let max_book = inner_width
            .saturating_sub(row.location_label.chars().count() + 8)
            .max(8);
        let book_chars: Vec<char> = row.book_label.chars().collect();
        let book_text = if book_chars.len() > max_book {
            let truncated: String = book_chars[..max_book.saturating_sub(1)].iter().collect();
            format!("  {truncated}…")
        } else {
            format!("  {}", row.book_label)
        };
        header.push(Span::styled(
            book_text,
            Style::default().fg(palette.base_0d),
        ));
    }

    let snippet_text = row
        .snippet
        .clone()
        .unwrap_or_else(|| "(no snippet captured)".to_string());
    let snippet_chars: Vec<char> = snippet_text.chars().collect();
    let snippet_max = inner_width.saturating_sub(4).max(8);
    let snippet_truncated = if snippet_chars.len() > snippet_max {
        let truncated: String = snippet_chars[..snippet_max.saturating_sub(1)]
            .iter()
            .collect();
        format!("    {truncated}…")
    } else {
        format!("    {snippet_text}")
    };

    let snippet_style = if row.snippet.is_some() {
        Style::default().fg(palette.base_05)
    } else {
        Style::default()
            .fg(palette.base_03)
            .add_modifier(Modifier::ITALIC)
    };

    ListItem::new(vec![
        Line::from(header),
        Line::from(vec![Span::styled(snippet_truncated, snippet_style)]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epub_loc_with_snip(
        path: &str,
        ch: usize,
        node: usize,
        snippet: Option<&str>,
    ) -> MarkLocation {
        MarkLocation::Epub {
            path: path.to_string(),
            chapter: ch,
            node,
            node_offset: None,
            snippet: snippet.map(|s| s.to_string()),
            chapter_title: None,
        }
    }

    fn epub_loc(path: &str, ch: usize, node: usize) -> MarkLocation {
        epub_loc_with_snip(path, ch, node, None)
    }

    fn pdf_loc(path: &str, page: usize, scroll: u32) -> MarkLocation {
        MarkLocation::Pdf {
            path: path.to_string(),
            page,
            scroll_offset: scroll,
            snippet: None,
            line_idx: None,
        }
    }

    fn seed_book(local: &mut Bookmarks, path: &str) {
        local.update_bookmark(
            path,
            "ch1".into(),
            Some(0),
            Some(0),
            Some(10),
            None,
            None,
            None,
            Some(0.1),
            None,
        );
    }

    #[test]
    fn rebuild_separates_local_and_global() {
        let mut local = Bookmarks::ephemeral();
        seed_book(&mut local, "./book.epub");
        local.set_local_mark("./book.epub", 'a', epub_loc("./book.epub", 0, 0));
        local.set_local_mark("./book.epub", 'b', epub_loc("./book.epub", 1, 5));
        let dir = tempfile::TempDir::new().unwrap();
        let mut global = GlobalMarks::load(dir.path().join("g.json"));
        global.set('A', pdf_loc("/abs/other.pdf", 5, 10));

        let mut popup = MarksPopup::new();
        popup.rebuild(Some("./book.epub"), &local, &global);

        assert_eq!(popup.current_book_rows.len(), 2);
        assert_eq!(popup.current_book_rows[0].ch, 'a');
        assert_eq!(popup.current_book_rows[1].ch, 'b');
        assert_eq!(popup.global_rows.len(), 1);
        assert_eq!(popup.global_rows[0].ch, 'A');
        assert!(popup.global_rows[0].is_global);
    }

    #[test]
    fn switch_tab_toggles_active_view() {
        let mut local = Bookmarks::ephemeral();
        seed_book(&mut local, "./book.epub");
        local.set_local_mark("./book.epub", 'a', epub_loc("./book.epub", 0, 0));
        let dir = tempfile::TempDir::new().unwrap();
        let mut global = GlobalMarks::load(dir.path().join("g.json"));
        global.set('A', epub_loc("/abs/x.epub", 0, 0));

        let mut popup = MarksPopup::new();
        popup.rebuild(Some("./book.epub"), &local, &global);
        assert_eq!(popup.current_tab, MarksTab::CurrentBook);
        assert_eq!(popup.active_rows().len(), 1);
        assert_eq!(popup.active_rows()[0].ch, 'a');

        popup.switch_tab();
        assert_eq!(popup.current_tab, MarksTab::Global);
        assert_eq!(popup.active_rows().len(), 1);
        assert_eq!(popup.active_rows()[0].ch, 'A');
    }

    #[test]
    fn rebuild_with_no_open_book_leaves_current_empty() {
        let local = Bookmarks::ephemeral();
        let dir = tempfile::TempDir::new().unwrap();
        let mut global = GlobalMarks::load(dir.path().join("g.json"));
        global.set('A', epub_loc("/abs/x.epub", 0, 0));

        let mut popup = MarksPopup::new();
        popup.rebuild(None, &local, &global);
        assert!(popup.current_book_rows.is_empty());
        assert_eq!(popup.global_rows.len(), 1);
    }

    #[test]
    fn empty_state_has_no_selection() {
        let local = Bookmarks::ephemeral();
        let dir = tempfile::TempDir::new().unwrap();
        let global = GlobalMarks::load(dir.path().join("g.json"));
        let mut popup = MarksPopup::new();
        popup.rebuild(None, &local, &global);
        assert!(popup.current_state.selected().is_none());
        assert!(popup.global_state.selected().is_none());
    }

    #[test]
    fn snippet_propagates_into_row() {
        let mut local = Bookmarks::ephemeral();
        seed_book(&mut local, "./book.epub");
        local.set_local_mark(
            "./book.epub",
            'a',
            epub_loc_with_snip("./book.epub", 0, 0, Some("Once upon a time...")),
        );
        let dir = tempfile::TempDir::new().unwrap();
        let global = GlobalMarks::load(dir.path().join("g.json"));

        let mut popup = MarksPopup::new();
        popup.rebuild(Some("./book.epub"), &local, &global);
        assert_eq!(
            popup.current_book_rows[0].snippet.as_deref(),
            Some("Once upon a time...")
        );
    }
}
