use crate::comments::{BookComments, Comment};
use crate::export::filename::sanitize_filename;
use crate::export::template::TemplateEngine;
use crate::parsing::html_to_markdown::HtmlToMarkdownConverter;
use crate::parsing::markdown_renderer::MarkdownRenderer;
use crate::widget::export_menu::{ExportContent, ExportFormat, ExportOrganization};
use anyhow::{Context, Result};
use chrono::Local;
use epub::doc::EpubDoc;
use log::{debug, info};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ExportError {
    NoAnnotations,
    ExportDirNotFound,
    WriteError(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::NoAnnotations => write!(f, "No annotations found to export"),
            ExportError::ExportDirNotFound => write!(f, "Export directory not found"),
            ExportError::WriteError(msg) => write!(f, "Failed to write export: {}", msg),
        }
    }
}

impl std::error::Error for ExportError {}

pub struct AnnotationExporter;

impl AnnotationExporter {
    pub fn export<R: Read + Seek>(
        book_comments: &BookComments,
        epub: &mut EpubDoc<R>,
        book_title: &str,
        export_dir: &Path,
        format: ExportFormat,
        content: ExportContent,
        organization: ExportOrganization,
        frontmatter_template: &str,
    ) -> Result<Vec<PathBuf>> {
        let comments = book_comments.get_all_comments();

        if comments.is_empty() {
            return Err(ExportError::NoAnnotations.into());
        }

        if !export_dir.exists() {
            return Err(ExportError::ExportDirNotFound.into());
        }

        info!(
            "Exporting {} annotations in {:?} format with {:?} organization",
            comments.len(),
            format,
            organization
        );

        match organization {
            ExportOrganization::SingleFile => Self::export_single_file(
                book_comments,
                epub,
                book_title,
                export_dir,
                format,
                content,
                frontmatter_template,
            ),
            ExportOrganization::ChapterBased => Self::export_chapter_based(
                book_comments,
                epub,
                book_title,
                export_dir,
                format,
                content,
                frontmatter_template,
            ),
        }
    }

    fn export_single_file<R: Read + Seek>(
        book_comments: &BookComments,
        epub: &mut EpubDoc<R>,
        book_title: &str,
        export_dir: &Path,
        format: ExportFormat,
        content: ExportContent,
        frontmatter_template: &str,
    ) -> Result<Vec<PathBuf>> {
        let comments = book_comments.get_all_comments();
        let extension = Self::format_extension(format);
        let sanitized_title = sanitize_filename(book_title);
        let filename = format!("{}.{}", sanitized_title, extension);
        let filepath = export_dir.join(&filename);

        let mut output = String::new();

        // Add frontmatter
        let template_vars = Self::build_template_vars(book_title, None, comments.len());
        let frontmatter = TemplateEngine::render(frontmatter_template, &template_vars);
        output.push_str(&frontmatter);
        output.push('\n');

        // Group comments by chapter
        let chapters = Self::group_comments_by_chapter(comments);

        // Get chapter titles
        let chapter_titles = Self::extract_chapter_titles(epub);

        for (chapter_href, chapter_comments) in chapters {
            let chapter_title = chapter_titles
                .get(&chapter_href)
                .map(|s| s.as_str())
                .unwrap_or(&chapter_href);

            output.push_str(&Self::format_chapter_header(chapter_title, format));
            output.push('\n');

            for (idx, comment) in chapter_comments.iter().enumerate() {
                let annotation_text =
                    Self::format_annotation(comment, idx + 1, format, content, epub)?;
                output.push_str(&annotation_text);
                output.push('\n');
            }

            output.push('\n');
        }

        fs::write(&filepath, output)
            .with_context(|| format!("Failed to write to {}", filepath.display()))?;

        info!("Exported annotations to: {}", filepath.display());
        Ok(vec![filepath])
    }

    fn export_chapter_based<R: Read + Seek>(
        book_comments: &BookComments,
        epub: &mut EpubDoc<R>,
        book_title: &str,
        export_dir: &Path,
        format: ExportFormat,
        content: ExportContent,
        frontmatter_template: &str,
    ) -> Result<Vec<PathBuf>> {
        let comments = book_comments.get_all_comments();
        let extension = Self::format_extension(format);
        let sanitized_title = sanitize_filename(book_title);

        let chapters = Self::group_comments_by_chapter(comments);
        let chapter_titles = Self::extract_chapter_titles(epub);

        let mut exported_files = Vec::new();

        for (chapter_href, chapter_comments) in chapters {
            let chapter_title = chapter_titles
                .get(&chapter_href)
                .map(|s| s.as_str())
                .unwrap_or(&chapter_href);

            // Get chapter index from href
            let chapter_index = Self::get_chapter_index_from_href(epub, &chapter_href);

            for (annotation_num, comment) in chapter_comments.iter().enumerate() {
                let filename = format!(
                    "{}-ch{}-{:02}.{}",
                    sanitized_title,
                    chapter_index + 1,
                    annotation_num + 1,
                    extension
                );
                let filepath = export_dir.join(&filename);

                let mut output = String::new();

                // Add frontmatter
                let template_vars = Self::build_template_vars(book_title, Some(chapter_title), 1);
                let frontmatter = TemplateEngine::render(frontmatter_template, &template_vars);
                output.push_str(&frontmatter);
                output.push('\n');

                // Add chapter header
                output.push_str(&Self::format_chapter_header(chapter_title, format));
                output.push('\n');

                let annotation_text = Self::format_annotation(comment, 1, format, content, epub)?;
                output.push_str(&annotation_text);
                output.push('\n');

                fs::write(&filepath, output)
                    .with_context(|| format!("Failed to write to {}", filepath.display()))?;

                debug!("Exported chapter annotation to: {}", filepath.display());
                exported_files.push(filepath);
            }
        }

        info!("Exported {} chapter files", exported_files.len());
        Ok(exported_files)
    }

    fn format_annotation<R: Read + Seek>(
        comment: &Comment,
        number: usize,
        format: ExportFormat,
        content: ExportContent,
        epub: &mut EpubDoc<R>,
    ) -> Result<String> {
        let mut output = String::new();

        // Format timestamp
        let timestamp = comment.updated_at.format("%Y-%m-%d %H:%M").to_string();

        match format {
            ExportFormat::Markdown => {
                output.push_str(&format!("### Annotation {}\n\n", number));
                output.push_str(&format!("**Date:** {}\n\n", timestamp));

                if matches!(content, ExportContent::AnnotationsWithContext) {
                    if let Some(context) = Self::extract_context(comment, epub)? {
                        output.push_str("> ");
                        output.push_str(&context.replace('\n', "\n> "));
                        output.push_str("\n\n");
                    }
                }

                output.push_str(&format!("**Note:** {}\n\n", comment.content));
            }
            ExportFormat::OrgMode => {
                output.push_str(&format!("*** Annotation {}\n", number));
                output.push_str(&format!(":PROPERTIES:\n:DATE: {}\n:END:\n\n", timestamp));

                if matches!(content, ExportContent::AnnotationsWithContext) {
                    if let Some(context) = Self::extract_context(comment, epub)? {
                        output.push_str("#+BEGIN_QUOTE\n");
                        output.push_str(&context);
                        output.push_str("\n#+END_QUOTE\n\n");
                    }
                }

                output.push_str(&format!("{}\n\n", comment.content));
            }
            ExportFormat::PlainText => {
                output.push_str(&format!("Annotation {}\n", number));
                output.push_str(&format!("Date: {}\n\n", timestamp));

                if matches!(content, ExportContent::AnnotationsWithContext) {
                    if let Some(context) = Self::extract_context(comment, epub)? {
                        output.push_str("Context:\n");
                        output.push_str(&context);
                        output.push_str("\n\n");
                    }
                }

                output.push_str(&format!("Note:\n{}\n\n", comment.content));
                output.push_str("---\n\n");
            }
        }

        Ok(output)
    }

    fn extract_context<R: Read + Seek>(
        comment: &Comment,
        epub: &mut EpubDoc<R>,
    ) -> Result<Option<String>> {
        // If selected_text is available, use it
        if let Some(ref selected) = comment.selected_text {
            return Ok(Some(selected.clone()));
        }

        // Otherwise, extract from EPUB
        let original_chapter = epub.get_current_chapter();

        // Navigate to comment's chapter
        let chapter_path = PathBuf::from(&comment.chapter_href);
        if let Some(chapter_idx) = epub.resource_uri_to_chapter(&chapter_path) {
            if epub.set_current_chapter(chapter_idx) {
                if let Some((content_bytes, _)) = epub.get_current_str() {
                    // Parse HTML to Markdown AST
                    let mut converter = HtmlToMarkdownConverter::new();
                    let doc = converter.convert(&content_bytes);

                    // Find the paragraph node
                    let node_index = comment.target.node_index();
                    if let Some(node) = doc.blocks.get(node_index) {
                        // Create a temporary Document with just this node for rendering
                        use crate::markdown::Document;
                        let temp_doc: Document = Document {
                            blocks: vec![node.clone()],
                        };

                        // Use MarkdownRenderer to convert to text
                        let renderer = MarkdownRenderer::new();
                        let paragraph_text = renderer.render(&temp_doc);

                        // If there's a word_range, extract just that portion
                        if let Some((start, end)) = comment.target.word_range() {
                            let words: Vec<&str> = paragraph_text.split_whitespace().collect();
                            if end <= words.len() {
                                let selected_words = &words[start..end];
                                epub.set_current_chapter(original_chapter);
                                return Ok(Some(selected_words.join(" ")));
                            }
                        }

                        // No word range or extraction failed, return full paragraph
                        epub.set_current_chapter(original_chapter);
                        return Ok(Some(paragraph_text));
                    }
                }
            }
        }

        // Restore original chapter
        epub.set_current_chapter(original_chapter);
        Ok(None)
    }

    fn group_comments_by_chapter(comments: &[Comment]) -> Vec<(String, Vec<&Comment>)> {
        let mut chapters: HashMap<String, Vec<&Comment>> = HashMap::new();

        for comment in comments {
            chapters
                .entry(comment.chapter_href.clone())
                .or_default()
                .push(comment);
        }

        let mut result: Vec<_> = chapters.into_iter().collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    fn extract_chapter_titles<R: Read + Seek>(epub: &mut EpubDoc<R>) -> HashMap<String, String> {
        let mut titles = HashMap::new();
        let original_chapter = epub.get_current_chapter();

        for idx in 0..epub.get_num_chapters() {
            if epub.set_current_chapter(idx) {
                if let Some((content, _)) = epub.get_current_str() {
                    let title = Self::extract_title_from_html(&content);

                    if let Some(spine_item) = epub.spine.get(idx) {
                        if let Some(resource) = epub.resources.get(&spine_item.idref) {
                            let href = resource.path.to_string_lossy().to_string();
                            titles.insert(href, title);
                        }
                    }
                }
            }
        }

        epub.set_current_chapter(original_chapter);
        titles
    }

    fn extract_title_from_html(html: &str) -> String {
        use regex::Regex;

        // Try to find h1, h2, or title tags
        let title_regex = Regex::new(r"<(?:h1|h2|title)[^>]*>(.*?)</(?:h1|h2|title)>").unwrap();

        if let Some(captures) = title_regex.captures(html) {
            if let Some(title_match) = captures.get(1) {
                let title = title_match.as_str();
                // Strip HTML tags
                let tag_strip = Regex::new(r"<[^>]+>").unwrap();
                let clean = tag_strip.replace_all(title, "");
                return clean.trim().to_string();
            }
        }

        "Untitled".to_string()
    }

    fn get_chapter_index_from_href<R: Read + Seek>(epub: &EpubDoc<R>, href: &str) -> usize {
        let path = PathBuf::from(href);
        epub.resource_uri_to_chapter(&path).unwrap_or(0)
    }

    fn format_chapter_header(title: &str, format: ExportFormat) -> String {
        match format {
            ExportFormat::Markdown => format!("## {}\n", title),
            ExportFormat::OrgMode => format!("** {}\n", title),
            ExportFormat::PlainText => format!("{}\n{}\n", title, "=".repeat(title.len())),
        }
    }

    fn format_extension(format: ExportFormat) -> &'static str {
        match format {
            ExportFormat::Markdown => "md",
            ExportFormat::OrgMode => "org",
            ExportFormat::PlainText => "txt",
        }
    }

    fn build_template_vars(
        book_title: &str,
        chapter_title: Option<&str>,
        annotation_count: usize,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("book_title".to_string(), book_title.to_string());
        vars.insert(
            "export_date".to_string(),
            Local::now().format("%Y-%m-%d").to_string(),
        );
        vars.insert("annotation_count".to_string(), annotation_count.to_string());
        vars.insert(
            "chapter_title".to_string(),
            chapter_title.unwrap_or("All Chapters").to_string(),
        );
        vars
    }
}
