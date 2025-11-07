use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
use crate::parsing::markdown_renderer::MarkdownRenderer;
use crate::parsing::toc_parser::TocParser;
use crate::theme::OCEANIC_NEXT;
use anyhow::Result;
use crossterm::event::KeyModifiers;
use epub::doc::EpubDoc;
use log::{debug, error};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use std::io::{Read, Seek};

pub struct BookStat {
    chapter_stats: Vec<ChapterStat>,
    list_state: ListState,
    visible: bool,
    terminal_size: (u16, u16),
    last_popup_area: Option<Rect>,
}

#[derive(Clone, Debug)]
struct ChapterStat {
    title: String,
    screens: usize,
    chapter_index: usize, // The actual chapter index in the EPUB
    is_top_level: bool,   // Whether this is a top-level chapter or nested section
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
        }
    }

    pub fn calculate_stats<R: Read + Seek>(
        &mut self,
        epub: &mut EpubDoc<R>,
        terminal_size: (u16, u16),
    ) -> Result<()> {
        self.terminal_size = terminal_size;
        self.chapter_stats.clear();

        let toc = TocParser::parse_toc_structure(epub);

        let popup_height = terminal_size.1.saturating_sub(4) as usize;
        let text_width = terminal_size.0.saturating_sub(6) as usize;
        let lines_per_screen = popup_height.saturating_sub(4); // Account for borders and padding

        if !toc.is_empty() {
            self.process_toc_items(&toc, epub, text_width, lines_per_screen);
        }

        if !self.chapter_stats.is_empty() {
            self.list_state.select(Some(0));
        }

        debug!("Chapter stats: {:?}", self.chapter_stats);

        Ok(())
    }

    fn process_toc_items<R: Read + Seek>(
        &mut self,
        items: &[crate::table_of_contents::TocItem],
        epub: &mut EpubDoc<R>,
        text_width: usize,
        lines_per_screen: usize,
    ) {
        self.process_toc_items_recursive(items, epub, text_width, lines_per_screen, true);
    }

    fn process_toc_items_recursive<R: Read + Seek>(
        &mut self,
        items: &[crate::table_of_contents::TocItem],
        epub: &mut EpubDoc<R>,
        text_width: usize,
        lines_per_screen: usize,
        is_top_level: bool,
    ) {
        for item in items {
            match item {
                crate::table_of_contents::TocItem::Chapter { title, href, .. } => {
                    // Find the spine index for this href
                    let current_page = epub.get_current_chapter();

                    // Try to find the chapter in the spine
                    let mut spine_index = None;
                    for (idx, spine_item) in epub.spine.iter().enumerate() {
                        if let Some(resource) = epub.resources.get(&spine_item.idref) {
                            let path_str = resource.path.to_string_lossy();
                            if path_str.ends_with(href) || href.ends_with(&*path_str) {
                                spine_index = Some(idx);
                                break;
                            }
                        }
                    }

                    let content_result = if let Some(idx) = spine_index {
                        if epub.set_current_chapter(idx) {
                            epub.get_current_str().map(|(content, _)| content)
                        } else {
                            None
                        }
                    } else {
                        // Fallback to trying to find the resource by path
                        let matching_id = epub
                            .resources
                            .iter()
                            .find(|(_, resource)| {
                                resource.path.to_string_lossy() == *href
                                    || resource.path.to_string_lossy().ends_with(href)
                            })
                            .map(|(id, _)| id.clone());

                        if let Some(id) = matching_id {
                            epub.get_resource_str(&id).map(|(content, _)| content)
                        } else {
                            None
                        }
                    };

                    // Restore original page
                    epub.set_current_chapter(current_page);

                    match content_result {
                        Some(content) => {
                            self.add_chapter_stat(
                                title,
                                &content,
                                text_width,
                                lines_per_screen,
                                spine_index.unwrap_or(0),
                                is_top_level,
                            );
                        }
                        None => {
                            error!(
                                "BookStat: Failed to get content for chapter '{title}' with href '{href}'"
                            );
                        }
                    }
                }
                crate::table_of_contents::TocItem::Section {
                    title,
                    href,
                    children,
                    ..
                } => {
                    // Process section if it has content
                    if let Some(href_str) = href {
                        // For sections, we need to find the actual chapter index from the epub spine
                        let current_page = epub.get_current_chapter();

                        // Try to find the section in the spine
                        let mut spine_index = None;
                        for (idx, spine_item) in epub.spine.iter().enumerate() {
                            if let Some(resource) = epub.resources.get(&spine_item.idref) {
                                let path_str = resource.path.to_string_lossy();
                                if path_str.ends_with(href_str) || href_str.ends_with(&*path_str) {
                                    spine_index = Some(idx);
                                    break;
                                }
                            }
                        }

                        let content_result = if let Some(idx) = spine_index {
                            // Use found index
                            if epub.set_current_chapter(idx) {
                                epub.get_current_str().map(|(content, _)| content)
                            } else {
                                None
                            }
                        } else {
                            // Try to find resource by path as fallback
                            let matching_id = epub
                                .resources
                                .iter()
                                .find(|(_, resource)| {
                                    resource.path.to_string_lossy() == *href_str
                                        || resource.path.to_string_lossy().ends_with(href_str)
                                })
                                .map(|(id, _)| id.clone());

                            if let Some(id) = matching_id {
                                epub.get_resource_str(&id).map(|(content, _)| content)
                            } else {
                                None
                            }
                        };

                        // Restore original page
                        epub.set_current_chapter(current_page);

                        match content_result {
                            Some(content) => {
                                self.add_chapter_stat(
                                    title,
                                    &content,
                                    text_width,
                                    lines_per_screen,
                                    spine_index.unwrap_or(0),
                                    is_top_level,
                                );
                            }
                            None => {
                                error!(
                                    "BookStat: Failed to get content for section '{title}' with href '{href_str}'"
                                );
                            }
                        }
                    } else if !children.is_empty() {
                        self.process_toc_items_recursive(
                            children,
                            epub,
                            text_width,
                            lines_per_screen,
                            false,
                        );
                    }
                }
            }
        }
    }

    fn add_chapter_stat(
        &mut self,
        title: &str,
        content: &str,
        text_width: usize,
        lines_per_screen: usize,
        chapter_index: usize,
        is_top_level: bool,
    ) {
        // Convert HTML to Markdown AST
        let mut converter = HtmlToMarkdownConverter::new();
        let document = converter.convert(content);

        // Render to text
        let renderer = MarkdownRenderer::new();
        let rendered_text = renderer.render(&document);

        // Calculate screens based on rendered text
        let screens = self.calculate_screens(&rendered_text, text_width, lines_per_screen);

        if is_top_level {
            // Only add top-level chapters to the visible stats list
            self.chapter_stats.push(ChapterStat {
                title: title.to_string(),
                screens,
                chapter_index,
                is_top_level,
            });
        } else {
            // For nested sections, contribute screens to the parent top-level chapter
            // Find the last top-level chapter and add screens to it
            if let Some(last_top_level) = self
                .chapter_stats
                .iter_mut()
                .rev()
                .find(|stat| stat.is_top_level)
            {
                last_top_level.screens += screens;
            }
        }
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
        let total_screens: usize = self.chapter_stats.iter().map(|s| s.screens).sum();
        let mut cumulative_screens = 0;

        // Create the list items
        let items: Vec<ListItem> = if self.chapter_stats.is_empty() {
            // Show a message if no chapters found
            vec![ListItem::new(vec![Line::from(vec![Span::styled(
                "No chapters found. Processing...",
                Style::default().fg(OCEANIC_NEXT.base_0a),
            )])])]
        } else {
            self.chapter_stats
                .iter()
                .map(|stat| {
                    // Calculate percentage read before this chapter
                    let percentage = if total_screens > 0 {
                        (cumulative_screens * 100) / total_screens
                    } else {
                        0
                    };

                    // Update cumulative for next iteration
                    cumulative_screens += stat.screens;

                    let screens_text = if stat.screens == 1 {
                        "1 screen".to_string()
                    } else {
                        format!("{} screens", stat.screens)
                    };

                    let content = vec![Line::from(vec![
                        Span::styled(
                            format!("{percentage:3}% "),
                            Style::default().fg(OCEANIC_NEXT.base_03),
                        ),
                        Span::raw(stat.title.replace("\n", " ")),
                        Span::raw(" "),
                        Span::styled(
                            format!("[{screens_text}]"),
                            Style::default().fg(OCEANIC_NEXT.base_0c),
                        ),
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
                    .border_style(Style::default().fg(OCEANIC_NEXT.base_0c))
                    .style(Style::default().bg(OCEANIC_NEXT.base_00)),
            )
            .highlight_style(
                Style::default()
                    .bg(OCEANIC_NEXT.base_02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("» ");

        // Render the list
        frame.render_stateful_widget(list, popup_area, &mut self.list_state);

        // Add help text at the bottom
        let help_text =
            "j/k/Scroll: Navigate | Enter/DblClick: Jump | G/gg: Bottom/Top | Esc: Close";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(OCEANIC_NEXT.base_03))
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
            KeyCode::Char('j') => {
                self.handle_j();
                None
            }
            KeyCode::Char('k') => {
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
