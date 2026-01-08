use crate::comments::{BookComments, Comment, CommentTarget};
use crate::inputs::KeySeq;
use crate::main_app::VimNavMotions;
use crate::markdown::Inline;
use crate::search::{SearchMode, SearchState, SearchablePanel, find_matches_in_text};
use crate::table_of_contents::TocItem;
use crate::theme::current_theme;
use epub::doc::EpubDoc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use textwrap::wrap;

pub enum CommentsViewerAction {
    JumpToComment {
        chapter_href: String,
        target: CommentTarget,
    },
    DeleteSelectedComment,
    ExportComments {
        filename: String,
    },
    Close,
}

pub struct CommentsViewer {
    all_entries: Vec<CommentEntry>,
    rendered_entries: Vec<CommentEntry>,
    scroll_offset: usize,
    selected_index: usize,
    search_state: SearchState,
    last_popup_area: Option<Rect>,
    last_position: Option<(usize, usize)>,
    total_rendered_lines: usize,
    chapters: Vec<ChapterDisplay>,
    chapter_scroll_offset: usize,
    selected_chapter_index: usize,
    focus: ViewerFocus,
    chapter_positions: HashMap<String, (usize, usize)>,
    last_chapter_area: Option<Rect>,
    last_comments_area: Option<Rect>,
    global_search_mode: bool,
    saved_chapter_index: usize,
    global_position: Option<(usize, usize)>,
    // Export mode
    export_mode: bool,
    export_filename: String,
    // Cached document for export
    doc_cache: HashMap<String, crate::markdown::Document>,
}

#[derive(Clone)]
pub struct ChapterDisplay {
    pub title: String,
    pub href: Option<String>,
    pub depth: usize,
    pub comment_count: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum ViewerFocus {
    Chapters,
    Comments,
}

#[derive(Clone)]
pub struct CommentEntry {
    pub chapter_title: String,
    pub chapter_href: String,
    pub quoted_text: String,
    pub comments: Vec<Comment>,
    pub render_start_line: usize,
    pub render_end_line: usize,
}

impl CommentEntry {
    pub fn primary_comment(&self) -> &Comment {
        self.comments
            .first()
            .expect("CommentEntry should contain at least one comment")
    }

    pub fn is_code_block(&self) -> bool {
        self.primary_comment().target.is_code_block()
    }

    pub fn comment_count(&self) -> usize {
        self.comments.len()
    }

    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }
}

impl CommentsViewer {
    pub fn new(
        comments: Arc<Mutex<BookComments>>,
        epub: &mut EpubDoc<BufReader<std::fs::File>>,
        toc_items: &[TocItem],
        current_chapter_href: Option<String>,
    ) -> Self {
        let (all_entries, doc_cache) = Self::build_entries(comments.clone(), epub, toc_items);
        let chapters = Self::build_chapter_index(toc_items, &all_entries);
        let initial_chapter =
            Self::initial_chapter_index(current_chapter_href.as_deref(), &chapters);

        let mut viewer = CommentsViewer {
            all_entries: all_entries.clone(),
            rendered_entries: Vec::new(),
            scroll_offset: 0,
            selected_index: 0,
            search_state: SearchState::new(),
            last_popup_area: None,
            last_position: Some((0, 0)),
            total_rendered_lines: 0,
            chapters,
            chapter_scroll_offset: 0,
            selected_chapter_index: initial_chapter,
            focus: ViewerFocus::Comments,
            chapter_positions: HashMap::new(),
            last_chapter_area: None,
            last_comments_area: None,
            global_search_mode: false,
            saved_chapter_index: initial_chapter,
            global_position: None,
            export_mode: false,
            export_filename: String::new(),
            doc_cache,
        };

        viewer.update_visible_entries();
        viewer.restore_position();
        viewer
    }

    pub fn restore_position(&mut self) {
        if self.global_search_mode {
            if let Some((scroll, selection)) = self.global_position {
                self.scroll_offset = scroll.min(self.total_rendered_lines.saturating_sub(1));
                self.selected_index = selection.min(self.rendered_entries.len().saturating_sub(1));
                return;
            }
        } else if let Some(key) = self.current_chapter_key() {
            if let Some((scroll, selection)) = self.chapter_positions.get(&key).copied() {
                self.scroll_offset = scroll.min(self.total_rendered_lines.saturating_sub(1));
                self.selected_index = selection.min(self.rendered_entries.len().saturating_sub(1));
                return;
            }
        }

        if let Some((scroll, selection)) = if self.global_search_mode {
            self.global_position
        } else {
            self.last_position
        } {
            self.scroll_offset = scroll.min(self.total_rendered_lines.saturating_sub(1));
            self.selected_index = selection.min(self.rendered_entries.len().saturating_sub(1));
        }
    }

    pub fn save_position(&mut self) {
        if self.global_search_mode {
            self.global_position = Some((self.scroll_offset, self.selected_index));
        } else if let Some(key) = self.current_chapter_key() {
            self.chapter_positions
                .insert(key, (self.scroll_offset, self.selected_index));
        } else {
            self.last_position = Some((self.scroll_offset, self.selected_index));
        }
    }

    fn current_chapter_key(&self) -> Option<String> {
        if self.global_search_mode {
            None
        } else {
            self.chapters
                .get(self.selected_chapter_index)
                .and_then(|chapter| chapter.href.clone())
        }
    }

    fn update_visible_entries(&mut self) {
        if self.global_search_mode {
            self.rendered_entries = self.all_entries.clone();
        } else if let Some(href) = self.current_chapter_key() {
            self.rendered_entries = self
                .all_entries
                .iter()
                .filter(|entry| entry.chapter_href == href)
                .cloned()
                .collect();
        } else {
            self.rendered_entries.clear();
        }

        self.scroll_offset = 0;
        self.selected_index = 0;
        self.total_rendered_lines = 0;
    }

    fn select_chapter(&mut self, new_index: usize) {
        if self.global_search_mode
            || new_index >= self.chapters.len()
            || new_index == self.selected_chapter_index
        {
            return;
        }

        self.save_position();
        self.selected_chapter_index = new_index;
        self.update_visible_entries();
        self.restore_position();
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            ViewerFocus::Chapters => ViewerFocus::Comments,
            ViewerFocus::Comments => ViewerFocus::Chapters,
        };
    }

    fn toggle_global_search_mode(&mut self) {
        if self.global_search_mode {
            self.global_search_mode = false;
            self.selected_chapter_index = self
                .saved_chapter_index
                .min(self.chapters.len().saturating_sub(1));
            self.update_visible_entries();
            self.restore_position();
        } else {
            self.save_position();
            self.saved_chapter_index = self.selected_chapter_index;
            self.global_search_mode = true;
            self.focus = ViewerFocus::Comments;
            self.rendered_entries = self.all_entries.clone();
            self.scroll_offset = 0;
            self.selected_index = 0;
            self.total_rendered_lines = 0;
            self.restore_position();
        }
    }

    fn move_chapter_down(&mut self) {
        if self.global_search_mode {
            return;
        }
        if self.selected_chapter_index + 1 < self.chapters.len() {
            self.select_chapter(self.selected_chapter_index + 1);
        }
    }

    fn move_chapter_up(&mut self) {
        if self.global_search_mode {
            return;
        }
        if self.selected_chapter_index > 0 {
            self.select_chapter(self.selected_chapter_index - 1);
        }
    }

    fn page_chapters_down(&mut self) {
        if self.global_search_mode {
            return;
        }
        let page = self
            .last_chapter_area
            .map(|area| area.height as usize)
            .filter(|h| *h > 0)
            .unwrap_or(5);
        let target =
            (self.selected_chapter_index + page).min(self.chapters.len().saturating_sub(1));
        self.select_chapter(target);
    }

    fn page_chapters_up(&mut self) {
        if self.global_search_mode {
            return;
        }
        let page = self
            .last_chapter_area
            .map(|area| area.height as usize)
            .filter(|h| *h > 0)
            .unwrap_or(5);
        let target = self.selected_chapter_index.saturating_sub(page);
        self.select_chapter(target);
    }

    fn recalculate_entry_layout(&mut self, content_width: usize, content_height: usize) {
        if self.rendered_entries.is_empty() {
            self.total_rendered_lines = 0;
            self.scroll_offset = 0;
            return;
        }
        if content_width == 0 {
            return;
        }

        let mut current_line = 0;
        let mut last_chapter_href: Option<String> = None;

        for entry in self.rendered_entries.iter_mut() {
            let show_chapter_header = last_chapter_href.as_ref() != Some(&entry.chapter_href);
            let entry_height =
                Self::calculate_entry_height_for_width(entry, content_width, show_chapter_header);
            entry.render_start_line = current_line;
            entry.render_end_line = current_line + entry_height;
            current_line = entry.render_end_line;
            last_chapter_href = Some(entry.chapter_href.clone());
        }

        self.total_rendered_lines = current_line;
        let max_scroll = self.total_rendered_lines.saturating_sub(content_height);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    fn build_entries(
        comments: Arc<Mutex<BookComments>>,
        epub: &mut EpubDoc<BufReader<std::fs::File>>,
        toc_items: &[TocItem],
    ) -> (
        Vec<CommentEntry>,
        HashMap<String, crate::markdown::Document>,
    ) {
        use crate::markdown::Document;
        use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;

        let comments_guard = comments.lock().unwrap();
        let all_comments = comments_guard.get_all_comments();

        if all_comments.is_empty() {
            return (Vec::new(), HashMap::new());
        }

        // Cache parsed documents by chapter href to avoid re-parsing
        let mut doc_cache: HashMap<String, Document> = HashMap::new();
        let original_chapter = epub.get_current_chapter();

        // Pre-parse all unique chapters
        let unique_hrefs: HashSet<_> = all_comments
            .iter()
            .map(|c| c.chapter_href.clone())
            .collect();
        for href in unique_hrefs {
            let chapter_path = std::path::PathBuf::from(&href);
            if let Some(chapter_id) = epub.resource_uri_to_chapter(&chapter_path) {
                if epub.set_current_chapter(chapter_id) {
                    if let Some((content, _)) = epub.get_current_str() {
                        let mut converter = HtmlToMarkdownConverter::new();
                        let doc = converter.convert(&content);
                        doc_cache.insert(href, doc);
                    }
                }
            }
        }
        let _ = epub.set_current_chapter(original_chapter);

        let mut entries: Vec<CommentEntry> = Vec::new();
        let mut last_chapter_href: Option<String> = None;
        let mut current_line = 0;
        let mut code_group_map: HashMap<(String, usize), usize> = HashMap::new();

        for comment in all_comments {
            let chapter_title = Self::find_chapter_title(&comment.chapter_href, toc_items, epub);
            let quoted_text = Self::extract_quoted_text_from_cache(
                &doc_cache,
                &comment.chapter_href,
                comment.node_index(),
                comment.target.word_range(),
                comment.target.list_item_index(),
                comment.target.definition_item_index(),
                comment.target.quote_paragraph_index(),
            );

            let entry_index = if comment.target.is_code_block() {
                let key = (comment.chapter_href.clone(), comment.node_index());
                if let Some(&idx) = code_group_map.get(&key) {
                    entries[idx].comments.push(comment.clone());
                    continue;
                } else {
                    let idx = entries.len();
                    code_group_map.insert(key, idx);
                    idx
                }
            } else {
                entries.len()
            };

            if entry_index == entries.len() {
                let show_chapter_header = match &last_chapter_href {
                    Some(prev_href) => prev_href != &comment.chapter_href,
                    None => true,
                };
                let render_start_line = current_line;
                let estimated_height =
                    Self::estimate_entry_height(show_chapter_header, 1, &comment.content);
                let render_end_line = current_line + estimated_height;

                entries.push(CommentEntry {
                    chapter_title,
                    chapter_href: comment.chapter_href.clone(),
                    quoted_text,
                    comments: vec![comment.clone()],
                    render_start_line,
                    render_end_line,
                });

                current_line = render_end_line;
                last_chapter_href = Some(comment.chapter_href.clone());
            }
        }

        (entries, doc_cache)
    }

    fn build_chapter_index(toc_items: &[TocItem], entries: &[CommentEntry]) -> Vec<ChapterDisplay> {
        let mut chapters = Vec::new();
        let mut href_to_index = HashMap::new();
        let mut seen_hrefs = HashSet::new();
        Self::flatten_toc_items(
            toc_items,
            &mut chapters,
            &mut href_to_index,
            &mut seen_hrefs,
        );

        let mut unmatched_counts: HashMap<String, usize> = HashMap::new();
        for entry in entries {
            if let Some(idx) = href_to_index.get(&entry.chapter_href) {
                if let Some(chapter) = chapters.get_mut(*idx) {
                    chapter.comment_count += 1;
                }
            } else {
                *unmatched_counts
                    .entry(entry.chapter_href.clone())
                    .or_default() += 1;
            }
        }

        for (href, count) in unmatched_counts {
            let title = Self::fallback_chapter_title(&href);
            href_to_index.insert(href.clone(), chapters.len());
            seen_hrefs.insert(href.clone());
            chapters.push(ChapterDisplay {
                title,
                href: Some(href),
                depth: 0,
                comment_count: count,
            });
        }

        if chapters.is_empty() {
            chapters.push(ChapterDisplay {
                title: "Chapters".to_string(),
                href: None,
                depth: 0,
                comment_count: 0,
            });
        }

        chapters
    }

    fn normalize_href(href: &str) -> &str {
        href.split('#').next().unwrap_or(href)
    }

    fn chapter_basename(href: &str) -> &str {
        let normalized = Self::normalize_href(href);
        normalized.rsplit('/').next().unwrap_or(normalized)
    }

    fn initial_chapter_index(current_href: Option<&str>, chapters: &[ChapterDisplay]) -> usize {
        if let Some(target) = current_href {
            let normalized_target = Self::normalize_href(target);
            if let Some(idx) = chapters.iter().position(|chapter| {
                chapter.href.as_deref().map(Self::normalize_href) == Some(normalized_target)
            }) {
                return idx;
            }

            let target_basename = Self::chapter_basename(target);
            if let Some(idx) = chapters.iter().position(|chapter| {
                chapter.href.as_deref().map(Self::chapter_basename) == Some(target_basename)
            }) {
                return idx;
            }
        }

        chapters
            .iter()
            .position(|chapter| chapter.comment_count > 0)
            .unwrap_or(0)
    }

    fn flatten_toc_items(
        items: &[TocItem],
        chapters: &mut Vec<ChapterDisplay>,
        href_to_index: &mut HashMap<String, usize>,
        seen_hrefs: &mut HashSet<String>,
    ) {
        for item in items {
            match item {
                TocItem::Chapter { title, href, .. } => {
                    if seen_hrefs.insert(href.clone()) {
                        href_to_index.insert(href.clone(), chapters.len());
                        chapters.push(ChapterDisplay {
                            title: title.clone(),
                            href: Some(href.clone()),
                            depth: 0,
                            comment_count: 0,
                        });
                    }
                }
                TocItem::Section {
                    title,
                    href,
                    children,
                    ..
                } => {
                    if let Some(href) = href {
                        if seen_hrefs.insert(href.clone()) {
                            href_to_index.insert(href.clone(), chapters.len());
                            chapters.push(ChapterDisplay {
                                title: title.clone(),
                                href: Some(href.clone()),
                                depth: 0,
                                comment_count: 0,
                            });
                        }
                    }
                    Self::flatten_toc_items(children, chapters, href_to_index, seen_hrefs);
                }
            }
        }
    }

    fn fallback_chapter_title(href: &str) -> String {
        href.rsplit('/')
            .next_back()
            .unwrap_or(href)
            .trim_end_matches(".xhtml")
            .trim_end_matches(".html")
            .replace(['-', '_'], " ")
    }

    fn find_chapter_title(
        chapter_href: &str,
        toc_items: &[TocItem],
        _epub: &mut EpubDoc<BufReader<std::fs::File>>,
    ) -> String {
        fn search_toc(items: &[TocItem], href: &str) -> Option<String> {
            for item in items {
                match item {
                    TocItem::Chapter {
                        title,
                        href: item_href,
                        ..
                    } => {
                        if item_href == href {
                            return Some(title.clone());
                        }
                    }
                    TocItem::Section {
                        title,
                        href: item_href,
                        children,
                        ..
                    } => {
                        if let Some(h) = item_href {
                            if h == href {
                                return Some(title.clone());
                            }
                        }
                        if let Some(found) = search_toc(children, href) {
                            return Some(found);
                        }
                    }
                }
            }
            None
        }

        if let Some(title) = search_toc(toc_items, chapter_href) {
            return title;
        }

        chapter_href
            .rsplit('/')
            .next_back()
            .unwrap_or(chapter_href)
            .trim_end_matches(".xhtml")
            .trim_end_matches(".html")
            .to_string()
    }

    fn extract_quoted_text_from_cache(
        doc_cache: &HashMap<String, crate::markdown::Document>,
        chapter_href: &str,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
        list_item_index: Option<usize>,
        definition_item_index: Option<usize>,
        quote_paragraph_index: Option<usize>,
    ) -> String {
        use crate::markdown::Block;

        let Some(doc) = doc_cache.get(chapter_href) else {
            return "[Unable to retrieve text]".to_string();
        };

        let Some(node) = doc.blocks.get(paragraph_index) else {
            return "[Unable to retrieve text]".to_string();
        };

        let text = match &node.block {
            Block::Paragraph { content } | Block::Heading { content, .. } => {
                let full_text = Self::extract_text_from_text(content);
                Self::apply_word_range(full_text, word_range)
            }
            Block::List { kind, items } => {
                let (full_text, ranges) = Self::build_list_text_with_ranges(items);
                let item_range = list_item_index.and_then(|idx| ranges.get(idx).copied());
                let adjusted_range =
                    Self::adjust_word_range_for_list(word_range, kind, list_item_index);
                Self::apply_word_range_with_item(full_text, adjusted_range, item_range)
            }
            Block::DefinitionList { items } => {
                let (full_text, ranges) = Self::build_definition_list_text_with_ranges(items);
                let item_range = definition_item_index.and_then(|idx| ranges.get(idx).copied());
                let adjusted_range =
                    Self::adjust_word_range_for_definition_list(word_range, definition_item_index);
                Self::apply_word_range_with_item(full_text, adjusted_range, item_range)
            }
            Block::Quote { content } => {
                let (full_text, ranges) = Self::build_quote_text_with_ranges(content);
                let item_range = quote_paragraph_index.and_then(|idx| ranges.get(idx).copied());
                let adjusted_range =
                    Self::adjust_word_range_for_quote(word_range, quote_paragraph_index);
                Self::apply_word_range_with_item(full_text, adjusted_range, item_range)
            }
            _ => Self::extract_text_from_node(node),
        };

        let max_chars = 80;
        if text.chars().count() > max_chars {
            let truncated: String = text.chars().take(max_chars).collect();
            return format!("{truncated}...");
        }
        text
    }

    fn apply_word_range(text: String, word_range: Option<(usize, usize)>) -> String {
        if let Some((start, end)) = word_range {
            let chars: Vec<char> = text.chars().collect();
            let actual_start = start.min(chars.len());
            let actual_end = end.min(chars.len());
            if actual_start < actual_end {
                return chars[actual_start..actual_end].iter().collect();
            }
        }
        text
    }

    fn apply_word_range_with_item(
        text: String,
        word_range: Option<(usize, usize)>,
        item_range: Option<(usize, usize)>,
    ) -> String {
        let chars: Vec<char> = text.chars().collect();
        let total_len = chars.len();

        let normalize = |(start, end): (usize, usize)| {
            let actual_start = start.min(total_len);
            let actual_end = end.min(total_len);
            if actual_start < actual_end {
                Some((actual_start, actual_end))
            } else {
                None
            }
        };

        let word_range = word_range.and_then(normalize);
        let item_range = item_range.and_then(normalize);

        if let Some((start, end)) = word_range {
            if let Some((item_start, item_end)) = item_range {
                let overlap_start = start.max(item_start);
                let overlap_end = end.min(item_end);
                if overlap_start < overlap_end {
                    return chars[overlap_start..overlap_end].iter().collect();
                }
            } else {
                return chars[start..end].iter().collect();
            }
        }

        if let Some((item_start, item_end)) = item_range {
            return chars[item_start..item_end].iter().collect();
        }

        text
    }

    fn adjust_word_range_for_list(
        word_range: Option<(usize, usize)>,
        kind: &crate::markdown::ListKind,
        item_index: Option<usize>,
    ) -> Option<(usize, usize)> {
        let Some((start, end)) = word_range else {
            return None;
        };
        let Some(item_index) = item_index else {
            return Some((start, end));
        };

        let mut prefix_total = 0usize;
        for idx in 0..=item_index {
            let prefix_len = match kind {
                crate::markdown::ListKind::Unordered => 2, // "• "
                crate::markdown::ListKind::Ordered { start } => {
                    let num = start + idx as u32;
                    format!("{num}. ").chars().count()
                }
            };
            prefix_total += prefix_len;
        }

        Some((
            start.saturating_sub(prefix_total),
            end.saturating_sub(prefix_total),
        ))
    }

    fn adjust_word_range_for_quote(
        word_range: Option<(usize, usize)>,
        quote_paragraph_index: Option<usize>,
    ) -> Option<(usize, usize)> {
        let Some((start, end)) = word_range else {
            return None;
        };
        let Some(para_idx) = quote_paragraph_index else {
            return Some((start, end));
        };
        let prefix_total = 2 * (para_idx + 1); // "> " per paragraph up to and including selection
        Some((
            start.saturating_sub(prefix_total),
            end.saturating_sub(prefix_total),
        ))
    }

    fn adjust_word_range_for_definition_list(
        word_range: Option<(usize, usize)>,
        definition_item_index: Option<usize>,
    ) -> Option<(usize, usize)> {
        let Some((start, end)) = word_range else {
            return None;
        };
        let Some(item_idx) = definition_item_index else {
            return Some((start, end));
        };
        // Definition lists have 4-space indent for definitions ("    ")
        // Each item has: term (no indent) + definitions (4-space each)
        // For simplicity, assume ~1 definition line per item before selection
        let prefix_total = 4 * (item_idx + 1);
        Some((
            start.saturating_sub(prefix_total),
            end.saturating_sub(prefix_total),
        ))
    }

    fn build_list_text_with_ranges(
        items: &[crate::markdown::ListItem],
    ) -> (String, Vec<(usize, usize)>) {
        let mut full_text = String::new();
        let mut ranges = Vec::with_capacity(items.len());
        let mut current_len = 0;

        for (idx, item) in items.iter().enumerate() {
            if idx > 0 {
                full_text.push(' ');
                current_len += 1;
            }
            let item_text = Self::extract_list_item_content(item);
            let item_len = item_text.chars().count();
            let start = current_len;
            let end = start + item_len;
            ranges.push((start, end));
            full_text.push_str(&item_text);
            current_len = end;
        }

        (full_text, ranges)
    }

    fn build_definition_list_text_with_ranges(
        items: &[crate::markdown::DefinitionListItem],
    ) -> (String, Vec<(usize, usize)>) {
        let mut full_text = String::new();
        let mut ranges = Vec::with_capacity(items.len());
        let mut current_len = 0;

        for (idx, item) in items.iter().enumerate() {
            if idx > 0 {
                full_text.push(' ');
                current_len += 1;
            }
            let item_text = Self::extract_definition_item_content(item);
            let item_len = item_text.chars().count();
            let start = current_len;
            let end = start + item_len;
            ranges.push((start, end));
            full_text.push_str(&item_text);
            current_len = end;
        }

        (full_text, ranges)
    }

    fn build_quote_text_with_ranges(
        content: &[crate::markdown::Node],
    ) -> (String, Vec<(usize, usize)>) {
        let mut full_text = String::new();
        let mut ranges = Vec::with_capacity(content.len());
        let mut current_len = 0;

        for (idx, node) in content.iter().enumerate() {
            if idx > 0 {
                full_text.push(' ');
                current_len += 1;
            }
            let node_text = Self::extract_text_from_node(node);
            let node_len = node_text.chars().count();
            let start = current_len;
            let end = start + node_len;
            ranges.push((start, end));
            full_text.push_str(&node_text);
            current_len = end;
        }

        (full_text, ranges)
    }

    fn extract_list_item_content(item: &crate::markdown::ListItem) -> String {
        item.content
            .iter()
            .map(Self::extract_text_from_node)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn extract_definition_item_content(item: &crate::markdown::DefinitionListItem) -> String {
        let term_text = Self::extract_text_from_text(&item.term);
        let def_texts: Vec<String> = item
            .definitions
            .iter()
            .flat_map(|def| def.iter().map(Self::extract_text_from_node))
            .collect();
        std::iter::once(term_text)
            .chain(def_texts)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn extract_text_from_node(node: &crate::markdown::Node) -> String {
        use crate::markdown::Block;

        match &node.block {
            Block::Paragraph { content } | Block::Heading { content, .. } => {
                Self::extract_text_from_text(content)
            }
            Block::CodeBlock { content, .. } => content.clone(),
            Block::Quote { content } => content
                .iter()
                .map(Self::extract_text_from_node)
                .collect::<Vec<_>>()
                .join(" "),
            Block::List { items, .. } => items
                .iter()
                .flat_map(|item| item.content.iter().map(Self::extract_text_from_node))
                .collect::<Vec<_>>()
                .join(" "),
            Block::DefinitionList { items } => items
                .iter()
                .flat_map(|item| {
                    let term_text = Self::extract_text_from_text(&item.term);
                    let def_texts: Vec<String> = item
                        .definitions
                        .iter()
                        .flat_map(|def| def.iter().map(Self::extract_text_from_node))
                        .collect();
                    std::iter::once(term_text).chain(def_texts)
                })
                .collect::<Vec<_>>()
                .join(" "),
            Block::Table { header, rows, .. } => {
                use crate::markdown::TableCellContent;
                let mut texts = Vec::new();
                if let Some(header_row) = header {
                    for cell in &header_row.cells {
                        match &cell.content {
                            TableCellContent::Simple(text) => {
                                texts.push(Self::extract_text_from_text(text));
                            }
                            TableCellContent::Rich(nodes) => {
                                for node in nodes {
                                    texts.push(Self::extract_text_from_node(node));
                                }
                            }
                        }
                    }
                }
                for row in rows {
                    for cell in &row.cells {
                        match &cell.content {
                            TableCellContent::Simple(text) => {
                                texts.push(Self::extract_text_from_text(text));
                            }
                            TableCellContent::Rich(nodes) => {
                                for node in nodes {
                                    texts.push(Self::extract_text_from_node(node));
                                }
                            }
                        }
                    }
                }
                texts.join(" ")
            }
            _ => String::new(),
        }
    }

    fn extract_text_from_text(text: &crate::markdown::Text) -> String {
        let mut result = String::new();
        for item in text.iter() {
            match item {
                crate::markdown::TextOrInline::Text(txt) => result.push_str(&txt.content),
                crate::markdown::TextOrInline::Inline(inline) => match inline {
                    Inline::Link { text, .. } => {
                        result.push_str(&Self::extract_text_from_text(text));
                    }
                    Inline::Image { alt_text, .. } => {
                        result.push_str(alt_text);
                    }
                    Inline::LineBreak | Inline::SoftBreak => result.push(' '),
                    _ => {}
                },
            }
        }
        result
    }

    fn estimate_entry_height(
        show_chapter_header: bool,
        comment_count: usize,
        comment_content: &str,
    ) -> usize {
        let mut height = 0;
        if show_chapter_header {
            height += 1; // Chapter title line
        }
        height += comment_content.lines().count().max(1); // Quoted text
        height += comment_count; // Headers/timestamps
        height += comment_count * comment_content.lines().count().max(1); // Approximate content
        height += 1; // Empty separator line
        height
    }

    fn calculate_entry_height_for_width(
        entry: &CommentEntry,
        content_width: usize,
        show_chapter_header: bool,
    ) -> usize {
        let mut height = 0;
        if show_chapter_header {
            height += Self::wrapped_line_count(&entry.chapter_title, content_width);
        }

        let quote_width = content_width.saturating_sub(4);
        height += Self::wrapped_line_count(&entry.quoted_text, quote_width);

        let comment_width = content_width.saturating_sub(2);
        for (idx, comment) in entry.comments().iter().enumerate() {
            let header_text = Self::comment_header_text(entry, idx, comment);
            height += Self::wrapped_line_count(&header_text, comment_width);

            for line in comment.content.lines() {
                height += Self::wrapped_line_count(line, comment_width);
            }

            if idx < entry.comment_count().saturating_sub(1) {
                height += 1;
            }
        }

        height += 1; // Blank line separator
        height
    }

    fn comment_header_text(entry: &CommentEntry, idx: usize, comment: &Comment) -> String {
        let timestamp = comment.updated_at.format("%m-%d-%y %H:%M").to_string();
        let mut header = if let Some(line_range) = comment.target.line_range() {
            if line_range.0 == line_range.1 {
                format!("Line {} // {}", line_range.0 + 1, timestamp)
            } else {
                format!(
                    "Lines {}-{} // {}",
                    line_range.0 + 1,
                    line_range.1 + 1,
                    timestamp
                )
            }
        } else {
            format!("Note // {timestamp}")
        };

        if entry.comment_count() > 1 {
            header = format!("[{}] {header}", idx + 1);
        }

        header
    }

    fn wrap_text(text: &str, width: usize) -> Vec<String> {
        let effective_width = width.max(1);
        let mut wrapped_lines = Vec::new();

        for line in text.split('\n') {
            let normalized = line.trim_end_matches('\r');
            if normalized.is_empty() {
                wrapped_lines.push(String::new());
                continue;
            }
            for segment in wrap(normalized, effective_width) {
                wrapped_lines.push(segment.into_owned());
            }
        }

        if wrapped_lines.is_empty() {
            wrapped_lines.push(String::new());
        }

        wrapped_lines
    }

    fn wrapped_line_count(text: &str, width: usize) -> usize {
        Self::wrap_text(text, width).len()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(85, 90, area);
        self.last_popup_area = Some(popup_area);
        f.render_widget(Clear, popup_area);

        let total_comments: usize = self
            .chapters
            .iter()
            .map(|chapter| chapter.comment_count)
            .sum();
        let title = format!(" All Comments ({total_comments}) ");
        let outer_block = self.build_outer_block(title);
        let inner_area = outer_block.inner(popup_area);
        f.render_widget(outer_block, popup_area);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(inner_area);

        let chapter_block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(current_theme().base_02));
        let chapter_inner = chapter_block.inner(columns[0]);
        self.last_chapter_area = Some(chapter_inner);
        f.render_widget(chapter_block, columns[0]);
        self.render_chapter_list(f, chapter_inner);

        self.last_comments_area = Some(columns[1]);
        self.render_comments_area(f, columns[1]);
    }

    fn truncate_with_ellipsis(text: &str, max_len: usize) -> String {
        if max_len == 0 {
            return String::new();
        }
        let mut truncated = String::new();
        let mut chars = text.chars();
        for _ in 0..max_len.saturating_sub(1) {
            if let Some(ch) = chars.next() {
                truncated.push(ch);
            } else {
                return text.to_string();
            }
        }
        truncated.push('…');
        truncated
    }

    fn build_outer_block(&self, title: String) -> Block<'static> {
        if self.export_mode {
            let export_text = format!("Export to: {}_ ", self.export_filename);
            Block::default()
                .title(title)
                .title_bottom(
                    Line::from(Span::styled(
                        export_text,
                        Style::default()
                            .fg(current_theme().base_0b)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .right_aligned(),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(current_theme().base_0d))
                .style(Style::default().bg(current_theme().base_00))
        } else if self.search_state.active {
            let search_text = if self.search_state.mode == SearchMode::InputMode {
                format!("/{}", self.search_state.query)
            } else {
                format!(
                    "/{} {}",
                    self.search_state.query,
                    self.search_state.get_match_info()
                )
            };
            Block::default()
                .title(title)
                .title_bottom(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        search_text,
                        Style::default()
                            .fg(current_theme().base_0a)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(current_theme().base_0d))
                .style(Style::default().bg(current_theme().base_00))
        } else {
            // Show hint when not in search or export mode (right-aligned)
            Block::default()
                .title(title)
                .title_bottom(
                    Line::from(vec![
                        Span::styled(
                            "Space+e ",
                            Style::default()
                                .fg(current_theme().base_03)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("export", Style::default().fg(current_theme().base_04)),
                        Span::raw("  "),
                        Span::styled(
                            "/ ",
                            Style::default()
                                .fg(current_theme().base_03)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("search", Style::default().fg(current_theme().base_04)),
                        Span::raw("  "),
                        Span::styled(
                            "? ",
                            Style::default()
                                .fg(current_theme().base_03)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "all chapters ",
                            Style::default().fg(current_theme().base_04),
                        ),
                    ])
                    .right_aligned(),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(current_theme().base_0d))
                .style(Style::default().bg(current_theme().base_00))
        }
    }

    fn render_chapter_list(&mut self, f: &mut Frame, area: Rect) {
        if self.global_search_mode {
            let total = self.all_entries.len();
            let mut style = Style::default().fg(current_theme().base_05);
            let mut count_style = Style::default()
                .fg(current_theme().base_0e)
                .add_modifier(Modifier::BOLD);
            let mut background = Style::default().bg(current_theme().base_00);

            if self.focus == ViewerFocus::Chapters {
                style = Style::default()
                    .fg(current_theme().base_00)
                    .bg(current_theme().base_02)
                    .add_modifier(Modifier::BOLD);
                count_style = Style::default()
                    .fg(current_theme().base_00)
                    .bg(current_theme().base_02)
                    .add_modifier(Modifier::BOLD);
                background = Style::default().bg(current_theme().base_02);
            }

            let line = Line::from(vec![
                Span::styled(format!("({total})"), count_style),
                Span::raw(" "),
                Span::styled("Comments Search", style),
            ])
            .bg(background.bg.unwrap_or(current_theme().base_00));

            let paragraph =
                Paragraph::new(vec![line]).style(Style::default().bg(current_theme().base_00));
            f.render_widget(paragraph, area);
            return;
        }

        if self.chapters.is_empty() {
            return;
        }

        let visible_height = area.height as usize;
        if visible_height == 0 {
            return;
        }

        if self.selected_chapter_index < self.chapter_scroll_offset {
            self.chapter_scroll_offset = self.selected_chapter_index;
        } else if self.selected_chapter_index >= self.chapter_scroll_offset + visible_height {
            self.chapter_scroll_offset = self.selected_chapter_index + 1 - visible_height;
        }

        let max_title_width = area.width.saturating_sub(4) as usize;
        let mut lines = Vec::new();

        for (idx, chapter) in self
            .chapters
            .iter()
            .enumerate()
            .skip(self.chapter_scroll_offset)
            .take(visible_height)
        {
            let is_selected = idx == self.selected_chapter_index;
            let mut title = format!("{}{}", "  ".repeat(chapter.depth.min(4)), chapter.title);
            if title.len() > max_title_width {
                title = Self::truncate_with_ellipsis(&title, max_title_width);
            }

            let base_style = if chapter.comment_count == 0 {
                Style::default().fg(current_theme().base_03)
            } else {
                Style::default().fg(current_theme().base_05)
            };

            let mut title_style = base_style;
            let mut count_style = if chapter.comment_count == 0 {
                Style::default().fg(current_theme().base_03)
            } else {
                Style::default()
                    .fg(current_theme().base_0e)
                    .add_modifier(Modifier::BOLD)
            };
            let mut background_style = Style::default().bg(current_theme().base_00);
            if self.focus == ViewerFocus::Chapters {
                if is_selected {
                    title_style = Style::default()
                        .fg(current_theme().base_00)
                        .bg(current_theme().base_02)
                        .add_modifier(Modifier::BOLD);
                    count_style = count_style
                        .fg(current_theme().base_00)
                        .bg(current_theme().base_02);
                    background_style = Style::default().bg(current_theme().base_02);
                }
            } else if is_selected {
                title_style = Style::default()
                    .fg(current_theme().base_08)
                    .add_modifier(Modifier::BOLD);
                count_style = Style::default()
                    .fg(current_theme().base_0d)
                    .add_modifier(Modifier::BOLD);
            };
            let count_text = if chapter.comment_count == 0 {
                "  (0)".to_string()
            } else {
                format!(" ({})", chapter.comment_count)
            };

            lines.push(
                Line::from(vec![
                    Span::styled(count_text, count_style),
                    Span::raw(" "),
                    Span::styled(title, title_style),
                ])
                .bg(background_style.bg.unwrap_or(current_theme().base_00)),
            );
        }

        let paragraph = Paragraph::new(lines).style(Style::default().bg(current_theme().base_00));
        f.render_widget(paragraph, area);
    }

    fn render_comments_area(&mut self, f: &mut Frame, area: Rect) {
        let content_width = area.width.saturating_sub(2) as usize;
        let content_height = area.height.saturating_sub(2) as usize;
        self.recalculate_entry_layout(content_width, content_height);

        if self.rendered_entries.is_empty() {
            self.render_empty_state(f, area);
            return;
        }

        let mut lines = Vec::new();
        for (idx, entry) in self.rendered_entries.iter().enumerate() {
            let is_selected = self.selected_index == idx;
            let show_header =
                idx == 0 || self.rendered_entries[idx - 1].chapter_href != entry.chapter_href;
            self.render_entry(
                entry,
                is_selected,
                idx,
                show_header,
                content_width,
                &mut lines,
            );
        }

        let paragraph = Paragraph::new(lines).scroll((self.scroll_offset as u16, 0));
        f.render_widget(paragraph, area);

        if self.total_rendered_lines > content_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(current_theme().base_03));
            let mut scrollbar_state =
                ScrollbarState::new(self.total_rendered_lines).position(self.scroll_offset);
            f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }

    pub fn handle_mouse_scroll(&mut self, column: u16, scroll_amount: i32) -> bool {
        if scroll_amount == 0 {
            return true;
        }

        let steps = scroll_amount.abs().min(10);
        let scroll_down = scroll_amount > 0;

        if let Some(area) = self.last_chapter_area {
            if column >= area.x && column < area.x + area.width {
                self.focus = ViewerFocus::Chapters;
                if !self.global_search_mode {
                    for _ in 0..steps {
                        if scroll_down {
                            self.move_chapter_down();
                        } else {
                            self.move_chapter_up();
                        }
                    }
                }
                return true;
            }
        }

        if let Some(area) = self.last_comments_area {
            if column >= area.x && column < area.x + area.width {
                self.focus = ViewerFocus::Comments;
                if self.rendered_entries.is_empty() {
                    return true;
                }
                for _ in 0..steps {
                    if scroll_down {
                        self.next();
                    } else {
                        self.previous();
                    }
                }
                return true;
            }
        }

        false
    }

    fn render_empty_state(&self, f: &mut Frame, area: Rect) {
        let message = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No comments in this chapter",
                Style::default()
                    .fg(current_theme().base_03)
                    .add_modifier(Modifier::ITALIC),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Select text and press 'a' to add a note",
                Style::default().fg(current_theme().base_04),
            )),
        ])
        .alignment(ratatui::layout::Alignment::Center);

        f.render_widget(message, area);
    }

    fn render_entry(
        &self,
        entry: &CommentEntry,
        is_selected: bool,
        entry_index: usize,
        show_chapter_header: bool,
        content_width: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        let chapter_style = Style::default()
            .fg(current_theme().base_0c)
            .add_modifier(Modifier::BOLD);

        let quote_style = if is_selected {
            Style::default()
                .fg(current_theme().base_04)
                .add_modifier(Modifier::ITALIC)
                .bg(current_theme().base_01)
        } else {
            Style::default()
                .fg(current_theme().base_03)
                .add_modifier(Modifier::ITALIC)
        };

        let comment_style = if is_selected {
            Style::default()
                .fg(current_theme().base_05)
                .bg(current_theme().base_01)
        } else {
            Style::default().fg(current_theme().base_05)
        };

        let timestamp_style = Style::default().fg(current_theme().base_03);

        // Chapter title - only show if this is a new chapter
        if show_chapter_header {
            for wrapped in Self::wrap_text(&entry.chapter_title, content_width) {
                let chapter_line =
                    if self.search_state.active && self.search_state.is_match(entry_index) {
                        self.create_highlighted_spans(&wrapped, entry_index, chapter_style)
                    } else {
                        Line::from(Span::styled(wrapped, chapter_style))
                    };
                lines.push(chapter_line);
            }
        }

        // Quoted text - check for search highlighting
        let quote_prefix = "  > ";
        let quote_width = content_width.saturating_sub(quote_prefix.len());
        for wrapped in Self::wrap_text(&entry.quoted_text, quote_width) {
            let mut spans = vec![Span::raw(quote_prefix)];
            let highlighted = if self.search_state.active && self.search_state.is_match(entry_index)
            {
                self.create_highlighted_text(&wrapped, entry_index, quote_style)
            } else {
                vec![Span::styled(wrapped, quote_style)]
            };
            spans.extend(highlighted);
            lines.push(Line::from(spans));
        }

        // Comment content - handle multiline comments with wrapping
        let comment_prefix = "  ";
        let comment_width = content_width.saturating_sub(comment_prefix.len());
        for (comment_idx, comment) in entry.comments().iter().enumerate() {
            let header_text = Self::comment_header_text(entry, comment_idx, comment);
            for wrapped in Self::wrap_text(&header_text, comment_width) {
                lines.push(Line::from(vec![
                    Span::raw(comment_prefix),
                    Span::styled(wrapped, timestamp_style),
                ]));
            }

            for content_line in comment.content.lines() {
                for wrapped in Self::wrap_text(content_line, comment_width) {
                    let mut spans = vec![Span::raw(comment_prefix)];
                    let highlighted =
                        if self.search_state.active && self.search_state.is_match(entry_index) {
                            self.create_highlighted_text(&wrapped, entry_index, comment_style)
                        } else {
                            vec![Span::styled(wrapped, comment_style)]
                        };
                    spans.extend(highlighted);
                    lines.push(Line::from(spans));
                }
            }

            if comment_idx < entry.comment_count().saturating_sub(1) {
                lines.push(Line::from(""));
            }
        }

        lines.push(Line::from(""));
    }

    fn create_highlighted_spans(
        &self,
        text: &str,
        index: usize,
        base_style: Style,
    ) -> Line<'static> {
        Line::from(self.create_highlighted_text(text, index, base_style))
    }

    fn create_highlighted_text(
        &self,
        text: &str,
        index: usize,
        base_style: Style,
    ) -> Vec<Span<'static>> {
        // Simply search for the query in the text directly
        if !self.search_state.active || self.search_state.query.is_empty() {
            return vec![Span::styled(text.to_string(), base_style)];
        }

        let query_lower = self.search_state.query.to_lowercase();
        let text_lower = text.to_lowercase();
        let mut spans = Vec::new();
        let mut last_end = 0;
        let is_current_match = self.search_state.is_current_match(index);

        // Find all occurrences of the query in this text
        let mut search_start = 0;
        while let Some(pos) = text_lower[search_start..].find(&query_lower) {
            let actual_pos = search_start + pos;

            // Add non-highlighted text before this match
            if actual_pos > last_end {
                spans.push(Span::styled(
                    text[last_end..actual_pos].to_string(),
                    base_style,
                ));
            }

            // Add highlighted match text
            let highlight_style = if is_current_match {
                // Current match: bright yellow background with black text
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                // Other matches: dim yellow background
                Style::default()
                    .bg(Color::Rgb(100, 100, 0))
                    .fg(base_style.fg.unwrap_or(current_theme().base_05))
            };

            let end_pos = actual_pos + self.search_state.query.len();
            spans.push(Span::styled(
                text[actual_pos..end_pos].to_string(),
                highlight_style,
            ));

            last_end = end_pos;
            search_start = actual_pos + 1; // Move forward to allow overlapping matches
        }

        // Add remaining non-highlighted text
        if last_end < text.len() {
            spans.push(Span::styled(text[last_end..].to_string(), base_style));
        }

        // If no matches found, return the full text with base style
        if spans.is_empty() {
            vec![Span::styled(text.to_string(), base_style)]
        } else {
            spans
        }
    }

    pub fn handle_mouse_click(&mut self, x: u16, y: u16) -> bool {
        if let Some(chapter_area) = self.last_chapter_area {
            if x >= chapter_area.x
                && x < chapter_area.x + chapter_area.width
                && y >= chapter_area.y
                && y < chapter_area.y + chapter_area.height
            {
                self.focus = ViewerFocus::Chapters;
                let relative_y = y.saturating_sub(chapter_area.y);
                let target_index = self.chapter_scroll_offset + relative_y as usize;
                if !self.global_search_mode && target_index < self.chapters.len() {
                    self.select_chapter(target_index);
                }
                return false;
            }
        }

        if let Some(comment_area) = self.last_comments_area {
            if x >= comment_area.x
                && x < comment_area.x + comment_area.width
                && y >= comment_area.y
                && y < comment_area.y + comment_area.height
            {
                self.focus = ViewerFocus::Comments;
                let relative_y = y.saturating_sub(comment_area.y);
                let clicked_line = self.scroll_offset + relative_y as usize;

                for (idx, entry) in self.rendered_entries.iter().enumerate() {
                    if clicked_line >= entry.render_start_line
                        && clicked_line < entry.render_end_line
                    {
                        self.selected_index = idx;
                        return true;
                    }
                }
            }
        }

        false
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

    pub fn selected_comment(&self) -> Option<&CommentEntry> {
        self.rendered_entries.get(self.selected_index)
    }

    pub fn remove_selected_comment(&mut self) -> Vec<Comment> {
        if self.rendered_entries.is_empty() {
            return Vec::new();
        }

        let removed = self.rendered_entries.remove(self.selected_index);
        let removed_comments = removed.comments.clone();
        if let Some(idx) = self.all_entries.iter().position(|entry| {
            entry.chapter_href == removed.chapter_href && entry.comments == removed_comments
        }) {
            self.all_entries.remove(idx);
        }

        if let Some(entries) = self.chapters.iter_mut().find(|chapter| {
            chapter
                .href
                .as_deref()
                .map(|href| href == removed.chapter_href)
                .unwrap_or(false)
        }) {
            entries.comment_count = entries.comment_count.saturating_sub(1);
        }

        if self.selected_index >= self.rendered_entries.len() && !self.rendered_entries.is_empty() {
            self.selected_index = self.rendered_entries.len() - 1;
        }

        self.total_rendered_lines = 0;
        self.scroll_offset = self.scroll_offset.min(
            self.rendered_entries
                .last()
                .map(|entry| entry.render_end_line)
                .unwrap_or(0),
        );

        removed_comments
    }

    fn collect_searchable_content(&self) -> Vec<String> {
        self.rendered_entries
            .iter()
            .map(|entry| {
                format!(
                    "{} {} {}",
                    entry.chapter_title,
                    entry.quoted_text,
                    entry
                        .comments
                        .iter()
                        .map(|c| c.content.clone())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect()
    }

    fn scroll_to_selected(&mut self) {
        if let Some(entry) = self.rendered_entries.get(self.selected_index) {
            let content_height = self
                .last_comments_area
                .map(|a| a.height.saturating_sub(2) as usize)
                .unwrap_or(20);

            if entry.render_start_line < self.scroll_offset {
                self.scroll_offset = entry.render_start_line;
            } else if entry.render_end_line > self.scroll_offset + content_height {
                self.scroll_offset = entry.render_end_line.saturating_sub(content_height);
            }

            // Ensure we don't scroll past the end
            let max_scroll = self.total_rendered_lines.saturating_sub(content_height);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        }
    }

    pub fn next(&mut self) {
        if self.rendered_entries.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1).min(self.rendered_entries.len() - 1);
        self.scroll_to_selected();
    }

    pub fn previous(&mut self) {
        if self.rendered_entries.is_empty() {
            return;
        }
        self.selected_index = self.selected_index.saturating_sub(1);
        self.scroll_to_selected();
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

impl VimNavMotions for CommentsViewer {
    fn handle_h(&mut self) {
        self.move_chapter_up();
        self.focus = ViewerFocus::Comments;
    }

    fn handle_j(&mut self) {
        match self.focus {
            ViewerFocus::Chapters => self.move_chapter_down(),
            ViewerFocus::Comments => self.next(),
        }
    }

    fn handle_k(&mut self) {
        match self.focus {
            ViewerFocus::Chapters => self.move_chapter_up(),
            ViewerFocus::Comments => self.previous(),
        }
    }

    fn handle_l(&mut self) {
        self.move_chapter_down();
        self.focus = ViewerFocus::Comments;
    }

    fn handle_ctrl_d(&mut self) {
        match self.focus {
            ViewerFocus::Chapters => self.page_chapters_down(),
            ViewerFocus::Comments => {
                let content_height = self
                    .last_comments_area
                    .map(|a| a.height.saturating_sub(2) as usize)
                    .unwrap_or(20);
                let half_page = content_height / 2;
                for _ in 0..half_page {
                    if self.rendered_entries.is_empty()
                        || self.selected_index >= self.rendered_entries.len() - 1
                    {
                        break;
                    }
                    self.next();
                }
            }
        }
    }

    fn handle_ctrl_u(&mut self) {
        match self.focus {
            ViewerFocus::Chapters => self.page_chapters_up(),
            ViewerFocus::Comments => {
                let content_height = self
                    .last_comments_area
                    .map(|a| a.height.saturating_sub(2) as usize)
                    .unwrap_or(20);
                let half_page = content_height / 2;
                for _ in 0..half_page {
                    if self.selected_index == 0 {
                        break;
                    }
                    self.previous();
                }
            }
        }
    }

    fn handle_gg(&mut self) {
        match self.focus {
            ViewerFocus::Chapters => self.select_chapter(0),
            ViewerFocus::Comments => {
                if !self.rendered_entries.is_empty() {
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                }
            }
        }
    }

    fn handle_upper_g(&mut self) {
        match self.focus {
            ViewerFocus::Chapters => {
                if !self.chapters.is_empty() {
                    self.select_chapter(self.chapters.len() - 1);
                }
            }
            ViewerFocus::Comments => {
                if !self.rendered_entries.is_empty() {
                    self.selected_index = self.rendered_entries.len() - 1;
                    self.scroll_to_selected();
                }
            }
        }
    }
}

impl SearchablePanel for CommentsViewer {
    fn start_search(&mut self) {
        self.search_state.start_search(self.selected_index);
    }

    fn cancel_search(&mut self) {
        let original_pos = self.search_state.cancel_search();
        self.selected_index = original_pos.min(self.rendered_entries.len().saturating_sub(1));
        self.scroll_to_selected();
    }

    fn confirm_search(&mut self) {
        self.search_state.confirm_search();
    }

    fn exit_search(&mut self) {
        self.search_state.exit_search();
    }

    fn update_search_query(&mut self, query: &str) {
        self.search_state.update_query(query.to_string());
        let content = self.collect_searchable_content();
        let matches = find_matches_in_text(query, &content);
        self.search_state.set_matches(matches);

        // Jump to first match
        if let Some(match_index) = self.search_state.get_current_match() {
            self.selected_index = match_index;
            self.scroll_to_selected();
        }
    }

    fn next_match(&mut self) {
        if let Some(match_index) = self.search_state.next_match() {
            self.selected_index = match_index;
            self.scroll_to_selected();
        }
    }

    fn previous_match(&mut self) {
        if let Some(match_index) = self.search_state.previous_match() {
            self.selected_index = match_index;
            self.scroll_to_selected();
        }
    }

    fn get_search_state(&self) -> &SearchState {
        &self.search_state
    }

    fn is_searching(&self) -> bool {
        self.search_state.active
    }

    fn has_matches(&self) -> bool {
        !self.search_state.matches.is_empty()
    }

    fn jump_to_match(&mut self, match_index: usize) {
        self.selected_index = match_index.min(self.rendered_entries.len().saturating_sub(1));
        self.scroll_to_selected();
    }

    fn get_searchable_content(&self) -> Vec<String> {
        self.collect_searchable_content()
    }
}

impl CommentsViewer {
    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<CommentsViewerAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Handle export mode input
        if self.export_mode {
            match key.code {
                KeyCode::Esc => {
                    self.export_mode = false;
                    self.export_filename.clear();
                    None
                }
                KeyCode::Enter => {
                    if !self.export_filename.is_empty() {
                        let filename = self.export_filename.clone();
                        self.export_mode = false;
                        self.export_filename.clear();
                        Some(CommentsViewerAction::ExportComments { filename })
                    } else {
                        None
                    }
                }
                KeyCode::Char(c) => {
                    self.export_filename.push(c);
                    None
                }
                KeyCode::Backspace => {
                    self.export_filename.pop();
                    None
                }
                _ => None,
            }
        } else if self.search_state.active && self.search_state.mode == SearchMode::InputMode {
            match key.code {
                KeyCode::Esc => {
                    self.cancel_search();
                    None
                }
                KeyCode::Enter => {
                    self.confirm_search();
                    None
                }
                KeyCode::Char(c) => {
                    let mut new_query = self.search_state.query.clone();
                    new_query.push(c);
                    self.update_search_query(&new_query);
                    None
                }
                KeyCode::Backspace => {
                    let mut new_query = self.search_state.query.clone();
                    new_query.pop();
                    self.update_search_query(&new_query);
                    None
                }
                _ => None,
            }
        } else {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.handle_j();
                    None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.handle_k();
                    None
                }
                KeyCode::Tab => {
                    self.toggle_focus();
                    None
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.handle_h();
                    None
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    self.handle_l();
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
                KeyCode::Char('/') => {
                    self.start_search();
                    None
                }
                KeyCode::Char('?') => {
                    self.toggle_global_search_mode();
                    None
                }
                KeyCode::Char('n')
                    if self.search_state.active
                        && self.search_state.mode == SearchMode::NavigationMode =>
                {
                    self.next_match();
                    None
                }
                KeyCode::Char('N')
                    if self.search_state.active
                        && self.search_state.mode == SearchMode::NavigationMode =>
                {
                    self.previous_match();
                    None
                }
                KeyCode::Char('e') if key_seq.handle_key('e') == " e" => {
                    key_seq.clear();
                    self.export_mode = true;
                    self.export_filename = "comments.md".to_string();
                    None
                }
                KeyCode::Char('d') => {
                    if key_seq.handle_key('d') == "dd" {
                        key_seq.clear();
                        Some(CommentsViewerAction::DeleteSelectedComment)
                    } else {
                        None
                    }
                }
                KeyCode::Esc => {
                    if self.search_state.active {
                        self.exit_search();
                        None
                    } else {
                        Some(CommentsViewerAction::Close)
                    }
                }
                KeyCode::Enter => {
                    self.selected_comment()
                        .map(|entry| CommentsViewerAction::JumpToComment {
                            chapter_href: entry.chapter_href.clone(),
                            target: entry.primary_comment().target.clone(),
                        })
                }
                KeyCode::Char(' ') => {
                    key_seq.handle_key(' ');
                    None
                }
                _ => None,
            }
        }
    }

    /// Generate markdown export content for all comments
    pub fn generate_export_markdown(&self) -> String {
        let mut output = String::new();
        output.push_str("# Exported Comments\n\n");

        let mut last_chapter: Option<String> = None;

        for entry in &self.all_entries {
            // Add chapter header if changed
            if last_chapter.as_ref() != Some(&entry.chapter_href) {
                if last_chapter.is_some() {
                    output.push_str("---\n\n");
                }
                output.push_str(&format!("## {}\n\n", entry.chapter_title));
                last_chapter = Some(entry.chapter_href.clone());
            }

            // Get full context from doc_cache
            let full_context =
                self.extract_full_context_for_export(&entry.chapter_href, entry.primary_comment());

            // Check if this is a blockquote comment (need nested quotes for the note)
            let is_quote_comment = entry
                .primary_comment()
                .target
                .quote_paragraph_index()
                .is_some();

            // Render the original content
            output.push_str(&full_context);
            output.push('\n');

            // Render comments
            for comment in &entry.comments {
                let timestamp = comment.updated_at.format("%m-%d-%y %H:%M");

                if let Some(line_range) = comment.target.line_range() {
                    // Code block comment with line info
                    if line_range.0 == line_range.1 {
                        output.push_str(&format!("*Line {} // {}*\n", line_range.0 + 1, timestamp));
                    } else {
                        output.push_str(&format!(
                            "*Lines {}-{} // {}*\n",
                            line_range.0 + 1,
                            line_range.1 + 1,
                            timestamp
                        ));
                    }
                } else {
                    output.push_str(&format!("*Note // {}*\n", timestamp));
                }

                // Use nested quote for blockquote comments, regular quote otherwise
                let quote_prefix = if is_quote_comment { ">> " } else { "> " };
                for line in comment.content.lines() {
                    output.push_str(quote_prefix);
                    output.push_str(line);
                    output.push('\n');
                }
                output.push('\n');
            }
        }

        output
    }

    fn extract_full_context_for_export(&self, chapter_href: &str, comment: &Comment) -> String {
        use crate::markdown::Block;

        let Some(doc) = self.doc_cache.get(chapter_href) else {
            return String::new();
        };

        let Some(node) = doc.blocks.get(comment.node_index()) else {
            return String::new();
        };

        match &node.block {
            Block::Paragraph { content } => Self::extract_text_from_text(content),
            Block::Heading { content, level } => {
                let prefix = "#".repeat(*level as usize);
                format!("{} {}", prefix, Self::extract_text_from_text(content))
            }
            Block::List { items, kind } => {
                let mut result = String::new();
                for (idx, item) in items.iter().enumerate() {
                    let prefix = match kind {
                        crate::markdown::ListKind::Unordered => "- ".to_string(),
                        crate::markdown::ListKind::Ordered { start } => {
                            format!("{}. ", start + idx as u32)
                        }
                    };
                    let content = Self::extract_list_item_content(item);
                    result.push_str(&format!("{}{}\n", prefix, content));
                }
                result
            }
            Block::Quote { content } => {
                let mut result = String::new();
                for node in content {
                    let text = Self::extract_text_from_node(node);
                    result.push_str(&format!("> {}\n", text));
                }
                result
            }
            Block::DefinitionList { items } => {
                let mut result = String::new();
                for item in items {
                    let term = Self::extract_text_from_text(&item.term);
                    result.push_str(&format!("**{}**\n", term));
                    for def in &item.definitions {
                        for node in def {
                            let text = Self::extract_text_from_node(node);
                            result.push_str(&format!(": {}\n", text));
                        }
                    }
                }
                result
            }
            Block::CodeBlock {
                content, language, ..
            } => {
                let lang = language.as_deref().unwrap_or("");
                format!("```{}\n{}\n```\n", lang, content)
            }
            Block::Table { header, rows, .. } => self.export_table_as_markdown(header, rows),
            _ => Self::extract_text_from_node(node),
        }
    }

    fn export_table_as_markdown(
        &self,
        header: &Option<crate::markdown::TableRow>,
        rows: &[crate::markdown::TableRow],
    ) -> String {
        use crate::markdown::TableCellContent;

        let mut result = String::new();

        if let Some(header_row) = header {
            result.push('|');
            for cell in &header_row.cells {
                let text = match &cell.content {
                    TableCellContent::Simple(t) => Self::extract_text_from_text(t),
                    TableCellContent::Rich(nodes) => nodes
                        .iter()
                        .map(Self::extract_text_from_node)
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                result.push_str(&format!(" {} |", text));
            }
            result.push('\n');

            // Separator row
            result.push('|');
            for _ in &header_row.cells {
                result.push_str(" --- |");
            }
            result.push('\n');
        }

        for row in rows {
            result.push('|');
            for cell in &row.cells {
                let text = match &cell.content {
                    TableCellContent::Simple(t) => Self::extract_text_from_text(t),
                    TableCellContent::Rich(nodes) => nodes
                        .iter()
                        .map(Self::extract_text_from_node)
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                result.push_str(&format!(" {} |", text));
            }
            result.push('\n');
        }

        result
    }

    pub fn is_in_export_mode(&self) -> bool {
        self.export_mode
    }

    pub fn export_filename(&self) -> &str {
        &self.export_filename
    }
}
