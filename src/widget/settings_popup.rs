use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::settings::{
    LookupDisplay, PdfPageLayoutMode, PdfRenderMode, get_lookup_command, get_lookup_display,
    get_pdf_page_layout_mode, get_pdf_render_mode, get_synctex_editor, is_pdf_enabled,
    is_transparent_background, set_lookup_command, set_lookup_display, set_pdf_enabled,
    set_pdf_page_layout_mode, set_pdf_render_mode, set_synctex_editor, set_transparent_background,
};
use crate::terminal;
use crate::theme::{
    Base16Palette, all_theme_names, current_theme, current_theme_index, set_theme_by_index_and_save,
};
use crate::widget::popup::Popup;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub enum SettingsAction {
    Close,
    SettingsChanged,
    PageLayoutChanged,
    TestLookupCommand,
    TestSynctexEditor,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    PdfSupport,
    Themes,
    Integrations,
}

impl SettingsTab {
    fn next(self) -> Self {
        match self {
            SettingsTab::PdfSupport => SettingsTab::Themes,
            SettingsTab::Themes => SettingsTab::Integrations,
            SettingsTab::Integrations => {
                if cfg!(feature = "pdf") {
                    SettingsTab::PdfSupport
                } else {
                    SettingsTab::Themes
                }
            }
        }
    }

    fn prev(self) -> Self {
        match self {
            SettingsTab::PdfSupport => SettingsTab::Integrations,
            SettingsTab::Themes => {
                if cfg!(feature = "pdf") {
                    SettingsTab::PdfSupport
                } else {
                    SettingsTab::Integrations
                }
            }
            SettingsTab::Integrations => SettingsTab::Themes,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IntegrationsFocus {
    LookupCommand,
    DisplayPopup,
    DisplayFireAndForget,
    TestLookup,
    SynctexEditor,
    TestSynctex,
}

impl IntegrationsFocus {
    fn next(self) -> Self {
        match self {
            Self::LookupCommand => Self::DisplayPopup,
            Self::DisplayPopup => Self::DisplayFireAndForget,
            Self::DisplayFireAndForget => Self::TestLookup,
            Self::TestLookup => Self::SynctexEditor,
            Self::SynctexEditor => Self::TestSynctex,
            Self::TestSynctex => Self::LookupCommand,
        }
    }
    fn prev(self) -> Self {
        match self {
            Self::LookupCommand => Self::TestSynctex,
            Self::DisplayPopup => Self::LookupCommand,
            Self::DisplayFireAndForget => Self::DisplayPopup,
            Self::TestLookup => Self::DisplayFireAndForget,
            Self::SynctexEditor => Self::TestLookup,
            Self::TestSynctex => Self::SynctexEditor,
        }
    }
    fn is_text_input(self) -> bool {
        matches!(self, Self::LookupCommand | Self::SynctexEditor)
    }
}

pub struct SettingsPopup {
    current_tab: SettingsTab,
    // PDF tab state
    pdf_selected_idx: usize,
    supports_scroll_mode: bool,
    supports_graphics: bool,
    // Themes tab state
    theme_selected_idx: usize,
    theme_names: Vec<String>,
    // Integrations tab state
    integrations_focus: IntegrationsFocus,
    lookup_command_input: crate::vendored::tui_textarea::TextArea<'static>,
    lookup_display_selected: LookupDisplay,
    synctex_editor_input: crate::vendored::tui_textarea::TextArea<'static>,
    // Click targets: stored during render for mouse hit-testing
    tab_area: Option<Rect>,
    content_chunks: Vec<Rect>,
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
        let caps = terminal::detect_terminal_with_probe();
        Self::new_with_caps(tab, caps.supports_graphics, caps.pdf.supports_scroll_mode)
    }

    pub fn new_with_caps(
        tab: SettingsTab,
        supports_graphics: bool,
        supports_scroll_mode: bool,
    ) -> Self {
        let pdf_selected_idx = if supports_graphics { 0 } else { 2 };
        let current_tab = if cfg!(feature = "pdf") {
            tab
        } else {
            SettingsTab::Themes
        };

        let theme_names = all_theme_names();

        let mut lookup_command_input = crate::vendored::tui_textarea::TextArea::default();
        lookup_command_input.set_placeholder_text("e.g. dict {}");
        lookup_command_input.set_cursor_line_style(Style::default());
        if let Some(cmd) = get_lookup_command() {
            lookup_command_input.insert_str(&cmd);
        }

        let mut synctex_editor_input = crate::vendored::tui_textarea::TextArea::default();
        synctex_editor_input.set_placeholder_text(
            "nvim --server /tmp/nvim.sock --remote-send '<C-\\><C-n>:e {file}<CR>:{line}<CR>'",
        );
        synctex_editor_input.set_cursor_line_style(Style::default());
        if let Some(cmd) = get_synctex_editor() {
            synctex_editor_input.insert_str(&cmd);
        }

        SettingsPopup {
            current_tab,
            pdf_selected_idx,
            supports_scroll_mode,
            supports_graphics,
            theme_selected_idx: 0,
            theme_names,
            integrations_focus: IntegrationsFocus::LookupCommand,
            lookup_command_input,
            lookup_display_selected: get_lookup_display(),
            synctex_editor_input,
            tab_area: None,
            content_chunks: Vec::new(),
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
        5
    }

    fn render_mode_available(&self) -> bool {
        self.supports_graphics && is_pdf_enabled()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 70, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let palette = current_theme();

        // Build footer hints string for bottom border
        let hints = if self.current_tab == SettingsTab::Integrations {
            " Tab switch tabs  ↑/↓ navigate  Enter select  Esc close "
        } else if cfg!(feature = "pdf") {
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
        self.tab_area = Some(main_chunks[0]);
        self.render_tabs(f, main_chunks[0], palette);

        // Render content based on selected tab
        let content_area = Rect {
            x: main_chunks[1].x,
            y: main_chunks[1].y + 1,
            width: main_chunks[1].width,
            height: main_chunks[1].height.saturating_sub(1),
        };

        match self.current_tab {
            SettingsTab::PdfSupport => self.render_pdf_tab(f, content_area, palette),
            SettingsTab::Themes => self.render_themes_tab(f, content_area, palette),
            SettingsTab::Integrations => self.render_integrations_tab(f, content_area, palette),
        }
    }

    fn render_tabs(&self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        let tab_names = ["PDF / DJVU", "Select Theme", "Integrations"];

        let mut spans = Vec::new();
        spans.push(Span::raw(" "));

        let tab_iter: Box<dyn Iterator<Item = (usize, &&str)>> = if cfg!(feature = "pdf") {
            Box::new(tab_names.iter().enumerate())
        } else {
            Box::new(tab_names.iter().enumerate().filter(|(idx, _)| *idx != 0))
        };

        for (idx, name) in tab_iter {
            let is_selected = matches!(
                (idx, self.current_tab),
                (0, SettingsTab::PdfSupport)
                    | (1, SettingsTab::Themes)
                    | (2, SettingsTab::Integrations)
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
                SettingsTab::PdfSupport => (1, 10),    // "PDF / DJVU"
                SettingsTab::Themes => (14, 12),       // "Select Theme"
                SettingsTab::Integrations => (29, 12), // "Integrations"
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
        let current_layout_mode = get_pdf_page_layout_mode();
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
                Constraint::Length(1), // Page Layout header
                Constraint::Length(1), // empty line
                Constraint::Length(2), // Page Layout options
                Constraint::Length(1), // spacer
                Constraint::Min(1),    // Info message
            ])
            .split(area);

        self.content_chunks = chunks.to_vec();

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
            Some("Kitty protocol")
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

        // Page Layout section header
        let layout_header_color = if render_mode_available {
            palette.base_06
        } else {
            palette.base_03
        };
        self.render_section_header(f, chunks[6], "Page Layout", palette, layout_header_color);

        // Page Layout options
        let layout_options_area = Rect {
            x: chunks[8].x + 2,
            y: chunks[8].y,
            width: chunks[8].width.saturating_sub(2),
            height: chunks[8].height,
        };

        let layout_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(layout_options_area);

        let layout_option_style = if render_mode_available {
            Style::default().fg(palette.base_06)
        } else {
            Style::default().fg(palette.base_03)
        };

        let single_radio = if current_layout_mode == PdfPageLayoutMode::Single {
            radio_selected
        } else {
            radio_unselected
        };
        let single_line = self.render_radio_option(
            single_radio,
            "Single",
            Some("one page"),
            layout_option_style,
            self.current_tab == SettingsTab::PdfSupport
                && self.pdf_selected_idx == 4
                && render_mode_available,
            palette,
        );
        f.render_widget(Paragraph::new(single_line), layout_chunks[0]);

        let dual_radio = if current_layout_mode == PdfPageLayoutMode::Dual {
            radio_selected
        } else {
            radio_unselected
        };
        let dual_line = self.render_radio_option(
            dual_radio,
            "Dual",
            Some("two pages"),
            layout_option_style,
            self.current_tab == SettingsTab::PdfSupport
                && self.pdf_selected_idx == 5
                && render_mode_available,
            palette,
        );
        f.render_widget(Paragraph::new(dual_line), layout_chunks[1]);

        // Info message
        let info_lines = self.get_pdf_info_lines(palette, pdf_enabled, current_mode);
        f.render_widget(Paragraph::new(info_lines), chunks[10]);
    }

    fn render_themes_tab(&mut self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        let theme_list_height = self.theme_names.len() as u16;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(theme_list_height), // Theme list
                Constraint::Length(1),                 // spacer
                Constraint::Length(1),                 // Background header
                Constraint::Length(1),                 // empty line
                Constraint::Length(2),                 // Transparent Background options
                Constraint::Min(0),                    // remaining space
            ])
            .split(area);

        self.content_chunks = chunks.to_vec();

        // Theme list (indices 0 to theme_names.len()-1)
        let current_theme_idx = current_theme_index();

        for (theme_idx, name) in self.theme_names.iter().enumerate() {
            let is_current = theme_idx == current_theme_idx;
            let is_selected = self.theme_selected_idx < self.theme_names.len()
                && self.theme_selected_idx == theme_idx;

            let line_area = Rect {
                x: chunks[0].x,
                y: chunks[0].y + theme_idx as u16,
                width: chunks[0].width,
                height: 1,
            };

            let line = self.render_theme_option(name, is_current, is_selected, palette);
            f.render_widget(Paragraph::new(line), line_area);
        }

        // Background section header
        self.render_section_header(f, chunks[2], "Background", palette, palette.base_06);

        // Transparent Background options (indices theme_names.len() and theme_names.len()+1)
        let transparent = is_transparent_background();
        let radio_selected = "●";
        let radio_unselected = "○";

        let trans_options_area = Rect {
            x: chunks[4].x + 2,
            y: chunks[4].y,
            width: chunks[4].width.saturating_sub(2),
            height: chunks[4].height,
        };

        let trans_options_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(trans_options_area);

        let theme_radio = if !transparent {
            radio_selected
        } else {
            radio_unselected
        };
        let theme_line = self.render_radio_option(
            theme_radio,
            "Theme color",
            None,
            Style::default().fg(palette.base_06),
            self.theme_selected_idx == self.theme_names.len(),
            palette,
        );
        f.render_widget(Paragraph::new(theme_line), trans_options_chunks[0]);

        let trans_radio = if transparent {
            radio_selected
        } else {
            radio_unselected
        };
        let trans_line = self.render_radio_option(
            trans_radio,
            "Transparent",
            None,
            Style::default().fg(palette.base_06),
            self.theme_selected_idx == self.theme_names.len() + 1,
            palette,
        );
        f.render_widget(Paragraph::new(trans_line), trans_options_chunks[1]);
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
                        "PDF / DJVU viewing requires a graphics-enabled terminal.",
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
                    "Enable PDF / DJVU support to change render mode",
                    Style::default().fg(palette.base_03),
                ),
            ])]
        } else if self.supports_scroll_mode && current_mode == PdfRenderMode::Scroll {
            vec![Line::from(vec![
                Span::styled("! ", Style::default().fg(palette.base_09)),
                Span::styled(
                    "Scroll mode uses 300-500MB memory",
                    Style::default().fg(palette.base_09),
                ),
            ])]
        } else if !self.supports_scroll_mode {
            vec![Line::from(vec![
                Span::styled("ⓘ ", Style::default().fg(palette.base_03)),
                Span::styled(
                    "Scroll mode requires Kitty graphics protocol",
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
        let mut next_idx = self.pdf_selected_idx;
        for _ in 0..=max_idx {
            next_idx = if next_idx >= max_idx {
                min_idx
            } else {
                next_idx + 1
            };
            if self.is_pdf_idx_selectable(next_idx) {
                self.pdf_selected_idx = next_idx;
                break;
            }
        }
    }

    fn pdf_previous(&mut self) {
        let min_idx = self.pdf_min_selectable_idx();
        let max_idx = self.pdf_max_selectable_idx();
        let mut prev_idx = self.pdf_selected_idx;
        for _ in 0..=max_idx {
            prev_idx = if prev_idx <= min_idx {
                max_idx
            } else {
                prev_idx - 1
            };
            if self.is_pdf_idx_selectable(prev_idx) {
                self.pdf_selected_idx = prev_idx;
                break;
            }
        }
    }

    fn is_pdf_idx_selectable(&self, idx: usize) -> bool {
        if !self.supports_graphics {
            return false;
        }
        match idx {
            0 | 1 => true,
            2 => self.render_mode_available(),
            3 => self.render_mode_available() && self.supports_scroll_mode,
            4 | 5 => self.render_mode_available(),
            _ => false,
        }
    }

    fn theme_max_idx(&self) -> usize {
        self.theme_names.len() + 1
    }

    fn theme_next(&mut self) {
        let max_idx = self.theme_max_idx();
        if self.theme_selected_idx >= max_idx {
            self.theme_selected_idx = 0;
        } else {
            self.theme_selected_idx += 1;
        }
    }

    fn theme_previous(&mut self) {
        if self.theme_selected_idx == 0 {
            self.theme_selected_idx = self.theme_max_idx();
        } else {
            self.theme_selected_idx -= 1;
        }
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
            4 if self.render_mode_available() => {
                if get_pdf_page_layout_mode() != PdfPageLayoutMode::Single {
                    set_pdf_page_layout_mode(PdfPageLayoutMode::Single);
                    return Some(SettingsAction::PageLayoutChanged);
                }
                None
            }
            5 if self.render_mode_available() => {
                if get_pdf_page_layout_mode() != PdfPageLayoutMode::Dual {
                    set_pdf_page_layout_mode(PdfPageLayoutMode::Dual);
                    return Some(SettingsAction::PageLayoutChanged);
                }
                None
            }
            _ => None,
        }
    }

    fn apply_theme_selected(&self) -> Option<SettingsAction> {
        let theme_count = self.theme_names.len();
        if self.theme_selected_idx < theme_count {
            // Theme selection
            if self.theme_selected_idx != current_theme_index() {
                set_theme_by_index_and_save(self.theme_selected_idx);
                return Some(SettingsAction::SettingsChanged);
            }
        } else if self.theme_selected_idx == theme_count {
            // "Theme color" option - disable transparency
            if is_transparent_background() {
                set_transparent_background(false);
                return Some(SettingsAction::SettingsChanged);
            }
        } else if self.theme_selected_idx == theme_count + 1 {
            // "Transparent" option - enable transparency
            if !is_transparent_background() {
                set_transparent_background(true);
                return Some(SettingsAction::SettingsChanged);
            }
        }
        None
    }

    fn render_integrations_tab(&mut self, f: &mut Frame, area: Rect, palette: &Base16Palette) {
        let hint_style = Style::default().fg(palette.base_03);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // 0:  Lookup header
                Constraint::Length(3), // 1:  Lookup command input
                Constraint::Length(3), // 2:  Lookup hints (with blank line after first)
                Constraint::Length(1), // 3:  Display mode header
                Constraint::Length(1), // 4:  ● Popup
                Constraint::Length(1), // 5:  ○ Fire and forget
                Constraint::Length(1), // 6:  spacing
                Constraint::Length(1), // 7:  Test lookup button
                Constraint::Length(1), // 8:  spacing
                Constraint::Length(1), // 9:  SyncTeX header
                Constraint::Length(3), // 10: SyncTeX editor input
                Constraint::Length(9), // 11: SyncTeX hints
                Constraint::Length(1), // 12: spacing
                Constraint::Length(1), // 13: Test synctex button
                Constraint::Min(0),    // 14: padding
            ])
            .split(area);

        self.content_chunks = chunks.to_vec();

        // -- Dictionary Lookup section --
        self.render_section_header(
            f,
            chunks[0],
            "Dictionary Lookup (Space+l)",
            palette,
            palette.base_06,
        );

        let lookup_focused = self.integrations_focus == IntegrationsFocus::LookupCommand;
        let lookup_border_color = if lookup_focused {
            palette.base_0d
        } else {
            palette.base_02
        };
        self.lookup_command_input.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(lookup_border_color)),
        );
        self.lookup_command_input
            .set_style(Style::default().fg(palette.base_05));
        f.render_widget(&self.lookup_command_input, chunks[1]);

        let lookup_hints = vec![
            Line::from(Span::styled(
                "  {} = selected word, e.g: dict {}",
                hint_style,
            )),
            Line::from(Span::styled("", hint_style)),
            Line::from(Span::styled(
                "  e.g: open \"https://www.merriam-webster.com/dictionary/{}\"",
                hint_style,
            )),
        ];
        f.render_widget(Paragraph::new(lookup_hints), chunks[2]);

        // Display mode (vertical radio buttons)
        self.render_section_header(f, chunks[3], "Display mode:", palette, palette.base_04);

        let radio_style = Style::default().fg(palette.base_05);
        let popup_selected = self.lookup_display_selected == LookupDisplay::Popup;

        let popup_radio = if popup_selected { "●" } else { "○" };
        let popup_line = self.render_radio_option(
            popup_radio,
            "Popup (show output)",
            None,
            radio_style,
            self.integrations_focus == IntegrationsFocus::DisplayPopup,
            palette,
        );
        f.render_widget(Paragraph::new(popup_line), chunks[4]);

        let faf_radio = if popup_selected { "○" } else { "●" };
        let faf_line = self.render_radio_option(
            faf_radio,
            "Fire and forget",
            None,
            radio_style,
            self.integrations_focus == IntegrationsFocus::DisplayFireAndForget,
            palette,
        );
        f.render_widget(Paragraph::new(faf_line), chunks[5]);

        // chunks[6] = spacing
        self.render_test_button(
            f,
            chunks[7],
            "Test",
            "lookup word \"hello\"",
            self.integrations_focus == IntegrationsFocus::TestLookup,
            palette,
        );

        // chunks[8] = spacing

        // -- SyncTeX section --
        self.render_section_header(
            f,
            chunks[9],
            "SyncTeX Editor (Ctrl+click, right-click / gd)",
            palette,
            palette.base_06,
        );

        let synctex_focused = self.integrations_focus == IntegrationsFocus::SynctexEditor;
        let synctex_border_color = if synctex_focused {
            palette.base_0d
        } else {
            palette.base_02
        };
        self.synctex_editor_input.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(synctex_border_color)),
        );
        self.synctex_editor_input
            .set_style(Style::default().fg(palette.base_05));
        f.render_widget(&self.synctex_editor_input, chunks[10]);

        let synctex_hints = vec![
            Line::from(Span::styled(
                "  {file}, {line}, {column} = source location",
                hint_style,
            )),
            Line::from(Span::styled("", hint_style)),
            Line::from(Span::styled(
                "  Neovim: start with nvim --listen /tmp/nvim.sock",
                hint_style,
            )),
            Line::from(Span::styled(
                "  bookokrat → nvim (Ctrl+click, right-click / gd):",
                hint_style,
            )),
            Line::from(Span::styled(
                "    nvim --server /tmp/nvim.sock --remote-send '<C-\\><C-n>:e {file}<CR>:{line}<CR>'",
                hint_style,
            )),
            Line::from(Span::styled(
                "  nvim → bookokrat (VimTeX \\lv):",
                hint_style,
            )),
            Line::from(Span::styled(
                "    let g:vimtex_view_general_viewer = 'bookokrat'",
                hint_style,
            )),
            Line::from(Span::styled(
                "    let g:vimtex_view_general_options = '--synctex-forward @line:@col:@tex @pdf'",
                hint_style,
            )),
        ];
        f.render_widget(Paragraph::new(synctex_hints), chunks[11]);

        // chunks[12] = spacing
        self.render_test_button(
            f,
            chunks[13],
            "Test",
            "open /tmp/synctex_test.txt:1",
            self.integrations_focus == IntegrationsFocus::TestSynctex,
            palette,
        );
    }

    fn render_test_button(
        &self,
        f: &mut Frame,
        area: Rect,
        label: &str,
        description: &str,
        focused: bool,
        palette: &Base16Palette,
    ) {
        let btn_style = if focused {
            Style::default()
                .fg(palette.base_01)
                .bg(palette.base_0d)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.base_05).bg(palette.base_02)
        };
        let desc_style = Style::default().fg(palette.base_03);
        let cursor = if focused {
            Span::styled(
                "» ",
                Style::default()
                    .fg(palette.base_0a)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("  ")
        };
        let line = Line::from(vec![
            cursor,
            Span::styled(format!(" {label} "), btn_style),
            Span::styled(format!("  {description}"), desc_style),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn apply_integrations_selected(&mut self) -> Option<SettingsAction> {
        match self.integrations_focus {
            IntegrationsFocus::DisplayPopup => {
                self.lookup_display_selected = LookupDisplay::Popup;
                set_lookup_display(LookupDisplay::Popup);
                Some(SettingsAction::SettingsChanged)
            }
            IntegrationsFocus::DisplayFireAndForget => {
                self.lookup_display_selected = LookupDisplay::FireAndForget;
                set_lookup_display(LookupDisplay::FireAndForget);
                Some(SettingsAction::SettingsChanged)
            }
            IntegrationsFocus::TestLookup => {
                self.save_integrations();
                Some(SettingsAction::TestLookupCommand)
            }
            IntegrationsFocus::TestSynctex => {
                self.save_integrations();
                Some(SettingsAction::TestSynctexEditor)
            }
            _ => None,
        }
    }

    pub fn handle_mouse_click(&mut self, col: u16, row: u16) -> Option<SettingsAction> {
        // Tab bar clicks
        if let Some(tab_area) = self.tab_area {
            if row >= tab_area.y && row < tab_area.y + tab_area.height {
                let rel_x = col.saturating_sub(tab_area.x);
                // Tab positions: " PDF / DJVU   Select Theme   Integrations"
                //                  1..11         14..26          29..41
                if cfg!(feature = "pdf") && rel_x >= 1 && rel_x < 14 {
                    if self.current_tab == SettingsTab::Integrations {
                        self.save_integrations();
                    }
                    self.current_tab = SettingsTab::PdfSupport;
                    return None;
                } else if rel_x >= 14 && rel_x < 29 {
                    if self.current_tab == SettingsTab::Integrations {
                        self.save_integrations();
                    }
                    self.current_tab = SettingsTab::Themes;
                    return None;
                } else if rel_x >= 29 {
                    if self.current_tab == SettingsTab::Integrations {
                        self.save_integrations();
                    }
                    self.current_tab = SettingsTab::Integrations;
                    return None;
                }
            }
        }

        if self.content_chunks.is_empty() {
            return None;
        }

        let chunks = &self.content_chunks;
        let hit = |idx: usize| -> bool {
            if let Some(r) = chunks.get(idx) {
                col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
            } else {
                false
            }
        };

        match self.current_tab {
            SettingsTab::PdfSupport => {
                // PDF chunks: 0: enable/disable, 4: render mode, 8: page layout
                // Sub-rows within chunk 0 (height=2): row 0 = enabled, row 1 = disabled
                if let Some(r) = chunks.get(0) {
                    if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
                        let sub = (row - r.y) as usize;
                        self.pdf_selected_idx = sub;
                        return self.apply_pdf_selected();
                    }
                }
                // Render mode options (chunk 4, height=2)
                if let Some(r) = chunks.get(4) {
                    if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
                        let sub = (row - r.y) as usize;
                        self.pdf_selected_idx = 2 + sub;
                        return self.apply_pdf_selected();
                    }
                }
                // Page layout options (chunk 8, height=2)
                if let Some(r) = chunks.get(8) {
                    if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
                        let sub = (row - r.y) as usize;
                        self.pdf_selected_idx = 4 + sub;
                        return self.apply_pdf_selected();
                    }
                }
            }
            SettingsTab::Themes => {
                // Theme list is in chunk 0 — each theme is one row
                if let Some(r) = chunks.get(0) {
                    if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
                        let idx = (row - r.y) as usize;
                        if idx < self.theme_names.len() {
                            self.theme_selected_idx = idx;
                            return self.apply_theme_selected();
                        }
                    }
                }
                // Background options in chunk 4 (height=2)
                if let Some(r) = chunks.get(4) {
                    if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
                        let sub = (row - r.y) as usize;
                        self.theme_selected_idx = self.theme_names.len() + sub;
                        return self.apply_theme_selected();
                    }
                }
            }
            SettingsTab::Integrations => {
                // 0: header, 1: input, 2: hints
                // 3: display header, 4: popup radio, 5: faf radio
                // 6: spacing, 7: test lookup, 8: spacing
                // 9: synctex header, 10: input, 11: hints
                // 12: spacing, 13: test synctex
                if hit(1) {
                    self.integrations_focus = IntegrationsFocus::LookupCommand;
                } else if hit(4) {
                    self.integrations_focus = IntegrationsFocus::DisplayPopup;
                    return self.apply_integrations_selected();
                } else if hit(5) {
                    self.integrations_focus = IntegrationsFocus::DisplayFireAndForget;
                    return self.apply_integrations_selected();
                } else if hit(7) {
                    self.integrations_focus = IntegrationsFocus::TestLookup;
                    return self.apply_integrations_selected();
                } else if hit(10) {
                    self.integrations_focus = IntegrationsFocus::SynctexEditor;
                } else if hit(13) {
                    self.integrations_focus = IntegrationsFocus::TestSynctex;
                    return self.apply_integrations_selected();
                }
            }
        }
        None
    }

    fn save_integrations(&self) {
        let lookup_text: String = self
            .lookup_command_input
            .lines()
            .first()
            .cloned()
            .unwrap_or_default();
        let lookup_cmd = if lookup_text.trim().is_empty() {
            None
        } else {
            Some(lookup_text)
        };
        set_lookup_command(lookup_cmd);
        set_lookup_display(self.lookup_display_selected);

        let synctex_text: String = self
            .synctex_editor_input
            .lines()
            .first()
            .cloned()
            .unwrap_or_default();
        let synctex_cmd = if synctex_text.trim().is_empty() {
            None
        } else {
            Some(synctex_text)
        };
        set_synctex_editor(synctex_cmd);
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<SettingsAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // When a text input is focused on the Integrations tab, route most
        // keys to the TextArea. Only Esc, Tab, and Up/Down escape.
        if self.current_tab == SettingsTab::Integrations && self.integrations_focus.is_text_input()
        {
            match key.code {
                KeyCode::Esc => {
                    self.save_integrations();
                    return Some(SettingsAction::Close);
                }
                KeyCode::Tab => {
                    self.save_integrations();
                    self.current_tab = self.current_tab.next();
                    return None;
                }
                KeyCode::BackTab => {
                    self.save_integrations();
                    self.current_tab = self.current_tab.prev();
                    return None;
                }
                KeyCode::Down => {
                    self.integrations_focus = self.integrations_focus.next();
                    return None;
                }
                KeyCode::Up => {
                    self.integrations_focus = self.integrations_focus.prev();
                    return None;
                }
                KeyCode::Enter => {
                    return None; // Don't insert newlines in single-line inputs
                }
                _ => {
                    if let Some(input) = crate::inputs::text_area_utils::map_keys_to_input(key) {
                        match self.integrations_focus {
                            IntegrationsFocus::LookupCommand => {
                                self.lookup_command_input.input(input);
                            }
                            IntegrationsFocus::SynctexEditor => {
                                self.synctex_editor_input.input(input);
                            }
                            _ => {}
                        }
                    }
                    return None;
                }
            }
        }

        match key.code {
            KeyCode::Tab => {
                if self.current_tab == SettingsTab::Integrations {
                    self.save_integrations();
                }
                self.current_tab = self.current_tab.next();
                None
            }
            KeyCode::BackTab => {
                if self.current_tab == SettingsTab::Integrations {
                    self.save_integrations();
                }
                self.current_tab = self.current_tab.prev();
                None
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.current_tab == SettingsTab::Integrations {
                    self.save_integrations();
                }
                self.current_tab = self.current_tab.prev();
                None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.current_tab == SettingsTab::Integrations {
                    self.save_integrations();
                }
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
            KeyCode::Esc => {
                if self.current_tab == SettingsTab::Integrations {
                    self.save_integrations();
                }
                Some(SettingsAction::Close)
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.current_tab {
                SettingsTab::PdfSupport => self.apply_pdf_selected(),
                SettingsTab::Themes => self.apply_theme_selected(),
                SettingsTab::Integrations => self.apply_integrations_selected(),
            },
            _ => None,
        }
    }
}

impl Popup for SettingsPopup {
    fn get_last_popup_area(&self) -> Option<Rect> {
        return self.last_popup_area;
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
            SettingsTab::Integrations => {
                self.integrations_focus = self.integrations_focus.next();
            }
        }
    }

    fn handle_k(&mut self) {
        match self.current_tab {
            SettingsTab::PdfSupport => self.pdf_previous(),
            SettingsTab::Themes => self.theme_previous(),
            SettingsTab::Integrations => {
                self.integrations_focus = self.integrations_focus.prev();
            }
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

    fn handle_ctrl_f(&mut self) {
        if self.current_tab == SettingsTab::Themes {
            for _ in 0..10 {
                self.theme_next();
            }
        }
    }

    fn handle_ctrl_b(&mut self) {
        if self.current_tab == SettingsTab::Themes {
            for _ in 0..10 {
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
                self.theme_selected_idx = 0;
            }
            SettingsTab::Integrations => {
                self.integrations_focus = IntegrationsFocus::LookupCommand;
            }
        }
    }

    fn handle_upper_g(&mut self) {
        match self.current_tab {
            SettingsTab::PdfSupport => {
                self.pdf_selected_idx = self.pdf_max_selectable_idx();
            }
            SettingsTab::Themes => {
                self.theme_selected_idx = self.theme_max_idx();
            }
            SettingsTab::Integrations => {
                self.integrations_focus = IntegrationsFocus::TestSynctex;
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
