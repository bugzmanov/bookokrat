use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::settings::{
    EpubColumnMode, LookupDisplay, PdfPageLayoutMode, PdfRenderMode, get_epub_column_mode,
    get_lookup_command, get_lookup_display, get_pdf_page_layout_mode, get_pdf_render_mode,
    get_synctex_editor, is_invert_scroll_direction, is_pdf_enabled, is_transparent_background,
    is_zen_hide_border, set_epub_column_mode, set_integrations, set_invert_scroll_direction,
    set_lookup_display, set_pdf_enabled, set_pdf_page_layout_mode, set_pdf_render_mode,
    set_transparent_background, set_zen_hide_border,
};
use crate::terminal;
use crate::theme::{
    Base16Palette, all_theme_names, current_theme, current_theme_index, set_theme_by_index_and_save,
};
use crate::widget::popup::Popup;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget,
    },
};

pub enum SettingsAction {
    Close,
    SettingsChanged,
    PageLayoutChanged,
    ZenBorderChanged,
    RenderModeChanged,
    TestLookupCommand,
    TestSynctexEditor,
}

const TRUECOLOR_NOTE_LINES: u16 = 4;
const TRUECOLOR_NOTE_SPACER_LINES: u16 = 1;
const GENERAL_TWO_COLUMN_MIN_WIDTH: u16 = 76;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Themes,
    Integrations,
}

impl SettingsTab {
    fn next(self) -> Self {
        match self {
            SettingsTab::General => SettingsTab::Themes,
            SettingsTab::Themes => SettingsTab::Integrations,
            SettingsTab::Integrations => {
                if cfg!(feature = "pdf") {
                    SettingsTab::General
                } else {
                    SettingsTab::Themes
                }
            }
        }
    }

    fn prev(self) -> Self {
        match self {
            SettingsTab::General => SettingsTab::Integrations,
            SettingsTab::Themes => {
                if cfg!(feature = "pdf") {
                    SettingsTab::General
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneralOption {
    PdfEnabled,
    PdfDisabled,
    PdfRenderPage,
    PdfRenderScroll,
    PdfLayoutSingle,
    PdfLayoutDual,
    MouseWheelNormal,
    MouseWheelInverted,
    EpubSingle,
    EpubDual,
    ZenBorderShown,
    ZenBorderHidden,
}

const GENERAL_OPTIONS: [GeneralOption; 12] = [
    GeneralOption::PdfEnabled,
    GeneralOption::PdfDisabled,
    GeneralOption::PdfRenderPage,
    GeneralOption::PdfRenderScroll,
    GeneralOption::PdfLayoutSingle,
    GeneralOption::PdfLayoutDual,
    GeneralOption::MouseWheelNormal,
    GeneralOption::MouseWheelInverted,
    GeneralOption::EpubSingle,
    GeneralOption::EpubDual,
    GeneralOption::ZenBorderShown,
    GeneralOption::ZenBorderHidden,
];

impl GeneralOption {
    fn index(self) -> usize {
        match self {
            Self::PdfEnabled => 0,
            Self::PdfDisabled => 1,
            Self::PdfRenderPage => 2,
            Self::PdfRenderScroll => 3,
            Self::PdfLayoutSingle => 4,
            Self::PdfLayoutDual => 5,
            Self::MouseWheelNormal => 6,
            Self::MouseWheelInverted => 7,
            Self::EpubSingle => 8,
            Self::EpubDual => 9,
            Self::ZenBorderShown => 10,
            Self::ZenBorderHidden => 11,
        }
    }

    fn is_pdf_option(self) -> bool {
        matches!(
            self,
            Self::PdfEnabled
                | Self::PdfDisabled
                | Self::PdfRenderPage
                | Self::PdfRenderScroll
                | Self::PdfLayoutSingle
                | Self::PdfLayoutDual
        )
    }
}

struct SettingsOption {
    id: GeneralOption,
    label: &'static str,
    hint: Option<&'static str>,
    selected: bool,
    enabled: bool,
}

struct SettingsSection {
    title: &'static str,
    title_indent: u16,
    options_indent: u16,
    options_top_spacing: u16,
    enabled: bool,
    options: Vec<SettingsOption>,
}

impl SettingsSection {
    fn height(&self) -> u16 {
        1 + self.options_top_spacing + self.options.len() as u16
    }
}

pub struct SettingsPopup {
    current_tab: SettingsTab,
    // General tab state
    general_selected: GeneralOption,
    // PDF section state
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
    // Click targets: stored during render for mouse hit-testing.
    // content_chunks are stored in virtual buffer coords (origin at content_buf_origin),
    // since content is rendered to an off-screen buffer when it overflows the viewport.
    tab_area: Option<Rect>,
    content_chunks: Vec<Rect>,
    // Origin of the virtual content buffer (matches visible content area's x/y).
    content_buf_origin: (u16, u16),
    // Visible content viewport (on-screen area where content is shown).
    content_viewport: Option<Rect>,
    // Vertical scroll offset into the virtual content buffer (in rows).
    scroll_offset: u16,
    // Total natural content height for current tab (set during render).
    content_total_height: u16,
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
            Self::new_with_tab(SettingsTab::General)
        } else {
            Self::new_with_tab(SettingsTab::Themes)
        }
    }

    pub fn current_tab(&self) -> SettingsTab {
        self.current_tab
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
        let current_tab = if cfg!(feature = "pdf") {
            tab
        } else {
            SettingsTab::Themes
        };

        let general_selected = Self::initial_general_selected_from_state(
            supports_graphics,
            supports_scroll_mode,
            is_pdf_enabled(),
            get_pdf_render_mode(),
            get_pdf_page_layout_mode(),
        );
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
            general_selected,
            supports_scroll_mode,
            supports_graphics,
            theme_selected_idx: Self::initial_theme_selected_idx_from_state(
                theme_names.len(),
                current_theme_index(),
                is_transparent_background(),
            ),
            theme_names,
            integrations_focus: IntegrationsFocus::LookupCommand,
            lookup_command_input,
            lookup_display_selected: get_lookup_display(),
            synctex_editor_input,
            tab_area: None,
            content_chunks: Vec::new(),
            content_buf_origin: (0, 0),
            content_viewport: None,
            scroll_offset: 0,
            content_total_height: 0,
            last_popup_area: None,
        }
    }

    fn initial_general_selected_from_state(
        supports_graphics: bool,
        supports_scroll_mode: bool,
        pdf_enabled: bool,
        render_mode: PdfRenderMode,
        layout_mode: PdfPageLayoutMode,
    ) -> GeneralOption {
        if !supports_graphics {
            return GeneralOption::MouseWheelNormal;
        }

        // PDF section: non-default → focus it
        if !pdf_enabled {
            return GeneralOption::PdfDisabled;
        }
        if render_mode == PdfRenderMode::Scroll && supports_scroll_mode {
            return GeneralOption::PdfRenderScroll;
        }
        if layout_mode == PdfPageLayoutMode::Dual {
            return GeneralOption::PdfLayoutDual;
        }
        GeneralOption::PdfEnabled
    }

    fn initial_theme_selected_idx_from_state(
        theme_count: usize,
        current_theme_idx: usize,
        transparent_background: bool,
    ) -> usize {
        if transparent_background {
            return theme_count + 1;
        }
        if theme_count == 0 {
            return 0;
        }
        current_theme_idx.min(theme_count - 1)
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
        } else if self.current_tab == SettingsTab::General {
            " Tab switch tabs  h/l columns  j/k navigate  Enter select  Esc close "
        } else if cfg!(feature = "pdf") {
            " Tab switch tabs  j/k navigate  Enter select  Esc close "
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

        // Render tabs directly into frame (tabs do not scroll)
        self.tab_area = Some(main_chunks[0]);
        self.render_tabs(f.buffer_mut(), main_chunks[0], palette);

        // Visible content viewport on screen
        let viewport = Rect {
            x: main_chunks[1].x,
            y: main_chunks[1].y + 1,
            width: main_chunks[1].width,
            height: main_chunks[1].height.saturating_sub(1),
        };
        self.content_viewport = Some(viewport);

        if viewport.width == 0 || viewport.height == 0 {
            return;
        }

        // Compute natural height for current tab. Scrollbar lives on the
        // popup border, so the content can use the full viewport width.
        let natural_height = self.compute_natural_height(viewport.width);
        let needs_scroll = natural_height > viewport.height;
        let content_width = viewport.width;

        // Off-screen buffer sized to natural content height. Origin matches
        // viewport so layouts produce on-screen-looking coordinates which we
        // then translate when blitting.
        let buf_area = Rect {
            x: viewport.x,
            y: viewport.y,
            width: content_width,
            height: natural_height.max(viewport.height),
        };
        self.content_buf_origin = (buf_area.x, buf_area.y);
        self.content_total_height = natural_height;

        let mut content_buf = Buffer::empty(buf_area);
        // Match popup background so unfilled cells don't show through with default colors.
        for cell in content_buf.content.iter_mut() {
            cell.set_style(Style::default().bg(palette.base_00));
        }

        match self.current_tab {
            SettingsTab::General => self.render_general_tab(&mut content_buf, buf_area, palette),
            SettingsTab::Themes => self.render_themes_tab(&mut content_buf, buf_area, palette),
            SettingsTab::Integrations => {
                self.render_integrations_tab(&mut content_buf, buf_area, palette)
            }
        }

        // Auto-scroll: ensure the currently selected row is in view.
        let max_scroll = natural_height.saturating_sub(viewport.height);
        if let Some((sel_y, sel_h)) = self.selected_row_range() {
            let sel_top = sel_y.saturating_sub(buf_area.y);
            let sel_bottom = sel_top + sel_h;
            if sel_top < self.scroll_offset {
                self.scroll_offset = sel_top;
            } else if sel_bottom > self.scroll_offset + viewport.height {
                self.scroll_offset = sel_bottom.saturating_sub(viewport.height);
            }
        }
        self.scroll_offset = self.scroll_offset.min(max_scroll);

        // Blit visible portion of the off-screen buffer to the frame.
        let frame_buf = f.buffer_mut();
        for vy in 0..viewport.height {
            let src_y = buf_area.y + self.scroll_offset + vy;
            if src_y >= buf_area.y + buf_area.height {
                break;
            }
            for vx in 0..content_width {
                let src_x = buf_area.x + vx;
                let cell = content_buf[(src_x, src_y)].clone();
                frame_buf[(viewport.x + vx, viewport.y + vy)] = cell;
            }
        }

        // Render scrollbar on the popup's right border, spanning the content
        // viewport vertically. Position reflects the cursor (selected option),
        // not just the scroll offset.
        if needs_scroll {
            let cursor_pos = self
                .selected_row_range()
                .map(|(y, _)| y.saturating_sub(buf_area.y) as usize)
                .unwrap_or(self.scroll_offset as usize);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(palette.base_04))
                .begin_symbol(None)
                .end_symbol(None);
            let mut scrollbar_state = ScrollbarState::new(natural_height as usize)
                .viewport_content_length(viewport.height as usize)
                .position(cursor_pos);
            let scrollbar_area = Rect {
                x: popup_area.x,
                y: viewport.y,
                width: popup_area.width,
                height: viewport.height,
            };
            f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }

    fn general_uses_columns(width: u16) -> bool {
        width >= GENERAL_TWO_COLUMN_MIN_WIDTH
    }

    fn general_natural_height(&self, width: u16) -> u16 {
        let pdf_sections = self.general_pdf_sections();
        let reader_sections = self.general_reader_sections();
        let info_h = self
            .get_pdf_info_lines(current_theme(), is_pdf_enabled(), get_pdf_render_mode())
            .len() as u16;
        let pdf_info_h = if info_h > 0 { 1 + info_h } else { 0 };
        let pdf_h = Self::settings_sections_height(&pdf_sections) + pdf_info_h;
        let reader_h = Self::settings_sections_height(&reader_sections);
        if Self::general_uses_columns(width) {
            pdf_h.max(reader_h)
        } else {
            pdf_h + 1 + reader_h
        }
    }

    /// Compute the natural (un-scrolled) height of content for the current tab.
    fn compute_natural_height(&self, width: u16) -> u16 {
        match self.current_tab {
            SettingsTab::General => self.general_natural_height(width),
            SettingsTab::Themes => {
                let truecolor_note_height = if crate::color_mode::supports_true_color() {
                    0
                } else {
                    TRUECOLOR_NOTE_SPACER_LINES + TRUECOLOR_NOTE_LINES
                };
                self.theme_names.len() as u16 + 1 + 1 + 1 + 2 + truecolor_note_height
            }
            SettingsTab::Integrations => {
                // 1+3+3+1+1+1+1+1+1+1+3+9+1+1 = 28
                28
            }
        }
    }

    /// Returns (virtual_y, height) of the currently selected row in the
    /// off-screen content buffer (absolute coords, matching content_chunks).
    fn selected_row_range(&self) -> Option<(u16, u16)> {
        match self.current_tab {
            SettingsTab::General => {
                let r = self.content_chunks.get(self.general_selected.index())?;
                Some((r.y, r.height))
            }
            SettingsTab::Themes => {
                let theme_count = self.theme_names.len();
                if self.theme_selected_idx < theme_count {
                    let r = self.content_chunks.first()?;
                    Some((r.y + self.theme_selected_idx as u16, 1))
                } else {
                    let sub = self.theme_selected_idx - theme_count;
                    let r = self.content_chunks.get(4)?;
                    Some((r.y + sub as u16, 1))
                }
            }
            SettingsTab::Integrations => {
                let chunk_idx = match self.integrations_focus {
                    IntegrationsFocus::LookupCommand => 1,
                    IntegrationsFocus::DisplayPopup => 4,
                    IntegrationsFocus::DisplayFireAndForget => 5,
                    IntegrationsFocus::TestLookup => 7,
                    IntegrationsFocus::SynctexEditor => 10,
                    IntegrationsFocus::TestSynctex => 13,
                };
                let r = self.content_chunks.get(chunk_idx)?;
                Some((r.y, r.height))
            }
        }
    }

    fn render_tabs(&self, buf: &mut Buffer, area: Rect, palette: &Base16Palette) {
        let tab_names = ["General", "Select Theme", "Integrations"];

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
                (0, SettingsTab::General)
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
        Paragraph::new(tabs_line).render(area, buf);

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
                SettingsTab::General => (1, 7),        // "General"
                SettingsTab::Themes => (11, 12),       // "Select Theme"
                SettingsTab::Integrations => (26, 12), // "Integrations"
            };

            let mut underline_spans = vec![Span::raw(" ".repeat(underline_x))];
            underline_spans.push(Span::styled(
                "─".repeat(underline_len),
                Style::default().fg(palette.base_0d),
            ));

            Paragraph::new(Line::from(underline_spans)).render(underline_area, buf);
        }
    }

    fn render_general_tab(&mut self, buf: &mut Buffer, area: Rect, palette: &Base16Palette) {
        self.content_chunks = vec![Rect::ZERO; GENERAL_OPTIONS.len()];

        let pdf_sections = self.general_pdf_sections();
        let reader_sections = self.general_reader_sections();
        let pdf_info_lines =
            self.get_pdf_info_lines(palette, is_pdf_enabled(), get_pdf_render_mode());
        let pdf_sections_height = Self::settings_sections_height(&pdf_sections);
        let pdf_info_height = if pdf_info_lines.is_empty() {
            0
        } else {
            1 + pdf_info_lines.len() as u16
        };

        if Self::general_uses_columns(area.width) {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Length(4),
                    Constraint::Percentage(50),
                ])
                .split(area);
            self.render_settings_sections(buf, columns[0], palette, &pdf_sections);
            Self::render_settings_info_lines(buf, columns[0], pdf_sections_height, pdf_info_lines);
            self.render_settings_sections(buf, columns[2], palette, &reader_sections);
        } else {
            let pdf_group_height = pdf_sections_height + pdf_info_height;
            let reader_group_height = Self::settings_sections_height(&reader_sections);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(pdf_group_height),
                    Constraint::Length(1),
                    Constraint::Length(reader_group_height),
                    Constraint::Min(0),
                ])
                .split(area);
            self.render_settings_sections(buf, chunks[0], palette, &pdf_sections);
            Self::render_settings_info_lines(buf, chunks[0], pdf_sections_height, pdf_info_lines);
            self.render_settings_sections(buf, chunks[2], palette, &reader_sections);
        }
    }

    fn general_pdf_sections(&self) -> Vec<SettingsSection> {
        let pdf_enabled = is_pdf_enabled();
        let current_mode = get_pdf_render_mode();
        let current_layout_mode = get_pdf_page_layout_mode();
        let effective_pdf_enabled = self.supports_graphics && pdf_enabled;
        let render_mode_available = self.render_mode_available();
        let scroll_suffix = if !self.supports_scroll_mode {
            Some("Kitty protocol")
        } else {
            Some("continuous scroll")
        };

        vec![
            SettingsSection {
                title: "PDF / DJVU",
                title_indent: 0,
                options_indent: 2,
                options_top_spacing: 0,
                enabled: self.supports_graphics,
                options: vec![
                    SettingsOption {
                        id: GeneralOption::PdfEnabled,
                        label: "Enabled",
                        hint: None,
                        selected: effective_pdf_enabled,
                        enabled: self.supports_graphics,
                    },
                    SettingsOption {
                        id: GeneralOption::PdfDisabled,
                        label: "Disabled",
                        hint: None,
                        selected: !effective_pdf_enabled,
                        enabled: self.supports_graphics,
                    },
                ],
            },
            SettingsSection {
                title: "Render Mode",
                title_indent: 2,
                options_indent: 4,
                options_top_spacing: 1,
                enabled: render_mode_available,
                options: vec![
                    SettingsOption {
                        id: GeneralOption::PdfRenderPage,
                        label: "Page",
                        hint: Some("one page at a time"),
                        selected: current_mode == PdfRenderMode::Page,
                        enabled: render_mode_available,
                    },
                    SettingsOption {
                        id: GeneralOption::PdfRenderScroll,
                        label: "Scroll",
                        hint: scroll_suffix,
                        selected: current_mode == PdfRenderMode::Scroll,
                        enabled: render_mode_available && self.supports_scroll_mode,
                    },
                ],
            },
            SettingsSection {
                title: "Page Layout",
                title_indent: 2,
                options_indent: 4,
                options_top_spacing: 1,
                enabled: render_mode_available,
                options: vec![
                    SettingsOption {
                        id: GeneralOption::PdfLayoutSingle,
                        label: "Single",
                        hint: Some("one page"),
                        selected: current_layout_mode == PdfPageLayoutMode::Single,
                        enabled: render_mode_available,
                    },
                    SettingsOption {
                        id: GeneralOption::PdfLayoutDual,
                        label: "Dual",
                        hint: Some("two pages"),
                        selected: current_layout_mode == PdfPageLayoutMode::Dual,
                        enabled: render_mode_available,
                    },
                ],
            },
        ]
    }

    fn general_reader_sections(&self) -> Vec<SettingsSection> {
        let invert_scroll = is_invert_scroll_direction();
        let current_column_mode = get_epub_column_mode();
        let zen_hide_border = is_zen_hide_border();

        vec![
            SettingsSection {
                title: "Mouse Wheel",
                title_indent: 0,
                options_indent: 2,
                options_top_spacing: 0,
                enabled: true,
                options: vec![
                    SettingsOption {
                        id: GeneralOption::MouseWheelNormal,
                        label: "Normal",
                        hint: Some("wheel down moves down"),
                        selected: !invert_scroll,
                        enabled: true,
                    },
                    SettingsOption {
                        id: GeneralOption::MouseWheelInverted,
                        label: "Inverted",
                        hint: Some("wheel down moves up"),
                        selected: invert_scroll,
                        enabled: true,
                    },
                ],
            },
            SettingsSection {
                title: "EPUB Layout",
                title_indent: 0,
                options_indent: 2,
                options_top_spacing: 0,
                enabled: true,
                options: vec![
                    SettingsOption {
                        id: GeneralOption::EpubSingle,
                        label: "Single",
                        hint: Some("one column"),
                        selected: current_column_mode == EpubColumnMode::Single,
                        enabled: true,
                    },
                    SettingsOption {
                        id: GeneralOption::EpubDual,
                        label: "Dual",
                        hint: Some("two columns when wide"),
                        selected: current_column_mode == EpubColumnMode::Dual,
                        enabled: true,
                    },
                ],
            },
            SettingsSection {
                title: "Zen Mode Border",
                title_indent: 0,
                options_indent: 2,
                options_top_spacing: 0,
                enabled: true,
                options: vec![
                    SettingsOption {
                        id: GeneralOption::ZenBorderShown,
                        label: "Shown",
                        hint: Some("frame around content"),
                        selected: !zen_hide_border,
                        enabled: true,
                    },
                    SettingsOption {
                        id: GeneralOption::ZenBorderHidden,
                        label: "Hidden",
                        hint: Some("content only"),
                        selected: zen_hide_border,
                        enabled: true,
                    },
                ],
            },
        ]
    }

    fn settings_sections_height(sections: &[SettingsSection]) -> u16 {
        sections.iter().map(SettingsSection::height).sum::<u16>()
            + sections.len().saturating_sub(1) as u16
    }

    fn render_settings_sections(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        palette: &Base16Palette,
        sections: &[SettingsSection],
    ) {
        let mut y = area.y;
        for (section_idx, section) in sections.iter().enumerate() {
            let header_area = Self::indented_area(
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
                section.title_indent,
            );
            let header_color = if section.enabled {
                palette.base_06
            } else {
                palette.base_03
            };
            self.render_section_header(buf, header_area, section.title, palette, header_color);

            y += 1 + section.options_top_spacing;
            for option in &section.options {
                let option_area = Self::indented_area(
                    Rect {
                        x: area.x,
                        y,
                        width: area.width,
                        height: 1,
                    },
                    section.options_indent,
                );
                self.render_general_radio_option(buf, option_area, option, palette);
                y += 1;
            }

            if section_idx + 1 < sections.len() {
                y += 1;
            }
        }
    }

    fn render_settings_info_lines(
        buf: &mut Buffer,
        area: Rect,
        sections_height: u16,
        lines: Vec<Line<'static>>,
    ) {
        if lines.is_empty() {
            return;
        }

        let info_area = Rect {
            x: area.x,
            y: area.y + sections_height + 1,
            width: area.width,
            height: lines.len() as u16,
        };
        Paragraph::new(lines).render(info_area, buf);
    }

    fn render_themes_tab(&mut self, buf: &mut Buffer, area: Rect, palette: &Base16Palette) {
        let theme_list_height = self.theme_names.len() as u16;
        let truecolor_note_height = if crate::color_mode::supports_true_color() {
            0
        } else {
            TRUECOLOR_NOTE_LINES
        };
        let truecolor_note_spacer_height = if truecolor_note_height > 0 {
            TRUECOLOR_NOTE_SPACER_LINES
        } else {
            0
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(theme_list_height),            // Theme list
                Constraint::Length(1),                            // spacer
                Constraint::Length(1),                            // Background header
                Constraint::Length(1),                            // empty line
                Constraint::Length(2),                            // Transparent Background options
                Constraint::Length(truecolor_note_spacer_height), // spacer
                Constraint::Length(truecolor_note_height),        // truecolor warning
                Constraint::Min(0),                               // remaining space
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
            Paragraph::new(line).render(line_area, buf);
        }

        // Background section header
        self.render_section_header(buf, chunks[2], "Background", palette, palette.base_06);

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
        Paragraph::new(theme_line).render(trans_options_chunks[0], buf);

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
        Paragraph::new(trans_line).render(trans_options_chunks[1], buf);

        if truecolor_note_height > 0 {
            let note_lines = vec![
                Line::from(vec![
                    Span::styled("ⓘ ", Style::default().fg(palette.base_0a)),
                    Span::styled(
                        "Truecolor was not detected; themes may look incorrect.",
                        Style::default().fg(palette.base_03),
                    ),
                ]),
                Line::from(Span::styled(
                    "If your terminal supports RGB colors, try:",
                    Style::default().fg(palette.base_03),
                )),
                Line::from(Span::styled(
                    "export COLORTERM=truecolor",
                    Style::default()
                        .fg(palette.base_06)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "then run bookokrat again.",
                    Style::default().fg(palette.base_03),
                )),
            ];
            Paragraph::new(note_lines).render(chunks[6], buf);
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
        buf: &mut Buffer,
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

        Paragraph::new(line).render(area, buf);
    }

    fn indented_area(area: Rect, indent: u16) -> Rect {
        Rect {
            x: area.x + indent.min(area.width),
            y: area.y,
            width: area.width.saturating_sub(indent),
            height: area.height,
        }
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
                        "PDF/DJVU viewing requires a graphics-enabled terminal.",
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
                    "Enable PDF/DJVU support to change render mode",
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

    fn render_general_radio_option(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        option: &SettingsOption,
        palette: &Base16Palette,
    ) {
        if let Some(chunk) = self.content_chunks.get_mut(option.id.index()) {
            *chunk = area;
        }

        let radio = if option.selected { "●" } else { "○" };
        let style = if option.enabled {
            Style::default().fg(palette.base_06)
        } else {
            Style::default().fg(palette.base_03)
        };
        let line = self.render_radio_option(
            radio,
            option.label,
            option.hint,
            style,
            self.current_tab == SettingsTab::General
                && self.general_selected == option.id
                && option.enabled,
            palette,
        );
        Paragraph::new(line).render(area, buf);
    }

    fn general_next(&mut self) {
        let current_pos = GENERAL_OPTIONS
            .iter()
            .position(|option| *option == self.general_selected)
            .unwrap_or(0);
        for offset in 1..=GENERAL_OPTIONS.len() {
            let option = GENERAL_OPTIONS[(current_pos + offset) % GENERAL_OPTIONS.len()];
            if self.is_general_option_selectable(option) {
                self.general_selected = option;
                break;
            }
        }
    }

    fn general_previous(&mut self) {
        let current_pos = GENERAL_OPTIONS
            .iter()
            .position(|option| *option == self.general_selected)
            .unwrap_or(0);
        for offset in 1..=GENERAL_OPTIONS.len() {
            let option = GENERAL_OPTIONS
                [(current_pos + GENERAL_OPTIONS.len() - offset) % GENERAL_OPTIONS.len()];
            if self.is_general_option_selectable(option) {
                self.general_selected = option;
                break;
            }
        }
    }

    fn general_move_horizontal(&mut self, to_pdf_options: bool) {
        if self.general_selected.is_pdf_option() == to_pdf_options {
            return;
        }

        let current_y = self
            .content_chunks
            .get(self.general_selected.index())
            .map(|area| area.y)
            .unwrap_or(0);
        if let Some(option) = GENERAL_OPTIONS
            .iter()
            .copied()
            .filter(|option| option.is_pdf_option() == to_pdf_options)
            .filter(|option| self.is_general_option_selectable(*option))
            .min_by_key(|option| {
                self.content_chunks
                    .get(option.index())
                    .map(|area| area.y.abs_diff(current_y))
                    .unwrap_or(u16::MAX)
            })
        {
            self.general_selected = option;
        }
    }

    fn is_general_option_selectable(&self, option: GeneralOption) -> bool {
        match option {
            GeneralOption::PdfEnabled | GeneralOption::PdfDisabled => self.supports_graphics,
            GeneralOption::PdfRenderPage => self.render_mode_available(),
            GeneralOption::PdfRenderScroll => {
                self.render_mode_available() && self.supports_scroll_mode
            }
            GeneralOption::PdfLayoutSingle | GeneralOption::PdfLayoutDual => {
                self.render_mode_available()
            }
            GeneralOption::MouseWheelNormal
            | GeneralOption::MouseWheelInverted
            | GeneralOption::EpubSingle
            | GeneralOption::EpubDual
            | GeneralOption::ZenBorderShown
            | GeneralOption::ZenBorderHidden => true,
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

    fn apply_general_selected(&mut self) -> Option<SettingsAction> {
        match self.general_selected {
            GeneralOption::PdfEnabled if self.supports_graphics => {
                if !is_pdf_enabled() {
                    set_pdf_enabled(true);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            GeneralOption::PdfDisabled if self.supports_graphics => {
                if is_pdf_enabled() {
                    set_pdf_enabled(false);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            GeneralOption::PdfRenderPage if self.render_mode_available() => {
                if get_pdf_render_mode() != PdfRenderMode::Page {
                    set_pdf_render_mode(PdfRenderMode::Page);
                    return Some(SettingsAction::RenderModeChanged);
                }
                None
            }
            GeneralOption::PdfRenderScroll
                if self.render_mode_available() && self.supports_scroll_mode =>
            {
                if get_pdf_render_mode() != PdfRenderMode::Scroll {
                    set_pdf_render_mode(PdfRenderMode::Scroll);
                    return Some(SettingsAction::RenderModeChanged);
                }
                None
            }
            GeneralOption::PdfLayoutSingle if self.render_mode_available() => {
                if get_pdf_page_layout_mode() != PdfPageLayoutMode::Single {
                    set_pdf_page_layout_mode(PdfPageLayoutMode::Single);
                    return Some(SettingsAction::PageLayoutChanged);
                }
                None
            }
            GeneralOption::PdfLayoutDual if self.render_mode_available() => {
                if get_pdf_page_layout_mode() != PdfPageLayoutMode::Dual {
                    set_pdf_page_layout_mode(PdfPageLayoutMode::Dual);
                    return Some(SettingsAction::PageLayoutChanged);
                }
                None
            }
            GeneralOption::MouseWheelNormal => {
                if is_invert_scroll_direction() {
                    set_invert_scroll_direction(false);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            GeneralOption::MouseWheelInverted => {
                if !is_invert_scroll_direction() {
                    set_invert_scroll_direction(true);
                    return Some(SettingsAction::SettingsChanged);
                }
                None
            }
            GeneralOption::EpubSingle => {
                if get_epub_column_mode() != EpubColumnMode::Single {
                    set_epub_column_mode(EpubColumnMode::Single);
                    return Some(SettingsAction::PageLayoutChanged);
                }
                None
            }
            GeneralOption::EpubDual => {
                if get_epub_column_mode() != EpubColumnMode::Dual {
                    set_epub_column_mode(EpubColumnMode::Dual);
                    return Some(SettingsAction::PageLayoutChanged);
                }
                None
            }
            GeneralOption::ZenBorderShown => {
                if is_zen_hide_border() {
                    set_zen_hide_border(false);
                    return Some(SettingsAction::ZenBorderChanged);
                }
                None
            }
            GeneralOption::ZenBorderHidden => {
                if !is_zen_hide_border() {
                    set_zen_hide_border(true);
                    return Some(SettingsAction::ZenBorderChanged);
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

    fn render_integrations_tab(&mut self, buf: &mut Buffer, area: Rect, palette: &Base16Palette) {
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
            buf,
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
        Widget::render(&self.lookup_command_input, chunks[1], buf);

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
        Paragraph::new(lookup_hints).render(chunks[2], buf);

        // Display mode (vertical radio buttons)
        self.render_section_header(buf, chunks[3], "Display mode:", palette, palette.base_04);

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
        Paragraph::new(popup_line).render(chunks[4], buf);

        let faf_radio = if popup_selected { "○" } else { "●" };
        let faf_line = self.render_radio_option(
            faf_radio,
            "Fire and forget",
            None,
            radio_style,
            self.integrations_focus == IntegrationsFocus::DisplayFireAndForget,
            palette,
        );
        Paragraph::new(faf_line).render(chunks[5], buf);

        // chunks[6] = spacing
        self.render_test_button(
            buf,
            chunks[7],
            "Test",
            "lookup word \"hello\"",
            self.integrations_focus == IntegrationsFocus::TestLookup,
            palette,
        );

        // chunks[8] = spacing

        // -- SyncTeX section --
        self.render_section_header(
            buf,
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
        Widget::render(&self.synctex_editor_input, chunks[10], buf);

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
        Paragraph::new(synctex_hints).render(chunks[11], buf);

        // chunks[12] = spacing
        self.render_test_button(
            buf,
            chunks[13],
            "Test",
            "open /tmp/synctex_test.txt:1",
            self.integrations_focus == IntegrationsFocus::TestSynctex,
            palette,
        );
    }

    fn render_test_button(
        &self,
        buf: &mut Buffer,
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
        Paragraph::new(line).render(area, buf);
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
                // Tab positions: " General   Select Theme   Integrations"
                //                  1..8       11..23          26..38
                let new_tab = if cfg!(feature = "pdf") && (1..11).contains(&rel_x) {
                    Some(SettingsTab::General)
                } else if (11..26).contains(&rel_x) {
                    Some(SettingsTab::Themes)
                } else if rel_x >= 26 {
                    Some(SettingsTab::Integrations)
                } else {
                    None
                };
                if let Some(tab) = new_tab {
                    if self.current_tab == SettingsTab::Integrations {
                        self.save_integrations();
                    }
                    self.set_tab(tab);
                    return None;
                }
            }
        }

        if self.content_chunks.is_empty() {
            return None;
        }

        // Translate screen click to virtual content-buffer coordinates.
        let viewport = self.content_viewport?;
        if col < viewport.x
            || col >= viewport.x + viewport.width
            || row < viewport.y
            || row >= viewport.y + viewport.height
        {
            return None;
        }
        let virtual_row = row + self.scroll_offset;

        let chunks = &self.content_chunks;
        let hit = |idx: usize| -> bool {
            if let Some(r) = chunks.get(idx) {
                col >= r.x
                    && col < r.x + r.width
                    && virtual_row >= r.y
                    && virtual_row < r.y + r.height
            } else {
                false
            }
        };

        match self.current_tab {
            SettingsTab::General => {
                for option in GENERAL_OPTIONS {
                    let Some(r) = chunks.get(option.index()) else {
                        continue;
                    };
                    if col >= r.x
                        && col < r.x + r.width
                        && virtual_row >= r.y
                        && virtual_row < r.y + r.height
                    {
                        if self.is_general_option_selectable(option) {
                            self.general_selected = option;
                            return self.apply_general_selected();
                        }
                        return None;
                    }
                }
            }
            SettingsTab::Themes => {
                if let Some(r) = chunks.first() {
                    if col >= r.x
                        && col < r.x + r.width
                        && virtual_row >= r.y
                        && virtual_row < r.y + r.height
                    {
                        let idx = (virtual_row - r.y) as usize;
                        if idx < self.theme_names.len() {
                            self.theme_selected_idx = idx;
                            return self.apply_theme_selected();
                        }
                    }
                }
                if let Some(r) = chunks.get(4) {
                    if col >= r.x
                        && col < r.x + r.width
                        && virtual_row >= r.y
                        && virtual_row < r.y + r.height
                    {
                        let sub = (virtual_row - r.y) as usize;
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

    /// Switch tabs and reset scroll state.
    fn set_tab(&mut self, tab: SettingsTab) {
        if self.current_tab != tab {
            self.current_tab = tab;
            self.scroll_offset = 0;
        }
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
        set_integrations(lookup_cmd, self.lookup_display_selected, synctex_cmd);
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<SettingsAction> {
        use crossterm::event::KeyCode;

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
                    self.set_tab(self.current_tab.next());
                    return None;
                }
                KeyCode::BackTab => {
                    self.save_integrations();
                    self.set_tab(self.current_tab.prev());
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

        {
            use crate::keybindings::action::Action;
            use crate::keybindings::context::KeyContext;
            use crate::keybindings::keymap::LookupResult;
            use crate::keybindings::notation::key_event_to_input;

            let input = key_event_to_input(&key);
            let km = crate::keybindings::keymap();
            let mut prospective: Vec<_> = key_seq.keys().iter().map(key_event_to_input).collect();
            prospective.push(input);

            match km.lookup(KeyContext::PopupSettings, &prospective) {
                LookupResult::Found(action) => {
                    key_seq.clear();
                    match action {
                        Action::NextTab => {
                            if self.current_tab == SettingsTab::Integrations {
                                self.save_integrations();
                            }
                            self.set_tab(self.current_tab.next());
                            None
                        }
                        Action::PrevTab => {
                            if self.current_tab == SettingsTab::Integrations {
                                self.save_integrations();
                            }
                            self.set_tab(self.current_tab.prev());
                            None
                        }
                        Action::MoveLeft => {
                            self.handle_h();
                            None
                        }
                        Action::MoveRight => {
                            self.handle_l();
                            None
                        }
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
                        Action::ScrollPageDown => {
                            self.handle_ctrl_f();
                            None
                        }
                        Action::ScrollPageUp => {
                            self.handle_ctrl_b();
                            None
                        }
                        Action::Cancel => {
                            if self.current_tab == SettingsTab::Integrations {
                                self.save_integrations();
                            }
                            Some(SettingsAction::Close)
                        }
                        Action::Select => match self.current_tab {
                            SettingsTab::General => self.apply_general_selected(),
                            SettingsTab::Themes => self.apply_theme_selected(),
                            SettingsTab::Integrations => self.apply_integrations_selected(),
                        },
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
}

impl Popup for SettingsPopup {
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

impl VimNavMotions for SettingsPopup {
    fn handle_h(&mut self) {
        if self.current_tab == SettingsTab::General {
            self.general_move_horizontal(true);
        }
    }

    fn handle_j(&mut self) {
        match self.current_tab {
            SettingsTab::General => self.general_next(),
            SettingsTab::Themes => self.theme_next(),
            SettingsTab::Integrations => {
                self.integrations_focus = self.integrations_focus.next();
            }
        }
    }

    fn handle_k(&mut self) {
        match self.current_tab {
            SettingsTab::General => self.general_previous(),
            SettingsTab::Themes => self.theme_previous(),
            SettingsTab::Integrations => {
                self.integrations_focus = self.integrations_focus.prev();
            }
        }
    }

    fn handle_l(&mut self) {
        if self.current_tab == SettingsTab::General {
            self.general_move_horizontal(false);
        }
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
            SettingsTab::General => {}
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
            SettingsTab::General => {}
            SettingsTab::Themes => {
                self.theme_selected_idx = self.theme_max_idx();
            }
            SettingsTab::Integrations => {
                self.integrations_focus = IntegrationsFocus::TestSynctex;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_general_focus_prefers_disabled_when_pdf_is_off() {
        assert_eq!(
            SettingsPopup::initial_general_selected_from_state(
                true,
                true,
                false,
                PdfRenderMode::Page,
                PdfPageLayoutMode::Single,
            ),
            GeneralOption::PdfDisabled
        );
    }

    #[test]
    fn initial_general_focus_prefers_non_default_selected_options() {
        assert_eq!(
            SettingsPopup::initial_general_selected_from_state(
                true,
                true,
                true,
                PdfRenderMode::Scroll,
                PdfPageLayoutMode::Dual,
            ),
            GeneralOption::PdfRenderScroll
        );
        assert_eq!(
            SettingsPopup::initial_general_selected_from_state(
                true,
                false,
                true,
                PdfRenderMode::Page,
                PdfPageLayoutMode::Dual,
            ),
            GeneralOption::PdfLayoutDual
        );
    }

    #[test]
    fn initial_theme_focus_tracks_current_theme_or_transparency() {
        assert_eq!(
            SettingsPopup::initial_theme_selected_idx_from_state(7, 3, false),
            3
        );
        assert_eq!(
            SettingsPopup::initial_theme_selected_idx_from_state(7, 3, true),
            8
        );
    }
}
