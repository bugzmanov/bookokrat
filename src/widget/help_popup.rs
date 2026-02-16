use crate::inputs::KeySeq;
use crate::search::{SearchMode, SearchState, find_matches_in_text};
use crate::theme::current_theme;
use codepage_437::{BorrowFromCp437, CP437_CONTROL};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};
use regex::Regex;
use std::sync::LazyLock;

pub enum HelpPopupAction {
    Close,
}

pub struct HelpPopup {
    parsed_content: Text<'static>,
    plain_text_lines: Vec<String>,
    total_lines: usize,
    scroll_offset: usize,
    last_popup_area: Option<Rect>,
    search_state: SearchState,
}

impl Default for HelpPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpPopup {
    pub fn new() -> Self {
        let ansi_art_bytes = include_bytes!("../../readme.ans");

        // Strip SAUCE metadata if present (last 128 bytes starting with "SAUCE00")
        let ansi_art_bytes = strip_sauce_metadata(ansi_art_bytes);

        // Convert CP437 to UTF-8 to get proper box-drawing characters
        let ansi_art = String::borrow_from_cp437(ansi_art_bytes, &CP437_CONTROL);

        // Pre-process: Convert non-standard ESC[1;R;G;Bt sequences to standard ESC[38;2;R;G;Bm
        let ansi_art = preprocess_custom_ansi(&ansi_art);

        // Parse ANSI sequences using vt100 - ANSI art is 90 columns wide, 34 lines tall
        let mut parser = vt100::Parser::new(34, 90, 0);
        parser.process(ansi_art.as_bytes());

        let screen = parser.screen().clone();
        let mut lines: Vec<Line> = Vec::new();

        // Process all rows from the vt100 screen
        for row in 0..34 {
            let mut spans = Vec::new();

            for col in 0..90 {
                if let Some(cell) = screen.cell(row, col) {
                    let ch = if cell.contents().is_empty() {
                        " "
                    } else {
                        &cell.contents()
                    };

                    let fg = to_color(cell.fgcolor());
                    let bg = to_color(cell.bgcolor());

                    let final_bg = if ch == " " && !matches!(bg, Color::Reset) {
                        bg
                    } else if matches!(bg, Color::Reset | Color::Rgb(0, 0, 0)) {
                        current_theme().base_00
                    } else {
                        bg
                    };

                    let mut style = Style::default().fg(fg).bg(final_bg);

                    if cell.bold() {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if cell.italic() {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                    if cell.underline() {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }

                    spans.push(Span::styled(ch.to_string(), style));
                }
            }

            lines.push(Line::from(spans));
        }

        // Add readme.txt content as plain text after the ANSI art
        let readme = include_str!("../../readme.txt");
        for line in readme.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(current_theme().base_05),
            )));
        }

        let plain_text_lines: Vec<String> = lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();

        let total_lines = lines.len();

        HelpPopup {
            parsed_content: Text::from(lines),
            plain_text_lines,
            total_lines,
            scroll_offset: 0,
            last_popup_area: None,
            search_state: SearchState::new(),
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = content_sized_rect(94, 90, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let searching = self.search_state.active && !self.search_state.query.is_empty();
        let lines: Vec<Line> = self
            .parsed_content
            .lines
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .map(|(idx, line)| {
                if searching {
                    let is_current = self.search_state.is_current_match(idx);
                    highlight_line(line, &self.search_state.query, is_current)
                } else {
                    line.clone()
                }
            })
            .collect();

        let bottom_title = match self.search_state.mode {
            SearchMode::InputMode => {
                format!(" /{} ", self.search_state.query)
            }
            SearchMode::NavigationMode => {
                format!(
                    " /{} {} ",
                    self.search_state.query,
                    self.search_state.get_match_info()
                )
            }
            SearchMode::Inactive => String::new(),
        };

        let mut block = Block::default()
            .title(" Help - Press ? or ESC to close ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(current_theme().popup_border_color()))
            .style(Style::default().bg(current_theme().base_00));

        if !bottom_title.is_empty() {
            block = block
                .title_bottom(Line::from(bottom_title).style(Style::default().fg(Color::Yellow)));
        }

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, popup_area);

        // Render scrollbar
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(current_theme().base_04))
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

        let mut scrollbar_state =
            ScrollbarState::new(self.total_lines).position(self.scroll_offset);

        f.render_stateful_widget(
            scrollbar,
            popup_area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_offset < self.total_lines.saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_page_down(&mut self, page_size: usize) {
        self.scroll_offset =
            (self.scroll_offset + page_size).min(self.total_lines.saturating_sub(1));
    }

    fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.total_lines.saturating_sub(1);
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<HelpPopupAction> {
        if self.search_state.mode == SearchMode::InputMode {
            return self.handle_search_input(key);
        }
        self.handle_normal_and_nav(key, key_seq)
    }

    fn handle_search_input(&mut self, key: crossterm::event::KeyEvent) -> Option<HelpPopupAction> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                let original = self.search_state.cancel_search();
                self.scroll_offset = original;
                None
            }
            KeyCode::Enter => {
                self.search_state.confirm_search();
                None
            }
            KeyCode::Backspace => {
                self.search_state.query.pop();
                self.perform_search();
                None
            }
            KeyCode::Char(c) => {
                self.search_state.query.push(c);
                self.perform_search();
                None
            }
            _ => None,
        }
    }

    fn handle_normal_and_nav(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<HelpPopupAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('/') => {
                self.search_state.start_search(self.scroll_offset);
                None
            }
            KeyCode::Char('n') if self.search_state.mode == SearchMode::NavigationMode => {
                if let Some(line_idx) = self.search_state.next_match() {
                    self.scroll_to_line(line_idx);
                }
                None
            }
            KeyCode::Char('N') if self.search_state.mode == SearchMode::NavigationMode => {
                if let Some(line_idx) = self.search_state.previous_match() {
                    self.scroll_to_line(line_idx);
                }
                None
            }
            KeyCode::Esc if self.search_state.mode == SearchMode::NavigationMode => {
                self.search_state.exit_search();
                None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_down();
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
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
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page_size = if let Some(area) = self.last_popup_area {
                    (area.height as usize).max(1)
                } else {
                    20
                };
                self.scroll_page_down(page_size);
                None
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page_size = if let Some(area) = self.last_popup_area {
                    (area.height as usize).max(1)
                } else {
                    20
                };
                self.scroll_page_up(page_size);
                None
            }
            KeyCode::PageDown => {
                let page_size = if let Some(area) = self.last_popup_area {
                    (area.height as usize).max(1)
                } else {
                    20
                };
                self.scroll_page_down(page_size);
                None
            }
            KeyCode::PageUp => {
                let page_size = if let Some(area) = self.last_popup_area {
                    (area.height as usize).max(1)
                } else {
                    20
                };
                self.scroll_page_up(page_size);
                None
            }
            KeyCode::Esc | KeyCode::Char('?') => Some(HelpPopupAction::Close),
            _ => None,
        }
    }

    fn perform_search(&mut self) {
        let matches = find_matches_in_text(&self.search_state.query, &self.plain_text_lines);
        self.search_state.set_matches(matches);
        if let Some(line_idx) = self.search_state.get_current_match() {
            self.scroll_to_line(line_idx);
        }
    }

    fn scroll_to_line(&mut self, line_idx: usize) {
        let page_size = self
            .last_popup_area
            .map(|a| a.height as usize)
            .unwrap_or(20);
        let half_page = page_size / 2;
        self.scroll_offset = line_idx
            .saturating_sub(half_page)
            .min(self.total_lines.saturating_sub(1));
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

/// Highlights occurrences of `query` within a pre-styled line, preserving original span styles
/// but overriding the background color for matching text.
fn highlight_line(line: &Line<'static>, query: &str, is_current_match_line: bool) -> Line<'static> {
    let plain_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let plain_lower = plain_text.to_lowercase();
    let query_lower = query.to_lowercase();

    // Find all match byte ranges and merge overlapping ones
    let mut match_ranges: Vec<(usize, usize)> = Vec::new();
    let mut search_start = 0;
    while let Some(pos) = plain_lower[search_start..].find(&query_lower) {
        let start = search_start + pos;
        let end = start + query_lower.len();
        if let Some(last) = match_ranges.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
            } else {
                match_ranges.push((start, end));
            }
        } else {
            match_ranges.push((start, end));
        }
        search_start = start + 1;
    }

    if match_ranges.is_empty() {
        return line.clone();
    }

    let highlight_bg = if is_current_match_line {
        Color::Yellow
    } else {
        Color::Rgb(100, 100, 0)
    };

    let mut new_spans: Vec<Span<'static>> = Vec::new();
    let mut byte_pos: usize = 0;
    let mut match_idx: usize = 0;

    for span in &line.spans {
        let text = span.content.as_ref();
        let span_start = byte_pos;
        let span_end = byte_pos + text.len();
        let mut local = 0;

        while match_idx < match_ranges.len() {
            let (m_start, m_end) = match_ranges[match_idx];

            if m_start >= span_end {
                break;
            }
            if m_end <= span_start {
                match_idx += 1;
                continue;
            }

            let overlap_start = m_start.max(span_start) - span_start;
            let overlap_end = m_end.min(span_end) - span_start;

            if overlap_start > local {
                new_spans.push(Span::styled(
                    text[local..overlap_start].to_string(),
                    span.style,
                ));
            }

            let mut hl_style = span.style.bg(highlight_bg);
            if is_current_match_line {
                hl_style = hl_style.fg(Color::Black);
            }
            new_spans.push(Span::styled(
                text[overlap_start..overlap_end].to_string(),
                hl_style,
            ));

            local = overlap_end;

            if m_end <= span_end {
                match_idx += 1;
            } else {
                break;
            }
        }

        if local < text.len() {
            new_spans.push(Span::styled(text[local..].to_string(), span.style));
        }

        byte_pos = span_end;
    }

    Line::from(new_spans)
}

/// Strips SAUCE metadata from ANSI art files
/// SAUCE (Standard Architecture for Universal Comment Extensions) is metadata
/// stored in the last 128 bytes of the file, starting with "SAUCE00"
fn strip_sauce_metadata(bytes: &[u8]) -> &[u8] {
    const SAUCE_SIZE: usize = 128;
    const SAUCE_ID: &[u8] = b"SAUCE00";

    // Check if file is large enough to contain SAUCE
    if bytes.len() < SAUCE_SIZE {
        return bytes;
    }

    // Check if SAUCE record exists at the expected position
    let sauce_offset = bytes.len() - SAUCE_SIZE;
    if &bytes[sauce_offset..sauce_offset + SAUCE_ID.len()] == SAUCE_ID {
        // Also strip the EOF marker (0x1A) if present before SAUCE
        let mut end = sauce_offset;
        if end > 0 && bytes[end - 1] == 0x1A {
            end -= 1;
        }
        &bytes[..end]
    } else {
        bytes
    }
}

/// Converts non-standard ANSI color sequences to standard SGR format
/// Handles ESC[1;R;G;Bt and ESC[0;R;G;Bt sequences
fn preprocess_custom_ansi(input: &str) -> String {
    // Match ESC[1;R;G;Bt or ESC[0;R;G;Bt sequences
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\x1b\[([01]);(\d+);(\d+);(\d+)t").unwrap());

    RE.replace_all(input, |caps: &regex::Captures| {
        let bold_flag = &caps[1];
        let r: u8 = caps[2].parse().unwrap_or(0);
        let g: u8 = caps[3].parse().unwrap_or(0);
        let b: u8 = caps[4].parse().unwrap_or(0);

        // If bold flag is set (1), include bold modifier
        if bold_flag == "1" {
            format!("\x1b[1m\x1b[38;2;{r};{g};{b}m")
        } else {
            format!("\x1b[38;2;{r};{g};{b}m")
        }
    })
    .into_owned()
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

fn to_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => match i {
            0 => Color::Black,
            1 => Color::Rgb(255, 0, 0),
            2 => Color::Rgb(0, 255, 0),
            3 => Color::Rgb(255, 255, 0),
            4 => Color::Rgb(0, 100, 255),
            5 => Color::Rgb(255, 0, 255),
            6 => Color::Rgb(0, 255, 255),
            7 => Color::Rgb(220, 220, 220),
            8 => Color::Rgb(128, 128, 128),
            9 => Color::Rgb(255, 100, 100),
            10 => Color::Rgb(100, 255, 100),
            11 => Color::Rgb(255, 255, 100),
            12 => Color::Rgb(100, 150, 255),
            13 => Color::Rgb(255, 100, 255),
            14 => Color::Rgb(100, 255, 255),
            15 => Color::White,
            _ => Color::Indexed(i),
        },
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
