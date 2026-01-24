use std::collections::HashMap;

use crate::comments::Comment;
use crate::markdown::{Document, Inline, TableCellContent};
use crate::widget::comments_viewer::{ChapterDisplay, CommentEntry};

pub struct CommentsExporter<'a> {
    entries: &'a [CommentEntry],
    chapters: &'a [ChapterDisplay],
    book_title: &'a str,
    doc_cache: &'a HashMap<String, Document>,
}

impl<'a> CommentsExporter<'a> {
    pub fn new(
        entries: &'a [CommentEntry],
        chapters: &'a [ChapterDisplay],
        book_title: &'a str,
        doc_cache: &'a HashMap<String, Document>,
    ) -> Self {
        Self {
            entries,
            chapters,
            book_title,
            doc_cache,
        }
    }

    pub fn generate_markdown(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("# {}\n\n", self.book_title));

        // Sort entries by book order (chapter index, then node index within chapter)
        let mut sorted_entries: Vec<_> = self.entries.iter().collect();
        let chapter_order: HashMap<_, _> = self
            .chapters
            .iter()
            .enumerate()
            .filter_map(|(i, ch)| ch.href.as_ref().map(|h| (h.clone(), i)))
            .collect();
        sorted_entries.sort_by(|a, b| {
            let a_chapter = chapter_order.get(&a.chapter_href).unwrap_or(&usize::MAX);
            let b_chapter = chapter_order.get(&b.chapter_href).unwrap_or(&usize::MAX);
            a_chapter.cmp(b_chapter).then_with(|| {
                a.primary_comment()
                    .node_index()
                    .cmp(&b.primary_comment().node_index())
            })
        });

        let mut last_chapter: Option<String> = None;

        for entry in sorted_entries {
            // Add chapter header if changed
            if last_chapter.as_ref() != Some(&entry.chapter_href) {
                output.push_str(&format!("## {}\n\n", entry.chapter_title));
                last_chapter = Some(entry.chapter_href.clone());
            }

            if entry.is_code_block() {
                self.export_code_block_with_comments(entry, &mut output);
            } else {
                let full_context = self
                    .extract_full_context_for_export(&entry.chapter_href, entry.primary_comment());

                // Render the book fragment as a quote
                for line in full_context.lines() {
                    output.push_str("> ");
                    output.push_str(line);
                    output.push('\n');
                }
                output.push('\n');

                // Render comments as plain text with timestamp at the end
                for comment in &entry.comments {
                    let timestamp = comment.updated_at.format("%m-%d-%Y %H:%M");

                    output.push_str(&comment.content);
                    output.push('\n');
                    output.push_str(&format!("*// {timestamp}*\n"));
                    output.push_str("\n---\n\n");
                }
            }
        }

        output
    }

    pub fn generate_filename(book_title: &str) -> String {
        let kebab_title: String = book_title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        if kebab_title.is_empty() {
            "comments.md".to_string()
        } else {
            format!("{kebab_title}_comments.md")
        }
    }

    fn export_code_block_with_comments(&self, entry: &CommentEntry, output: &mut String) {
        use crate::markdown::Block;

        let Some(doc) = self.doc_cache.get(&entry.chapter_href) else {
            return;
        };

        let Some(node) = doc.blocks.get(entry.primary_comment().node_index()) else {
            return;
        };

        let Block::CodeBlock {
            content, language, ..
        } = &node.block
        else {
            return;
        };

        let lang = language.as_deref().unwrap_or("");
        let code_lines: Vec<&str> = content.lines().collect();

        // Build maps for where to insert comment markers
        let mut open_brackets: HashMap<usize, ()> = HashMap::new();
        let mut comments_after_line: HashMap<usize, Vec<&Comment>> = HashMap::new();

        for comment in &entry.comments {
            if let Some((start, end)) = comment.target.line_range() {
                if start != end {
                    open_brackets.insert(start, ());
                }
                comments_after_line.entry(end).or_default().push(comment);
            }
        }

        output.push_str(&format!("```{lang}\n"));

        for (line_idx, code_line) in code_lines.iter().enumerate() {
            if open_brackets.contains_key(&line_idx) {
                output.push_str("# ┌──\n");
            }

            output.push_str(code_line);
            output.push('\n');

            if let Some(comments) = comments_after_line.get(&line_idx) {
                for comment in comments {
                    let is_multiline = comment
                        .target
                        .line_range()
                        .map(|(s, e)| s != e)
                        .unwrap_or(false);

                    for (i, comment_line) in comment.content.lines().enumerate() {
                        if i == 0 {
                            if is_multiline {
                                output.push_str(&format!("# └── {comment_line}\n"));
                            } else {
                                output.push_str(&format!("# ^ {comment_line}\n"));
                            }
                        } else if comment_line.trim().is_empty() {
                            output.push_str("#\n");
                        } else {
                            output.push_str(&format!("#     {comment_line}\n"));
                        }
                    }
                }
            }
        }

        output.push_str("```\n\n---\n\n");
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
                    result.push_str(&format!("{prefix}{content}\n"));
                }
                result
            }
            Block::Quote { content } => {
                let mut result = String::new();
                for node in content {
                    let text = Self::extract_text_from_node(node);
                    result.push_str(&format!("> {text}\n"));
                }
                result
            }
            Block::DefinitionList { items } => {
                let mut result = String::new();
                for item in items {
                    let term = Self::extract_text_from_text(&item.term);
                    result.push_str(&format!("**{term}**\n"));
                    for def in &item.definitions {
                        for node in def {
                            let text = Self::extract_text_from_node(node);
                            result.push_str(&format!(": {text}\n"));
                        }
                    }
                }
                result
            }
            Block::CodeBlock {
                content, language, ..
            } => {
                let lang = language.as_deref().unwrap_or("");
                format!("```{lang}\n{content}\n```\n")
            }
            Block::Table { header, rows, .. } => Self::export_table_as_markdown(header, rows),
            _ => Self::extract_text_from_node(node),
        }
    }

    fn export_table_as_markdown(
        header: &Option<crate::markdown::TableRow>,
        rows: &[crate::markdown::TableRow],
    ) -> String {
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
                result.push_str(&format!(" {text} |"));
            }
            result.push('\n');

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
                result.push_str(&format!(" {text} |"));
            }
            result.push('\n');
        }

        result
    }

    fn extract_list_item_content(item: &crate::markdown::ListItem) -> String {
        item.content
            .iter()
            .map(Self::extract_text_from_node)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{Block, Node, TextNode, TextOrInline};

    #[test]
    fn test_generate_filename() {
        assert_eq!(
            CommentsExporter::generate_filename("Clean Code - A Handbook"),
            "clean-code-a-handbook_comments.md"
        );
        assert_eq!(
            CommentsExporter::generate_filename("The Art of Programming (2nd Ed.)"),
            "the-art-of-programming-2nd-ed_comments.md"
        );
        assert_eq!(CommentsExporter::generate_filename(""), "comments.md");
        assert_eq!(CommentsExporter::generate_filename("   "), "comments.md");
    }

    #[test]
    fn test_export_paragraph_comments() {
        use crate::comments::CommentTarget;
        use chrono::TimeZone;

        let make_paragraph = |text: &str| Node {
            block: Block::Paragraph {
                content: vec![TextOrInline::Text(TextNode {
                    content: text.to_string(),
                    style: None,
                })]
                .into(),
            },
            source_range: 0..100,
            id: None,
        };

        let doc = Document {
            blocks: vec![
                make_paragraph("First paragraph text."),
                make_paragraph("Second paragraph text."),
            ],
        };

        let comment1 = Comment::new(
            "chapter1.html".to_string(),
            CommentTarget::paragraph(0, None),
            "Comment on first paragraph.".to_string(),
            chrono::Utc
                .with_ymd_and_hms(2025, 11, 9, 10, 30, 0)
                .unwrap(),
        );

        let comment2 = Comment::new(
            "chapter1.html".to_string(),
            CommentTarget::paragraph(1, None),
            "Comment on second paragraph.\nWith multiple lines.".to_string(),
            chrono::Utc
                .with_ymd_and_hms(2025, 11, 10, 14, 45, 0)
                .unwrap(),
        );

        let entries = vec![
            CommentEntry {
                chapter_title: "Chapter 1".to_string(),
                chapter_href: "chapter1.html".to_string(),
                quoted_text: "First paragraph".to_string(),
                comments: vec![comment1],
                render_start_line: 0,
                render_end_line: 0,
            },
            CommentEntry {
                chapter_title: "Chapter 1".to_string(),
                chapter_href: "chapter1.html".to_string(),
                quoted_text: "Second paragraph".to_string(),
                comments: vec![comment2],
                render_start_line: 0,
                render_end_line: 0,
            },
        ];

        let chapters = vec![ChapterDisplay {
            title: "Chapter 1".to_string(),
            href: Some("chapter1.html".to_string()),
            depth: 0,
            comment_count: 2,
        }];

        let mut doc_cache = HashMap::new();
        doc_cache.insert("chapter1.html".to_string(), doc);

        let exporter = CommentsExporter::new(&entries, &chapters, "Test Book", &doc_cache);
        let export = exporter.generate_markdown();

        let expected = "\
# Test Book

## Chapter 1

> First paragraph text.

Comment on first paragraph.
*// 11-09-2025 10:30*

---

> Second paragraph text.

Comment on second paragraph.
With multiple lines.
*// 11-10-2025 14:45*

---

";

        assert_eq!(export, expected);
    }

    #[test]
    fn test_export_single_line_code_comment() {
        use crate::comments::CommentTarget;
        use chrono::TimeZone;

        let doc = Document {
            blocks: vec![Node {
                block: Block::CodeBlock {
                    content: "fn main() {\n    println!(\"Hello\");\n    return 0;\n}".to_string(),
                    language: Some("rust".to_string()),
                },
                source_range: 0..100,
                id: None,
            }],
        };

        let comment = Comment::new(
            "chapter1.html".to_string(),
            CommentTarget::code_block(0, (1, 1)),
            "This prints a greeting".to_string(),
            chrono::Utc.with_ymd_and_hms(2025, 12, 1, 9, 0, 0).unwrap(),
        );

        let entries = vec![CommentEntry {
            chapter_title: "Chapter 1".to_string(),
            chapter_href: "chapter1.html".to_string(),
            quoted_text: "println".to_string(),
            comments: vec![comment],
            render_start_line: 0,
            render_end_line: 0,
        }];

        let chapters = vec![ChapterDisplay {
            title: "Chapter 1".to_string(),
            href: Some("chapter1.html".to_string()),
            depth: 0,
            comment_count: 1,
        }];

        let mut doc_cache = HashMap::new();
        doc_cache.insert("chapter1.html".to_string(), doc);

        let exporter = CommentsExporter::new(&entries, &chapters, "Code Book", &doc_cache);
        let export = exporter.generate_markdown();

        let expected = "\
# Code Book

## Chapter 1

```rust
fn main() {
    println!(\"Hello\");
# ^ This prints a greeting
    return 0;
}
```

---

";

        assert_eq!(export, expected);
    }

    #[test]
    fn test_export_multiline_code_comment() {
        use crate::comments::CommentTarget;
        use chrono::TimeZone;

        let doc = Document {
            blocks: vec![Node {
                block: Block::CodeBlock {
                    content: "fn add(a: i32, b: i32) -> i32 {\n    let sum = a + b;\n    sum\n}"
                        .to_string(),
                    language: Some("rust".to_string()),
                },
                source_range: 0..100,
                id: None,
            }],
        };

        let comment = Comment::new(
            "chapter1.html".to_string(),
            CommentTarget::code_block(0, (1, 2)),
            "Calculate and return sum".to_string(),
            chrono::Utc.with_ymd_and_hms(2025, 12, 1, 10, 0, 0).unwrap(),
        );

        let entries = vec![CommentEntry {
            chapter_title: "Chapter 1".to_string(),
            chapter_href: "chapter1.html".to_string(),
            quoted_text: "sum calculation".to_string(),
            comments: vec![comment],
            render_start_line: 0,
            render_end_line: 0,
        }];

        let chapters = vec![ChapterDisplay {
            title: "Chapter 1".to_string(),
            href: Some("chapter1.html".to_string()),
            depth: 0,
            comment_count: 1,
        }];

        let mut doc_cache = HashMap::new();
        doc_cache.insert("chapter1.html".to_string(), doc);

        let exporter = CommentsExporter::new(&entries, &chapters, "Code Book", &doc_cache);
        let export = exporter.generate_markdown();

        let expected = "\
# Code Book

## Chapter 1

```rust
fn add(a: i32, b: i32) -> i32 {
# ┌──
    let sum = a + b;
    sum
# └── Calculate and return sum
}
```

---

";

        assert_eq!(export, expected);
    }

    #[test]
    fn test_export_comments_ordered_by_book_structure() {
        use crate::comments::CommentTarget;
        use chrono::TimeZone;

        let make_paragraph = |text: &str| Node {
            block: Block::Paragraph {
                content: vec![TextOrInline::Text(TextNode {
                    content: text.to_string(),
                    style: None,
                })]
                .into(),
            },
            source_range: 0..100,
            id: None,
        };

        let doc1 = Document {
            blocks: vec![make_paragraph("Chapter 1 text.")],
        };
        let doc2 = Document {
            blocks: vec![make_paragraph("Chapter 2 text.")],
        };

        let comment1 = Comment::new(
            "chapter2.html".to_string(),
            CommentTarget::paragraph(0, None),
            "Comment on chapter 2".to_string(),
            chrono::Utc.with_ymd_and_hms(2025, 1, 1, 8, 0, 0).unwrap(),
        );

        let comment2 = Comment::new(
            "chapter1.html".to_string(),
            CommentTarget::paragraph(0, None),
            "Comment on chapter 1".to_string(),
            chrono::Utc.with_ymd_and_hms(2025, 1, 2, 9, 0, 0).unwrap(),
        );

        // Entries are given in wrong order (ch2 before ch1)
        let entries = vec![
            CommentEntry {
                chapter_title: "Chapter 2".to_string(),
                chapter_href: "chapter2.html".to_string(),
                quoted_text: "text".to_string(),
                comments: vec![comment1],
                render_start_line: 0,
                render_end_line: 0,
            },
            CommentEntry {
                chapter_title: "Chapter 1".to_string(),
                chapter_href: "chapter1.html".to_string(),
                quoted_text: "text".to_string(),
                comments: vec![comment2],
                render_start_line: 0,
                render_end_line: 0,
            },
        ];

        // Chapters define the book order (ch1 first, then ch2)
        let chapters = vec![
            ChapterDisplay {
                title: "Chapter 1".to_string(),
                href: Some("chapter1.html".to_string()),
                depth: 0,
                comment_count: 1,
            },
            ChapterDisplay {
                title: "Chapter 2".to_string(),
                href: Some("chapter2.html".to_string()),
                depth: 0,
                comment_count: 1,
            },
        ];

        let mut doc_cache = HashMap::new();
        doc_cache.insert("chapter1.html".to_string(), doc1);
        doc_cache.insert("chapter2.html".to_string(), doc2);

        let exporter = CommentsExporter::new(&entries, &chapters, "Test Book", &doc_cache);
        let export = exporter.generate_markdown();

        // Chapter 1 should appear before Chapter 2 in the export
        let expected = "\
# Test Book

## Chapter 1

> Chapter 1 text.

Comment on chapter 1
*// 01-02-2025 09:00*

---

## Chapter 2

> Chapter 2 text.

Comment on chapter 2
*// 01-01-2025 08:00*

---

";

        assert_eq!(export, expected);
    }
}
