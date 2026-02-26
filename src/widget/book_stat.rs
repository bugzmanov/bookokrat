use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
use crate::parsing::markdown_renderer::MarkdownRenderer;
use crate::parsing::text_generator::TextGenerator;
use crate::parsing::toc_parser::TocParser;
use crate::table_of_contents::TocItem;
use crate::theme::current_theme;
use anyhow::Result;
use crossterm::event::KeyModifiers;
use epub::doc::EpubDoc;
use log::{debug, error};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use std::collections::HashMap;
use std::io::{Read, Seek};

/// URL-decode percent-encoded characters in a string (e.g., %27 -> ')
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }

    result
}

pub struct BookStat {
    chapter_stats: Vec<ChapterStat>,
    list_state: ListState,
    visible: bool,
    terminal_size: (u16, u16),
    last_popup_area: Option<Rect>,
    stat_unit: StatUnit,
}

#[derive(Clone, Debug)]
struct ChapterStat {
    title: String,
    count: usize,
    chapter_index: usize, // The spine index in the EPUB
}

#[derive(Clone, Copy, Debug)]
enum StatUnit {
    Screens,
    Pages,
}

impl StatUnit {
    fn format_count(self, count: usize) -> String {
        match self {
            StatUnit::Screens => {
                if count == 1 {
                    "1 screen".to_string()
                } else {
                    format!("{count} screens")
                }
            }
            StatUnit::Pages => {
                if count == 1 {
                    "1 page".to_string()
                } else {
                    format!("{count} pages")
                }
            }
        }
    }
}

pub enum BookStatAction {
    JumpToChapter { chapter_index: usize },
    Close,
}

impl Default for BookStat {
    fn default() -> Self {
        Self::new()
    }
}

impl BookStat {
    pub fn new() -> Self {
        Self {
            chapter_stats: Vec::new(),
            list_state: ListState::default(),
            visible: false,
            terminal_size: (80, 24),
            last_popup_area: None,
            stat_unit: StatUnit::Screens,
        }
    }

    pub fn calculate_stats<R: Read + Seek>(
        &mut self,
        epub: &mut EpubDoc<R>,
        terminal_size: (u16, u16),
    ) -> Result<()> {
        self.terminal_size = terminal_size;
        self.chapter_stats.clear();
        self.stat_unit = StatUnit::Screens;

        let popup_height = terminal_size.1.saturating_sub(4) as usize;
        let text_width = terminal_size.0.saturating_sub(6) as usize;
        let lines_per_screen = popup_height.saturating_sub(4);

        let toc = TocParser::parse_toc_structure(epub);
        let toc_title_map = Self::build_toc_title_map(&toc);

        let original_chapter = epub.get_current_chapter();
        let num_chapters = epub.spine.len();

        for idx in 0..num_chapters {
            if !epub.set_current_chapter(idx) {
                continue;
            }

            let content = match epub.get_current_str() {
                Some((content, _)) => content,
                None => {
                    error!("BookStat: Failed to get content for spine index {idx}");
                    continue;
                }
            };

            let title = self
                .resolve_spine_title(epub, idx, &toc_title_map, &content)
                .unwrap_or_else(|| format!("Chapter {}", idx + 1));

            self.add_chapter_stat(&title, &content, text_width, lines_per_screen, idx);
        }

        epub.set_current_chapter(original_chapter);

        if !self.chapter_stats.is_empty() {
            self.list_state.select(Some(0));
        }

        debug!("Chapter stats: {:?}", self.chapter_stats);

        Ok(())
    }

    #[cfg(feature = "pdf")]
    pub fn calculate_pdf_stats(
        &mut self,
        toc_entries: &[crate::pdf::TocEntry],
        page_count: usize,
        page_numbers: &crate::pdf::PageNumberTracker,
        terminal_size: (u16, u16),
    ) -> Result<()> {
        use crate::pdf::TocTarget;

        self.terminal_size = terminal_size;
        self.chapter_stats.clear();
        self.stat_unit = StatUnit::Pages;

        if page_count == 0 {
            return Ok(());
        }

        let mut resolved_entries: Vec<(String, usize, usize)> = Vec::new();
        for entry in toc_entries {
            let page = match &entry.target {
                TocTarget::InternalPage(page) => Some(*page),
                TocTarget::PrintedPage(printed) => page_numbers
                    .map_printed_to_pdf(*printed, page_count)
                    .or_else(|| printed.checked_sub(1)),
                TocTarget::External(_) => None,
            };

            if let Some(page) = page {
                resolved_entries.push((entry.title.clone(), entry.level, page));
            }
        }

        let has_level_one = resolved_entries.iter().any(|(_, level, _)| *level == 1);
        let target_level = if has_level_one { 1 } else { 0 };
        let anchors: Vec<(String, usize)> = resolved_entries
            .into_iter()
            .filter(|(_, level, _)| *level == target_level)
            .map(|(title, _, page)| (title, page))
            .collect();

        for (idx, (title, start_page)) in anchors.iter().enumerate() {
            if *start_page >= page_count {
                continue;
            }

            let mut end_page = if let Some((_, next_start)) = anchors.get(idx + 1) {
                *next_start
            } else {
                page_count
            };

            if end_page <= *start_page {
                end_page = (*start_page + 1).min(page_count);
            } else {
                end_page = end_page.min(page_count);
            }

            let pages = end_page.saturating_sub(*start_page).max(1);

            self.chapter_stats.push(ChapterStat {
                title: title.clone(),
                count: pages,
                chapter_index: *start_page,
            });
        }

        if !self.chapter_stats.is_empty() {
            self.list_state.select(Some(0));
        }

        debug!("PDF chapter stats: {:?}", self.chapter_stats);

        Ok(())
    }

    /// Build a flat map from resource path suffix → TOC title by recursively walking the TOC.
    /// Strips anchors from hrefs (e.g. "ch1.xhtml#section" → "ch1.xhtml") since spine items
    /// don't have anchors. When multiple TOC entries map to the same file, the first one wins.
    fn build_toc_title_map(items: &[TocItem]) -> HashMap<String, String> {
        let mut map = HashMap::new();
        Self::collect_toc_titles(items, &mut map);
        map
    }

    fn collect_toc_titles(items: &[TocItem], map: &mut HashMap<String, String>) {
        for item in items {
            match item {
                TocItem::Chapter { title, href, .. } => {
                    let stripped = Self::strip_anchor(href);
                    let decoded = percent_decode(&stripped);
                    map.entry(decoded).or_insert_with(|| title.clone());
                }
                TocItem::Section {
                    title,
                    href,
                    children,
                    ..
                } => {
                    if let Some(href_str) = href {
                        let stripped = Self::strip_anchor(href_str);
                        let decoded = percent_decode(&stripped);
                        map.entry(decoded).or_insert_with(|| title.clone());
                    }
                    Self::collect_toc_titles(children, map);
                }
            }
        }
    }

    fn strip_anchor(href: &str) -> String {
        match href.find('#') {
            Some(pos) => href[..pos].to_string(),
            None => href.to_string(),
        }
    }

    /// Resolve a title for a spine item by looking up the TOC map, then falling back
    /// to extracting a title from the HTML content.
    fn resolve_spine_title<R: Read + Seek>(
        &self,
        epub: &EpubDoc<R>,
        spine_index: usize,
        toc_title_map: &HashMap<String, String>,
        content: &str,
    ) -> Option<String> {
        if let Some(spine_item) = epub.spine.get(spine_index) {
            if let Some(resource) = epub.resources.get(&spine_item.idref) {
                let path_str = resource.path.to_string_lossy();
                // Try matching against TOC entries by path suffix
                for (toc_href, title) in toc_title_map {
                    if path_str.ends_with(toc_href.as_str()) || toc_href.ends_with(&*path_str) {
                        return Some(title.clone());
                    }
                }
            }
        }

        // Fallback: extract title from HTML content
        if let Some(title) = TextGenerator::extract_chapter_title(content) {
            return Some(title);
        }

        // Last resort: use the spine idref
        epub.spine.get(spine_index).map(|item| item.idref.clone())
    }

    fn add_chapter_stat(
        &mut self,
        title: &str,
        content: &str,
        text_width: usize,
        lines_per_screen: usize,
        chapter_index: usize,
    ) {
        let mut converter = HtmlToMarkdownConverter::new();
        let document = converter.convert(content);

        let renderer = MarkdownRenderer::new();
        let rendered_text = renderer.render(&document);

        let screens = self.calculate_screens(&rendered_text, text_width, lines_per_screen);

        self.chapter_stats.push(ChapterStat {
            title: title.to_string(),
            count: screens,
            chapter_index,
        });
    }

    fn calculate_screens(&self, text: &str, width: usize, lines_per_screen: usize) -> usize {
        if lines_per_screen == 0 || width == 0 {
            return 0;
        }

        let mut total_lines = 0;

        for line in text.lines() {
            if line.is_empty() {
                total_lines += 1;
            } else {
                // Calculate wrapped lines
                let line_length = line.chars().count();
                let wrapped_lines = line_length.div_ceil(width);
                total_lines += wrapped_lines.max(1);
            }
        }

        // Calculate number of screens
        total_lines.div_ceil(lines_per_screen)
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

    /// Get the actual EPUB chapter index of the currently selected chapter
    pub fn get_selected_chapter_index(&self) -> Option<usize> {
        self.list_state
            .selected()
            .and_then(|idx| self.chapter_stats.get(idx).map(|stat| stat.chapter_index))
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate popup dimensions
        let popup_width = area.width.saturating_sub(10).min(80);
        let popup_height = area.height.saturating_sub(4).min(30);

        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        self.last_popup_area = Some(popup_area);

        // Clear background
        frame.render_widget(Clear, popup_area);

        // Calculate cumulative percentages
        let total_screens: usize = self.chapter_stats.iter().map(|s| s.count).sum();
        let mut cumulative_screens = 0;

        // Create the list items
        let items: Vec<ListItem> = if self.chapter_stats.is_empty() {
            // Show a message if no chapters found
            vec![ListItem::new(vec![Line::from(vec![Span::styled(
                "No chapters found. Processing...",
                Style::default().fg(current_theme().base_0a),
            )])])]
        } else {
            // Available width inside the popup (borders=2, highlight symbol "» "=2)
            let inner_width = popup_width.saturating_sub(4) as usize;

            self.chapter_stats
                .iter()
                .map(|stat| {
                    let percentage = if total_screens > 0 {
                        (cumulative_screens * 100) / total_screens
                    } else {
                        0
                    };

                    cumulative_screens += stat.count;
                    let count_text = self.stat_unit.format_count(stat.count);
                    let count_suffix = format!(" [{count_text}]");
                    let prefix = format!("{percentage:3}% ");

                    // Truncate title so the full line fits within inner_width
                    let title_clean = stat.title.replace('\n', " ");
                    let overhead = prefix.chars().count() + count_suffix.chars().count();
                    let max_title = inner_width.saturating_sub(overhead);
                    let title_chars: Vec<char> = title_clean.chars().collect();
                    let title = if title_chars.len() > max_title {
                        let truncated: String =
                            title_chars[..max_title.saturating_sub(1)].iter().collect();
                        format!("{truncated}…")
                    } else {
                        title_clean
                    };

                    let content = vec![Line::from(vec![
                        Span::styled(prefix, Style::default().fg(current_theme().base_03)),
                        Span::raw(title),
                        Span::styled(count_suffix, Style::default().fg(current_theme().base_0c)),
                    ])];

                    ListItem::new(content)
                })
                .collect()
        };

        // Create the list widget
        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Chapter Statistics ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(current_theme().popup_border_color()))
                    .style(Style::default().bg(current_theme().base_00)),
            )
            .highlight_style(
                Style::default()
                    .bg(current_theme().base_02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("» ");

        // Render the list
        frame.render_stateful_widget(list, popup_area, &mut self.list_state);

        // Add help text at the bottom
        let help_text =
            "j/k/Scroll: Navigate | Enter/DblClick: Jump | G/gg: Bottom/Top | Esc: Close";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(current_theme().base_03))
            .alignment(Alignment::Center);

        let help_area = Rect {
            x: popup_area.x,
            y: popup_area.y + popup_area.height - 1,
            width: popup_area.width,
            height: 1,
        };

        frame.render_widget(help, help_area);
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<BookStatAction> {
        use crossterm::event::KeyCode;

        match key.code {
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
            KeyCode::Esc => Some(BookStatAction::Close),
            KeyCode::Enter => Some(BookStatAction::JumpToChapter {
                chapter_index: self.get_selected_chapter_index().unwrap_or(0),
            }),
            _ => None,
        }
    }

    /// Handle mouse click at the given position
    /// Returns true if an item was clicked (for double-click detection)
    pub fn handle_mouse_click(&mut self, x: u16, y: u16) -> bool {
        debug!("BookStat: Mouse click at ({x}, {y})");

        if let Some(popup_area) = self.last_popup_area {
            debug!(
                "BookStat: Popup area: x={}, y={}, w={}, h={}",
                popup_area.x, popup_area.y, popup_area.width, popup_area.height
            );

            // Check if click is within the popup area
            if x >= popup_area.x
                && x < popup_area.x + popup_area.width
                && y > popup_area.y
                && y < popup_area.y + popup_area.height.saturating_sub(2)
            {
                // Calculate which item was clicked
                // Account for the border (1 line at top)
                let relative_y = y.saturating_sub(popup_area.y).saturating_sub(1);

                // Get the current scroll offset from the list state
                let offset = self.list_state.offset();

                // Calculate the actual index in the list
                let new_index = offset + relative_y as usize;

                debug!(
                    "BookStat: relative_y={}, offset={}, new_index={}, items_len={}",
                    relative_y,
                    offset,
                    new_index,
                    self.chapter_stats.len()
                );

                if new_index < self.chapter_stats.len() {
                    self.list_state.select(Some(new_index));
                    debug!("BookStat: Selected item at index {new_index}");
                    return true;
                }
            } else {
                debug!("BookStat: Click outside popup area");
            }
        } else {
            debug!("BookStat: No popup area set");
        }
        false
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

impl VimNavMotions for BookStat {
    fn handle_h(&mut self) {
        // No horizontal movement in list
    }

    fn handle_j(&mut self) {
        let current = self.list_state.selected().unwrap_or(0);
        let max_pos = self.chapter_stats.len().saturating_sub(1);
        let new_pos = (current + 1).min(max_pos);
        self.list_state.select(Some(new_pos));
    }

    fn handle_k(&mut self) {
        let current = self.list_state.selected().unwrap_or(0);
        let new_pos = current.saturating_sub(1);
        self.list_state.select(Some(new_pos));
    }

    fn handle_l(&mut self) {
        // No horizontal movement in list
    }

    fn handle_ctrl_d(&mut self) {
        // Move down half screen
        let half_height = 10; // Approximate half of popup height
        let current = self.list_state.selected().unwrap_or(0);
        let max_pos = self.chapter_stats.len().saturating_sub(1);
        let new_pos = (current + half_height).min(max_pos);
        self.list_state.select(Some(new_pos));
    }

    fn handle_ctrl_u(&mut self) {
        // Move up half screen
        let half_height = 10; // Approximate half of popup height
        let current = self.list_state.selected().unwrap_or(0);
        let new_pos = current.saturating_sub(half_height);
        self.list_state.select(Some(new_pos));
    }

    fn handle_ctrl_f(&mut self) {
        let full_height = 20;
        let current = self.list_state.selected().unwrap_or(0);
        let max_pos = self.chapter_stats.len().saturating_sub(1);
        let new_pos = (current + full_height).min(max_pos);
        self.list_state.select(Some(new_pos));
    }

    fn handle_ctrl_b(&mut self) {
        let full_height = 20;
        let current = self.list_state.selected().unwrap_or(0);
        let new_pos = current.saturating_sub(full_height);
        self.list_state.select(Some(new_pos));
    }

    fn handle_gg(&mut self) {
        if !self.chapter_stats.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn handle_upper_g(&mut self) {
        if !self.chapter_stats.is_empty() {
            self.list_state.select(Some(self.chapter_stats.len() - 1));
        }
    }
}
