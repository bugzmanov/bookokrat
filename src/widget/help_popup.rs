use crate::inputs::KeySeq;
use crate::theme::OCEANIC_NEXT;
use codepage_437::{BorrowFromCp437, CP437_CONTROL};
use once_cell::sync::Lazy;
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

pub enum HelpPopupAction {
    Close,
}

pub struct HelpPopup {
    parsed_content: Text<'static>,
    total_lines: usize,
    scroll_offset: usize,
    last_popup_area: Option<Rect>,
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
                        OCEANIC_NEXT.base_00
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
                Style::default().fg(OCEANIC_NEXT.base_05),
            )));
        }

        let total_lines = lines.len();

        HelpPopup {
            parsed_content: Text::from(lines),
            total_lines,
            scroll_offset: 0,
            last_popup_area: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = content_sized_rect(94, 90, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let lines: Vec<Line> = self
            .parsed_content
            .lines
            .iter()
            .skip(self.scroll_offset)
            .cloned()
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

        // Render scrollbar
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(OCEANIC_NEXT.base_04))
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
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[([01]);(\d+);(\d+);(\d+)t").unwrap());

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
