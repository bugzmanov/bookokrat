use crate::inputs::KeySeq;
use crate::theme::current_theme;
use crate::widget::popup::Popup;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

pub enum LookupPopupAction {
    Close,
}

pub struct LookupPopup {
    title: String,
    lines: Vec<Line<'static>>,
    total_lines: usize,
    scroll_offset: usize,
    last_popup_area: Option<Rect>,
}

impl LookupPopup {
    pub fn new(word: String, result: Result<String, String>) -> Self {
        let lines: Vec<Line<'static>> = match result {
            Ok(output) => {
                if output.trim().is_empty() {
                    vec![Line::from(Span::styled(
                        "  (no output)",
                        Style::default().fg(current_theme().base_03),
                    ))]
                } else if has_ansi_escapes(&output) {
                    parse_ansi_output(&output)
                } else {
                    output.lines().map(style_by_indent).collect()
                }
            }
            Err(err) => err
                .lines()
                .map(|l| {
                    Line::from(Span::styled(
                        format!("  {l}"),
                        Style::default().fg(current_theme().base_08),
                    ))
                })
                .collect(),
        };

        let total_lines = lines.len();

        Self {
            title: word,
            lines,
            total_lines,
            scroll_offset: 0,
            last_popup_area: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_width = (area.width * 70 / 100)
            .max(40)
            .min(area.width.saturating_sub(4));
        let popup_height = (area.height * 70 / 100)
            .max(10)
            .min(area.height.saturating_sub(4));

        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let visible_lines: Vec<Line> = self
            .lines
            .iter()
            .skip(self.scroll_offset)
            .cloned()
            .collect();

        let title = format!(" Lookup: {} ", self.title);
        let block = Block::default()
            .title(Span::styled(
                title,
                Style::default()
                    .fg(current_theme().base_0a)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(current_theme().popup_border_color()))
            .style(Style::default().bg(current_theme().base_00));

        let paragraph = Paragraph::new(visible_lines)
            .block(block)
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, popup_area);

        if self.total_lines > popup_height.saturating_sub(2) as usize {
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

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<LookupPopupAction> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_down();
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_up();
                None
            }
            KeyCode::Char('g') if key_seq.handle_key('g') == "gg" => {
                self.scroll_offset = 0;
                None
            }
            KeyCode::Char('G') => {
                self.scroll_offset = self.total_lines.saturating_sub(1);
                None
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = 10;
                self.scroll_offset =
                    (self.scroll_offset + half).min(self.total_lines.saturating_sub(1));
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = 10;
                self.scroll_offset = self.scroll_offset.saturating_sub(half);
                None
            }
            KeyCode::Esc | KeyCode::Char('q') => Some(LookupPopupAction::Close),
            _ => None,
        }
    }
}

impl Popup for LookupPopup {
    fn get_last_popup_area(&self) -> Option<Rect> {
        return self.last_popup_area;
    }
}

/// Style a line based on its indentation level using theme colors.
/// No indent = heading/source (bold + accent), shallow = entry, deep = body text.
fn style_by_indent(line: &str) -> Line<'static> {
    let theme = current_theme();

    if line.trim().is_empty() {
        return Line::from(Span::raw(""));
    }

    let indent = line.len() - line.trim_start().len();

    let style = match indent {
        0 => Style::default()
            .fg(theme.base_0d)
            .add_modifier(Modifier::BOLD),
        1..=5 => Style::default()
            .fg(theme.base_07)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(theme.base_07),
    };

    Line::from(Span::styled(format!("  {line}"), style))
}

fn has_ansi_escapes(text: &str) -> bool {
    text.contains("\x1b[")
}

/// Parse output containing ANSI escape codes using vt100.
fn parse_ansi_output(text: &str) -> Vec<Line<'static>> {
    let line_count = text.lines().count().max(1);
    let max_width = text.lines().map(|l| l.len()).max().unwrap_or(80).max(80);

    let mut parser = vt100::Parser::new(line_count as u16, max_width as u16, 0);
    parser.process(text.as_bytes());

    let screen = parser.screen().clone();
    let theme = current_theme();
    let mut lines: Vec<Line<'static>> = Vec::new();

    for row in 0..line_count as u16 {
        let mut spans = Vec::new();
        spans.push(Span::raw("  "));

        let mut col = 0u16;
        while col < max_width as u16 {
            let Some(cell) = screen.cell(row, col) else {
                break;
            };

            let ch = if cell.contents().is_empty() {
                " ".to_string()
            } else {
                cell.contents()
            };

            let fg = vt100_color_to_ratatui(cell.fgcolor(), theme.base_07);
            let bg = vt100_color_to_ratatui(cell.bgcolor(), Color::Reset);

            let final_bg = if matches!(bg, Color::Reset) {
                theme.base_00
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

            spans.push(Span::styled(ch, style));
            col += 1;
        }

        // Trim trailing spaces
        while spans.last().is_some_and(|s| s.content.as_ref() == " ") {
            spans.pop();
        }

        lines.push(Line::from(spans));
    }

    // Trim trailing empty lines
    while lines
        .last()
        .is_some_and(|l| l.spans.iter().all(|s| s.content.as_ref().trim().is_empty()))
    {
        lines.pop();
    }

    lines
}

fn vt100_color_to_ratatui(c: vt100::Color, default: Color) -> Color {
    let theme = current_theme();
    match c {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => match i {
            0 => theme.base_00,
            1 => theme.base_08,
            2 => theme.base_0b,
            3 => theme.base_0a,
            4 => theme.base_0d,
            5 => theme.base_0e,
            6 => theme.base_0c,
            7 => theme.base_05,
            8 => theme.base_03,
            9 => theme.base_08,
            10 => theme.base_0b,
            11 => theme.base_0a,
            12 => theme.base_0d,
            13 => theme.base_0e,
            14 => theme.base_0c,
            15 => theme.base_07,
            _ => Color::Indexed(i),
        },
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
