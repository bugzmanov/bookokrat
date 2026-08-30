use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use epub::doc::EpubDoc;

use crate::markdown::{Block, Document, Inline, TableCellContent, Text, TextOrInline};
use crate::parsing::html_to_markdown::{HtmlToMarkdownConverter, extract_chapter_title};
use crate::search_engine::SearchLine;

/// The parsed form of the chapter that will be displayed immediately after
/// the EPUB is opened.
pub(crate) struct AnalyzedChapter {
    pub index: usize,
    pub title: Option<String>,
    pub document: Arc<Document>,
}

/// Search and progress data derived during the single full-book analysis pass.
pub(crate) struct EpubAnalysis {
    pub search_sections: Vec<(usize, String, Vec<SearchLine>)>,
    pub node_counts: Vec<usize>,
    pub total_nodes: usize,
    pub initial_chapter: Option<AnalyzedChapter>,
}

pub(crate) fn analyze_epub(
    doc: &mut EpubDoc<BufReader<File>>,
    initial_chapter: usize,
) -> EpubAnalysis {
    let original_chapter = doc.get_current_chapter();
    let chapter_count = doc.get_num_chapters();
    let mut converter = HtmlToMarkdownConverter::new();
    let mut search_sections = Vec::with_capacity(chapter_count);
    let mut node_counts = Vec::with_capacity(chapter_count);
    let mut retained = None;

    for chapter_index in 0..chapter_count {
        if !doc.set_current_chapter(chapter_index) {
            node_counts.push(0);
            continue;
        }

        let Some((raw_html, _mime)) = doc.get_current_str() else {
            node_counts.push(0);
            continue;
        };

        let title = extract_chapter_title(&raw_html);
        let document = Arc::new(converter.convert(&raw_html));
        let node_count = if is_non_content_chapter(title.as_deref(), &raw_html) {
            0
        } else {
            document.blocks.len()
        };
        node_counts.push(node_count);

        let search_title = title
            .clone()
            .unwrap_or_else(|| format!("Chapter {}", chapter_index + 1));
        search_sections.push((chapter_index, search_title, extract_search_lines(&document)));

        if chapter_index == initial_chapter {
            retained = Some(AnalyzedChapter {
                index: chapter_index,
                title,
                document,
            });
        }
    }

    if !doc.set_current_chapter(original_chapter) {
        log::warn!("Failed to restore EPUB chapter cursor after analysis");
    }

    let total_nodes = node_counts.iter().sum();
    EpubAnalysis {
        search_sections,
        node_counts,
        total_nodes,
        initial_chapter: retained,
    }
}

fn extract_search_lines(document: &Document) -> Vec<SearchLine> {
    let mut lines = Vec::new();
    for (node_index, node) in document.blocks.iter().enumerate() {
        extract_text_from_block(&node.block, node_index, &mut lines);
    }
    lines
}

fn extract_text_from_block(block: &Block, node_index: usize, lines: &mut Vec<SearchLine>) {
    match block {
        Block::Paragraph { content } | Block::Heading { content, .. } => {
            let plain_text = extract_text_from_text(content);
            if !plain_text.trim().is_empty() {
                lines.push(SearchLine {
                    text: plain_text,
                    node_index,
                    y_bounds: None,
                });
            }
        }
        Block::List { items, .. } => {
            for item in items {
                for node in &item.content {
                    extract_text_from_block(&node.block, node_index, lines);
                }
            }
        }
        Block::Quote { content } | Block::EpubBlock { content, .. } => {
            for node in content {
                extract_text_from_block(&node.block, node_index, lines);
            }
        }
        Block::CodeBlock { content, .. } => {
            lines.push(SearchLine {
                text: content.clone(),
                node_index,
                y_bounds: None,
            });
        }
        Block::Table { rows, header, .. } => {
            if let Some(header_row) = header {
                let row_text: Vec<String> = header_row
                    .cells
                    .iter()
                    .map(|cell| extract_text_from_cell_content(&cell.content, node_index, lines))
                    .collect();
                if !row_text.is_empty() {
                    lines.push(SearchLine {
                        text: row_text.join(" "),
                        node_index,
                        y_bounds: None,
                    });
                }
            }
            for row in rows {
                let row_text: Vec<String> = row
                    .cells
                    .iter()
                    .map(|cell| extract_text_from_cell_content(&cell.content, node_index, lines))
                    .collect();
                if !row_text.is_empty() {
                    lines.push(SearchLine {
                        text: row_text.join(" "),
                        node_index,
                        y_bounds: None,
                    });
                }
            }
        }
        Block::DefinitionList { items } => {
            for item in items {
                lines.push(SearchLine {
                    text: extract_text_from_text(&item.term),
                    node_index,
                    y_bounds: None,
                });
                for definition in &item.definitions {
                    for node in definition {
                        extract_text_from_block(&node.block, node_index, lines);
                    }
                }
            }
        }
        _ => {}
    }
}

fn extract_text_from_text(text: &Text) -> String {
    let mut result = String::new();

    for part in text.iter() {
        match part {
            TextOrInline::Text(text_node) => result.push_str(&text_node.content),
            TextOrInline::Inline(inline) => match inline {
                Inline::Link { text, .. } => result.push_str(&extract_text_from_text(text)),
                Inline::Image { alt_text, .. } => result.push_str(alt_text),
                Inline::LineBreak => result.push(' '),
                _ => {}
            },
        }
    }

    result
}

fn extract_text_from_cell_content(
    content: &TableCellContent,
    node_index: usize,
    lines: &mut Vec<SearchLine>,
) -> String {
    match content {
        TableCellContent::Simple(text) => extract_text_from_text(text),
        TableCellContent::Rich(nodes) => {
            let mut result = String::new();
            for node in nodes {
                extract_text_from_block(&node.block, node_index, lines);
                result.push_str(&extract_node_text(node));
            }
            result
        }
    }
}

fn extract_node_text(node: &crate::markdown::Node) -> String {
    match &node.block {
        Block::Paragraph { content } | Block::Heading { content, .. } => {
            extract_text_from_text(content)
        }
        Block::CodeBlock { content, .. } => content.clone(),
        Block::Quote { content } => content
            .iter()
            .map(extract_node_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::List { items, .. } => items
            .iter()
            .flat_map(|item| item.content.iter().map(extract_node_text))
            .collect::<Vec<_>>()
            .join(" "),
        Block::Table { header, rows, .. } => {
            let mut text = String::new();
            if let Some(header) = header {
                text.push_str(
                    &header
                        .cells
                        .iter()
                        .map(|cell| extract_cell_text(&cell.content))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            for row in rows {
                text.push_str(
                    &row.cells
                        .iter()
                        .map(|cell| extract_cell_text(&cell.content))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            text
        }
        _ => String::new(),
    }
}

fn extract_cell_text(content: &TableCellContent) -> String {
    match content {
        TableCellContent::Simple(text) => extract_text_from_text(text),
        TableCellContent::Rich(nodes) => nodes
            .iter()
            .map(extract_node_text)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Detect reference/backmatter chapters that should not count toward reading progress.
fn is_non_content_chapter(title: Option<&str>, html: &str) -> bool {
    const EPUB_TYPE_PATTERNS: &[&str] = &[
        "epub:type=\"index\"",
        "epub:type=\"glossary\"",
        "epub:type=\"bibliography\"",
    ];
    if EPUB_TYPE_PATTERNS
        .iter()
        .any(|pattern| html.contains(pattern))
    {
        return true;
    }

    let Some(title) = title else {
        return false;
    };
    let normalized = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    const BACKMATTER_EXACT: &[&str] = &[
        "index",
        "glossary",
        "bibliography",
        "works cited",
        "further reading",
        "list of figures",
        "list of tables",
        "list of illustrations",
    ];
    BACKMATTER_EXACT.contains(&normalized.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_engine::{SearchEngine, SearchResultTarget};
    use crate::test_utils::simple_fake_books::{FakeBookConfig, create_fake_epub_file};

    #[test]
    fn analysis_preserves_cursor_and_supplies_search_progress_and_initial_document() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("analysis.epub");
        create_fake_epub_file(
            &path,
            &FakeBookConfig {
                title: "Analysis Test".into(),
                chapter_count: 3,
                words_per_chapter: 80,
            },
        )
        .unwrap();

        let mut epub = EpubDoc::new(&path).unwrap();
        assert!(epub.set_current_chapter(1));

        let analysis = analyze_epub(&mut epub, 1);

        assert_eq!(epub.get_current_chapter(), 1);
        assert_eq!(analysis.search_sections.len(), 3);
        assert_eq!(analysis.node_counts.len(), 3);
        assert_eq!(
            analysis.total_nodes,
            analysis.node_counts.iter().sum::<usize>()
        );
        assert!(
            analysis
                .search_sections
                .iter()
                .all(|(_, _, lines)| !lines.is_empty())
        );

        let initial = analysis.initial_chapter.unwrap();
        assert_eq!(initial.index, 1);
        assert_eq!(initial.document.blocks.len(), analysis.node_counts[1]);

        let (raw_html, _) = epub.get_current_str().unwrap();
        let expected_document = HtmlToMarkdownConverter::new().convert(&raw_html);
        assert_eq!(*initial.document, expected_document);

        let mut search_engine = SearchEngine::new();
        search_engine.process_chapters(analysis.search_sections);
        let results = search_engine.search_fuzzy("tempor");
        let result_chapters = results
            .iter()
            .filter_map(|result| match result.target {
                SearchResultTarget::Epub { chapter_index, .. } => Some(chapter_index),
                SearchResultTarget::Pdf { .. } => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(result_chapters, [0, 1, 2].into_iter().collect());
    }

    #[test]
    fn only_backmatter_titles_and_epub_types_are_excluded_from_progress() {
        assert!(is_non_content_chapter(Some("Index"), "<body/>"));
        assert!(is_non_content_chapter(
            Some("Chapter 9"),
            "<section epub:type=\"bibliography\">"
        ));
        assert!(!is_non_content_chapter(
            Some("Building an Index"),
            "<body/>"
        ));
    }
}
