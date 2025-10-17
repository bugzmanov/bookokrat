use super::types::*;
use crate::comments::{BookComments, Comment};
use crate::theme::Base16Palette;
use log::{debug, warn};
use ratatui::style::Style as RatatuiStyle;
use ratatui::text::Span;
use std::sync::{Arc, Mutex};
use tui_textarea::{Input, Key, TextArea};

type CommentSelection = (String, usize, Option<(usize, usize)>);

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
                        self.current_chapter_comments
                            .entry(comment.paragraph_index)
                            .or_default()
                            .push(comment.clone());
                    }
                }
            }
        }
    }

    /// Start editing an existing comment
    pub fn start_editing_comment(
        &mut self,
        chapter_href: String,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> bool {
        if let Some(comments_arc) = &self.book_comments {
            if let Ok(comments) = comments_arc.lock() {
                let existing_content = comments
                    .get_paragraph_comments(&chapter_href, paragraph_index)
                    .iter()
                    .find(|c| c.word_range == word_range)
                    .map(|c| c.content.clone());

                if let Some(content) = existing_content {
                    let comment_start_line =
                        self.find_comment_visual_line(&chapter_href, paragraph_index, word_range);

                    if let Some(start_line) = comment_start_line {
                        let mut textarea = TextArea::default();
                        for line in content.lines() {
                            textarea.insert_str(line);
                            textarea.insert_newline();
                        }

                        self.comment_input.textarea = Some(textarea);
                        self.comment_input.target_node_index = Some(paragraph_index);
                        self.comment_input.target_line = Some(start_line);
                        self.comment_input.edit_mode = Some(CommentEditMode::Editing {
                            chapter_href,
                            paragraph_index,
                            word_range,
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

        if let Some((chapter_href, paragraph_index, word_range)) = self.get_comment_at_cursor() {
            return self.start_editing_comment(chapter_href, paragraph_index, word_range);
        }

        if let Some((start, _end)) = self.text_selection.get_selection_range() {
            let visual_line = start.line;

            let mut node_index = None;
            for (idx, line) in self.rendered_content.lines.iter().enumerate() {
                if idx == visual_line {
                    node_index = line.node_index;
                    break;
                }
            }

            if let Some(node_idx) = node_index {
                let mut last_line_of_node = visual_line;
                for (idx, line) in self
                    .rendered_content
                    .lines
                    .iter()
                    .enumerate()
                    .skip(visual_line)
                {
                    if let Some(line_node_idx) = line.node_index {
                        if line_node_idx != node_idx {
                            break;
                        }
                    }
                    last_line_of_node = idx;
                }

                let mut textarea = TextArea::default();
                textarea.set_placeholder_text("Type your comment here...");

                self.comment_input.textarea = Some(textarea);
                self.comment_input.target_node_index = Some(node_idx);
                self.comment_input.target_line = Some(last_line_of_node + 1);
                self.comment_input.edit_mode = Some(CommentEditMode::Creating);

                self.text_selection.clear_selection();

                return true;
            }
        }

        false
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
                if let Some(node_idx) = self.comment_input.target_node_index {
                    if let Some(chapter_file) = &self.current_chapter_file {
                        if let Some(comments_arc) = &self.book_comments {
                            if let Ok(mut comments) = comments_arc.lock() {
                                use chrono::Utc;

                                // Get word_range from edit mode if editing, otherwise None
                                let word_range = match &self.comment_input.edit_mode {
                                    Some(CommentEditMode::Editing { word_range, .. }) => {
                                        *word_range
                                    }
                                    _ => None,
                                };

                                let comment = Comment {
                                    chapter_href: chapter_file.clone(),
                                    paragraph_index: node_idx,
                                    word_range,
                                    content: comment_text.clone(),
                                    updated_at: Utc::now(),
                                };

                                if let Err(e) = comments.add_comment(comment) {
                                    warn!("Failed to add comment: {e}");
                                } else {
                                    debug!("Saved comment for node {node_idx}: {comment_text}");
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
                        paragraph_index,
                        word_range,
                    } = &line.line_type
                    {
                        return Some((chapter_href.clone(), *paragraph_index, *word_range));
                    }
                }
            }
        }

        None
    }

    /// Delete comment at current selection
    /// Returns true if a comment was deleted
    pub fn delete_comment_at_cursor(&mut self) -> anyhow::Result<bool> {
        if let Some((chapter_href, paragraph_index, word_range)) = self.get_comment_at_cursor() {
            if let Some(comments_arc) = &self.book_comments {
                let mut comments = comments_arc.lock().unwrap();
                comments.delete_comment(&chapter_href, paragraph_index, word_range)?;

                drop(comments);
                self.rebuild_chapter_comments();

                self.cache_generation += 1;

                self.text_selection.clear_selection();

                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Find the visual line where a specific comment starts rendering
    pub fn find_comment_visual_line(
        &self,
        chapter_href: &str,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> Option<usize> {
        for (idx, line) in self.rendered_content.lines.iter().enumerate() {
            if let LineType::Comment {
                chapter_href: line_href,
                paragraph_index: line_para,
                word_range: line_range,
            } = &line.line_type
            {
                if line_href == chapter_href
                    && *line_para == paragraph_index
                    && *line_range == word_range
                {
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
            paragraph_index,
            word_range,
        }) = &self.comment_input.edit_mode
        {
            &comment.chapter_href == chapter_href
                && comment.paragraph_index == *paragraph_index
                && comment.word_range == *word_range
        } else {
            false
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
        // Skip rendering if we're currently editing this comment
        if self.is_editing_this_comment(comment) {
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
                paragraph_index: comment.paragraph_index,
                word_range: comment.word_range,
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
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
                    paragraph_index: comment.paragraph_index,
                    word_range: comment.word_range,
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
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
                paragraph_index: comment.paragraph_index,
                word_range: comment.word_range,
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
        });
        self.raw_text_lines.push(String::new());
        *total_height += 1;
    }
}
