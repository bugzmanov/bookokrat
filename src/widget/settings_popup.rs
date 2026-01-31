use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::settings::{
    PdfRenderMode, get_pdf_render_mode, is_pdf_enabled, set_pdf_enabled, set_pdf_render_mode,
};
use crate::terminal;
use crate::theme::{
    Base16Palette, all_theme_names, current_theme, current_theme_index, set_theme_by_index_and_save,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, ListState, Paragraph},
};

pub enum SettingsAction {
    Close,
    SettingsChanged,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    PdfSupport,
    Themes,
}

impl SettingsTab {
    fn next(self) -> Self {
        if !cfg!(feature = "pdf") {
            return SettingsTab::Themes;
        }
        match self {
            SettingsTab::PdfSupport => SettingsTab::Themes,
            SettingsTab::Themes => SettingsTab::PdfSupport,
        }
    }

    fn prev(self) -> Self {
        if !cfg!(feature = "pdf") {
            return SettingsTab::Themes;
        }
        self.next() // Only 2 tabs, so next == prev
    }
}

pub struct SettingsPopup {
    current_tab: SettingsTab,
    // PDF tab state
    pdf_selected_idx: usize,
    supports_scroll_mode: bool,
    supports_graphics: bool,
    // Themes tab state
    theme_list_state: ListState,
    theme_names: Vec<String>,
    // Common
    last_popup_area: Option<Rect>,
}

impl Default for SettingsPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsPopup {
    pub fn new() -> Self {
        if cfg!(feature = "pdf") {
            Self::new_with_tab(SettingsTab::PdfSupport)
        } else {
            Self::new_with_tab(SettingsTab::Themes)
        }
    }

    pub fn new_with_tab(tab: SettingsTab) -> Self {
        let caps = terminal::detect_terminal();
        let supports_graphics = caps.supports_graphics;
        let supports_scroll_mode = caps.pdf.supports_scroll_mode;
        let pdf_selected_idx = if supports_graphics { 0 } else { 2 };
        let current_tab = if cfg!(feature = "pdf") {
            tab
        } else {
            SettingsTab::Themes
        };

        let theme_names = all_theme_names();
        let current_idx = current_theme_index();
        let mut theme_list_state = ListState::default();
        theme_list_state.select(Some(current_idx));

        SettingsPopup {
            current_tab,
            pdf_selected_idx,
            supports_scroll_mode,
            supports_graphics,
            theme_list_state,
            theme_names,
            last_popup_area: None,
        }
    }

    fn pdf_min_selectable_idx(&self) -> usize {
        if self.supports_graphics { 0 } else { 2 }
    }

    fn pdf_max_selectable_idx(&self) -> usize {
        if !is_pdf_enabled() {
            return 1;
        }
        if self.supports_scroll_mode { 3 } else { 2 }
    }

    fn render_mode_available(&self) -> bool {
        self.supports_graphics && is_pdf_enabled()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 60, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let palette = current_theme();

        // Build footer hints string for bottom border
        let hints = if cfg!(feature = "pdf") {
            " Tab/h/l switch tabs  j/k navigate  Enter select  Esc close "
        } else {
            " j/k navigate  Enter select  Esc close "
        };

        // Main block with title and footer hints on bottom border
        let block = Block::default()
            .title(" Settings ")
            .title_bottom(Line::from(hints).right_aligned())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette.popup_border_color()))
            .style(Style::default().bg(palette.base_00));

        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        // Add padding inside the border
        let padded = Rect {
            x: inner.x + 2,
            y: inner.y + 1,
            width: inner.width.saturating_sub(4),
            height: inner.height.saturating_sub(2),
        };

        // Layout: tabs row + content area
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Tabs
                Constraint::Min(1),    // Content
            ])
            .split(padded);

        // Render tabs
        self.render_tabs(f, main_chunks[0], &palette);

        // Render content based on selected tab
        let content_area = Rect {
            x: main_chunks[1].x,
            y: main_chunks[1].y + 1,
            width: main_chunks[1].width,
            height: main_chunks[1].height.saturating_sub(1),
        };

        match self.current_tab {
            SettingsTab::PdfSupport => self.render_pdf_tab(f, content_area, &palette),
            SettingsTab::Themes => self.render_themes_tab(f, content_area, &palette),
        }
    }

    fn render_tabs(&self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        let tab_names = ["PDF Support", "Select Theme"];

        let mut spans = Vec::new();
        spans.push(Span::raw(" "));

        let tab_iter: Box<dyn Iterator<Item = (usize, &&str)>> = if cfg!(feature = "pdf") {
            Box::new(tab_names.iter().enumerate())
        } else {
            Box::new(tab_names.iter().enumerate().filter(|(idx, _)| *idx == 1))
        };

        for (idx, name) in tab_iter {
            let is_selected = match (idx, self.current_tab) {
                (0, SettingsTab::PdfSupport) => true,
                (1, SettingsTab::Themes) => true,
                _ => false,
            };

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

        let tabs_line = Line::from(spans);
        f.render_widget(Paragraph::new(tabs_line), area);

        // Render underline for selected tab
        let underline_y = area.y + 1;
        if underline_y < area.y + area.height && cfg!(feature = "pdf") {
            let underline_area = Rect {
                x: area.x,
                y: underline_y,
                width: area.width,
                height: 1,
            };

            let (underline_x, underline_len) = match self.current_tab {
                SettingsTab::PdfSupport => (1, 11), // "PDF Support" length
                SettingsTab::Themes => (15, 12), // Position after "PDF Support   ", "Select Theme" length
            };

            let mut underline_spans = vec![Span::raw(" ".repeat(underline_x))];
            underline_spans.push(Span::styled(
                "─".repeat(underline_len),
                Style::default().fg(palette.base_0d),
            ));

            f.render_widget(Paragraph::new(Line::from(underline_spans)), underline_area);
        }
    }

    fn render_pdf_tab(&mut self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        let pdf_enabled = is_pdf_enabled();
        let current_mode = get_pdf_render_mode();
        let effective_pdf_enabled = self.supports_graphics && pdf_enabled;
        let render_mode_available = self.render_mode_available();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // PDF Support options (Enabled/Disabled)
                Constraint::Length(1), // spacer
                Constraint::Length(1), // Render Mode header
                Constraint::Length(1), // empty line
                Constraint::Length(2), // Render Mode options
                Constraint::Length(1), // spacer
                Constraint::Min(1),    // Info message
            ])
            .split(area);

        // PDF Support options (no section header - derived from tab name)
        let radio_selected = "●";
        let radio_unselected = "○";

        let pdf_options_area = Rect {
            x: chunks[0].x,
            y: chunks[0].y,
            width: chunks[0].width,
            height: chunks[0].height,
        };

        let options_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(pdf_options_area);

        let enabled_radio = if effective_pdf_enabled {
            radio_selected
        } else {
            radio_unselected
        };
        let enabled_style = if self.supports_graphics {
            Style::default().fg(palette.base_06)
        } else {
            Style::default().fg(palette.base_03)
        };
        let enabled_line = self.render_radio_option(
            enabled_radio,
            "Enabled",
            None,
            enabled_style,
            self.current_tab == SettingsTab::PdfSupport
                && self.pdf_selected_idx == 0
                && self.supports_graphics,
            palette,
        );
        f.render_widget(Paragraph::new(enabled_line), options_chunks[0]);

        let disabled_radio = if !effective_pdf_enabled {
            radio_selected
        } else {
            radio_unselected
        };
        let disabled_line = self.render_radio_option(
            disabled_radio,
            "Disabled",
            None,
            enabled_style,
            self.current_tab == SettingsTab::PdfSupport
                && self.pdf_selected_idx == 1
                && self.supports_graphics,
            palette,
        );
        f.render_widget(Paragraph::new(disabled_line), options_chunks[1]);

        // Render Mode section header
        let render_header_color = if render_mode_available {
            palette.base_06
        } else {
            palette.base_03
        };
        self.render_section_header(f, chunks[2], "Render Mode", palette, render_header_color);

        // Render Mode options
        let render_options_area = Rect {
            x: chunks[4].x + 2,
            y: chunks[4].y,
            width: chunks[4].width.saturating_sub(2),
            height: chunks[4].height,
        };

        let render_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(render_options_area);

        let option_style = if render_mode_available {
            Style::default().fg(palette.base_06)
        } else {
            Style::default().fg(palette.base_03)
        };

        let page_radio = if current_mode == PdfRenderMode::Page {
            radio_selected
        } else {
            radio_unselected
        };
        let page_line = self.render_radio_option(
            page_radio,
            "Page",
            Some("one page at a time"),
            option_style,
            self.current_tab == SettingsTab::PdfSupport
                && self.pdf_selected_idx == 2
                && render_mode_available,
            palette,
        );
        f.render_widget(Paragraph::new(page_line), render_chunks[0]);

        let scroll_radio = if current_mode == PdfRenderMode::Scroll {
            radio_selected
        } else {
            radio_unselected
        };
        let scroll_style = if render_mode_available && self.supports_scroll_mode {
            Style::default().fg(palette.base_06)
        } else {
            Style::default().fg(palette.base_03)
        };
        let scroll_suffix = if !self.supports_scroll_mode {
            Some("Kitty only")
        } else {
            Some("continuous scroll")
        };
        let scroll_line = self.render_radio_option(
            scroll_radio,
            "Scroll",
            scroll_suffix,
            scroll_style,
            self.current_tab == SettingsTab::PdfSupport
                && self.pdf_selected_idx == 3
                && render_mode_available
                && self.supports_scroll_mode,
            palette,
        );
        f.render_widget(Paragraph::new(scroll_line), render_chunks[1]);

        // Info message
        let info_lines = self.get_pdf_info_lines(palette, pdf_enabled, current_mode);
        f.render_widget(Paragraph::new(info_lines), chunks[6]);
    }

    fn render_themes_tab(&mut self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        // No section header - derived from tab name "Select Theme"
        let current_idx = current_theme_index();
        let selected_idx = self.theme_list_state.selected().unwrap_or(0);

        // Calculate visible range based on area height and scroll offset
        let visible_height = area.height as usize;
        let offset = self.theme_list_state.offset();

        for (visual_idx, theme_idx) in (offset..self.theme_names.len())
            .take(visible_height)
            .enumerate()
        {
            if visual_idx >= visible_height {
                break;
            }

            let name = &self.theme_names[theme_idx];
            let is_current = theme_idx == current_idx;
            let is_selected = theme_idx == selected_idx;

            let line_area = Rect {
                x: area.x,
                y: area.y + visual_idx as u16,
                width: area.width,
                height: 1,
            };

            let line = self.render_theme_option(name, is_current, is_selected, palette);
            f.render_widget(Paragraph::new(line), line_area);
        }
    }

    fn render_theme_option(
        &self,
        name: &str,
        is_current: bool,
        is_selected: bool,
        palette: &Base16Palette,
    ) -> Line<'static> {
        let prefix = if is_selected { "» " } else { "  " };
        let radio = if is_current { "● " } else { "○ " };

        let spans = vec![
            Span::styled(
                prefix.to_string(),
                if is_selected {
                    Style::default()
                        .fg(palette.base_0a)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(radio.to_string(), Style::default().fg(palette.base_06)),
            Span::styled(name.to_string(), Style::default().fg(palette.base_06)),
        ];

        Line::from(spans)
    }

    fn render_section_header(
        &self,
        f: &mut Frame,
        area: Rect,
        title: &str,
        palette: &Base16Palette,
        title_color: ratatui::style::Color,
    ) {
        // Format: "▸ Title" - prefix in accent color, title in passed color
        let line = Line::from(vec![
            Span::styled("▸ ", Style::default().fg(palette.base_0d)),
            Span::styled(title, Style::default().fg(title_color)),
        ]);

        f.render_widget(Paragraph::new(line), area);
    }

    fn get_pdf_info_lines(
        &self,
        palette: &Base16Palette,
        pdf_enabled: bool,
        current_mode: PdfRenderMode,
    ) -> Vec<Line<'static>> {
        if !self.supports_graphics {
            vec![
                Line::from(vec![
                    Span::styled("ⓘ ", Style::default().fg(palette.base_03)),
                    Span::styled(
                        "PDF viewing requires a graphics-enabled terminal.",
                        Style::default().fg(palette.base_03),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        "Consider using Kitty, Ghostty, WezTerm, or iTerm2.",
                        Style::default().fg(palette.base_03),
                    ),
                ]),
            ]
        } else if !pdf_enabled {
            vec![Line::from(vec![
                Span::styled("ⓘ ", Style::default().fg(palette.base_03)),
                Span::styled(
                    "Enable PDF support to change render mode",
                    Style::default().fg(palette.base_03),
                ),
            ])]
        } else if self.supports_scroll_mode && current_mode == PdfRenderMode::Scroll {
            vec![Line::from(vec![
                Span::styled("! ", Style::default().fg(palette.base_09)),
                Span::styled(
                    "Scroll mode uses 500MB-1GB memory",
                    Style::default().fg(palette.base_09),
                ),
            ])]
        } else if !self.supports_scroll_mode {
            vec![Line::from(vec![
                Span::styled("ⓘ ", Style::default().fg(palette.base_03)),
                Span::styled(
                    "Scroll mode requires Kitty or Ghostty terminal",
                    Style::default().fg(palette.base_03),
                ),
            ])]
        } else {
            vec![]
        }
    }

    fn render_radio_option(
        &self,
        radio: &str,
        label: &str,
        suffix: Option<&str>,
        style: Style,
        is_focused: bool,
        palette: &Base16Palette,
    ) -> Line<'static> {
        let prefix = if is_focused { "» " } else { "  " };
        let mut spans = vec![
            Span::styled(
                prefix.to_string(),
                if is_focused {
                    Style::default()
                        .fg(palette.base_0a)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(format!("{} ", radio), style),
            Span::styled(label.to_string(), style),
        ];

        if let Some(s) = suffix {
            spans.push(Span::styled(
                format!(" ({})", s),
                Style::default().fg(palette.base_03),
            ));
        }

        Line::from(spans)
    }

    fn pdf_next(&mut self) {
        let min_idx = self.pdf_min_selectable_idx();
        let max_idx = self.pdf_max_selectable_idx();
        let mut next_idx = self.pdf_selected_idx + 1;
        if next_idx > max_idx {
            next_idx = min_idx;
        }
        if !self.supports_graphics && (next_idx == 0 || next_idx == 1) {
            next_idx = 2;
        }
        if !self.render_mode_available() && (next_idx == 2 || next_idx == 3) {
            next_idx = min_idx;
        }
        if !self.supports_scroll_mode && next_idx == 3 {
            next_idx = min_idx;
        }
        self.pdf_selected_idx = next_idx;
    }

    fn pdf_previous(&mut self) {
        let min_idx = self.pdf_min_selectable_idx();
        let max_idx = self.pdf_max_selectable_idx();
        let mut prev_idx = if self.pdf_selected_idx <= min_idx {
            max_idx
        } else {
            self.pdf_selected_idx - 1
        };
        if !self.supports_graphics && (prev_idx == 0 || prev_idx == 1) {
            prev_idx = max_idx;
        }
        if !self.render_mode_available() && (prev_idx == 2 || prev_idx == 3) {
            prev_idx = max_idx;
        }
        if !self.supports_scroll_mode && prev_idx == 3 {
            prev_idx = 2;
        }
        self.pdf_selected_idx = prev_idx;
    }

    fn theme_next(&mut self) {
        let i = match self.theme_list_state.selected() {
            Some(i) => {
                if i >= self.theme_names.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.theme_list_state.select(Some(i));
    }

    fn theme_previous(&mut self) {
        let i = match self.theme_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.theme_names.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.theme_list_state.select(Some(i));
    }

    fn apply_pdf_selected(&self) -> Option<SettingsAction> {
        match self.pdf_selected_idx {
            0 if self.supports_graphics => {
                if !is_pdf_enabled() {
                    set_pdf_enabled(true);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            1 if self.supports_graphics => {
                if is_pdf_enabled() {
                    set_pdf_enabled(false);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            2 if self.render_mode_available() => {
                if get_pdf_render_mode() != PdfRenderMode::Page {
                    set_pdf_render_mode(PdfRenderMode::Page);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            3 if self.render_mode_available() && self.supports_scroll_mode => {
                if get_pdf_render_mode() != PdfRenderMode::Scroll {
                    set_pdf_render_mode(PdfRenderMode::Scroll);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            _ => None,
        }
    }

    fn apply_theme_selected(&self) -> Option<SettingsAction> {
        if let Some(idx) = self.theme_list_state.selected() {
            if idx != current_theme_index() {
                set_theme_by_index_and_save(idx);
                return Some(SettingsAction::SettingsChanged);
            }
        }
        None
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
    ) -> Option<SettingsAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Tab => {
                self.current_tab = self.current_tab.next();
                None
            }
            KeyCode::BackTab => {
                self.current_tab = self.current_tab.prev();
                None
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.current_tab = self.current_tab.prev();
                None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.current_tab = self.current_tab.next();
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
            KeyCode::Esc => Some(SettingsAction::Close),
            KeyCode::Enter | KeyCode::Char(' ') => match self.current_tab {
                SettingsTab::PdfSupport => self.apply_pdf_selected(),
                SettingsTab::Themes => self.apply_theme_selected(),
            },
            _ => None,
        }
    }
}

impl VimNavMotions for SettingsPopup {
    fn handle_h(&mut self) {
        self.current_tab = self.current_tab.prev();
    }

    fn handle_j(&mut self) {
        match self.current_tab {
            SettingsTab::PdfSupport => self.pdf_next(),
            SettingsTab::Themes => self.theme_next(),
        }
    }

    fn handle_k(&mut self) {
        match self.current_tab {
            SettingsTab::PdfSupport => self.pdf_previous(),
            SettingsTab::Themes => self.theme_previous(),
        }
    }

    fn handle_l(&mut self) {
        self.current_tab = self.current_tab.next();
    }

    fn handle_ctrl_d(&mut self) {
        if self.current_tab == SettingsTab::Themes {
            for _ in 0..5 {
                self.theme_next();
            }
        }
    }

    fn handle_ctrl_u(&mut self) {
        if self.current_tab == SettingsTab::Themes {
            for _ in 0..5 {
                self.theme_previous();
            }
        }
    }

    fn handle_gg(&mut self) {
        match self.current_tab {
            SettingsTab::PdfSupport => {
                self.pdf_selected_idx = self.pdf_min_selectable_idx();
            }
            SettingsTab::Themes => {
                if !self.theme_names.is_empty() {
                    self.theme_list_state.select(Some(0));
                }
            }
        }
    }

    fn handle_upper_g(&mut self) {
        match self.current_tab {
            SettingsTab::PdfSupport => {
                self.pdf_selected_idx = self.pdf_max_selectable_idx();
            }
            SettingsTab::Themes => {
                if !self.theme_names.is_empty() {
                    self.theme_list_state
                        .select(Some(self.theme_names.len() - 1));
                }
            }
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
