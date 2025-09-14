use crate::parsing::toc_parser::TocParser;
use crate::table_of_contents::TocItem;
use epub::doc::EpubDoc;
use regex::Regex;
use std::io::{Read, Seek};

pub struct TextGenerator {
    toc_parser: TocParser,
}

impl TextGenerator {
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

    pub fn parse_toc_structure<R: Read + Seek>(&self, doc: &mut EpubDoc<R>) -> Vec<TocItem> {
        self.toc_parser.parse_toc_structure(doc)
    }
}

impl Default for TextGenerator {
    fn default() -> Self {
        Self::new()
    }
}
