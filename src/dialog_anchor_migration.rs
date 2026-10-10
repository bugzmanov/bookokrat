//! Remaps persisted EPUB positions recorded against chapters where runs of
//! consecutive dialog paragraphs were merged into a single block (see
//! `group_dialog_paragraphs`) onto chapters where every paragraph is its own
//! block.
//!
//! Old positions are interpreted the way the old app consumed them:
//! - comment and highlight ranges were drawn with every line break counting as
//!   one character, so `map_target` preserves exactly the characters that were
//!   underlined or highlighted;
//! - mark offsets are wrapped-line starts, which never counted line breaks, so
//!   `map_position` lays dialog lines out back to back and then adds back the
//!   line breaks that current line starts count.

use crate::comments::{BlockSubtarget, Comment, CommentTarget, TextSlice};
use crate::markdown::{Block, Document, Inline, Text, TextOrInline};
use crate::markdown_text_reader::MarkdownTextReader;
use crate::marks::MarkLocation;
use std::collections::HashMap;

/// Layout version stamped on persisted positions. Positions without it were
/// recorded against the merged-dialog layout.
pub const CURRENT_ANCHOR_LAYOUT: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
enum OldBlock {
    Single(usize),
    Group {
        first: usize,
        /// Canonical length of each dialog line, line breaks included.
        part_lens: Vec<usize>,
        /// The same lengths as seen by wrapped-line starts, line breaks excluded.
        line_lens: Vec<usize>,
    },
}

/// Old-to-new block mapping for a single chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogAnchorMap {
    old_blocks: Vec<OldBlock>,
    new_block_count: usize,
    /// Line break positions of new paragraphs that have any, measured the way
    /// old line starts were: without counting the line breaks themselves.
    line_breaks: HashMap<usize, Vec<usize>>,
}

impl DialogAnchorMap {
    /// Builds the map from the chapter parsed without dialog grouping.
    pub fn new(ungrouped: &Document) -> Self {
        let line_breaks: HashMap<usize, Vec<usize>> = ungrouped
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match &node.block {
                Block::Paragraph { content } => {
                    let positions = MarkdownTextReader::paragraph_line_break_positions(content);
                    (!positions.is_empty()).then(|| {
                        let without_breaks = positions
                            .iter()
                            .enumerate()
                            .map(|(seen, position)| position - seen)
                            .collect();
                        (index, without_breaks)
                    })
                }
                _ => None,
            })
            .collect();

        let mut old_blocks = Vec::with_capacity(ungrouped.blocks.len());
        let mut groups = dialog_paragraph_groups(ungrouped).into_iter().peekable();
        let mut index = 0;
        while index < ungrouped.blocks.len() {
            if let Some(group) = groups.next_if(|group| group.start == index) {
                let (part_lens, line_lens) = group
                    .clone()
                    .map(|index| match &ungrouped.blocks[index].block {
                        Block::Paragraph { content } => {
                            let len = MarkdownTextReader::paragraph_canonical_len(content);
                            let breaks = line_breaks.get(&index).map_or(0, Vec::len);
                            (len, len - breaks)
                        }
                        _ => (0, 0),
                    })
                    .unzip();
                old_blocks.push(OldBlock::Group {
                    first: group.start,
                    part_lens,
                    line_lens,
                });
                index = group.end;
            } else {
                old_blocks.push(OldBlock::Single(index));
                index += 1;
            }
        }

        Self {
            old_blocks,
            new_block_count: ungrouped.blocks.len(),
            line_breaks,
        }
    }

    /// True when no saved position in the chapter needs to change: there was
    /// no merged dialog and no paragraph has a line break.
    pub fn is_identity(&self) -> bool {
        self.old_blocks.len() == self.new_block_count && self.line_breaks.is_empty()
    }

    pub fn old_block_count(&self) -> usize {
        self.old_blocks.len()
    }

    pub fn new_block_count(&self) -> usize {
        self.new_block_count
    }

    /// New index of the block that starts where old block `old_index` started.
    /// Used for reading positions, which only record a block.
    pub fn map_node(&self, old_index: usize) -> usize {
        match self.old_blocks.get(old_index) {
            Some(OldBlock::Single(new_index)) => *new_index,
            Some(OldBlock::Group { first, .. }) => *first,
            None => old_index + self.new_block_count - self.old_blocks.len(),
        }
    }

    /// Maps a wrapped-line start position (block index + canonical offset).
    pub fn map_position(&self, old_index: usize, old_offset: usize) -> (usize, usize) {
        let (node, offset) = match self.old_blocks.get(old_index) {
            Some(OldBlock::Group {
                first, line_lens, ..
            }) => Self::locate_line_start(*first, line_lens, old_offset),
            _ => (self.map_node(old_index), old_offset),
        };
        let breaks_before = self.line_breaks.get(&node).map_or(0, |breaks| {
            breaks.iter().take_while(|b| **b <= offset).count()
        });
        (node, offset + breaks_before)
    }

    fn locate_line_start(first: usize, line_lens: &[usize], offset: usize) -> (usize, usize) {
        let mut part_start = 0;
        for (part, len) in line_lens.iter().enumerate() {
            if offset < part_start + len {
                return (first + part, offset - part_start);
            }
            part_start += len;
        }
        let last = line_lens.len() - 1;
        (first + last, line_lens[last])
    }

    /// Maps a comment or highlight target. PDF targets are returned unchanged.
    pub fn map_target(&self, target: &CommentTarget) -> CommentTarget {
        match target {
            CommentTarget::Text { slices } => CommentTarget::from_slices(
                slices
                    .iter()
                    .flat_map(|slice| self.map_slice(slice))
                    .collect(),
            ),
            CommentTarget::Pdf { .. } => target.clone(),
        }
    }

    pub fn migrate_comment(&self, comment: &Comment) -> Comment {
        Comment {
            target: self.map_target(&comment.target),
            ..comment.clone()
        }
    }

    /// Maps an EPUB mark recorded against the merged-dialog layout that points
    /// into this map's chapter. Marks already on the current layout and PDF
    /// marks are returned unchanged.
    pub fn migrate_mark(&self, mark: &MarkLocation) -> MarkLocation {
        match mark {
            MarkLocation::Epub {
                path,
                chapter,
                node,
                node_offset,
                snippet,
                chapter_title,
                anchor_layout: None,
            } => {
                let (node, node_offset) = match node_offset {
                    Some(offset) => {
                        let (node, offset) = self.map_position(*node, *offset);
                        (node, Some(offset))
                    }
                    None => (self.map_node(*node), None),
                };
                MarkLocation::Epub {
                    path: path.clone(),
                    chapter: *chapter,
                    node,
                    node_offset,
                    snippet: snippet.clone(),
                    chapter_title: chapter_title.clone(),
                    anchor_layout: Some(CURRENT_ANCHOR_LAYOUT),
                }
            }
            MarkLocation::Epub { .. } | MarkLocation::Pdf { .. } => mark.clone(),
        }
    }

    fn map_slice(&self, slice: &TextSlice) -> Vec<TextSlice> {
        let (
            Some(OldBlock::Group {
                first, part_lens, ..
            }),
            BlockSubtarget::Paragraph { word_range },
            true,
        ) = (
            self.old_blocks.get(slice.block.node_index),
            &slice.subtarget,
            slice.block.child_path.is_empty(),
        )
        else {
            let mut mapped = slice.clone();
            mapped.block.node_index = self.map_node(slice.block.node_index);
            return vec![mapped];
        };

        let paragraph = |part: usize, word_range: Option<(usize, usize)>| {
            TextSlice::new(first + part, BlockSubtarget::Paragraph { word_range })
        };

        let Some((start, end)) = *word_range else {
            return (0..part_lens.len())
                .map(|part| paragraph(part, None))
                .collect();
        };

        let mut slices = Vec::new();
        let mut part_start = 0;
        for (part, len) in part_lens.iter().enumerate() {
            let lo = start.max(part_start);
            let hi = end.min(part_start + len);
            if lo < hi {
                slices.push(paragraph(part, Some((lo - part_start, hi - part_start))));
            }
            part_start += len + 1;
        }

        if slices.is_empty() {
            // The range covered nothing but line breaks, so nothing was ever
            // drawn. Keep the comment, still empty, on the dialog line where
            // the range started.
            let mut part_start = 0;
            let mut location = None;
            for (part, len) in part_lens.iter().enumerate() {
                if start <= part_start + len {
                    location = Some((part, start.saturating_sub(part_start)));
                    break;
                }
                part_start += len + 1;
            }
            let (part, offset) = location.unwrap_or_else(|| {
                let last = part_lens.len() - 1;
                (last, part_lens[last])
            });
            slices.push(paragraph(part, Some((offset, offset))));
        }

        slices
    }
}

/// Runs of two or more consecutive top-level dialog paragraphs: the blocks
/// that releases up to 0.3.12 merged into one.
fn dialog_paragraph_groups(document: &Document) -> Vec<std::ops::Range<usize>> {
    let mut groups = Vec::new();
    let mut i = 0;

    while i < document.blocks.len() {
        let mut j = i;
        while j < document.blocks.len() {
            match &document.blocks[j].block {
                Block::Paragraph { content } if is_dialog_content(content) => j += 1,
                _ => break,
            }
        }

        if j - i > 1 {
            groups.push(i..j);
        }
        i = j.max(i + 1);
    }

    groups
}

/// Check if a Text content represents dialog (contains dialog lines)
fn is_dialog_content(content: &Text) -> bool {
    for item in content.clone().into_iter() {
        match item {
            TextOrInline::Text(text_node) => {
                let trimmed = text_node.content.trim_start();
                if !trimmed.is_empty() {
                    return trimmed.starts_with('-')
                        || trimmed.starts_with('–')
                        || trimmed.starts_with('—')
                        || trimmed.starts_with('\u{2010}')
                        || trimmed.starts_with('\u{2011}')
                        || trimmed.starts_with('\u{2012}')
                        || trimmed.starts_with('\u{2013}')
                        || trimmed.starts_with('\u{2014}');
                }
            }
            TextOrInline::Inline(Inline::Link { text, .. }) => {
                if is_dialog_content(&text) {
                    return true;
                }
            }
            _ => {
                // Skip images, line breaks, etc. and continue looking
            }
        }
    }
    false
}

/// The merge releases up to 0.3.12 applied after parsing a chapter.
#[cfg(test)]
pub(crate) fn group_dialog_paragraphs(document: &mut Document) {
    for group in dialog_paragraph_groups(document).into_iter().rev() {
        let dialog_contents: Vec<Text> = document.blocks[group.clone()]
            .iter()
            .filter_map(|node| match &node.block {
                Block::Paragraph { content } => Some(content.clone()),
                _ => None,
            })
            .collect();
        let merged_content = merge_dialog_group(&dialog_contents);
        if let Block::Paragraph { content } = &mut document.blocks[group.start].block {
            *content = merged_content;
        }
        document.blocks.drain(group.start + 1..group.end);
    }
}

#[cfg(test)]
fn merge_dialog_group(dialog_group: &[Text]) -> Text {
    let mut merged = Text::default();

    for (i, dialog_text) in dialog_group.iter().enumerate() {
        for item in dialog_text.clone().into_iter() {
            merged.push(item);
        }

        if i < dialog_group.len() - 1 {
            merged.push_inline(Inline::LineBreak);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::HighlightColor;
    use crate::comments::{AnnotationBody, BlockAddress, BookComments, PdfSelectionRect};
    use crate::markdown_text_reader::{LineType, RenderedLine};
    use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
    use crate::parsing::markdown_renderer::MarkdownRenderer;
    use crate::settings::{RuntimeSettings, Settings};
    use crate::theme;
    use chrono::{TimeZone, Utc};
    use ratatui::style::Style;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    const CHAPTER: &str = "OEBPS/chapter3.xhtml";

    // Blocks without grouping:
    //  0 h2 | 1 p | 2-5 dialog | 6 p | 7-8 dialog | 9 h3 | 10 lone dash p | 11 p
    // Blocks with grouping:
    //  0 h2 | 1 p | 2 [2-5]    | 3 p | 4 [7-8]    | 5 h3 | 6 lone dash p  | 7 p
    const QUIJOTE: &str = r##"
        <h2>Capítulo III</h2>
        <p>Hízole levantar don Quijote y dijo:</p>
        <p>—De esa manera, ¿verdad es que hay historia mía y que fue moro y sabio el que la compuso?</p>
        <p>—Es tan verdad, señor —dijo Sansón—, que tengo para mí que el día de hoy están impresos más de doce mil libros de la tal historia: si no, dígalo Portugal, Barcelona y Valencia, donde se han impreso.<sup><a href="#n8">[8]</a></sup></p>
        <p>—Una de las cosas —dijo a esta sazón don Quijote— que más debe de dar contento a un hombre <em>virtuoso y eminente</em> es verse, viviendo, andar con buen nombre por las lenguas de las gentes.</p>
        <p>—Si por buena fama y si por buen nombre va <a href="#n9">[9]</a> —dijo el bachiller—, solo vuestra merced lleva la palma.</p>
        <p>Calló el bachiller, y don Quijote quedó pensativo.</p>
        <p>—¿Y qué dicen de mí?</p>
        <p>—Muchas cosas.</p>
        <h3>Notas</h3>
        <p>—Una línea suelta con raya.</p>
        <p>Fin del capítulo.</p>
    "##;

    const RUSSIAN: &str = r#"
        <p>Вот пример из жизни. Молодой человек знакомится с родителями невесты.</p>
        <p>— А кем работаешь?</p>
        <p>— Я аналитик, работаю на рынке ценных бумаг.</p>
        <p>— Пирамиды, что ли? Ваучеры?</p>
        <p>Видя, что тесть не понимает, молодой человек меняет тактику:</p>
    "#;

    // Groups at the very start and the very end, plain ASCII hyphens, a dialog
    // line that starts with a link, and a dialog line with its own <br>.
    const EDGES: &str = r##"
        <p>- Hola.</p>
        <p>- ¿Qué tal?</p>
        <p>Narración intermedia.</p>
        <p><a href="#x">—Sí</a>, dijo ella.</p>
        <p>—Bueno.<br/>Entonces vamos.</p>
        <p>— Último.</p>
    "##;

    const NO_DIALOG: &str = r#"
        <h1>Title</h1>
        <p>First paragraph.</p>
        <p>—A single dialog line is never merged.</p>
        <p>Last paragraph.</p>
    "#;

    // No dialog; paragraphs with line breaks, wrapping after a break, and two
    // breaks in a row.
    const POEM: &str = r#"
        <p>Stopping by Woods on a Snowy Evening.</p>
        <p>The woods are lovely, dark and deep,<br/>But I have promises to keep,<br/>And miles to go before I sleep,<br/>And miles to go before I sleep.</p>
        <p>Short.<br/>A long line after the break that wraps onto several more lines at narrow widths, again and again and again.</p>
        <p>Two breaks in a row.<br/><br/>After the empty line.</p>
    "#;

    const ALL_FIXTURES: [(&str, &str); 4] = [
        ("quijote", QUIJOTE),
        ("russian", RUSSIAN),
        ("edges", EDGES),
        ("poem", POEM),
    ];

    const FIXTURES: [(&str, &str); 3] =
        [("quijote", QUIJOTE), ("russian", RUSSIAN), ("edges", EDGES)];

    /// (old grouped document, new ungrouped document, map)
    fn chapter(html: &str) -> (Document, Document, DialogAnchorMap) {
        let ungrouped = HtmlToMarkdownConverter::new().convert(html);
        let mut grouped = ungrouped.clone();
        group_dialog_paragraphs(&mut grouped);
        let map = DialogAnchorMap::new(&ungrouped);
        (grouped, ungrouped, map)
    }

    fn paragraph_len(doc: &Document, index: usize) -> usize {
        match &doc.blocks[index].block {
            Block::Paragraph { content } => MarkdownTextReader::paragraph_canonical_len(content),
            other => panic!("block {index} is not a paragraph: {other:?}"),
        }
    }

    fn group_parts(map: &DialogAnchorMap, old_index: usize) -> (usize, Vec<usize>) {
        match &map.old_blocks[old_index] {
            OldBlock::Group {
                first, part_lens, ..
            } => (*first, part_lens.clone()),
            OldBlock::Single(_) => panic!("old block {old_index} is not a dialog group"),
        }
    }

    fn group_indices(map: &DialogAnchorMap) -> Vec<usize> {
        (0..map.old_block_count())
            .filter(|i| matches!(map.old_blocks[*i], OldBlock::Group { .. }))
            .collect()
    }

    fn paragraph_slice(node: usize, range: Option<(usize, usize)>) -> TextSlice {
        TextSlice::new(node, BlockSubtarget::Paragraph { word_range: range })
    }

    fn timestamp() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 1, 12, 0, 0).unwrap()
    }

    fn highlight(id: &str, target: CommentTarget) -> Comment {
        Comment {
            id: id.to_string(),
            chapter_href: CHAPTER.to_string(),
            target,
            content: String::new(),
            body: AnnotationBody::Highlight {
                color: HighlightColor::Yellow,
            },
            updated_at: timestamp(),
            quoted_text: None,
        }
    }

    fn note(id: &str, target: CommentTarget) -> Comment {
        Comment {
            id: id.to_string(),
            chapter_href: CHAPTER.to_string(),
            target,
            content: format!("note {id}"),
            body: AnnotationBody::Comment,
            updated_at: timestamp(),
            quoted_text: None,
        }
    }

    struct Renderer(MarkdownTextReader);

    impl Renderer {
        fn new() -> Self {
            let mut reader =
                MarkdownTextReader::new(RuntimeSettings::in_memory(Settings::default()));
            reader.set_current_chapter_file(Some(CHAPTER.to_string()));
            Self(reader)
        }

        fn render(
            &mut self,
            doc: &Document,
            comments: &[Comment],
            width: usize,
        ) -> Vec<RenderedLine> {
            let mut book_comments = BookComments::new_empty();
            for comment in comments {
                book_comments.add_comment(comment.clone()).unwrap();
            }
            self.0
                .set_book_comments(Arc::new(Mutex::new(book_comments)));
            self.0
                .render_document_to_lines(doc, width, theme::current_theme(), true)
                .lines
        }
    }

    fn render(doc: &Document, comments: &[Comment], width: usize) -> Vec<RenderedLine> {
        Renderer::new().render(doc, comments, width)
    }

    /// Every visible text line, with each character's style. Blank lines and
    /// note boxes are dropped: ungrouping adds blank lines between dialog lines
    /// and moves note boxes, but must not change any text or its styling.
    fn styled_text(lines: &[RenderedLine]) -> Vec<Vec<(char, Style)>> {
        lines
            .iter()
            .filter(|line| !line.raw_text.trim().is_empty())
            .filter(|line| !matches!(line.line_type, LineType::Comment { .. }))
            .map(|line| {
                line.spans
                    .iter()
                    .flat_map(|span| span.content.chars().map(move |ch| (ch, span.style)))
                    .filter(|(ch, _)| *ch != '\n')
                    .collect()
            })
            .collect()
    }

    fn highlighted_text(lines: &[RenderedLine]) -> String {
        styled_text(lines)
            .iter()
            .map(|line| {
                line.iter()
                    .filter(|(_, style)| style.bg.is_some())
                    .map(|(ch, _)| *ch)
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("|")
    }

    fn first_line_of_node(lines: &[RenderedLine], node: usize) -> String {
        lines
            .iter()
            .find(|line| line.node_index == Some(node) && !line.raw_text.trim().is_empty())
            .map(|line| line.raw_text.clone())
            .unwrap_or_else(|| panic!("node {node} rendered no text"))
    }

    /// Mirrors `find_line_for_node_offset`: the wrapped line of `node` with the
    /// largest canonical start <= `offset`.
    fn line_for_position(lines: &[RenderedLine], node: usize, offset: usize) -> usize {
        let mut best = None;
        let mut first = None;
        for (index, line) in lines.iter().enumerate() {
            if line.node_index != Some(node) {
                if first.is_some() {
                    break;
                }
                continue;
            }
            first.get_or_insert(index);
            if let Some(start) = line.canonical_content_start {
                if start <= offset {
                    best = Some(index);
                } else {
                    break;
                }
            }
        }
        best.or(first)
            .unwrap_or_else(|| panic!("node {node} has no lines"))
    }

    fn paragraph_indices(doc: &Document) -> Vec<usize> {
        (0..doc.blocks.len())
            .filter(|i| matches!(doc.blocks[*i].block, Block::Paragraph { .. }))
            .collect()
    }

    /// Wrapped-line starts of a block as releases up to 0.3.12 stored them in
    /// marks: the line break that ended a line was never stepped over.
    fn legacy_line_starts(doc: &Document, index: usize, width: usize) -> Vec<usize> {
        let Block::Paragraph { content } = &doc.blocks[index].block else {
            panic!("block {index} is not a paragraph");
        };
        let plain = MarkdownTextReader::text_to_string(content);
        let chars: Vec<char> = plain.chars().collect();
        let mut starts = Vec::new();
        let mut idx = 0;
        for (line, wrapped) in textwrap::wrap(&plain, width).iter().enumerate() {
            if line > 0 {
                while idx < chars.len() && chars[idx] == ' ' {
                    idx += 1;
                }
            }
            starts.push(idx);
            idx += wrapped.chars().count();
        }
        starts
    }

    /// For every wrapped line of old block `old`: the offset an old release
    /// stored for a mark on it, and the index of the same line in `new_lines`.
    fn mark_cases(
        old_doc: &Document,
        map: &DialogAnchorMap,
        old: usize,
        width: usize,
        new_lines: &[RenderedLine],
    ) -> Vec<(usize, usize)> {
        let starts = legacy_line_starts(old_doc, old, width);
        let new_blocks = map.map_node(old)..map.map_node(old + 1);
        let lines: Vec<usize> = new_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.canonical_content_start.is_some()
                    && line.node_index.is_some_and(|n| new_blocks.contains(&n))
            })
            .map(|(index, _)| index)
            .collect();
        assert_eq!(starts.len(), lines.len(), "block {old} at width {width}");
        starts.into_iter().zip(lines).collect()
    }

    /// Range endpoints worth probing in a merged block: both sides of every
    /// line break, the edges of the block, and a coarse grid in between.
    fn probe_points(part_lens: &[usize]) -> Vec<usize> {
        let total = part_lens.iter().sum::<usize>() + part_lens.len() - 1;
        let mut points = vec![0, 1, total - 1, total, total + 7];
        let mut part_start = 0;
        for len in part_lens {
            let end = part_start + len;
            points.extend([part_start, part_start + 1, end - 1, end, end + 1, end + 2]);
            part_start = end + 1;
        }
        points.extend((0..total).step_by(23));
        points.sort_unstable();
        points.dedup();
        points
    }

    // ---- The migration reproduces the old grouping exactly ----

    #[test]
    fn block_counts_match_both_layouts() {
        for (name, html) in FIXTURES.iter().chain([("no_dialog", NO_DIALOG)].iter()) {
            let (grouped, ungrouped, map) = chapter(html);
            assert_eq!(map.old_block_count(), grouped.blocks.len(), "{name}");
            assert_eq!(map.new_block_count(), ungrouped.blocks.len(), "{name}");
        }
    }

    // The assertions the 0.3.12 converter test made about its grouping.
    #[test]
    fn legacy_grouping_matches_the_released_behaviour() {
        let (grouped, _, _) = chapter(RUSSIAN);
        let rendered = MarkdownRenderer::new().render(&grouped);
        let paragraphs: Vec<&str> = rendered
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .collect();
        assert_eq!(paragraphs.len(), 3);
        assert!(paragraphs[1].contains("— А кем работаешь?"));
        assert!(paragraphs[1].contains("— Я аналитик"));
        assert!(paragraphs[1].contains("— Пирамиды"));
        assert!(paragraphs[1].matches("  \n").count() >= 2);
    }

    #[test]
    fn quijote_fixture_has_the_expected_layout() {
        let (grouped, ungrouped, map) = chapter(QUIJOTE);
        assert_eq!(ungrouped.blocks.len(), 12);
        assert_eq!(grouped.blocks.len(), 8);
        assert_eq!(group_indices(&map), vec![2, 4]);
        assert_eq!(group_parts(&map, 2).0, 2);
        assert_eq!(group_parts(&map, 2).1.len(), 4);
        assert_eq!(group_parts(&map, 4).0, 7);
        assert_eq!(group_parts(&map, 4).1.len(), 2);
    }

    #[test]
    fn part_lengths_are_the_renderer_canonical_lengths() {
        for (name, html) in FIXTURES {
            let (_, ungrouped, map) = chapter(html);
            for old in group_indices(&map) {
                let (first, part_lens) = group_parts(&map, old);
                for (part, len) in part_lens.iter().enumerate() {
                    assert_eq!(*len, paragraph_len(&ungrouped, first + part), "{name}");
                    assert!(*len > 0, "{name}: empty dialog line");
                }
            }
        }
    }

    #[test]
    fn merged_block_length_is_parts_plus_one_per_line_break() {
        for (name, html) in FIXTURES {
            let (grouped, _, map) = chapter(html);
            for old in group_indices(&map) {
                let (_, part_lens) = group_parts(&map, old);
                let expected = part_lens.iter().sum::<usize>() + part_lens.len() - 1;
                assert_eq!(paragraph_len(&grouped, old), expected, "{name} block {old}");
            }
        }
    }

    // ---- Block index mapping (reading positions) ----

    #[test]
    fn chapter_without_dialog_groups_is_identity() {
        let (grouped, ungrouped, map) = chapter(NO_DIALOG);
        assert!(map.is_identity());
        assert_eq!(grouped, ungrouped);
        for index in 0..ungrouped.blocks.len() + 3 {
            assert_eq!(map.map_node(index), index);
            assert_eq!(map.map_position(index, 5), (index, 5));
        }
        let target = CommentTarget::paragraph(2, Some((3, 9)));
        assert_eq!(map.map_target(&target), target);
    }

    #[test]
    fn chapter_with_dialog_groups_is_not_identity() {
        for (name, html) in FIXTURES {
            assert!(!chapter(html).2.is_identity(), "{name}");
        }
    }

    #[test]
    fn blocks_before_the_first_group_keep_their_index() {
        let (_, _, map) = chapter(QUIJOTE);
        assert_eq!(map.map_node(0), 0);
        assert_eq!(map.map_node(1), 1);
    }

    #[test]
    fn a_group_maps_to_its_first_dialog_line() {
        let (_, _, map) = chapter(QUIJOTE);
        assert_eq!(map.map_node(2), 2);
        assert_eq!(map.map_node(4), 7);
    }

    #[test]
    fn blocks_after_groups_shift_by_the_cumulative_number_of_merged_lines() {
        let (_, _, map) = chapter(QUIJOTE);
        // After the first group (4 lines -> 1 block): +3.
        assert_eq!(map.map_node(3), 6);
        // After the second group (2 lines -> 1 block): +3 +1.
        assert_eq!(map.map_node(5), 9);
        assert_eq!(map.map_node(6), 10);
        assert_eq!(map.map_node(7), 11);
    }

    #[test]
    fn indices_past_the_end_shift_by_the_total_delta() {
        let (_, _, map) = chapter(QUIJOTE);
        assert_eq!(map.map_node(8), 12);
        assert_eq!(map.map_node(100), 104);
    }

    #[test]
    fn groups_at_chapter_start_and_end() {
        let (_, ungrouped, map) = chapter(EDGES);
        assert_eq!(ungrouped.blocks.len(), 6);
        assert_eq!(map.old_block_count(), 3);
        assert_eq!(group_indices(&map), vec![0, 2]);
        assert_eq!(map.map_node(0), 0);
        assert_eq!(map.map_node(1), 2);
        assert_eq!(map.map_node(2), 3);
    }

    #[test]
    fn a_lone_dialog_line_is_not_a_group() {
        let (_, _, map) = chapter(QUIJOTE);
        assert_eq!(map.old_blocks[6], OldBlock::Single(10));
    }

    #[test]
    fn every_old_block_starts_with_the_same_text_after_mapping() {
        for (name, html) in FIXTURES {
            let (grouped, ungrouped, map) = chapter(html);
            for width in [32, 80] {
                let old_lines = render(&grouped, &[], width);
                let new_lines = render(&ungrouped, &[], width);
                for old in 0..map.old_block_count() {
                    assert_eq!(
                        first_line_of_node(&old_lines, old),
                        first_line_of_node(&new_lines, map.map_node(old)),
                        "{name}: block {old} at width {width}"
                    );
                }
            }
        }
    }

    // ---- Comment and highlight targets ----

    #[test]
    fn targets_on_single_blocks_only_change_their_index() {
        let (_, _, map) = chapter(QUIJOTE);
        let cases = [
            (CommentTarget::paragraph(1, Some((0, 6))), 1),
            (CommentTarget::paragraph(3, Some((6, 18))), 6),
            (CommentTarget::paragraph(3, None), 6),
            (CommentTarget::paragraph(6, Some((2, 4))), 10),
            (CommentTarget::code_block(5, (0, 2)), 9),
            (CommentTarget::list_item(7, 1, Some((0, 3))), 11),
            (CommentTarget::quote_paragraph(7, 2, None), 11),
            (CommentTarget::definition_item(7, 0, true, Some((1, 2))), 11),
        ];
        for (target, new_index) in cases {
            let mapped = map.map_target(&target);
            let mut expected = target.clone();
            if let CommentTarget::Text { slices } = &mut expected {
                slices[0].block.node_index = new_index;
            }
            assert_eq!(mapped, expected);
        }
    }

    #[test]
    fn nested_block_addresses_keep_their_child_path() {
        let (_, _, map) = chapter(QUIJOTE);
        let slice = TextSlice::new_at(
            BlockAddress {
                node_index: 5,
                child_path: vec![1, 0, 2],
            },
            BlockSubtarget::CodeLines { line_range: (3, 4) },
        );
        let mapped = map.map_target(&CommentTarget::from_slices(vec![slice]));
        assert_eq!(
            mapped.slices(),
            &[TextSlice::new_at(
                BlockAddress {
                    node_index: 9,
                    child_path: vec![1, 0, 2],
                },
                BlockSubtarget::CodeLines { line_range: (3, 4) },
            )]
        );
    }

    #[test]
    fn pdf_targets_are_untouched() {
        let (_, _, map) = chapter(QUIJOTE);
        let target = CommentTarget::pdf(
            3,
            vec![PdfSelectionRect {
                page: 3,
                topleft_x: 1,
                topleft_y: 2,
                bottomright_x: 30,
                bottomright_y: 40,
            }],
        );
        assert_eq!(map.map_target(&target), target);
    }

    #[test]
    fn range_inside_one_dialog_line() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let line3_start = lens[0] + 1 + lens[1] + 1;
        let mapped = map.map_target(&CommentTarget::paragraph(
            2,
            Some((line3_start + 4, line3_start + 10)),
        ));
        assert_eq!(mapped.slices(), &[paragraph_slice(4, Some((4, 10)))]);
    }

    #[test]
    fn range_covering_a_whole_dialog_line_exactly() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let start = lens[0] + 1;
        let mapped = map.map_target(&CommentTarget::paragraph(2, Some((start, start + lens[1]))));
        assert_eq!(mapped.slices(), &[paragraph_slice(3, Some((0, lens[1])))]);
    }

    #[test]
    fn range_across_a_line_break_splits_into_two_slices() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let mapped = map.map_target(&CommentTarget::paragraph(
            2,
            Some((lens[0] - 5, lens[0] + 1 + 7)),
        ));
        assert_eq!(
            mapped.slices(),
            &[
                paragraph_slice(2, Some((lens[0] - 5, lens[0]))),
                paragraph_slice(3, Some((0, 7))),
            ]
        );
    }

    #[test]
    fn range_across_every_dialog_line() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let total = lens.iter().sum::<usize>() + lens.len() - 1;
        let mapped = map.map_target(&CommentTarget::paragraph(2, Some((3, total - 2))));
        assert_eq!(
            mapped.slices(),
            &[
                paragraph_slice(2, Some((3, lens[0]))),
                paragraph_slice(3, Some((0, lens[1]))),
                paragraph_slice(4, Some((0, lens[2]))),
                paragraph_slice(5, Some((0, lens[3] - 2))),
            ]
        );
    }

    #[test]
    fn range_starting_on_a_line_break_starts_at_the_next_line() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let mapped = map.map_target(&CommentTarget::paragraph(2, Some((lens[0], lens[0] + 6))));
        assert_eq!(mapped.slices(), &[paragraph_slice(3, Some((0, 5)))]);
    }

    #[test]
    fn range_ending_on_a_line_break_stays_on_its_line() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let mapped = map.map_target(&CommentTarget::paragraph(
            2,
            Some((lens[0] - 4, lens[0] + 1)),
        ));
        assert_eq!(
            mapped.slices(),
            &[paragraph_slice(2, Some((lens[0] - 4, lens[0])))]
        );
    }

    #[test]
    fn range_covering_only_a_line_break_keeps_an_empty_anchor_on_its_line() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let mapped = map.map_target(&CommentTarget::paragraph(2, Some((lens[0], lens[0] + 1))));
        assert_eq!(
            mapped.slices(),
            &[paragraph_slice(2, Some((lens[0], lens[0])))]
        );
    }

    #[test]
    fn range_past_the_end_is_clamped_to_the_last_dialog_line() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 4);
        let total = lens[0] + 1 + lens[1];
        let mapped = map.map_target(&CommentTarget::paragraph(
            4,
            Some((lens[0] + 3, usize::MAX)),
        ));
        assert_eq!(mapped.slices(), &[paragraph_slice(8, Some((2, lens[1])))]);

        let mapped = map.map_target(&CommentTarget::paragraph(4, Some((total + 5, total + 9))));
        assert_eq!(
            mapped.slices(),
            &[paragraph_slice(8, Some((lens[1], lens[1])))]
        );
    }

    #[test]
    fn whole_block_target_covers_every_dialog_line() {
        let (_, _, map) = chapter(QUIJOTE);
        let mapped = map.map_target(&CommentTarget::paragraph(4, None));
        assert_eq!(
            mapped.slices(),
            &[paragraph_slice(7, None), paragraph_slice(8, None)]
        );
    }

    #[test]
    fn multi_slice_target_spanning_into_and_out_of_a_group() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let total = lens.iter().sum::<usize>() + lens.len() - 1;
        let tail_start = total - lens[3] - 3;
        let target = CommentTarget::from_slices(vec![
            paragraph_slice(1, Some((10, 34))),
            paragraph_slice(2, Some((tail_start, total))),
            paragraph_slice(3, Some((0, 5))),
        ]);
        assert_eq!(
            map.map_target(&target).slices(),
            &[
                paragraph_slice(1, Some((10, 34))),
                paragraph_slice(4, Some((lens[2] - 2, lens[2]))),
                paragraph_slice(5, Some((0, lens[3]))),
                paragraph_slice(6, Some((0, 5))),
            ]
        );
    }

    #[test]
    fn migrate_comment_changes_only_the_target() {
        let (_, _, map) = chapter(QUIJOTE);
        let original = Comment {
            quoted_text: Some("quoted".to_string()),
            ..note("c1", CommentTarget::paragraph(3, Some((0, 5))))
        };
        let migrated = map.migrate_comment(&original);
        assert_eq!(migrated.target, CommentTarget::paragraph(6, Some((0, 5))));
        assert_eq!(
            Comment {
                target: original.target.clone(),
                ..migrated
            },
            original
        );
    }

    #[test]
    fn remapping_twice_moves_targets_again() {
        // The map is not idempotent: the caller must record which positions
        // were already migrated, or every reopen shifts them further.
        let (_, _, map) = chapter(QUIJOTE);
        let once = map.map_target(&CommentTarget::paragraph(3, Some((0, 5))));
        let twice = map.map_target(&once);
        assert_eq!(once, CommentTarget::paragraph(6, Some((0, 5))));
        assert_eq!(twice, CommentTarget::paragraph(10, Some((0, 5))));
    }

    // ---- Comparison against the real renderer ----

    #[test]
    fn highlights_cover_the_same_characters_after_migration() {
        let mut renderer = Renderer::new();
        for (name, html) in FIXTURES {
            let (grouped, ungrouped, map) = chapter(html);
            for old in group_indices(&map) {
                let (_, part_lens) = group_parts(&map, old);
                let points = probe_points(&part_lens);
                for (i, &start) in points.iter().enumerate() {
                    for &end in &points[i + 1..] {
                        let before =
                            highlight("h", CommentTarget::paragraph(old, Some((start, end))));
                        let after = map.migrate_comment(&before);
                        for width in [37, 200] {
                            let old_view = styled_text(&renderer.render(
                                &grouped,
                                std::slice::from_ref(&before),
                                width,
                            ));
                            let new_view = styled_text(&renderer.render(
                                &ungrouped,
                                std::slice::from_ref(&after),
                                width,
                            ));
                            assert_eq!(
                                old_view, new_view,
                                "{name}: block {old} range {start}..{end} at width {width} -> {:?}",
                                after.target
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn highlight_comparison_detects_a_misplaced_highlight() {
        // Guards the comparison above against passing vacuously.
        let (grouped, ungrouped, map) = chapter(QUIJOTE);
        let (first, lens) = group_parts(&map, 2);
        let line3_start = lens[0] + 1 + lens[1] + 1;
        let before = highlight(
            "h",
            CommentTarget::paragraph(2, Some((line3_start + 2, line3_start + 30))),
        );
        let old_view = render(&grouped, std::slice::from_ref(&before), 60);
        assert!(!highlighted_text(&old_view).is_empty());

        let unmigrated = render(&ungrouped, std::slice::from_ref(&before), 60);
        assert_ne!(styled_text(&old_view), styled_text(&unmigrated));

        let off_by_one = highlight("h", CommentTarget::paragraph(first + 2, Some((1, 29))));
        let shifted = render(&ungrouped, &[off_by_one], 60);
        assert_ne!(styled_text(&old_view), styled_text(&shifted));

        let migrated = render(&ungrouped, &[map.migrate_comment(&before)], 60);
        assert_eq!(styled_text(&old_view), styled_text(&migrated));
        assert_eq!(highlighted_text(&old_view), highlighted_text(&migrated));
    }

    #[test]
    fn note_underlines_cover_the_same_characters_after_migration() {
        for (name, html) in FIXTURES {
            let (grouped, ungrouped, map) = chapter(html);
            for old in group_indices(&map) {
                let (_, part_lens) = group_parts(&map, old);
                let total = part_lens.iter().sum::<usize>() + part_lens.len() - 1;
                for (start, end) in [(0, 3), (2, total - 2), (part_lens[0] - 2, part_lens[0] + 4)] {
                    let before = note("n", CommentTarget::paragraph(old, Some((start, end))));
                    let after = map.migrate_comment(&before);
                    for width in [41, 120] {
                        assert_eq!(
                            styled_text(&render(&grouped, std::slice::from_ref(&before), width)),
                            styled_text(&render(&ungrouped, std::slice::from_ref(&after), width)),
                            "{name}: block {old} range {start}..{end} at width {width}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn note_box_renders_once_right_after_the_last_annotated_dialog_line() {
        let (_, ungrouped, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let before = note(
            "n",
            CommentTarget::paragraph(2, Some((lens[0] - 3, lens[0] + 5))),
        );
        let after = map.migrate_comment(&before);
        let lines = render(&ungrouped, &[after], 50);

        let box_lines: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| matches!(&line.line_type, LineType::Comment { comment_id, .. } if comment_id == "n"))
            .map(|(index, _)| index)
            .collect();
        assert!(!box_lines.is_empty(), "note box missing");
        assert!(
            box_lines.windows(2).all(|w| w[1] == w[0] + 1),
            "note box rendered more than once"
        );

        let last_text_before_box = lines[..box_lines[0]]
            .iter()
            .rev()
            .find(|line| line.node_index.is_some() && !line.raw_text.trim().is_empty())
            .unwrap();
        assert_eq!(last_text_before_box.node_index, Some(3));
        let next_dialog_line = lines
            .iter()
            .position(|line| line.node_index == Some(4))
            .unwrap();
        assert!(box_lines[0] < next_dialog_line);
    }

    // ---- Wrapped-line positions (marks) ----

    #[test]
    fn positions_inside_a_group() {
        let (_, _, map) = chapter(QUIJOTE);
        let (first, lens) = group_parts(&map, 2);
        assert_eq!(map.map_position(2, 0), (first, 0));
        assert_eq!(map.map_position(2, lens[0] - 1), (first, lens[0] - 1));
        assert_eq!(map.map_position(2, lens[0]), (first + 1, 0));
        assert_eq!(map.map_position(2, lens[0] + 9), (first + 1, 9));
        assert_eq!(map.map_position(2, lens[0] + lens[1]), (first + 2, 0));
        let total = lens.iter().sum::<usize>();
        assert_eq!(map.map_position(2, total - 1), (first + 3, lens[3] - 1));
        assert_eq!(map.map_position(2, total), (first + 3, lens[3]));
        assert_eq!(map.map_position(2, total + 50), (first + 3, lens[3]));
    }

    #[test]
    fn positions_skip_line_breaks_inside_a_dialog_line() {
        // Old block 2 of EDGES: "—Sí, dijo ella." | "—Bueno.<br>Entonces vamos." | "— Último."
        let (_, _, map) = chapter(EDGES);
        let OldBlock::Group {
            first,
            part_lens,
            line_lens,
        } = map.old_blocks[2].clone()
        else {
            panic!("old block 2 is not a dialog group");
        };
        assert_eq!(part_lens[0], line_lens[0]);
        assert_eq!(part_lens[1], line_lens[1] + 1);
        let bueno = "—Bueno.".chars().count();
        // Old line starts never counted line breaks; current ones do.
        assert_eq!(
            map.map_position(2, line_lens[0] + bueno - 1),
            (first + 1, bueno - 1)
        );
        assert_eq!(
            map.map_position(2, line_lens[0] + bueno),
            (first + 1, bueno + 1)
        );
        let third_line = line_lens[0] + line_lens[1];
        assert_eq!(
            map.map_position(2, third_line - 1),
            (first + 1, part_lens[1] - 1)
        );
        assert_eq!(map.map_position(2, third_line), (first + 2, 0));
    }

    #[test]
    fn positions_in_a_paragraph_with_line_breaks_count_the_breaks() {
        let (_, _, map) = chapter(POEM);
        let line = |s: &str| s.chars().count();
        let first = line("The woods are lovely, dark and deep,");
        let second = line("But I have promises to keep,");
        assert_eq!(map.map_position(1, 0), (1, 0));
        assert_eq!(map.map_position(1, first - 1), (1, first - 1));
        assert_eq!(map.map_position(1, first), (1, first + 1));
        assert_eq!(map.map_position(1, first + 4), (1, first + 5));
        assert_eq!(map.map_position(1, first + second), (1, first + second + 2));
    }

    #[test]
    fn positions_outside_groups_keep_their_offset() {
        let (_, _, map) = chapter(QUIJOTE);
        assert_eq!(map.map_position(1, 12), (1, 12));
        assert_eq!(map.map_position(3, 7), (6, 7));
        assert_eq!(map.map_position(7, 0), (11, 0));
    }

    #[test]
    fn chapter_with_line_breaks_but_no_dialog_only_moves_mark_offsets() {
        let (grouped, ungrouped, map) = chapter(POEM);
        assert_eq!(grouped, ungrouped);
        assert!(!map.is_identity());
        for index in 0..ungrouped.blocks.len() {
            assert_eq!(map.map_node(index), index);
        }
        let target = CommentTarget::paragraph(1, Some((40, 52)));
        assert_eq!(map.map_target(&target), target);
    }

    #[test]
    fn old_mark_offsets_resolve_to_the_same_line_when_lines_do_not_wrap() {
        for (name, html) in ALL_FIXTURES {
            let (grouped, ungrouped, map) = chapter(html);
            let width = 400;
            let new_lines = render(&ungrouped, &[], width);
            for old in paragraph_indices(&grouped) {
                for (start, intended) in mark_cases(&grouped, &map, old, width, &new_lines) {
                    if new_lines[intended].raw_text.is_empty() {
                        continue;
                    }
                    let (node, offset) = map.map_position(old, start);
                    assert_eq!(
                        line_for_position(&new_lines, node, offset),
                        intended,
                        "{name}: mark at {old}/{start}"
                    );
                }
            }
        }
    }

    #[test]
    fn old_mark_offsets_never_resolve_past_their_line_when_lines_wrap() {
        for (name, html) in ALL_FIXTURES {
            let (grouped, ungrouped, map) = chapter(html);
            for width in [24, 37, 61] {
                let new_lines = render(&ungrouped, &[], width);
                for old in paragraph_indices(&grouped) {
                    let cases = mark_cases(&grouped, &map, old, width, &new_lines);
                    let block_start = cases[0].1;
                    for (start, intended) in cases {
                        if new_lines[intended].raw_text.is_empty() {
                            continue;
                        }
                        let (node, offset) = map.map_position(old, start);
                        let resolved = line_for_position(&new_lines, node, offset);
                        assert!(
                            (block_start..=intended).contains(&resolved),
                            "{name}: mark at {old}/{start} resolved to line {resolved}, \
                             expected {block_start}..={intended} at width {width}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn short_dialog_marks_are_exact_at_common_widths() {
        let (grouped, ungrouped, map) = chapter(RUSSIAN);
        for width in [60, 80, 120] {
            let new_lines = render(&ungrouped, &[], width);
            for (start, intended) in mark_cases(&grouped, &map, 1, width, &new_lines) {
                let (node, offset) = map.map_position(1, start);
                assert_eq!(line_for_position(&new_lines, node, offset), intended);
            }
        }
    }

    #[test]
    fn migrate_mark_maps_epub_marks_and_preserves_metadata() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let mark = MarkLocation::Epub {
            path: "/books/quijote.epub".to_string(),
            chapter: 5,
            node: 2,
            node_offset: Some(lens[0] + lens[1] + 3),
            snippet: Some("—Una de las cosas".to_string()),
            chapter_title: Some("Capítulo III".to_string()),
            anchor_layout: None,
        };
        assert_eq!(
            map.migrate_mark(&mark),
            MarkLocation::Epub {
                path: "/books/quijote.epub".to_string(),
                chapter: 5,
                node: 4,
                node_offset: Some(3),
                snippet: Some("—Una de las cosas".to_string()),
                chapter_title: Some("Capítulo III".to_string()),
                anchor_layout: Some(CURRENT_ANCHOR_LAYOUT),
            }
        );
    }

    #[test]
    fn migrate_mark_without_offset_maps_the_block() {
        let (_, _, map) = chapter(QUIJOTE);
        let mark = MarkLocation::Epub {
            path: "b.epub".to_string(),
            chapter: 1,
            node: 5,
            node_offset: None,
            snippet: None,
            chapter_title: None,
            anchor_layout: None,
        };
        let MarkLocation::Epub {
            node, node_offset, ..
        } = map.migrate_mark(&mark)
        else {
            panic!("expected an EPUB mark");
        };
        assert_eq!((node, node_offset), (9, None));
    }

    #[test]
    fn migrate_mark_leaves_pdf_marks_alone() {
        let (_, _, map) = chapter(QUIJOTE);
        let mark = MarkLocation::Pdf {
            path: "b.pdf".to_string(),
            page: 4,
            scroll_offset: 120,
            snippet: None,
            line_idx: Some(3),
        };
        assert_eq!(map.migrate_mark(&mark), mark);
    }

    // ---- Persisted comments ----

    const PLAIN_CHAPTER: &str = "OEBPS/chapter4.xhtml";
    const MISSING_CHAPTER: &str = "OEBPS/missing.xhtml";

    fn book_path(dir: &TempDir) -> std::path::PathBuf {
        dir.path().join("quijote.epub")
    }

    fn load(dir: &TempDir) -> BookComments {
        BookComments::new(&book_path(dir), Some(dir.path())).unwrap()
    }

    fn write_comments_file(dir: &TempDir, yaml: &str) -> std::path::PathBuf {
        let file = load(dir).file_path;
        std::fs::write(&file, yaml).unwrap();
        file
    }

    /// Remaps the way `load_epub` does: chapters the book can read go through
    /// their map, anything else is left for a later attempt.
    fn migrate(comments: &mut BookComments) -> usize {
        let quijote = chapter(QUIJOTE).2;
        let plain = chapter(NO_DIALOG).2;
        comments
            .migrate_legacy_layout(|href, target| match href {
                CHAPTER => Some(quijote.map_target(target)),
                PLAIN_CHAPTER => Some(plain.map_target(target)),
                _ => None,
            })
            .unwrap()
    }

    fn raw_entries(file: &std::path::Path) -> Vec<serde_yaml::Value> {
        serde_yaml::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
    }

    fn raw_entry<'a>(
        raw: &'a [serde_yaml::Value],
        key: &str,
        value: &str,
    ) -> &'a serde_yaml::Value {
        raw.iter()
            .find(|v| v.get(key).and_then(|v| v.as_str()) == Some(value))
            .unwrap_or_else(|| panic!("no entry with {key} = {value}"))
    }

    fn layout_of(entry: &serde_yaml::Value) -> Option<u64> {
        entry.get("anchor_layout").and_then(|v| v.as_u64())
    }

    /// A comments file as releases up to 0.3.12 wrote it.
    fn old_app_file(lens: &[usize]) -> String {
        format!(
            r#"- id: before-group
  chapter_href: {CHAPTER}
  target_type: text
  node_index: 1
  subtarget_kind: paragraph
  word_range: [0, 6]
  content: untouched
  updated_at: 2025-06-01T12:00:00Z
- id: across-line-break
  chapter_href: {CHAPTER}
  annotation_type: highlight
  color: green
  target_type: text
  node_index: 2
  subtarget_kind: paragraph
  word_range: [{}, {}]
  updated_at: 2025-06-01T12:00:00Z
- id: after-group
  chapter_href: {CHAPTER}
  target_type: text
  node_index: 3
  subtarget_kind: paragraph
  content: whole paragraph
  updated_at: 2025-06-01T12:00:00Z
- chapter_href: {CHAPTER}
  paragraph_index: 7
  word_range: [0, 3]
  content: pre-refactor format
  updated_at: 2025-06-01T12:00:00Z
- id: plain-chapter
  chapter_href: {PLAIN_CHAPTER}
  target_type: text
  node_index: 3
  subtarget_kind: paragraph
  content: chapter without dialog
  updated_at: 2025-06-01T12:00:00Z
- id: missing-chapter
  chapter_href: {MISSING_CHAPTER}
  target_type: text
  node_index: 5
  subtarget_kind: paragraph
  content: chapter the book can't read
  updated_at: 2025-06-01T12:00:00Z
- id: pdf
  chapter_href: {CHAPTER}
  target_type: pdf
  page: 3
  rects: []
  content: pdf note
  updated_at: 2025-06-01T12:00:00Z
"#,
            lens[0] - 4,
            lens[0] + 6
        )
    }

    #[test]
    fn comments_file_written_by_the_old_app_migrates_and_reloads() {
        let dir = TempDir::new().unwrap();
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        write_comments_file(&dir, &old_app_file(&lens));

        let mut comments = load(&dir);
        assert!(comments.has_legacy_layout_comments());
        let before: Vec<Comment> = comments.get_all_comments().to_vec();
        assert_eq!(migrate(&mut comments), 3);

        let reloaded = load(&dir);
        let by_id = |id: &str| reloaded.get_comment_by_id(id).unwrap().clone();
        assert_eq!(
            by_id("before-group").target,
            CommentTarget::paragraph(1, Some((0, 6)))
        );
        assert_eq!(
            by_id("across-line-break").target.slices(),
            &[
                paragraph_slice(2, Some((lens[0] - 4, lens[0]))),
                paragraph_slice(3, Some((0, 5))),
            ]
        );
        assert_eq!(
            by_id("across-line-break").body,
            AnnotationBody::Highlight {
                color: HighlightColor::Green
            }
        );
        assert_eq!(
            by_id("after-group").target,
            CommentTarget::paragraph(6, None)
        );
        assert_eq!(
            by_id("plain-chapter").target,
            CommentTarget::paragraph(3, None)
        );
        assert_eq!(
            by_id("missing-chapter").target,
            CommentTarget::paragraph(5, None)
        );
        assert!(by_id("pdf").target.is_pdf());

        let legacy = reloaded
            .get_all_comments()
            .iter()
            .find(|c| c.content == "pre-refactor format")
            .unwrap();
        assert_eq!(legacy.target, CommentTarget::paragraph(11, Some((0, 3))));

        assert_eq!(reloaded.get_all_comments().len(), before.len());
        for old in &before {
            let new = reloaded
                .get_all_comments()
                .iter()
                .find(|c| c.content == old.content && c.chapter_href == old.chapter_href)
                .unwrap();
            assert_eq!(new.body, old.body);
            assert_eq!(new.updated_at, old.updated_at);
            assert_eq!(new.quoted_text, old.quoted_text);
        }
    }

    #[test]
    fn migrated_comments_are_stamped_and_unreadable_ones_are_not() {
        let dir = TempDir::new().unwrap();
        let (_, _, map) = chapter(QUIJOTE);
        let file = write_comments_file(&dir, &old_app_file(&group_parts(&map, 2).1));
        migrate(&mut load(&dir));

        let raw = raw_entries(&file);
        for id in [
            "before-group",
            "across-line-break",
            "after-group",
            "plain-chapter",
        ] {
            assert_eq!(
                layout_of(raw_entry(&raw, "id", id)),
                Some(u64::from(CURRENT_ANCHOR_LAYOUT)),
                "{id}"
            );
        }
        assert_eq!(
            layout_of(raw_entry(&raw, "content", "pre-refactor format")),
            Some(u64::from(CURRENT_ANCHOR_LAYOUT))
        );
        assert_eq!(layout_of(raw_entry(&raw, "id", "missing-chapter")), None);
        assert_eq!(layout_of(raw_entry(&raw, "id", "pdf")), None);
    }

    #[test]
    fn reopening_never_migrates_the_same_comment_twice() {
        let dir = TempDir::new().unwrap();
        let (_, _, map) = chapter(QUIJOTE);
        let file = write_comments_file(&dir, &old_app_file(&group_parts(&map, 2).1));
        migrate(&mut load(&dir));
        let after_first_open = std::fs::read_to_string(&file).unwrap();
        let targets: Vec<CommentTarget> = load(&dir)
            .get_all_comments()
            .iter()
            .map(|c| c.target.clone())
            .collect();

        for _ in 0..3 {
            let mut comments = load(&dir);
            assert_eq!(migrate(&mut comments), 0);
            let again: Vec<CommentTarget> = comments
                .get_all_comments()
                .iter()
                .map(|c| c.target.clone())
                .collect();
            assert_eq!(again, targets);
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), after_first_open);
    }

    #[test]
    fn unreadable_chapters_are_migrated_once_they_can_be_read() {
        let dir = TempDir::new().unwrap();
        let (_, _, map) = chapter(QUIJOTE);
        write_comments_file(&dir, &old_app_file(&group_parts(&map, 2).1));
        migrate(&mut load(&dir));

        let mut comments = load(&dir);
        assert!(comments.has_legacy_layout_comments());
        let moved = comments
            .migrate_legacy_layout(|href, target| {
                assert_eq!(href, MISSING_CHAPTER, "only the unread chapter is retried");
                Some(map.map_target(target))
            })
            .unwrap();
        assert_eq!(moved, 1);
        assert!(!load(&dir).has_legacy_layout_comments());
        assert_eq!(
            load(&dir)
                .get_comment_by_id("missing-chapter")
                .unwrap()
                .target,
            CommentTarget::paragraph(9, None)
        );
    }

    #[test]
    fn comments_created_by_this_version_are_never_migrated() {
        let dir = TempDir::new().unwrap();
        let mut comments = load(&dir);
        comments
            .add_comment(note("new", CommentTarget::paragraph(3, Some((0, 4)))))
            .unwrap();
        comments
            .add_comment(highlight(
                "new-hl",
                CommentTarget::paragraph(5, Some((1, 2))),
            ))
            .unwrap();
        assert!(!comments.has_legacy_layout_comments());

        let raw = raw_entries(&comments.file_path);
        assert_eq!(
            layout_of(raw_entry(&raw, "id", "new")),
            Some(u64::from(CURRENT_ANCHOR_LAYOUT))
        );

        let mut reloaded = load(&dir);
        assert!(!reloaded.has_legacy_layout_comments());
        assert_eq!(migrate(&mut reloaded), 0);
        assert_eq!(
            reloaded.get_comment_by_id("new").unwrap().target,
            CommentTarget::paragraph(3, Some((0, 4)))
        );
        assert_eq!(
            reloaded.get_comment_by_id("new-hl").unwrap().target,
            CommentTarget::paragraph(5, Some((1, 2)))
        );
    }

    #[test]
    fn comments_added_after_a_migration_keep_their_position() {
        let dir = TempDir::new().unwrap();
        let (_, _, map) = chapter(QUIJOTE);
        write_comments_file(&dir, &old_app_file(&group_parts(&map, 2).1));
        let mut comments = load(&dir);
        migrate(&mut comments);
        comments
            .add_comment(note("fresh", CommentTarget::paragraph(3, Some((2, 8)))))
            .unwrap();

        let mut reloaded = load(&dir);
        migrate(&mut reloaded);
        assert_eq!(
            reloaded.get_comment_by_id("fresh").unwrap().target,
            CommentTarget::paragraph(3, Some((2, 8)))
        );
    }

    #[test]
    fn migrated_comments_are_found_on_their_new_blocks() {
        let dir = TempDir::new().unwrap();
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        write_comments_file(
            &dir,
            &format!(
                r#"- id: split
  chapter_href: {CHAPTER}
  target_type: text
  node_index: 2
  subtarget_kind: paragraph
  word_range: [{}, {}]
  content: split
  updated_at: 2025-06-01T12:00:00Z
- id: after
  chapter_href: {CHAPTER}
  target_type: text
  node_index: 5
  subtarget_kind: paragraph
  content: after
  updated_at: 2025-06-01T12:00:00Z
"#,
                lens[0] - 2,
                lens[0] + 3
            ),
        );
        let mut comments = load(&dir);
        migrate(&mut comments);

        let ids = |node: usize| -> Vec<String> {
            comments
                .get_node_comments(CHAPTER, node)
                .iter()
                .map(|c| c.id.clone())
                .collect()
        };
        assert_eq!(ids(2), vec!["split"]);
        assert_eq!(ids(3), vec!["split"]);
        assert_eq!(ids(9), vec!["after"]);
        assert!(ids(5).is_empty());
        assert_eq!(comments.get_chapter_comments(CHAPTER).len(), 2);
    }

    #[test]
    fn split_comments_use_the_multi_slice_format_and_others_keep_the_legacy_shape() {
        let dir = TempDir::new().unwrap();
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let file = write_comments_file(&dir, &old_app_file(&lens));
        migrate(&mut load(&dir));

        let raw = raw_entries(&file);
        let split = raw_entry(&raw, "id", "across-line-break");
        assert!(split.get("slices").is_some());
        assert!(split.get("node_index").is_none());
        let moved = raw_entry(&raw, "id", "after-group");
        assert!(moved.get("slices").is_none());
        assert_eq!(moved.get("node_index").and_then(|v| v.as_u64()), Some(6));
    }

    #[test]
    fn chapters_without_dialog_only_gain_the_stamp() {
        let dir = TempDir::new().unwrap();
        let file = write_comments_file(
            &dir,
            &format!(
                r#"- id: a
  chapter_href: {PLAIN_CHAPTER}
  target_type: text
  node_index: 2
  subtarget_kind: paragraph
  word_range: [1, 4]
  content: plain
  updated_at: 2025-06-01T12:00:00Z
"#
            ),
        );
        let mut comments = load(&dir);
        assert_eq!(migrate(&mut comments), 0);
        assert_eq!(
            load(&dir).get_comment_by_id("a").unwrap().target,
            CommentTarget::paragraph(2, Some((1, 4)))
        );
        assert_eq!(
            layout_of(raw_entry(&raw_entries(&file), "id", "a")),
            Some(u64::from(CURRENT_ANCHOR_LAYOUT))
        );
    }

    // ---- Persisted reading positions and marks ----

    #[test]
    fn bookmarks_from_older_releases_have_no_layout() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("bookmarks.json");
        std::fs::write(
            &file,
            r#"{"books": {"/b/q.epub": {"chapter_href": "OEBPS/chapter3.xhtml", "node_index": 3, "last_read": "2025-06-01T12:00:00Z"}}}"#,
        )
        .unwrap();
        let bookmarks =
            crate::bookmarks::Bookmarks::load_from_file(file.to_str().unwrap()).unwrap();
        let bookmark = bookmarks.get_bookmark("/b/q.epub").unwrap();
        assert_eq!(bookmark.node_index, Some(3));
        assert_eq!(bookmark.anchor_layout, None);
    }

    #[test]
    fn reading_positions_written_by_this_version_carry_the_layout() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("bookmarks.json");
        let path = file.to_str().unwrap();
        let mut bookmarks = crate::bookmarks::Bookmarks::with_file(path);
        bookmarks.update_bookmark(
            "/b/q.epub",
            CHAPTER.to_string(),
            Some(6),
            Some(2),
            Some(5),
            None,
            None,
            None,
            None,
            None,
        );
        bookmarks.save().unwrap();

        let reloaded = crate::bookmarks::Bookmarks::load_from_file(path).unwrap();
        let bookmark = reloaded.get_bookmark("/b/q.epub").unwrap();
        assert_eq!(bookmark.node_index, Some(6));
        assert_eq!(bookmark.anchor_layout, Some(CURRENT_ANCHOR_LAYOUT));
    }

    #[test]
    fn refreshing_book_metadata_does_not_claim_an_old_position_was_migrated() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("bookmarks.json");
        let path = file.to_str().unwrap();
        std::fs::write(
            &file,
            r#"{"books": {"/b/q.epub": {"chapter_href": "OEBPS/chapter3.xhtml", "node_index": 3, "last_read": "2025-06-01T12:00:00Z"}}}"#,
        )
        .unwrap();
        let mut bookmarks = crate::bookmarks::Bookmarks::load_from_file(path).unwrap();
        bookmarks.save_initial_bookmark(
            "/b/q.epub",
            CHAPTER.to_string(),
            Some(2),
            Some(5),
            None,
            Some("Quijote".to_string()),
            None,
            None,
        );

        let reloaded = crate::bookmarks::Bookmarks::load_from_file(path).unwrap();
        let bookmark = reloaded.get_bookmark("/b/q.epub").unwrap();
        assert_eq!(bookmark.node_index, Some(3));
        assert_eq!(bookmark.anchor_layout, None);
    }

    #[test]
    fn marks_from_older_releases_have_no_layout() {
        let mark: MarkLocation = serde_json::from_str(
            r#"{"kind": "epub", "path": "/b/q.epub", "chapter": 2, "node": 2, "node_offset": 40}"#,
        )
        .unwrap();
        let MarkLocation::Epub { anchor_layout, .. } = mark else {
            panic!("expected an EPUB mark");
        };
        assert_eq!(anchor_layout, None);
    }

    #[test]
    fn migrated_marks_are_stamped_and_never_migrated_again() {
        let (_, _, map) = chapter(QUIJOTE);
        let (_, lens) = group_parts(&map, 2);
        let old = MarkLocation::Epub {
            path: "/b/q.epub".to_string(),
            chapter: 2,
            node: 2,
            node_offset: Some(lens[0] + 4),
            snippet: None,
            chapter_title: None,
            anchor_layout: None,
        };
        let once = map.migrate_mark(&old);
        let MarkLocation::Epub {
            node,
            node_offset,
            anchor_layout,
            ..
        } = once.clone()
        else {
            panic!("expected an EPUB mark");
        };
        assert_eq!((node, node_offset), (3, Some(4)));
        assert_eq!(anchor_layout, Some(CURRENT_ANCHOR_LAYOUT));
        assert_eq!(map.migrate_mark(&once), once);

        let json = serde_json::to_string(&once).unwrap();
        let reloaded: MarkLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(map.migrate_mark(&reloaded), once);
    }
}
