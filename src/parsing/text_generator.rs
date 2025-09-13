use crate::parsing::{
    html_to_markdown::HtmlToMarkdownConverter, markdown_renderer::MarkdownRenderer,
};
use crate::table_of_contents::TocItem;
use crate::toc_parser::TocParser;
use epub::doc::EpubDoc;
use log::{debug, warn};
use regex::Regex;
use std::io::BufReader;

/// High-level orchestrator for EPUB HTML content processing and text generation.
///
/// This component serves as the main interface for converting EPUB HTML content
/// into clean, readable text suitable for terminal display. It coordinates the
/// entire HTML→Markdown→Text pipeline and manages EPUB-specific operations.
///
/// # Responsibilities
///
/// ## EPUB Content Processing
/// ## Content Preprocessing
/// ## Chapter Title Extraction
/// ## Table of Contents Integration
/// ## Pipeline Orchestration
/// # Architecture Integration
///
/// # Usage
///
/// ```rust,no_run
/// use bookrat::parsing::html5ever_text_generator::TextGenerator;
/// # use std::io::BufReader;
/// # use epub::doc::EpubDoc;
/// # fn main() -> Result<(), String> {
/// let generator = TextGenerator::new();
/// # let mut epub_doc = EpubDoc::new("test.epub").unwrap();
/// let (processed_text, chapter_title) = generator
///     .process_chapter_content(&mut epub_doc)?;
/// # Ok(())
/// # }
/// ```
///
/// # Design Notes
///
/// This component maintains the original TextGenerator's public interface for
/// backward compatibility while internally using the new HTML5ever-based pipeline.
/// The `clean_xml_and_doctype` method is marked for future migration to
/// HtmlToMarkdownConverter as part of ongoing refactoring efforts.
pub struct TextGenerator {
    toc_parser: TocParser,
}

impl TextGenerator {
    // IDENTICAL public interface to original TextGenerator
    pub fn new() -> Self {
        Self {
            toc_parser: TocParser::new(),
        }
    }

    pub fn extract_chapter_title(&self, html_content: &str) -> Option<String> {
        let h1_pattern = Regex::new(r"(?s)<h1[^>]*>(.*?)</h1>").ok()?;
        let h2_pattern = Regex::new(r"(?s)<h2[^>]*>(.*?)</h2>").ok()?;
        let h3_pattern = Regex::new(r"(?s)<h3[^>]*>(.*?)</h3>").ok()?;
        let title_pattern = Regex::new(r"(?s)<title[^>]*>(.*?)</title>").ok()?;

        for re in vec![h1_pattern, h2_pattern, h3_pattern, title_pattern] {
            if let Some(captures) = re.captures(html_content) {
                if let Some(title_match) = captures.get(1) {
                    let title = self.extract_text_from_html(title_match.as_str());
                    if !title.is_empty() && title.len() < 100 {
                        return Some(title);
                    }
                }
            }
        }

        None
    }

    /// Helper function to extract plain text from HTML, removing tags but keeping content
    fn extract_text_from_html(&self, html: &str) -> String {
        // Remove all HTML tags but keep their content, don't add spaces
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let text = tag_re.replace_all(html, " ");

        // Clean up all whitespace (spaces, newlines, tabs) and collapse to single space
        let whitespace_re = Regex::new(r"\s+").unwrap();
        let cleaned = whitespace_re.replace_all(&text, " ");

        cleaned.trim().to_string()
    }

    /// Normalize href for comparison by removing relative path prefixes, OEBPS directory, and fragments
    pub fn normalize_href(&self, href: &str) -> String {
        let normalized = href
            .trim_start_matches("../")
            .trim_start_matches("./")
            .trim_start_matches("OEBPS/");

        // Remove fragment identifiers (e.g., "#ch1", "#tit") for matching
        if let Some(fragment_pos) = normalized.find('#') {
            normalized[..fragment_pos].to_string()
        } else {
            normalized.to_string()
        }
    }

    pub fn parse_toc_structure(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Vec<TocItem> {
        self.toc_parser.parse_toc_structure(doc)
    }

    /// Processes EPUB chapter content and extracts clean text and title.
    ///
    /// This is the main method for extracting and processing content from an EPUB chapter.
    /// It handles the complete pipeline from raw EPUB HTML to clean, readable text.
    ///
    /// # Arguments
    /// * `doc` - Mutable reference to the EPUB document positioned at desired chapter
    ///
    /// # Returns
    /// * `Ok((String, Option<String>))` - Tuple of (processed_text, chapter_title)
    /// * `Err(String)` - Error message if processing fails
    ///
    /// # Process
    /// 1. Extract raw HTML content from current EPUB chapter
    /// 2. Extract chapter title from HTML headings or title tags
    /// 3. Clean XML declarations and unwanted tags
    /// 4. Convert through HTML→Markdown→Text pipeline
    /// 5. Return processed text with optional chapter title
    pub fn process_chapter_content(
        &self,
        doc: &mut EpubDoc<BufReader<std::fs::File>>,
    ) -> Result<(String, Option<String>), String> {
        let content = doc
            .get_current_str()
            .map_err(|e| format!("Failed to get chapter content: {}", e))?;
        debug!("Raw content length: {} bytes", content.len());

        let chapter_title = self.extract_chapter_title(&content);

        let cleaned_content = self.clean_xml_and_doctype(&content);
        let processed_text = self.convert_to_clean_text(&cleaned_content);

        if processed_text.is_empty() {
            warn!("Converted text is empty");
            Ok((
                "No content available in this chapter.".to_string(),
                chapter_title,
            ))
        } else {
            debug!("Final text length: {} bytes", processed_text.len());
            Ok((processed_text, chapter_title))
        }
    }

    /// Converts HTML content to clean text through the complete pipeline.
    ///
    /// This is the main entry point for the HTML→Markdown→Text conversion process.
    /// It orchestrates all phases of the conversion pipeline.
    ///
    /// # Process
    /// 1. Convert HTML to clean Markdown AST (with text cleanup during conversion)
    /// 2. Render clean AST to string (simple rendering only)
    /// 3. Apply placeholder substitutions and restore MathML content
    ///
    /// # Arguments
    /// * `html` - Raw HTML content to convert
    ///
    /// # Returns
    /// Clean, formatted text suitable for terminal display
    pub fn convert_to_clean_text(&self, html: &str) -> String {
        let mut converter = HtmlToMarkdownConverter::new();
        let clean_markdown_doc = converter.convert(html);

        let renderer = MarkdownRenderer::new();
        let rendered = renderer.render(&clean_markdown_doc);

        converter.decode_entities(&rendered)
    }

    //todo! this is needs to be moved to html_to_markdown.rs
    fn clean_xml_and_doctype(&self, content: &str) -> String {
        // First remove XML declaration and DOCTYPE
        let xml_decl_re = Regex::new(r"<\?xml[^?]*\?>").unwrap();
        let doctype_re = Regex::new(r"(?i)<!DOCTYPE[^>]*>").unwrap();
        let mut content = xml_decl_re.replace_all(content, "").into_owned();
        content = doctype_re.replace_all(&content, "").into_owned();

        // Remove style and script tags completely
        let style_re = Regex::new(r"(?s)<style[^>]*>.*?</style>").unwrap();
        let script_re = Regex::new(r"(?s)<script[^>]*>.*?</script>").unwrap();
        content = style_re.replace_all(&content, "").into_owned();
        content = script_re.replace_all(&content, "").into_owned();

        content
    }
}

impl Default for TextGenerator {
    fn default() -> Self {
        Self::new()
    }
}
