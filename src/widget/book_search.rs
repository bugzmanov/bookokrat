use crate::main_app::VimNavMotions;
use crate::search_engine::{BookSearchResult, SearchEngine};
use crate::theme::Base16Palette;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use log::debug;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::time::{Duration, Instant};

pub enum BookSearchAction {
    JumpToChapter {
        chapter_index: usize,
        line_number: usize,
    },
    Close,
}

enum FocusMode {
    Input,
    Results,
}

pub struct BookSearch {
    active: bool,
    search_input: String,
    cursor_position: usize,

    results: Vec<BookSearchResult>,
    selected_result: usize,
    scroll_offset: usize,
    visible_results: usize,

    search_engine: SearchEngine,
    last_search_query: String,

    last_input_time: Instant,
    pending_search: Option<String>,

    focus_mode: FocusMode,
    cached_results: Option<Vec<BookSearchResult>>,
}

impl BookSearch {
    pub fn new(search_engine: SearchEngine) -> Self {
        Self {
            active: false,
            search_input: String::new(),
            cursor_position: 0,
            results: Vec::new(),
            selected_result: 0,
            scroll_offset: 0,
            visible_results: 10,
            search_engine,
            last_search_query: String::new(),
            last_input_time: Instant::now(),
            pending_search: None,
            focus_mode: FocusMode::Input,
            cached_results: None,
        }
    }

    pub fn open(&mut self, clear_input: bool) {
        self.active = true;
        if clear_input {
            self.search_input.clear();
            self.cursor_position = 0;
            self.results.clear();
            self.selected_result = 0;
            self.scroll_offset = 0;
            self.last_search_query.clear();
        } else if let Some(cached) = &self.cached_results {
            self.results = cached.clone();
        }
        self.focus_mode = FocusMode::Input;
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn update(&mut self) -> Option<BookSearchAction> {
        if let Some(ref query) = self.pending_search {
            if self.last_input_time.elapsed() > Duration::from_millis(200) {
                self.execute_search(query.clone());
                self.pending_search = None;
            }
        }
        None
    }

    fn execute_search(&mut self, query: String) {
        if query == self.last_search_query {
            return;
        }
        self.results = self.search_engine.search_fuzzy(&query);
        self.cached_results = Some(self.results.clone());
        self.last_search_query = query;
        self.selected_result = 0;
        self.scroll_offset = 0;
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<BookSearchAction> {
        match self.focus_mode {
            FocusMode::Input => {
                let action = self.handle_input_key(key);
                self.update();
                action
            }
            FocusMode::Results => self.handle_results_key(key),
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> Option<BookSearchAction> {
        match key.code {
            KeyCode::Esc => {
                self.active = false;
                return Some(BookSearchAction::Close);
            }
            KeyCode::Enter => {
                if self.search_input.is_empty() {
                    return None;
                }
                // Execute search and switch to results
                self.execute_search(self.search_input.clone());
                if !self.results.is_empty() {
                    self.focus_mode = FocusMode::Results;
                }
            }
            KeyCode::Down => {
                if !self.results.is_empty() {
                    self.focus_mode = FocusMode::Results;
                    self.move_selection_down();
                }
            }
            KeyCode::Up => {
                if !self.results.is_empty() {
                    self.focus_mode = FocusMode::Results;
                    self.move_selection_up();
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_input.clear();
                self.cursor_position = 0;
                self.schedule_search();
            }
            KeyCode::Char(c) => {
                // In input mode, all characters including 'j' and 'k' should be typed
                self.search_input.insert(self.cursor_position, c);
                self.cursor_position += 1;
                self.schedule_search();
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.search_input.remove(self.cursor_position);
                    self.schedule_search();
                }
            }
            KeyCode::Left => {
                self.cursor_position = self.cursor_position.saturating_sub(1);
            }
            KeyCode::Right => {
                self.cursor_position = (self.cursor_position + 1).min(self.search_input.len());
            }
            _ => {}
        }
        None
    }

    fn handle_results_key(&mut self, key: KeyEvent) -> Option<BookSearchAction> {
        match key.code {
            KeyCode::Esc => {
                self.active = false;
                return Some(BookSearchAction::Close);
            }
            KeyCode::Enter => {
                if !self.results.is_empty() {
                    let result = &self.results[self.selected_result];
                    self.active = false;
                    return Some(BookSearchAction::JumpToChapter {
                        chapter_index: result.chapter_index,
                        line_number: result.line_number,
                    });
                }
            }
            KeyCode::Char(' ') if key.modifiers.is_empty() => {
                // Space+f behavior - go back to input mode
                self.focus_mode = FocusMode::Input;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection_up();
            }
            KeyCode::Char('g') => {
                self.selected_result = 0;
                self.scroll_offset = 0;
            }
            KeyCode::Char('G') => {
                if !self.results.is_empty() {
                    self.selected_result = self.results.len() - 1;
                    self.update_scroll();
                }
            }
            _ => {}
        }
        None
    }

    fn schedule_search(&mut self) {
        self.last_input_time = Instant::now();
        self.pending_search = Some(self.search_input.clone());
    }

    fn move_selection_down(&mut self) {
        if self.selected_result < self.results.len().saturating_sub(1) {
            self.selected_result += 1;
            self.update_scroll();
        }
    }

    fn move_selection_up(&mut self) {
        if self.selected_result > 0 {
            self.selected_result -= 1;
            self.update_scroll();
        }
    }

    /// Scroll the view down while keeping cursor at same screen position if possible
    pub fn scroll_down(&mut self, area_height: u16) {
        if self.results.is_empty() {
            return;
        }

        // Calculate visible height (accounting for borders and search input area)
        let visible_height = area_height.saturating_sub(5) as usize; // Account for borders and input
        let total_items = self.results.len();

        // Calculate cursor position relative to viewport
        let cursor_viewport_pos = self.selected_result.saturating_sub(self.scroll_offset);

        // Check if we can scroll down
        if self.scroll_offset + visible_height < total_items {
            // Scroll viewport down by 1
            self.scroll_offset += 1;

            // Try to maintain cursor at same viewport position
            let new_selected = (self.scroll_offset + cursor_viewport_pos).min(total_items - 1);
            self.selected_result = new_selected;
        } else if self.selected_result < total_items - 1 {
            self.selected_result += 1;
        }
    }

    /// Scroll the view up while keeping cursor at same screen position if possible
    pub fn scroll_up(&mut self, area_height: u16) {
        if self.results.is_empty() {
            return;
        }

        let visible_height = area_height.saturating_sub(5) as usize;

        let cursor_viewport_pos = self.selected_result.saturating_sub(self.scroll_offset);

        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;

            let new_selected = self.scroll_offset + cursor_viewport_pos;
            self.selected_result = new_selected;
        } else if self.selected_result > 0 {
            self.selected_result -= 1;
        }

        if visible_height > 0 {
            let max_visible_index = self
                .scroll_offset
                .saturating_add(visible_height.saturating_sub(1));
            if self.selected_result > max_visible_index {
                self.selected_result = max_visible_index.min(self.results.len().saturating_sub(1));
            }
        }
    }

    fn update_scroll(&mut self) {
        // Scroll to keep selected result visible
        // If selected is before current scroll, scroll up to it
        if self.selected_result < self.scroll_offset {
            self.scroll_offset = self.selected_result;
        }
        // If selected is too far down, we need to scroll down
        // Since we can't know the exact visible count without the render area,
        // we use a conservative approach: ensure at least 2 results are visible
        else if self.selected_result > self.scroll_offset + 2 {
            // Scroll so selected is the second visible item (leaves room to see context)
            self.scroll_offset = self.selected_result.saturating_sub(1);
        }
    }

    pub fn handle_mouse_event(&mut self, _event: MouseEvent) -> Option<BookSearchAction> {
        None
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        if !self.active {
            return;
        }

        // Make the popup use most of the screen (90% width, 80% height)
        let popup_width = ((area.width as f32 * 0.9) as u16).max(80);
        let popup_height = ((area.height as f32 * 0.8) as u16).max(20);

        let popup_area = Rect {
            x: (area.width - popup_width) / 2,
            y: (area.height - popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        f.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Search Book ")
            .borders(Borders::ALL)
            .style(Style::default().bg(palette.base_00).fg(palette.base_05));

        f.render_widget(block.clone(), popup_area);

        let inner = block.inner(popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(inner);

        // Calculate visible results based on the actual results area height
        let _visible_count = chunks[1].height as usize;

        self.render_search_input(f, chunks[0], palette);
        self.render_results(f, chunks[1], palette);
        self.render_status_bar(f, chunks[2], palette);
    }

    fn render_search_input(&self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        let input_style = match self.focus_mode {
            FocusMode::Input => Style::default()
                .fg(palette.base_05)
                .add_modifier(Modifier::BOLD),
            FocusMode::Results => Style::default().fg(palette.base_03),
        };

        let input_text = vec![
            Span::raw("🔍 Search: "),
            Span::styled(&self.search_input, input_style),
        ];

        let input = Paragraph::new(Line::from(input_text))
            .style(Style::default().bg(palette.base_00))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .style(Style::default().fg(palette.base_03)),
            );

        f.render_widget(input, area);

        if matches!(self.focus_mode, FocusMode::Input) {
            let cursor_x = area.x + 11 + self.cursor_position as u16;
            let cursor_y = area.y; // Cursor should be on the same line as the text
            f.set_cursor_position(ratatui::layout::Position {
                x: cursor_x,
                y: cursor_y,
            });
        }
    }

    fn render_results(&self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        debug!(
            "Rendering {} results in area {:?}",
            self.results.len(),
            area
        );

        if self.results.is_empty() {
            let no_results = Paragraph::new("No results found")
                .style(Style::default().fg(palette.base_03).bg(palette.base_00))
                .alignment(Alignment::Center);
            f.render_widget(no_results, area);
            return;
        }

        // Calculate exactly how many results fit by counting actual lines needed
        let mut total_lines = 0;
        let mut visible_count = 0;

        for i in self.scroll_offset..self.results.len() {
            let result = &self.results[i];

            // Count lines for this result:
            // 1. Header line (always 1 line - it has chapter title, line number, and score)
            let mut result_lines = 1;

            // 2. Context before (if present)
            if !result.context_before.is_empty() {
                for line in result.context_before.lines() {
                    // Calculate wrapped lines: characters / width + 1 for any remainder
                    let line_width = line.chars().count();
                    let wrapped_lines = (line_width + 4) / (area.width as usize - 4).max(1); // 4 char indent
                    result_lines += wrapped_lines.max(1);
                }
            }

            // 3. Main snippet with arrow prefix
            if !result.snippet.is_empty() {
                let snippet_width = result.snippet.chars().count() + 4; // "  → " prefix
                let wrapped_lines = snippet_width / (area.width as usize).max(1) + 1;
                result_lines += wrapped_lines;
            }

            // 4. Context after (if present)
            if !result.context_after.is_empty() {
                for line in result.context_after.lines() {
                    let line_width = line.chars().count();
                    let wrapped_lines = (line_width + 4) / (area.width as usize - 4).max(1);
                    result_lines += wrapped_lines.max(1);
                }
            }

            // 5. Separator line
            result_lines += 1;

            // Check if this result fits
            if total_lines + result_lines > area.height as usize {
                break;
            }

            total_lines += result_lines;
            visible_count += 1;
        }

        let visible_end = (self.scroll_offset + visible_count.max(1)).min(self.results.len());
        let visible_results = &self.results[self.scroll_offset..visible_end];

        debug!(
            "Showing results {} to {} of {}",
            self.scroll_offset,
            visible_end,
            self.results.len()
        );

        // Build all lines for display
        let mut all_lines = Vec::new();

        for (idx, result) in visible_results.iter().enumerate() {
            let is_selected = idx + self.scroll_offset == self.selected_result;

            let score_color = if result.match_score > 0.8 {
                palette.base_0b
            } else if result.match_score > 0.6 {
                palette.base_0a
            } else {
                palette.base_08
            };

            // Create header line
            let header_spans = vec![
                Span::styled(
                    if is_selected { "▶ " } else { "  " },
                    Style::default().fg(palette.base_0d),
                ),
                Span::styled(
                    format!("{} ", result.chapter_title),
                    Style::default()
                        .fg(palette.base_0d)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("(line {}) ", result.line_number + 1),
                    Style::default().fg(palette.base_03),
                ),
                Span::styled(
                    format!("[{:.2}]", result.match_score),
                    Style::default().fg(score_color),
                ),
            ];

            if is_selected {
                all_lines
                    .push(Line::from(header_spans).style(Style::default().bg(palette.base_02)));
            } else {
                all_lines.push(Line::from(header_spans));
            }

            if !result.context_before.is_empty() {
                for line in result.context_before.lines().take(1) {
                    let prefixed_line = format!("    {line}");
                    if is_selected {
                        all_lines.push(Line::from(Span::styled(
                            prefixed_line,
                            Style::default().fg(palette.base_03).bg(palette.base_02),
                        )));
                    } else {
                        all_lines.push(Line::from(Span::styled(
                            prefixed_line,
                            Style::default().fg(palette.base_03),
                        )));
                    }
                }
            }

            if !result.snippet.is_empty() {
                if is_selected {
                    // For selected items, rebuild with background
                    let highlighted =
                        self.highlight_match(&result.snippet, &result.match_positions, palette);
                    let mut styled_spans = vec![Span::styled(
                        "  → ",
                        Style::default().fg(palette.base_0d).bg(palette.base_02),
                    )];
                    for span in highlighted {
                        // Apply selection background to each span
                        styled_spans.push(Span::styled(
                            span.content.to_string(),
                            span.style.bg(palette.base_02),
                        ));
                    }
                    all_lines.push(Line::from(styled_spans));
                } else {
                    // For non-selected, build normally
                    let mut line_spans = vec![Span::raw("  → ")];
                    let highlighted =
                        self.highlight_match(&result.snippet, &result.match_positions, palette);
                    line_spans.extend(highlighted);
                    all_lines.push(Line::from(line_spans));
                }
            }

            if !result.context_after.is_empty() {
                for line in result.context_after.lines().take(1) {
                    let prefixed_line = format!("    {line}");
                    if is_selected {
                        all_lines.push(Line::from(Span::styled(
                            prefixed_line,
                            Style::default().fg(palette.base_03).bg(palette.base_02),
                        )));
                    } else {
                        all_lines.push(Line::from(Span::styled(
                            prefixed_line,
                            Style::default().fg(palette.base_03),
                        )));
                    }
                }
            }

            all_lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(all_lines)
            .style(Style::default().bg(palette.base_00))
            .block(Block::default())
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }

    fn highlight_match(
        &self,
        text: &str,
        positions: &[usize],
        palette: &Base16Palette,
    ) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut last_pos = 0;
        let chars: Vec<char> = text.chars().collect();

        for &pos in positions {
            if pos < chars.len() {
                if pos > last_pos {
                    let segment: String = chars[last_pos..pos].iter().collect();
                    spans.push(Span::styled(segment, Style::default().fg(palette.base_05)));
                }

                spans.push(Span::styled(
                    chars[pos].to_string(),
                    Style::default()
                        .fg(palette.base_0a)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));

                last_pos = pos + 1;
            }
        }

        if last_pos < chars.len() {
            let remaining: String = chars[last_pos..].iter().collect();
            spans.push(Span::styled(
                remaining,
                Style::default().fg(palette.base_05),
            ));
        }

        spans
    }

    fn render_status_bar(&self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        let help_text = match self.focus_mode {
            FocusMode::Input => "Enter:Search  \"phrase\":Exact  Esc:Cancel",
            FocusMode::Results => {
                "j/k:Navigate  Enter:Jump  g/G:Top/Bottom  Space+f:Edit Query  Esc:Cancel"
            }
        };

        let status = vec![
            Span::styled(
                format!("{} results  ", self.results.len()),
                Style::default().fg(palette.base_0b),
            ),
            Span::styled(help_text, Style::default().fg(palette.base_03)),
        ];

        let status_bar = Paragraph::new(Line::from(status))
            .style(Style::default().bg(palette.base_00))
            .alignment(Alignment::Center);

        f.render_widget(status_bar, area);
    }
}

impl VimNavMotions for BookSearch {
    fn handle_h(&mut self) {
        // Not applicable for search
    }

    fn handle_j(&mut self) {
        self.move_selection_down();
    }

    fn handle_k(&mut self) {
        self.move_selection_up();
    }

    fn handle_l(&mut self) {
        // Not applicable for search
    }

    fn handle_ctrl_d(&mut self) {
        // Move half page down
        let half = self.visible_results / 2;
        for _ in 0..half {
            self.move_selection_down();
        }
    }

    fn handle_ctrl_u(&mut self) {
        // Move half page up
        let half = self.visible_results / 2;
        for _ in 0..half {
            self.move_selection_up();
        }
    }

    fn handle_gg(&mut self) {
        self.selected_result = 0;
        self.scroll_offset = 0;
    }

    fn handle_upper_g(&mut self) {
        if !self.results.is_empty() {
            self.selected_result = self.results.len() - 1;
            self.update_scroll();
        }
    }
}
