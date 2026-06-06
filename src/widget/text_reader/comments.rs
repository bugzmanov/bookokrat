use super::types::*;
use crate::annotations::HighlightColor;
use crate::comments::{BlockAddress, BookComments, Comment, CommentTarget};
use crate::markdown_text_reader::text_selection::SelectionPoint;
use crate::theme::Base16Palette;
use crate::vendored::tui_textarea::{Input, Key, TextArea};
use log::{debug, warn};
use ratatui::style::Style as RatatuiStyle;
use ratatui::text::Span;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct CommentSelection {
    comment_id: String,
}

/// A highlight annotation found under the cursor / selection.
#[derive(Clone)]
struct HighlightHit {
    comment_id: String,
    color: HighlightColor,
}

#[derive(Clone)]
struct InlineCodeCommentHit {
    comment_id: String,
}

#[derive(Clone, Copy, Debug)]
pub struct HighlightRange {
    pub start: usize,
    pub end: usize,
    pub color: HighlightColor,
}

/// Identifies which annotations in a node should be considered when collecting ranges.
/// Adding a new content variant means adding one enum case + one arm in [`scope_matches`].
#[derive(Clone, Copy)]
enum CommentScope<'a> {
    /// Every comment on the node.
    Node,
    /// Top-level list items only (legacy list rendering).
    LegacyList,
    /// A specific list item at the top level.
    ListItem { item_index: usize },
    /// A specific list item at a nested path.
    ListItemPath(&'a [usize]),
    /// A definition list term/definition.
    DefinitionItem { item_index: usize, is_term: bool },
    /// A specific paragraph inside a quote block.
    QuoteParagraph { paragraph_index: usize },
}

/// Slice-level scope match. Each rendered block asks `does this slice
/// belong here?` — multi-slice highlights now have one slice per touched
/// block, so the rendering pipeline pulls only the slice that targets the
/// node it's currently drawing.
fn slice_matches_scope(slice: &crate::comments::TextSlice, scope: CommentScope<'_>) -> bool {
    use crate::comments::BlockSubtarget;
    match scope {
        CommentScope::Node => true,
        CommentScope::LegacyList => matches!(
            &slice.subtarget,
            BlockSubtarget::ListItem { list_path, .. } if list_path.is_empty()
        ),
        CommentScope::ListItem { item_index } => match &slice.subtarget {
            BlockSubtarget::ListItem {
                item_index: idx,
                list_path,
                ..
            } => *idx == item_index && list_path.is_empty(),
            _ => false,
        },
        CommentScope::ListItemPath(path) => matches!(
            &slice.subtarget,
            BlockSubtarget::ListItem { list_path, .. } if list_path.as_slice() == path
        ),
        CommentScope::DefinitionItem {
            item_index,
            is_term,
        } => matches!(
            &slice.subtarget,
            BlockSubtarget::DefinitionItem {
                item_index: idx,
                is_term: term,
                ..
            } if *idx == item_index && *term == is_term
        ),
        CommentScope::QuoteParagraph { paragraph_index } => matches!(
            &slice.subtarget,
            BlockSubtarget::QuoteParagraph {
                paragraph_index: idx,
                ..
            } if *idx == paragraph_index
        ),
    }
}

fn slice_annotation_range(
    c: &Comment,
    slice: &crate::comments::TextSlice,
) -> Option<(usize, usize)> {
    if !c.is_comment() {
        return None;
    }
    slice.subtarget.word_range()
}

fn slice_highlight_range(
    c: &Comment,
    slice: &crate::comments::TextSlice,
) -> Option<HighlightRange> {
    let color = c.highlight_color()?;
    // `word_range == None` is the legacy "whole block" shorthand. Treat it as
    // a range covering everything in scope so previously-saved
    // whole-paragraph highlights still render. The render code already
    // filters by node + subtarget, and clamps `end` against each span, so an
    // unbounded end is safe here.
    let (start, end) = slice.subtarget.word_range().unwrap_or((0, usize::MAX));
    Some(HighlightRange { start, end, color })
}

impl crate::markdown_text_reader::MarkdownTextReader {
    pub fn set_book_comments(&mut self, comments: Arc<Mutex<BookComments>>) {
        self.book_comments = Some(comments);
        self.rebuild_chapter_comments();
    }

    /// Rebuild the comment lookup for the current chapter. A multi-slice
    /// comment is filed under every block index it touches so each block's
    /// render pass finds it via `get_node_comments(block_idx)`.
    pub fn rebuild_chapter_comments(&mut self) {
        use std::collections::HashSet;
        self.current_chapter_comments.clear();

        if let Some(chapter_file) = &self.current_chapter_file {
            if let Some(comments_arc) = &self.book_comments {
                if let Ok(comments) = comments_arc.lock() {
                    for comment in comments.get_chapter_comments(chapter_file) {
                        // Text reader only handles EPUB Text comments. The
                        // slices vec is empty for PDF comments, so they
                        // contribute nothing here — exactly what we want.
                        let mut seen: HashSet<usize> = HashSet::new();
                        for slice in comment.target.slices() {
                            if seen.insert(slice.block.node_index) {
                                self.current_chapter_comments
                                    .entry(slice.block.node_index)
                                    .or_default()
                                    .push(comment.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    /// Start editing an existing comment
    fn start_editing_comment(&mut self, selection: CommentSelection) -> bool {
        if let Some(comments_arc) = &self.book_comments {
            if let Ok(comments) = comments_arc.lock() {
                let comment = comments.get_comment_by_id(&selection.comment_id).cloned();

                if let Some(comment) = comment {
                    if !comment.is_comment() {
                        return false;
                    }
                    let content = comment.content.clone();
                    let comment_start_line = self.find_comment_visual_line(&comment.id);

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
                        if let Some(node_index) = comment.target.node_index() {
                            self.comment_input.target_node_index.replace(node_index);
                        }
                        self.comment_input.target_line = Some(start_line);
                        self.comment_input.target_start_line = Some(start_line);
                        self.comment_input.target_end_line = Some(start_line);
                        self.comment_input.target = Some(comment.target.clone());
                        self.comment_input.edit_mode = Some(CommentEditMode::Editing {
                            comment_id: comment.id.clone(),
                            chapter_href: comment.chapter_href.clone(),
                            target: comment.target.clone(),
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
        // Try mouse selection first
        if self.has_text_selection() {
            if let Some(selection) = self.get_comment_at_cursor() {
                return self.start_editing_comment(selection);
            }

            if let Some((start, end)) = self.text_selection.get_selection_range() {
                let (norm_start, norm_end) = self.normalize_selection_points(&start, &end);
                if let Some(target) = self.compute_selection_target(&norm_start, &norm_end) {
                    if self.selection_overlaps_annotation(&target) {
                        self.set_error_hud("Selection overlaps an existing annotation");
                        self.text_selection.clear_selection();
                        return false;
                    }
                    self.init_comment_textarea(target, norm_start.line, norm_end.line);
                    self.text_selection.clear_selection();
                    return true;
                }
            }
            return false;
        }

        // Try visual mode selection
        if self.is_visual_mode_active() {
            // Check if selection is on an existing comment first
            if let Some(selection) = self.get_comment_at_cursor() {
                self.exit_visual_mode();
                return self.start_editing_comment(selection);
            }

            if let Some((start_line, start_col, end_line, end_col)) =
                self.get_visual_selection_range()
            {
                let start = SelectionPoint {
                    line: start_line,
                    column: start_col,
                };
                let end = SelectionPoint {
                    line: end_line,
                    column: end_col,
                };
                let (norm_start, norm_end) = self.normalize_selection_points(&start, &end);
                if let Some(target) = self.compute_selection_target(&norm_start, &norm_end) {
                    if self.selection_overlaps_annotation(&target) {
                        self.set_error_hud("Selection overlaps an existing annotation");
                        self.exit_visual_mode();
                        return false;
                    }
                    self.init_comment_textarea(target, norm_start.line, norm_end.line);
                    self.exit_visual_mode();
                    return true;
                }
                self.exit_visual_mode();
            }
        }

        false
    }

    fn selection_overlaps_annotation(&self, target: &CommentTarget) -> bool {
        let Some(chapter_file) = self.current_chapter_file.as_deref() else {
            return false;
        };
        let Some(comments_arc) = self.book_comments.as_ref() else {
            return false;
        };
        let Ok(comments) = comments_arc.lock() else {
            return false;
        };
        comments.has_overlapping_annotation(chapter_file, target)
    }

    pub fn add_highlight_from_visual_selection(&mut self, color: HighlightColor) -> bool {
        if !self.is_visual_mode_active() {
            self.set_error_hud("Select text first, then press H and a color");
            return false;
        }

        let selected_text = self.get_selected_text();
        let Some((start_line, start_col, end_line, end_col)) = self.get_visual_selection_range()
        else {
            self.set_error_hud("No visual selection");
            return false;
        };
        let start = SelectionPoint {
            line: start_line,
            column: start_col,
        };
        let end = SelectionPoint {
            line: end_line,
            column: end_col,
        };
        let (norm_start, norm_end) = self.normalize_selection_points(&start, &end);

        // One unified target: single-block selections produce a 1-slice
        // target, cross-block selections produce a multi-slice target.
        // Either way the result is exactly one Comment in storage.
        let Some(target) = self.compute_selection_target(&norm_start, &norm_end) else {
            self.set_error_hud("This selection cannot be highlighted");
            self.exit_visual_mode();
            return false;
        };

        let Some(chapter_file) = self.current_chapter_file.clone() else {
            self.set_error_hud("No chapter loaded");
            self.exit_visual_mode();
            return false;
        };

        let Some(comments_arc) = self.book_comments.as_ref().cloned() else {
            self.set_error_hud("Annotations are unavailable");
            self.exit_visual_mode();
            return false;
        };

        if let Ok(mut comments) = comments_arc.lock() {
            if comments.has_overlapping_annotation(&chapter_file, &target) {
                self.set_error_hud("Selection overlaps an existing annotation");
                self.exit_visual_mode();
                return false;
            }

            let highlight = Comment::new_highlight(
                chapter_file,
                target,
                color,
                chrono::Utc::now(),
                selected_text,
            );
            if let Err(e) = comments.add_comment(highlight) {
                warn!("Failed to add highlight: {e}");
                self.set_error_hud(format!("Failed to add highlight: {e}"));
                self.exit_visual_mode();
                return false;
            }
        } else {
            self.set_error_hud("Annotations are unavailable");
            self.exit_visual_mode();
            return false;
        }

        self.rebuild_chapter_comments();
        self.exit_visual_mode();
        self.cache_generation += 1;
        self.set_normal_hud(format!("{} highlight", color.label()));
        true
    }

    fn init_comment_textarea(&mut self, target: CommentTarget, start_line: usize, end_line: usize) {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Type your comment here...");

        self.comment_input.textarea = Some(textarea);
        self.comment_input.target = Some(target.clone());
        self.comment_input.target_node_index = target.node_index();
        self.comment_input.target_start_line = Some(start_line);
        self.comment_input.target_end_line = Some(end_line);
        self.comment_input
            .target_line
            .replace(end_line.saturating_add(1));
        self.comment_input.edit_mode = Some(CommentEditMode::Creating);
    }

    /// Build a single `CommentTarget` for a visual selection. Single-block
    /// selections produce a 1-slice target; selections that cross multiple
    /// AnnotatableSegments produce a multi-slice target with one slice per
    /// touched block. Code-block selections still produce a single-slice
    /// code target — code never crosses blocks today.
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

        // Code lines short-circuit the slice flow. Mixed code + prose, or
        // code lines from different code blocks, return None — same gate as
        // before. Code highlights always stay single-slice.
        let mut has_code = false;
        let mut min_code = usize::MAX;
        let mut max_code = 0;
        let mut code_block = None;
        for idx in start.line..=end.line {
            if let Some(line) = self.rendered_content.lines.get(idx) {
                if let Some(meta) = &line.code_line {
                    if code_block
                        .as_ref()
                        .is_none_or(|found_block| *found_block == meta.block)
                    {
                        code_block = Some(meta.block.clone());
                        has_code = true;
                        min_code = min_code.min(meta.line_index);
                        max_code = max_code.max(meta.line_index);
                    } else {
                        return None;
                    }
                }
            }
        }
        if has_code {
            return code_block
                .map(|block| CommentTarget::code_block_at(block, (min_code, max_code)));
        }

        // Walk the selection top-down, grouping consecutive lines that
        // belong to the same AnnotatableSegment. The result is one entry
        // per distinct block the selection touches.
        let mut segments: Vec<(AnnotatableSegment, usize, usize)> = Vec::new();
        for idx in start.line..=end.line {
            let Some(line) = self.rendered_content.lines.get(idx) else {
                continue;
            };
            let Some(seg) = line.annotatable_segment.clone() else {
                continue;
            };
            match segments.last_mut() {
                Some((existing, _, last_line)) if *existing == seg => {
                    *last_line = idx;
                }
                _ => segments.push((seg, idx, idx)),
            }
        }
        if segments.is_empty() {
            return None;
        }

        let mut slices: Vec<crate::comments::TextSlice> = Vec::with_capacity(segments.len());
        for (seg, first_line, last_line) in segments {
            // Per-segment selection endpoints. For the segment containing
            // the global start, use it verbatim; same for the global end.
            // For segments fully in the middle of the selection, synthesise
            // endpoints at the segment's first/last line — `map_point` in
            // `compute_canonical_word_range` requires the point to land on
            // one of the filtered lines.
            let seg_start = if start.line >= first_line && start.line <= last_line {
                start.clone()
            } else {
                SelectionPoint {
                    line: first_line,
                    column: 0,
                }
            };
            let seg_end = if end.line >= first_line && end.line <= last_line {
                end.clone()
            } else {
                let last_text_len = self
                    .rendered_content
                    .lines
                    .get(last_line)
                    .map(|l| l.raw_text.chars().count())
                    .unwrap_or(0);
                SelectionPoint {
                    line: last_line,
                    column: last_text_len,
                }
            };

            let seg_ref = seg.clone();
            let word_range =
                self.compute_canonical_word_range(&seg.block, &seg_start, &seg_end, |line| {
                    line.annotatable_segment.as_ref() == Some(&seg_ref)
                });

            let subtarget = match seg.target {
                AnnotatableTarget::Paragraph => {
                    crate::comments::BlockSubtarget::Paragraph { word_range }
                }
                AnnotatableTarget::ListItem {
                    item_index,
                    list_path,
                } => crate::comments::BlockSubtarget::ListItem {
                    item_index,
                    list_path,
                    word_range,
                },
                AnnotatableTarget::QuoteParagraph { paragraph_index } => {
                    crate::comments::BlockSubtarget::QuoteParagraph {
                        paragraph_index,
                        word_range,
                    }
                }
                AnnotatableTarget::DefinitionItem {
                    item_index,
                    is_term,
                } => crate::comments::BlockSubtarget::DefinitionItem {
                    item_index,
                    is_term,
                    word_range,
                },
            };
            slices.push(crate::comments::TextSlice::new_at(seg.block, subtarget));
        }

        Some(CommentTarget::from_slices(slices))
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
                    if let Some(comments_arc) = &self.book_comments {
                        if let Ok(mut comments) = comments_arc.lock() {
                            use chrono::Utc;

                            if let Some(CommentEditMode::Editing { comment_id, .. }) =
                                &self.comment_input.edit_mode
                            {
                                if let Err(e) =
                                    comments.update_comment_by_id(comment_id, comment_text.clone())
                                {
                                    warn!("Failed to update comment: {e}");
                                } else {
                                    debug!("Updated comment: {comment_text}");
                                }
                            } else if let Some(chapter_file) = &self.current_chapter_file {
                                let comment = Comment::new(
                                    chapter_file.clone(),
                                    target,
                                    comment_text.clone(),
                                    Utc::now(),
                                );

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

    /// Check if we're currently in comment input mode
    pub fn is_comment_input_active(&self) -> bool {
        self.comment_input.is_active()
    }

    /// Get comment ID from current text selection, visual mode selection, or normal mode cursor
    /// Returns the comment ID if any line in the selection is a comment line
    fn get_comment_at_cursor(&self) -> Option<CommentSelection> {
        // Try mouse selection first
        if let Some((start, end)) = self.text_selection.get_selection_range() {
            if let Some(result) = self.find_comment_in_range(start.line, end.line, &start, &end) {
                return Some(result);
            }
        }

        // Try visual mode selection
        if self.is_visual_mode_active() {
            if let Some((start_line, start_col, end_line, end_col)) =
                self.get_visual_selection_range()
            {
                let start = SelectionPoint {
                    line: start_line,
                    column: start_col,
                };
                let end = SelectionPoint {
                    line: end_line,
                    column: end_col,
                };
                if let Some(result) = self.find_comment_in_range(start_line, end_line, &start, &end)
                {
                    return Some(result);
                }
            }
        }

        // Try normal mode cursor position (single line)
        if self.is_normal_mode_active() {
            let cursor_line = self.normal_mode.cursor.line;
            let cursor_col = self.normal_mode.cursor.column;
            let point = SelectionPoint {
                line: cursor_line,
                column: cursor_col,
            };
            if let Some(result) =
                self.find_comment_in_range(cursor_line, cursor_line, &point, &point)
            {
                return Some(result);
            }
        }

        None
    }

    /// Resolve the active cursor position into a line/point range.
    /// Prefers a mouse selection, then a visual-mode selection, then the
    /// normal-mode cursor (a zero-width range on a single line).
    fn cursor_selection_range(&self) -> Option<(usize, usize, SelectionPoint, SelectionPoint)> {
        if let Some((start, end)) = self.text_selection.get_selection_range() {
            return Some((start.line, end.line, start, end));
        }

        if self.is_visual_mode_active() {
            if let Some((start_line, start_col, end_line, end_col)) =
                self.get_visual_selection_range()
            {
                let start = SelectionPoint {
                    line: start_line,
                    column: start_col,
                };
                let end = SelectionPoint {
                    line: end_line,
                    column: end_col,
                };
                return Some((start_line, end_line, start, end));
            }
        }

        if self.is_normal_mode_active() {
            let line = self.normal_mode.cursor.line;
            let column = self.normal_mode.cursor.column;
            let point = SelectionPoint { line, column };
            return Some((line, line, point.clone(), point));
        }

        None
    }

    /// Get all comments and highlights from the current text selection,
    /// visual mode selection, or normal mode cursor.
    fn get_comments_at_cursor(&self) -> Vec<CommentSelection> {
        let Some((start_line, end_line, start, end)) = self.cursor_selection_range() else {
            return Vec::new();
        };

        let mut results = self.find_comments_in_range(start_line, end_line, &start, &end);
        // Highlights render inline (not as their own lines), so the comment-line
        // scan above never sees them. Fold in any highlight under the cursor.
        for hit in self.highlight_hits_in_range(start_line, end_line, &start, &end) {
            if !results
                .iter()
                .any(|entry| entry.comment_id == hit.comment_id)
            {
                results.push(CommentSelection {
                    comment_id: hit.comment_id,
                });
            }
        }
        results
    }

    /// Find highlight annotations whose word range overlaps the given
    /// selection range. When `start == end` (a bare cursor) the character
    /// under the cursor is tested.
    fn highlight_hits_in_range(
        &self,
        start_line: usize,
        end_line: usize,
        start: &SelectionPoint,
        end: &SelectionPoint,
    ) -> Vec<HighlightHit> {
        let is_point = start_line == end_line && start.column == end.column;
        let mut results: Vec<HighlightHit> = Vec::new();

        for line_idx in start_line..=end_line {
            let Some(line) = self.rendered_content.lines.get(line_idx) else {
                continue;
            };
            let Some(segment) = line.annotatable_segment.as_ref() else {
                continue;
            };
            let block = &segment.block;
            let Some(canon_start) = line.canonical_content_start else {
                continue;
            };
            let col_start = line.content_column_start;
            let content_len = canonical_content_len(line, col_start);
            let line_canon_end = canon_start + content_len;

            let map_col = |column: usize| -> usize {
                let visual_rel = column.saturating_sub(col_start);
                let rel = if let Some(ref jmap) = line.justify_map {
                    jmap.get(visual_rel)
                        .copied()
                        .unwrap_or_else(|| jmap.last().copied().map(|v| v + 1).unwrap_or(0))
                        as usize
                } else {
                    visual_rel
                };
                canon_start + rel.min(content_len)
            };

            let canon_lo = if line_idx == start_line {
                map_col(start.column)
            } else {
                canon_start
            };
            let canon_hi = if is_point {
                canon_lo + 1
            } else if line_idx == end_line {
                map_col(end.column)
            } else {
                line_canon_end
            };
            if canon_hi <= canon_lo {
                continue;
            }

            for comment in self.get_node_comments(Some(block.node_index)) {
                if !comment.is_highlight() {
                    continue;
                }
                let Some(color) = comment.highlight_color() else {
                    continue;
                };
                // For multi-slice highlights, `target.word_range()` returns
                // the FIRST slice's range — which is wrong when the cursor
                // is in any other block this highlight touches. Pull the
                // slice that targets *this* node.
                let Some((h_start, h_end)) = comment
                    .target
                    .slices()
                    .iter()
                    .find(|s| s.block == *block)
                    .and_then(|s| s.subtarget.word_range())
                else {
                    continue;
                };
                if h_start < canon_hi
                    && h_end > canon_lo
                    && !results.iter().any(|entry| entry.comment_id == comment.id)
                {
                    results.push(HighlightHit {
                        comment_id: comment.id.clone(),
                        color,
                    });
                }
            }
        }

        results
    }

    /// Resolve the highlight the highlight palette should act on: the first
    /// highlight overlapping the current selection or sitting under the cursor.
    ///
    /// For a range selection this re-uses [`compute_selection_target`] +
    /// [`BookComments::find_overlapping_highlight`] so that detection here can
    /// never disagree with the overlap check inside
    /// [`add_highlight_from_visual_selection`]. For a bare cursor we fall back
    /// to the canonical-position scan in [`highlight_hits_in_range`].
    pub fn highlight_for_palette(&self) -> Option<(String, HighlightColor)> {
        let (start_line, end_line, start, end) = self.cursor_selection_range()?;
        let is_point = start_line == end_line && start.column == end.column;

        if !is_point {
            let (norm_start, norm_end) = self.normalize_selection_points(&start, &end);
            if let Some(target) = self.compute_selection_target(&norm_start, &norm_end) {
                if let Some(chapter_href) = self.current_chapter_file.as_deref() {
                    if let Some(comments_arc) = self.book_comments.as_ref() {
                        if let Ok(comments) = comments_arc.lock() {
                            if let Some(comment) =
                                comments.find_overlapping_highlight(chapter_href, &target)
                            {
                                if let Some(color) = comment.highlight_color() {
                                    return Some((comment.id.clone(), color));
                                }
                            }
                        }
                    }
                }
            }
        }

        self.highlight_hits_in_range(start_line, end_line, &start, &end)
            .into_iter()
            .next()
            .map(|hit| (hit.comment_id, hit.color))
    }

    /// Change the color of an existing highlight in place.
    pub fn recolor_highlight(&mut self, comment_id: &str, color: HighlightColor) {
        if self.is_visual_mode_active() {
            self.exit_visual_mode();
        }
        self.text_selection.clear_selection();

        if let Some(comments_arc) = self.book_comments.as_ref().cloned() {
            if let Ok(mut comments) = comments_arc.lock() {
                if let Err(e) = comments.set_highlight_color_by_id(comment_id, color) {
                    warn!("Failed to recolor highlight: {e}");
                    self.set_error_hud(format!("Failed to recolor highlight: {e}"));
                    return;
                }
            }
        }

        self.rebuild_chapter_comments();
        self.cache_generation += 1;
        self.set_normal_hud(format!("{} highlight", color.label()));
    }

    /// Delete an existing highlight by id.
    pub fn remove_highlight_by_id(&mut self, comment_id: &str) {
        if self.is_visual_mode_active() {
            self.exit_visual_mode();
        }
        self.text_selection.clear_selection();
        self.delete_comment_by_id(comment_id);
        self.set_normal_hud("Highlight removed".to_string());
    }

    fn find_comment_in_range(
        &self,
        start_line: usize,
        end_line: usize,
        start: &SelectionPoint,
        end: &SelectionPoint,
    ) -> Option<CommentSelection> {
        for line_idx in start_line..=end_line {
            if let Some(line) = self.rendered_content.lines.get(line_idx) {
                if let LineType::Comment { comment_id, .. } = &line.line_type {
                    return Some(CommentSelection {
                        comment_id: comment_id.clone(),
                    });
                } else if let LineType::CodeBlock { .. } = &line.line_type {
                    if let Some((_, target)) = self.inline_code_comment_hit(line_idx, start, end) {
                        return Some(CommentSelection {
                            comment_id: target.comment_id,
                        });
                    }
                }
            }
        }
        None
    }

    fn find_comments_in_range(
        &self,
        start_line: usize,
        end_line: usize,
        start: &SelectionPoint,
        end: &SelectionPoint,
    ) -> Vec<CommentSelection> {
        let mut results = Vec::new();

        for line_idx in start_line..=end_line {
            if let Some(line) = self.rendered_content.lines.get(line_idx) {
                match &line.line_type {
                    LineType::Comment { comment_id, .. } => {
                        if !results
                            .iter()
                            .any(|entry: &CommentSelection| entry.comment_id == *comment_id)
                        {
                            results.push(CommentSelection {
                                comment_id: comment_id.clone(),
                            });
                        }
                    }
                    LineType::CodeBlock { .. } => {
                        for (_, target) in self.inline_code_comment_hits(line_idx, start, end) {
                            if !results.iter().any(|entry: &CommentSelection| {
                                entry.comment_id == target.comment_id
                            }) {
                                results.push(CommentSelection {
                                    comment_id: target.comment_id,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        results
    }

    fn inline_code_comment_hit(
        &self,
        line_idx: usize,
        selection_start: &SelectionPoint,
        selection_end: &SelectionPoint,
    ) -> Option<(String, InlineCodeCommentHit)> {
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
                return Some((
                    fragment.chapter_href.clone(),
                    InlineCodeCommentHit {
                        comment_id: fragment.comment_id.clone(),
                    },
                ));
            }
        }

        None
    }

    fn inline_code_comment_hits(
        &self,
        line_idx: usize,
        selection_start: &SelectionPoint,
        selection_end: &SelectionPoint,
    ) -> Vec<(String, InlineCodeCommentHit)> {
        let Some(line) = self.rendered_content.lines.get(line_idx) else {
            return Vec::new();
        };
        if line.inline_code_comments.is_empty() {
            return Vec::new();
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
            return Vec::new();
        }

        let mut hits = Vec::new();
        for fragment in &line.inline_code_comments {
            if start_col < fragment.end_column && end_col > fragment.start_column {
                hits.push((
                    fragment.chapter_href.clone(),
                    InlineCodeCommentHit {
                        comment_id: fragment.comment_id.clone(),
                    },
                ));
            }
        }

        hits
    }

    /// Delete comment at current selection
    /// Returns true if a comment was deleted
    pub fn delete_comment_at_cursor(&mut self) -> anyhow::Result<bool> {
        let was_visual_mode = self.is_visual_mode_active();
        let comment_selections = self.get_comments_at_cursor();

        if comment_selections.is_empty() {
            return Ok(false);
        }

        if let Some(comments_arc) = &self.book_comments {
            if let Ok(mut comments) = comments_arc.lock() {
                for selection in &comment_selections {
                    let _ = comments.delete_comment_by_id(&selection.comment_id);
                }
            }
        }

        self.rebuild_chapter_comments();
        self.cache_generation += 1;

        self.text_selection.clear_selection();
        if was_visual_mode {
            self.exit_visual_mode();
        }

        Ok(true)
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

    pub fn delete_comment_by_id(&mut self, comment_id: &str) {
        if let Some(comments_arc) = &self.book_comments {
            if let Ok(mut comments) = comments_arc.lock() {
                let _ = comments.delete_comment_by_id(comment_id);
            }
        }
        self.rebuild_chapter_comments();
        self.cache_generation += 1;
    }

    /// Find the visual line where a specific comment starts rendering
    pub fn find_comment_visual_line(&self, comment_id: &str) -> Option<usize> {
        for (idx, line) in self.rendered_content.lines.iter().enumerate() {
            if let LineType::Comment {
                comment_id: line_comment_id,
                ..
            } = &line.line_type
            {
                if line_comment_id == comment_id {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Check if we're currently editing a specific comment
    pub fn is_editing_this_comment(&self, comment: &Comment) -> bool {
        if let Some(CommentEditMode::Editing { comment_id, .. }) = &self.comment_input.edit_mode {
            comment.id == *comment_id
        } else {
            false
        }
    }

    /// Get all comments for a specific node index
    pub fn get_node_comments(&self, node_index: Option<usize>) -> Vec<Comment> {
        node_index
            .and_then(|idx| self.current_chapter_comments.get(&idx))
            .cloned()
            .unwrap_or_default()
    }

    pub fn has_rendered_comment_for_block(&self, block_address: Option<&BlockAddress>) -> bool {
        let Some(target_block) = block_address else {
            return false;
        };
        self.get_node_comments(Some(target_block.node_index))
            .into_iter()
            .any(|comment| {
                comment.is_comment()
                    && comment
                        .target
                        .slices()
                        .last()
                        .is_some_and(|slice| slice.block == *target_block)
            })
    }

    /// Walk every comment indexed under `node_index`, then walk each of its
    /// slices that targets this exact node, pass the (comment, slice) pair
    /// through the extractor. This is how a multi-slice highlight renders
    /// the right range in each block it spans: the same Comment lives in
    /// multiple node buckets, and the extractor pulls the slice for the
    /// node currently being drawn.
    fn collect_in_scope<R, F>(
        &self,
        block_address: Option<&BlockAddress>,
        scope: CommentScope<'_>,
        extractor: F,
    ) -> Vec<R>
    where
        F: Fn(&Comment, &crate::comments::TextSlice) -> Option<R>,
    {
        let Some(target_block) = block_address else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for comment in self.get_node_comments(Some(target_block.node_index)).iter() {
            for slice in comment.target.slices() {
                if slice.block != *target_block {
                    continue;
                }
                if !slice_matches_scope(slice, scope) {
                    continue;
                }
                if let Some(r) = extractor(comment, slice) {
                    out.push(r);
                }
            }
        }
        out
    }

    /// Get annotation (word) ranges from comments for a node - used for underline styling.
    pub fn get_annotation_ranges(
        &self,
        block_address: Option<&BlockAddress>,
    ) -> Vec<(usize, usize)> {
        self.collect_in_scope(block_address, CommentScope::Node, slice_annotation_range)
    }

    pub fn get_highlight_ranges(
        &self,
        block_address: Option<&BlockAddress>,
    ) -> Vec<HighlightRange> {
        self.collect_in_scope(block_address, CommentScope::Node, slice_highlight_range)
    }

    pub fn get_annotation_ranges_for_legacy_list(
        &self,
        block_address: Option<&BlockAddress>,
    ) -> Vec<(usize, usize)> {
        self.collect_in_scope(
            block_address,
            CommentScope::LegacyList,
            slice_annotation_range,
        )
    }

    pub fn get_highlight_ranges_for_legacy_list(
        &self,
        block_address: Option<&BlockAddress>,
    ) -> Vec<HighlightRange> {
        self.collect_in_scope(
            block_address,
            CommentScope::LegacyList,
            slice_highlight_range,
        )
    }

    pub fn get_annotation_ranges_for_list_item(
        &self,
        block_address: Option<&BlockAddress>,
        item_index: usize,
    ) -> Vec<(usize, usize)> {
        self.collect_in_scope(
            block_address,
            CommentScope::ListItem { item_index },
            slice_annotation_range,
        )
    }

    pub fn get_highlight_ranges_for_list_item(
        &self,
        block_address: Option<&BlockAddress>,
        item_index: usize,
    ) -> Vec<HighlightRange> {
        self.collect_in_scope(
            block_address,
            CommentScope::ListItem { item_index },
            slice_highlight_range,
        )
    }

    pub fn get_annotation_ranges_for_list_item_path(
        &self,
        block_address: Option<&BlockAddress>,
        list_path: &[usize],
    ) -> Vec<(usize, usize)> {
        self.collect_in_scope(
            block_address,
            CommentScope::ListItemPath(list_path),
            slice_annotation_range,
        )
    }

    pub fn get_highlight_ranges_for_list_item_path(
        &self,
        block_address: Option<&BlockAddress>,
        list_path: &[usize],
    ) -> Vec<HighlightRange> {
        self.collect_in_scope(
            block_address,
            CommentScope::ListItemPath(list_path),
            slice_highlight_range,
        )
    }

    pub fn get_annotation_ranges_for_definition_item(
        &self,
        block_address: Option<&BlockAddress>,
        item_index: usize,
        is_term: bool,
    ) -> Vec<(usize, usize)> {
        self.collect_in_scope(
            block_address,
            CommentScope::DefinitionItem {
                item_index,
                is_term,
            },
            slice_annotation_range,
        )
    }

    pub fn get_highlight_ranges_for_definition_item(
        &self,
        block_address: Option<&BlockAddress>,
        item_index: usize,
        is_term: bool,
    ) -> Vec<HighlightRange> {
        self.collect_in_scope(
            block_address,
            CommentScope::DefinitionItem {
                item_index,
                is_term,
            },
            slice_highlight_range,
        )
    }

    pub fn get_annotation_ranges_for_quote_paragraph(
        &self,
        block_address: Option<&BlockAddress>,
        paragraph_index: usize,
    ) -> Vec<(usize, usize)> {
        self.collect_in_scope(
            block_address,
            CommentScope::QuoteParagraph { paragraph_index },
            slice_annotation_range,
        )
    }

    pub fn get_highlight_ranges_for_quote_paragraph(
        &self,
        block_address: Option<&BlockAddress>,
        paragraph_index: usize,
    ) -> Vec<HighlightRange> {
        self.collect_in_scope(
            block_address,
            CommentScope::QuoteParagraph { paragraph_index },
            slice_highlight_range,
        )
    }

    /// Render all paragraph comments for a node as quote blocks.
    /// This is the centralized method for rendering comment blocks after content.
    #[allow(clippy::too_many_arguments)]
    pub fn render_node_comments(
        &mut self,
        block_address: Option<&BlockAddress>,
        lines: &mut Vec<RenderedLine>,
        total_height: &mut usize,
        width: usize,
        palette: &Base16Palette,
        is_focused: bool,
        indent: usize,
    ) {
        let Some(target_block) = block_address else {
            return;
        };
        let comments = self.get_node_comments(Some(target_block.node_index));
        for comment in comments.into_iter().filter(|comment| comment.is_comment()) {
            // Multi-slice comments are indexed under every node they touch
            // (so inline highlight/underline ranges render in each block).
            // The `Note // …` block, however, should appear exactly once —
            // attach it to the LAST slice's node so it sits after the
            // entire annotated range.
            let render_at = comment.target.slices().last().map(|s| &s.block);
            if render_at != Some(target_block) {
                continue;
            }
            self.render_comment_as_quote(
                &comment,
                lines,
                total_height,
                width,
                palette,
                is_focused,
                indent,
            );
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

        if !comment.is_paragraph_comment() {
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
                comment_id: comment.id.clone(),
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
            canonical_content_start: None,
            content_column_start: 0,
            justify_map: None,
            annotatable_segment: None,
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
                    comment_id: comment.id.clone(),
                },
                link_nodes: vec![],
                node_anchor: None,
                node_index: None,
                code_line: None,
                inline_code_comments: Vec::new(),
                canonical_content_start: None,
                content_column_start: 0,
                justify_map: None,
                annotatable_segment: None,
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
                comment_id: comment.id.clone(),
            },
            link_nodes: vec![],
            node_anchor: None,
            node_index: None,
            code_line: None,
            inline_code_comments: Vec::new(),
            canonical_content_start: None,
            content_column_start: 0,
            justify_map: None,
            annotatable_segment: None,
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

    fn compute_canonical_word_range(
        &self,
        block: &BlockAddress,
        start: &SelectionPoint,
        end: &SelectionPoint,
        line_filter: impl Fn(&RenderedLine) -> bool,
    ) -> Option<(usize, usize)> {
        let relevant: Vec<(usize, usize, usize)> = self
            .rendered_content
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.annotatable_segment
                    .as_ref()
                    .is_some_and(|segment| segment.block == *block)
                    && line_filter(line)
            })
            .filter_map(|(idx, line)| {
                line.canonical_content_start
                    .map(|cs| (idx, cs, line.content_column_start))
            })
            .collect();

        if relevant.is_empty() {
            return None;
        }

        let total_len = {
            let (last_idx, last_cs, last_ccs) = relevant.last().unwrap();
            let last_line = &self.rendered_content.lines[*last_idx];
            let content_len = canonical_content_len(last_line, *last_ccs);
            last_cs + content_len
        };

        let map_point = |point: &SelectionPoint| -> Option<usize> {
            relevant.iter().find(|(idx, _, _)| *idx == point.line).map(
                |(idx, canon_start, col_start)| {
                    let line = &self.rendered_content.lines[*idx];
                    let content_len = canonical_content_len(line, *col_start);
                    let visual_rel = point.column.saturating_sub(*col_start);
                    let rel = if let Some(ref jmap) = line.justify_map {
                        jmap.get(visual_rel)
                            .copied()
                            .unwrap_or_else(|| jmap.last().copied().map(|v| v + 1).unwrap_or(0))
                            as usize
                    } else {
                        visual_rel
                    };
                    canon_start + rel.min(content_len)
                },
            )
        };

        let s = map_point(start)?;
        let e = map_point(end).unwrap_or(total_len);
        if s >= e {
            return None;
        }
        // Always emit a concrete (start, end). The previous code collapsed a
        // whole-block selection to `None` as a "no precise range" shorthand —
        // but inline highlight rendering depends on `word_range()` being
        // `Some`, so whole-paragraph highlights silently disappeared from the
        // screen (the YAML had them but `slice_highlight_range` returned None).
        Some((s, e.min(total_len)))
    }

    #[doc(hidden)]
    pub fn testing_comment_target_for_selection(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Option<CommentTarget> {
        let start = SelectionPoint {
            line: start_line,
            column: start_col,
        };
        let end = SelectionPoint {
            line: end_line,
            column: end_col,
        };
        self.compute_selection_target(&start, &end)
    }

    #[doc(hidden)]
    pub fn testing_rendered_lines(&self) -> &[RenderedLine] {
        self.rendered_content.lines.as_slice()
    }
}

/// Get the canonical (unjustified) content length for a rendered line.
fn canonical_content_len(line: &RenderedLine, col_start: usize) -> usize {
    if let Some(ref jmap) = line.justify_map {
        jmap.last().copied().map(|v| v as usize + 1).unwrap_or(0)
    } else {
        line.raw_text.chars().count().saturating_sub(col_start)
    }
}
