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
        use log::debug;

        self.current_chapter_comments.clear();

        if let Some(chapter_file) = &self.current_chapter_file {
            if let Some(comments_arc) = &self.book_comments {
                if let Ok(comments) = comments_arc.lock() {
                    let chapter_comments = comments.get_chapter_comments(chapter_file);
                    debug!(
                        "Rebuilding comments for chapter {}: found {} comments",
                        chapter_file,
                        chapter_comments.len()
                    );
                    for comment in chapter_comments {
                        debug!(
                            "  Comment at node {}: highlight_only={}, has_word_range={}",
                            comment.node_index(),
                            comment.highlight_only,
                            comment.target.word_range().is_some()
                        );
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

        // After rebuilding, extract missing context for old comments
        self.backfill_missing_context();
    }

    /// Extract context for comments that don't have it
    /// This provides backward compatibility for old comments
    fn backfill_missing_context(&mut self) {
        use log::{debug, info, warn};

        // Check if we have rendered content to extract from
        if self.rendered_content.lines.is_empty() {
            return;
        }

        let chapter_file = match &self.current_chapter_file {
            Some(file) => file.clone(),
            None => return,
        };

        let comments_arc = match &self.book_comments {
            Some(arc) => arc.clone(),
            None => return,
        };

        let mut comments_to_update = Vec::new();

        // Find comments with missing context
        if let Ok(comments) = comments_arc.lock() {
            let chapter_comments = comments.get_chapter_comments(&chapter_file);
            for comment in chapter_comments {
                if comment.context.is_none() {
                    debug!(
                        "Found comment without context at node {}: extracting...",
                        comment.node_index()
                    );
                    if let Some(extracted) = self.extract_context_for_comment(&comment.target) {
                        comments_to_update.push((comment.clone(), extracted));
                    }
                }
            }
        }

        // Update comments with extracted context
        if !comments_to_update.is_empty() {
            info!(
                "Backfilling context for {} comments in chapter {}",
                comments_to_update.len(),
                chapter_file
            );

            if let Ok(mut comments) = comments_arc.lock() {
                for (comment, extracted_context) in comments_to_update {
                    if let Err(e) = comments.update_comment_context(
                        &chapter_file,
                        &comment.target,
                        extracted_context,
                    ) {
                        warn!("Failed to update comment with context: {}", e);
                    }
                }
            }
        }
    }

    /// Extract the highlighted text for a given comment target
    fn extract_context_for_comment(&self, target: &CommentTarget) -> Option<String> {
        use log::debug;

        match target {
            CommentTarget::ParagraphRange {
                start_paragraph_index,
                end_paragraph_index,
                start_word_offset,
                end_word_offset,
                list_item_index,
            } => {
                let start_node = *start_paragraph_index;
                let end_node = *end_paragraph_index;

                // Collect all lines for the node range
                let mut text_parts = Vec::new();

                for node_idx in start_node..=end_node {
                    // If list_item_index is set AND this is the start node,
                    // filter lines to only that specific bullet
                    let node_lines: Vec<&str> = if let Some(target_bullet_idx) = list_item_index {
                        if node_idx == start_node {
                            // For the start node (list), track which bullet each line belongs to
                            let mut current_bullet_idx = None;
                            let mut bullet_counter = 0;

                            self.rendered_content
                                .lines
                                .iter()
                                .filter_map(|line| {
                                    if line.node_index != Some(node_idx) {
                                        return None;
                                    }

                                    // Check if this line starts a new bullet
                                    if matches!(line.line_type, LineType::ListItem { .. }) {
                                        let text = line.raw_text.trim_start();
                                        if text.starts_with('•')
                                            || text.starts_with('-')
                                            || text.chars().next().map_or(false, |c| c.is_numeric())
                                        {
                                            current_bullet_idx = Some(bullet_counter);
                                            bullet_counter += 1;
                                        }
                                    }

                                    // Only include lines that belong to the target bullet
                                    if current_bullet_idx == Some(*target_bullet_idx) {
                                        Some(line.raw_text.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        } else {
                            // For subsequent nodes in a multi-paragraph range, get all lines
                            self.rendered_content
                                .lines
                                .iter()
                                .filter(|line| line.node_index == Some(node_idx))
                                .map(|line| line.raw_text.as_str())
                                .collect()
                        }
                    } else {
                        // No list_item_index - get all lines for this node
                        self.rendered_content
                            .lines
                            .iter()
                            .filter(|line| line.node_index == Some(node_idx))
                            .map(|line| line.raw_text.as_str())
                            .collect()
                    };

                    if node_lines.is_empty() {
                        continue;
                    }

                    // Join lines for this node
                    let node_text = node_lines.join(" ");

                    // For SINGLE-node selections with list_item_index, return just that bullet
                    if start_node == end_node && list_item_index.is_some() {
                        return Some(node_text);
                    }

                    // If this is a single node with word offsets, extract just that range
                    if start_node == end_node
                        && start_word_offset.is_some()
                        && end_word_offset.is_some()
                    {
                        let words: Vec<&str> = node_text.split_whitespace().collect();
                        let start = start_word_offset.unwrap();
                        let end = end_word_offset.unwrap().min(words.len());

                        if start < words.len() {
                            let selected_words = &words[start..end];
                            return Some(selected_words.join(" "));
                        }
                    } else {
                        // Multi-node or full node - add all text
                        text_parts.push(node_text);
                    }
                }

                if text_parts.is_empty() {
                    debug!(
                        "Could not extract context for nodes {}-{}",
                        start_node, end_node
                    );
                    None
                } else {
                    Some(text_parts.join(" "))
                }
            }
            CommentTarget::Paragraph {
                paragraph_index,
                word_range,
            } => {
                // Legacy format - extract using paragraph index and word range
                let node_lines: Vec<&str> = self
                    .rendered_content
                    .lines
                    .iter()
                    .filter(|line| line.node_index == Some(*paragraph_index))
                    .map(|line| line.raw_text.as_str())
                    .collect();

                if node_lines.is_empty() {
                    return None;
                }

                let node_text = node_lines.join(" ");

                if let Some((start, end)) = word_range {
                    let words: Vec<&str> = node_text.split_whitespace().collect();
                    if *start < words.len() {
                        let end_idx = (*end).min(words.len());
                        return Some(words[*start..end_idx].join(" "));
                    }
                }

                Some(node_text)
            }
            CommentTarget::CodeBlock { .. } => {
                // For code blocks, we don't extract context since it's code
                None
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
            let mut is_list_item = false;

            for idx in start.line..=end.line {
                if let Some(line) = self.rendered_content.lines.get(idx) {
                    if let Some(meta) = &line.code_line {
                        if meta.node_index == start_node_idx {
                            has_code = true;
                            min_code = min_code.min(meta.line_index);
                            max_code = max_code.max(meta.line_index);
                        }
                    }
                    // Check if any line in the selection is a list item
                    if matches!(line.line_type, LineType::ListItem { .. }) {
                        is_list_item = true;
                    }
                }
            }

            if has_code {
                return Some(CommentTarget::CodeBlock {
                    paragraph_index: start_node_idx,
                    line_range: (min_code, max_code),
                });
            }

            // For list items, determine which bullet is selected
            let list_item_index = if is_list_item {
                self.determine_list_item_index(start_node_idx, start.line)
            } else {
                None
            };

            // For list items, don't use word ranges - each bullet is atomic
            let word_range = if is_list_item {
                None
            } else {
                self.compute_paragraph_word_range(start_node_idx, start, end)
            };

            Some(CommentTarget::ParagraphRange {
                start_paragraph_index: start_node_idx,
                end_paragraph_index: start_node_idx,
                start_word_offset: word_range.map(|(start, _)| start),
                end_word_offset: word_range.map(|(_, end)| end),
                list_item_index,
            })
        } else {
            // Multi-paragraph range
            // Check if the start node is a list item
            let start_is_list_item = self
                .rendered_content
                .lines
                .get(start.line)
                .map(|line| matches!(line.line_type, LineType::ListItem { .. }))
                .unwrap_or(false);

            // For multi-paragraph selections starting from a list item,
            // determine which bullet to indicate where the selection starts
            let list_item_index = if start_is_list_item {
                self.determine_list_item_index(start_node_idx, start.line)
            } else {
                None
            };

            // For multi-paragraph ranges, don't use word offsets - they're too complex
            // Just indicate the full range of nodes involved
            Some(CommentTarget::ParagraphRange {
                start_paragraph_index: start_node_idx,
                end_paragraph_index: end_node_idx,
                start_word_offset: None,
                end_word_offset: None,
                list_item_index,
            })
        }
    }

    /// Determine which bullet item (0-indexed) contains the given line
    /// Returns None if not in a list or cannot determine
    fn determine_list_item_index(&self, node_idx: usize, line_num: usize) -> Option<usize> {
        // Count how many bullet "starts" we've seen before this line
        // A bullet start is indicated by a line starting with "• " or "1. " etc.
        let mut bullet_count = 0;
        let mut current_bullet = None;

        for (idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line.node_index != Some(node_idx) {
                continue;
            }

            // Check if this line starts a new bullet (has the bullet prefix)
            // List items have their text with the bullet prefix in raw_text
            if matches!(line.line_type, LineType::ListItem { .. }) {
                let text = line.raw_text.trim_start();
                if text.starts_with('•')
                    || text.starts_with('-')
                    || text.chars().next().map_or(false, |c| c.is_numeric())
                {
                    // This is a bullet start line
                    if idx <= line_num {
                        current_bullet = Some(bullet_count);
                        bullet_count += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        current_bullet
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
                                    context: selected_text,
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
                                context: selected_text,
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
        let mut line_data = Vec::new();

        // Collect all lines for this node with their word and character positions
        for (idx, line) in self.rendered_content.lines.iter().enumerate() {
            if line.node_index == Some(node_idx) {
                line_data.push((idx, line.raw_text.clone()));
            }
        }

        if line_data.is_empty() {
            return None;
        }

        // Build cumulative word and character positions for each line
        let mut cumulative_words = 0;
        let mut cumulative_chars = 0;
        let mut line_info = Vec::new();

        for (line_idx, text) in &line_data {
            let char_count = text.chars().count();
            let word_count = Self::count_words_in_text(text);

            line_info.push((
                *line_idx,
                cumulative_words,
                word_count,
                cumulative_chars,
                char_count,
            ));

            cumulative_words += word_count;
            cumulative_chars += char_count;
        }

        let total_words = cumulative_words;

        // Find start word index
        let start_word = line_info
            .iter()
            .find(|(line_idx, _, _, _, _)| *line_idx == start.line)
            .and_then(|(_, base_words, _, _base_chars, char_count)| {
                let line_text = line_data
                    .iter()
                    .find(|(idx, _)| *idx == start.line)
                    .map(|(_, text)| text)?;
                let char_offset = start.column.min(*char_count);
                let word_offset = Self::char_offset_to_word_index(line_text, char_offset);
                Some(base_words + word_offset)
            })?;

        // Find end word index
        let end_word = line_info
            .iter()
            .find(|(line_idx, _, _, _, _)| *line_idx == end.line)
            .and_then(|(_, base_words, _, _, char_count)| {
                let line_text = line_data
                    .iter()
                    .find(|(idx, _)| *idx == end.line)
                    .map(|(_, text)| text)?;
                let char_offset = end.column.min(*char_count);
                let word_offset = Self::char_offset_to_word_index(line_text, char_offset);
                Some(base_words + word_offset)
            })
            .unwrap_or(total_words);

        if start_word >= end_word {
            return None;
        }

        // If we're selecting (nearly) the entire paragraph, don't use word offsets
        // Check if we're starting at or near the beginning (0 or 1) and ending at or near the end
        if start_word <= 1 && end_word >= total_words.saturating_sub(1) {
            return None;
        }

        Some((start_word, end_word))
    }

    /// Count the number of words in a text string
    fn count_words_in_text(text: &str) -> usize {
        let mut word_count = 0;
        let mut in_word = false;

        for ch in text.chars() {
            if ch.is_whitespace() {
                in_word = false;
            } else if !in_word {
                word_count += 1;
                in_word = true;
            }
        }

        word_count
    }

    /// Convert character offset to word index within a line
    fn char_offset_to_word_index(text: &str, char_offset: usize) -> usize {
        let chars: Vec<char> = text.chars().collect();
        let safe_offset = char_offset.min(chars.len());

        // Count completed words before the offset
        let text_before: String = chars.iter().take(safe_offset).collect();
        Self::count_words_in_text(&text_before)
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
