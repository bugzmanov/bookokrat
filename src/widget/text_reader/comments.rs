use super::types::*;
use crate::comments::{BookComments, Comment, CommentTarget};
use crate::markdown_text_reader::text_selection::SelectionPoint;
use crate::theme::Base16Palette;
use log::{debug, warn};
use ratatui::style::Style as RatatuiStyle;
use ratatui::text::Span;
use std::sync::{Arc, Mutex};
use tui_textarea::{Input, Key, TextArea};

type CommentSelection = (String, CommentTarget);

impl crate::markdown_text_reader::MarkdownTextReader {
    pub fn set_book_comments(&mut self, comments: Arc<Mutex<BookComments>>) {
        self.book_comments = Some(comments);
        self.rebuild_chapter_comments();
    }

    /// Rebuild the comment lookup for the current chapter
    pub fn rebuild_chapter_comments(&mut self) {
        self.current_chapter_comments.clear();

        if let Some(chapter_file) = &self.current_chapter_file {
            if let Some(comments_arc) = &self.book_comments {
                if let Ok(comments) = comments_arc.lock() {
                    for comment in comments.get_chapter_comments(chapter_file) {
                        match &comment.target {
                            CommentTarget::ParagraphRange {
                                start_paragraph_index,
                                end_paragraph_index,
                                ..
                            } => {
                                for node_idx in *start_paragraph_index..=*end_paragraph_index {
                                    let list =
                                        self.current_chapter_comments.entry(node_idx).or_default();

                                    let already_exists = list.iter().any(|c| {
                                        c.chapter_href == comment.chapter_href
                                            && c.target == comment.target
                                            && c.content == comment.content
                                    });

                                    if !already_exists {
                                        list.push(comment.clone());
                                    }
                                }
                            }
                            _ => {
                                let list = self
                                    .current_chapter_comments
                                    .entry(comment.node_index())
                                    .or_default();

                                let already_exists = list.iter().any(|c| {
                                    c.chapter_href == comment.chapter_href
                                        && c.target == comment.target
                                        && c.content == comment.content
                                });

                                if !already_exists {
                                    list.push(comment.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Start editing an existing comment
    pub fn start_editing_comment(&mut self, chapter_href: String, target: CommentTarget) -> bool {
        if let Some(comments_arc) = &self.book_comments {
            if let Ok(comments) = comments_arc.lock() {
                let existing_content = comments
                    .get_node_comments(&chapter_href, target.node_index())
                    .iter()
                    .find(|c| c.target == target)
                    .map(|c| c.content.clone());

                if let Some(content) = existing_content {
                    let comment_start_line = self.find_comment_visual_line(&chapter_href, &target);

                    if let Some(start_line) = comment_start_line {
                        let mut textarea = TextArea::default();
                        let lines: Vec<&str> = content.split('\n').collect();
                        for (idx, line) in lines.iter().enumerate() {
                            textarea.insert_str(line);
                            if idx < lines.len().saturating_sub(1) {
                                textarea.insert_newline();
                            }
                        }

                        self.comment_input.textarea = Some(textarea);
                        self.comment_input.target_node_index = Some(target.node_index());
                        self.comment_input.target_line = Some(start_line);
                        self.comment_input.target = Some(target.clone());
                        self.comment_input.edit_mode = Some(CommentEditMode::Editing {
                            chapter_href,
                            target,
                        });

                        self.cache_generation += 1;

                        self.text_selection.clear_selection();
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn start_comment_input(&mut self) -> bool {
        if !self.has_text_selection() {
            return false;
        }

        if let Some((chapter_href, target)) = self.get_comment_at_cursor() {
            return self.start_editing_comment(chapter_href, target);
        }

        if let Some((start, end)) = self.text_selection.get_selection_range() {
            let (norm_start, norm_end) = self.normalize_selection_points(&start, &end);
            if let Some(target) = self.compute_selection_target(&norm_start, &norm_end) {
                let mut textarea = TextArea::default();
                textarea.set_placeholder_text("Type your comment here...");

                self.comment_input.textarea = Some(textarea);
                self.comment_input.target = Some(target.clone());
                self.comment_input.target_node_index = Some(target.node_index());
                self.comment_input
                    .target_line
                    .replace(norm_end.line.saturating_add(1));
                self.comment_input.edit_mode = Some(CommentEditMode::Creating);

                self.text_selection.clear_selection();

                return true;
            }
        }

        false
    }

    fn compute_selection_target(
        &self,
        start: &SelectionPoint,
        end: &SelectionPoint,
    ) -> Option<CommentTarget> {
        if self.rendered_content.lines.is_empty() {
            return None;
        }

        if start.line > end.line {
            return None;
        }

        let start_node_idx = (start.line..=end.line).find_map(|idx| {
            self.rendered_content
                .lines
                .get(idx)
                .and_then(|line| line.node_index)
        })?;

        let mut end_node_idx = start_node_idx;
        let mut is_single_node = true;

        for idx in start.line..=end.line {
            if let Some(line) = self.rendered_content.lines.get(idx) {
                if let Some(line_node_idx) = line.node_index {
                    if line_node_idx != start_node_idx {
                        is_single_node = false;
                        end_node_idx = end_node_idx.max(line_node_idx);
                    }
                }
            }
        }

        if is_single_node {
            let mut has_code = false;
            let mut min_code = usize::MAX;
            let mut max_code = 0;

            for idx in start.line..=end.line {
                if let Some(line) = self.rendered_content.lines.get(idx) {
                    if let Some(meta) = &line.code_line {
                        if meta.node_index == start_node_idx {
                            has_code = true;
                            min_code = min_code.min(meta.line_index);
                            max_code = max_code.max(meta.line_index);
                        }
                    }
                }
            }

            if has_code {
                return Some(CommentTarget::CodeBlock {
                    paragraph_index: start_node_idx,
                    line_range: (min_code, max_code),
                });
            }

            let word_range = self.compute_paragraph_word_range(start_node_idx, start, end);

            Some(CommentTarget::Paragraph {
                paragraph_index: start_node_idx,
                word_range,
            })
        } else {
            let start_word_offset = self.compute_word_offset_in_node(start_node_idx, start);
            let end_word_offset = self.compute_word_offset_in_node(end_node_idx, end);

            Some(CommentTarget::ParagraphRange {
                start_paragraph_index: start_node_idx,
                end_paragraph_index: end_node_idx,
                start_word_offset,
                end_word_offset,
            })
        }
    }

    /// Handle input events when in comment mode
    pub fn handle_comment_input(&mut self, input: Input) -> bool {
        if !self.comment_input.is_active() {
            return false;
        }

        if let Some(textarea) = &mut self.comment_input.textarea {
            match input {
                Input { key: Key::Esc, .. } => {
                    self.save_comment();
                    return true;
                }
                _ => {
                    textarea.input(input);
                    return true;
                }
            }
        }
        false
    }

    pub fn save_comment(&mut self) {
        if let Some(textarea) = &self.comment_input.textarea {
            let comment_text = textarea.lines().join("\n");

            if !comment_text.trim().is_empty() {
                if let Some(target) = self.comment_input.target.clone() {
                    // Extract the selected text before clearing the selection
                    let selected_text = self
                        .text_selection
                        .extract_selected_text(&self.raw_text_lines);

                    if let Some(comments_arc) = &self.book_comments {
                        if let Ok(mut comments) = comments_arc.lock() {
                            use chrono::Utc;

                            if let Some(CommentEditMode::Editing { chapter_href, .. }) =
                                &self.comment_input.edit_mode
                            {
                                if let Err(e) = comments.update_comment(
                                    chapter_href,
                                    &target,
                                    comment_text.clone(),
                                ) {
                                    warn!("Failed to update comment: {e}");
                                } else {
                                    debug!("Updated comment: {comment_text}");
                                }
                            } else if let Some(chapter_file) = &self.current_chapter_file {
                                let comment = Comment {
                                    chapter_href: chapter_file.clone(),
                                    target,
                                    content: comment_text.clone(),
                                    selected_text,
                                    highlight_only: false,
                                    updated_at: Utc::now(),
                                };

                                if let Err(e) = comments.add_comment(comment) {
                                    warn!("Failed to add comment: {e}");
                                } else {
                                    debug!("Saved comment: {comment_text}");
                                }
                            }
                        }
                    }
                }
            }
        }

        self.rebuild_chapter_comments();

        // Clear comment input state AFTER rebuilding so the re-render doesn't try to show textarea
        self.comment_input.clear();

        self.cache_generation += 1;
    }

    /// Create a highlight-only comment (no note text)
    pub fn create_highlight_only(&mut self) -> bool {
        if !self.has_text_selection() {
            return false;
        }

        if let Some((start, end)) = self.text_selection.get_selection_range() {
            let (norm_start, norm_end) = self.normalize_selection_points(&start, &end);
            if let Some(target) = self.compute_selection_target(&norm_start, &norm_end) {
                let selected_text = self
                    .text_selection
                    .extract_selected_text(&self.raw_text_lines);

                if let Some(chapter_file) = &self.current_chapter_file {
                    if let Some(comments_arc) = &self.book_comments {
                        if let Ok(mut comments) = comments_arc.lock() {
                            use chrono::Utc;

                            let comment = Comment {
                                chapter_href: chapter_file.clone(),
                                target,
                                content: String::new(), // No text content for highlight-only
                                selected_text,
                                highlight_only: true,
                                updated_at: Utc::now(),
                            };

                            if let Err(e) = comments.add_comment(comment) {
                                warn!("Failed to add highlight: {e}");
                                return false;
                            } else {
                                debug!("Saved highlight-only comment");
                            }
                        }
                    }
                }

                self.rebuild_chapter_comments();
                self.text_selection.clear_selection();
                self.cache_generation += 1;
                return true;
            }
        }

        false
    }

    /// Check if we're currently in comment input mode
    pub fn is_comment_input_active(&self) -> bool {
        self.comment_input.is_active()
    }

    /// Get comment ID from current text selection
    /// Returns the comment ID if any line in the selection is a comment line
    pub fn get_comment_at_cursor(&self) -> Option<CommentSelection> {
        if let Some((start, end)) = self.text_selection.get_selection_range() {
            // Check all lines in the selection range
            for line_idx in start.line..=end.line {
                if let Some(line) = self.rendered_content.lines.get(line_idx) {
                    if let LineType::Comment {
                        chapter_href,
                        target,
                    } = &line.line_type
                    {
                        return Some((chapter_href.clone(), target.clone()));
                    } else if let LineType::CodeBlock { .. } = &line.line_type {
                        if let Some((chapter, target)) =
                            self.inline_code_comment_hit(line_idx, &start, &end)
                        {
                            return Some((chapter, target));
                        }
                    }
                }
            }
        }

        None
    }

    fn inline_code_comment_hit(
        &self,
        line_idx: usize,
        selection_start: &SelectionPoint,
        selection_end: &SelectionPoint,
    ) -> Option<(String, CommentTarget)> {
        let line = self.rendered_content.lines.get(line_idx)?;
        if line.inline_code_comments.is_empty() {
            return None;
        }

        let line_length = line.raw_text.chars().count();
        let start_col = if line_idx == selection_start.line {
            selection_start.column.min(line_length)
        } else {
            0
        };
        let end_col = if line_idx == selection_end.line {
            selection_end.column.min(line_length)
        } else {
            line_length
        };

        if start_col >= end_col {
            return None;
        }

        for fragment in &line.inline_code_comments {
            if start_col < fragment.end_column && end_col > fragment.start_column {
                return Some((fragment.chapter_href.clone(), fragment.target.clone()));
            }
        }

        None
    }

    /// Delete comment at current selection
    /// Returns true if a comment was deleted
    pub fn delete_comment_at_cursor(&mut self) -> anyhow::Result<bool> {
        if let Some((chapter_href, target)) = self.get_comment_at_cursor() {
            self.delete_comment_by_location(&chapter_href, &target);
            self.text_selection.clear_selection();
            return Ok(true);
        }

        Ok(false)
    }

    pub fn delete_comment_by_location(&mut self, chapter_href: &str, target: &CommentTarget) {
        if let Some(comments_arc) = &self.book_comments {
            if let Ok(mut comments) = comments_arc.lock() {
                let _ = comments.delete_comment(chapter_href, target);
            }
        }
        self.rebuild_chapter_comments();
        self.cache_generation += 1;
    }

    /// Find the visual line where a specific comment starts rendering
    pub fn find_comment_visual_line(
        &self,
        chapter_href: &str,
        target: &CommentTarget,
    ) -> Option<usize> {
        for (idx, line) in self.rendered_content.lines.iter().enumerate() {
            if let LineType::Comment {
                chapter_href: line_href,
                target: line_target,
            } = &line.line_type
            {
                if line_href == chapter_href && line_target == target {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Check if we're currently editing a specific comment
    pub fn is_editing_this_comment(&self, comment: &Comment) -> bool {
        if let Some(CommentEditMode::Editing {
            chapter_href,
            target,
        }) = &self.comment_input.edit_mode
        {
            &comment.chapter_href == chapter_href && &comment.target == target
        } else {
            false
        }
    }

    /// Determines if a comment should be rendered at the given node_idx
    /// For single-paragraph comments, always render
    /// For multi-paragraph comments, only render at the last paragraph in the range
    pub fn should_render_comment_at_node(&self, comment: &Comment, node_idx: usize) -> bool {
        match &comment.target {
            CommentTarget::Paragraph {
                paragraph_index, ..
            } => *paragraph_index == node_idx,
            CommentTarget::CodeBlock {
                paragraph_index, ..
            } => *paragraph_index == node_idx,
            CommentTarget::ParagraphRange {
                end_paragraph_index,
                ..
            } => *end_paragraph_index == node_idx,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_comment_as_quote(
        &mut self,
        comment: &Comment,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        _is_focused: bool,
        indent: usize,
    ) {
        // Skip rendering if this is a highlight-only comment (no text)
        if comment.highlight_only {
            return;
        }

        // Skip rendering if we're currently editing this comment
        if self.is_editing_this_comment(comment) {
            return;
        }

        if !comment.is_paragraph_comment() && !comment.is_paragraph_range_comment() {
            return;
        }

        let comment_header = format!("Note // {}", comment.updated_at.format("%m-%d-%y %H:%M"));

        lines.push(RenderedLine {
            spans: vec![Span::styled(
                comment_header.clone(),
                RatatuiStyle::default().fg(palette.base_0e), // Purple text color
            )],
            raw_text: comment_header.clone(),
            line_type: LineType::Comment {
                chapter_href: comment.chapter_href.clone(),
                target: comment.target.clone(),
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
        });
        self.raw_text_lines.push(comment_header);
        *total_height += 1;

        let quote_prefix = "> ";
        let effective_width = width.saturating_sub(indent + quote_prefix.len());

        let wrapped_lines = textwrap::wrap(&comment.content, effective_width);

        for line in wrapped_lines {
            let quoted_line = format!("{}{}{}", " ".repeat(indent), quote_prefix, line);
            lines.push(RenderedLine {
                spans: vec![Span::styled(
                    quoted_line.clone(),
                    RatatuiStyle::default().fg(palette.base_0e), // Purple text color
                )],
                raw_text: line.to_string(),
                line_type: LineType::Comment {
                    chapter_href: comment.chapter_href.clone(),
                    target: comment.target.clone(),
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
                code_line: None,
                inline_code_comments: Vec::new(),
            });
            self.raw_text_lines.push(quoted_line);
            *total_height += 1;
        }

        // Add empty line after comment
        lines.push(RenderedLine {
            spans: vec![Span::raw("")],
            raw_text: String::new(),
            line_type: LineType::Comment {
                chapter_href: comment.chapter_href.clone(),
                target: comment.target.clone(),
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }

    fn normalize_selection_points(
        &self,
        start: &SelectionPoint,
        end: &SelectionPoint,
    ) -> (SelectionPoint, SelectionPoint) {
        let total_lines = self.rendered_content.lines.len();
        if total_lines == 0 {
            return (start.clone(), end.clone());
        }

        let start_line = start.line.min(total_lines - 1);
        let start_col = start.column.min(self.line_display_length(start_line));

        let mut end_line = end.line.min(total_lines - 1);
        let mut end_col = end.column;

        if end_line > start_line && end_col == 0 {
            end_line = end_line.saturating_sub(1);
            end_col = self.line_display_length(end_line);
        } else {
            end_col = end_col.min(self.line_display_length(end_line));
        }

        (
            SelectionPoint {
                line: start_line,
                column: start_col,
            },
            SelectionPoint {
                line: end_line,
                column: end_col,
            },
        )
    }

    fn line_display_length(&self, line_idx: usize) -> usize {
        self.rendered_content
            .lines
            .get(line_idx)
            .map(|line| line.raw_text.chars().count())
            .unwrap_or(0)
    }

    fn compute_paragraph_word_range(
        &self,
        node_idx: usize,
        start: &SelectionPoint,
        end: &SelectionPoint,
    ) -> Option<(usize, usize)> {
        let mut offsets = Vec::new();
        let mut cumulative = 0;

        for (idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line.node_index == Some(node_idx) {
                let len = line.raw_text.chars().count();
                offsets.push((idx, cumulative, len));
                cumulative += len;
            }
        }

        if offsets.is_empty() {
            return None;
        }

        let total_len = cumulative;

        let start_offset = offsets
            .iter()
            .find(|(line_idx, _, _)| *line_idx == start.line)
            .map(|(_, base, len)| base + start.column.min(*len))?;

        let end_offset = offsets
            .iter()
            .find(|(line_idx, _, _)| *line_idx == end.line)
            .map(|(_, base, len)| base + end.column.min(*len))
            .unwrap_or(total_len);

        if start_offset >= end_offset {
            return None;
        }

        if start_offset == 0 && end_offset >= total_len {
            return None;
        }

        Some((start_offset, end_offset.min(total_len)))
    }

    fn compute_word_offset_in_node(
        &self,
        node_idx: usize,
        point: &SelectionPoint,
    ) -> Option<usize> {
        let mut offsets = Vec::new();
        let mut cumulative = 0;

        for (idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line.node_index == Some(node_idx) {
                let len = line.raw_text.chars().count();
                offsets.push((idx, cumulative, len));
                cumulative += len;
            }
        }

        if offsets.is_empty() {
            return None;
        }

        offsets
            .iter()
            .find(|(line_idx, _, _)| *line_idx == point.line)
            .map(|(_, base, len)| base + point.column.min(*len))
    }
}
