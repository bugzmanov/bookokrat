use crate::table_of_contents::TocItem;
use crate::toc_parser::TocParser;
use epub::doc::EpubDoc;
use log::{debug, warn};
use regex::Regex;
use std::io::BufReader;

pub struct TextGenerator {
    p_tag_re: Regex,
    h_open_re: Regex,
    h_close_re: Regex,
    remaining_tags_re: Regex,
    multi_space_re: Regex,
    multi_newline_re: Regex,
    leading_space_re: Regex,
    line_leading_space_re: Regex,
    img_tag_re: Regex,
    toc_parser: TocParser,
}

impl TextGenerator {
    pub fn new() -> Self {
        Self {
            p_tag_re: Regex::new(r"<p[^>]*>").expect("Failed to compile paragraph tag regex"),
            h_open_re: Regex::new(r"<h[1-6][^>]*>")
                .expect("Failed to compile header open tag regex"),
            h_close_re: Regex::new(r"</h[1-6]>").expect("Failed to compile header close tag regex"),
            remaining_tags_re: Regex::new(r"<[^>]*>")
                .expect("Failed to compile remaining tags regex"),
            multi_space_re: Regex::new(r" +").expect("Failed to compile multi space regex"),
            multi_newline_re: Regex::new(r"\n{3,}").expect("Failed to compile multi newline regex"),
            leading_space_re: Regex::new(r"^ +").expect("Failed to compile leading space regex"),
            line_leading_space_re: Regex::new(r"\n +")
                .expect("Failed to compile line leading space regex"),
            img_tag_re: Regex::new(r#"<img[^>]*\ssrc\s*=\s*["']([^"']+)["'][^>]*>"#)
                .expect("Failed to compile image tag regex"),
            toc_parser: TocParser::new(),
        }
    }

    pub fn extract_chapter_title(&self, html_content: &str) -> Option<String> {
        let title_patterns = [
            Regex::new(r"<h[12][^>]*>([^<]+)</h[12]>").ok()?,
            Regex::new(r"<title[^>]*>([^<]+)</title>").ok()?,
        ];

        for pattern in &title_patterns {
            if let Some(captures) = pattern.captures(html_content) {
                if let Some(title_match) = captures.get(1) {
                    let title = title_match.as_str().trim();
                    if !title.is_empty() && title.len() < 100 {
                        return Some(title.to_string());
                    }
                }
            }
        }
        None
    }

    /// Check if this chapter is a section header by comparing its href with the TOC structure
    /// A chapter is a section header if it appears in the TOC and has children
    pub fn is_section_header(&self, chapter_href: &str, toc_entries: &[TocItem]) -> bool {
        self.find_entry_with_children(chapter_href, toc_entries)
    }

    /// Recursively search for an entry with the given href that has children
    fn find_entry_with_children(&self, href: &str, entries: &[TocItem]) -> bool {
        for entry in entries {
            match entry {
                TocItem::Chapter {
                    href: entry_href, ..
                } => {
                    // Chapters don't have children
                    let _ = entry_href; // Unused, just for clarity
                }
                TocItem::Section {
                    href: entry_href,
                    children,
                    ..
                } => {
                    if let Some(entry_href) = entry_href {
                        // Normalize hrefs for comparison (remove leading ../ and ./)
                        let normalized_entry_href = self.normalize_href(entry_href);
                        let normalized_target_href = self.normalize_href(href);

                        if normalized_entry_href == normalized_target_href && !children.is_empty() {
                            return true;
                        }
                    }

                    // Also check children recursively
                    if self.find_entry_with_children(href, children) {
                        return true;
                    }
                }
            }
        }
        false
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

    /// Extract section title - will be handled by TOC parsing
    pub fn extract_section_title(&self, html_content: &str) -> Option<String> {
        // Fallback to regular chapter title extraction
        self.extract_chapter_title(html_content)
    }

    /// Parse EPUB table of contents to get hierarchical structure
    pub fn parse_toc_structure(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Vec<TocItem> {
        debug!("TextGenerator::parse_toc_structure called");
        let result = self.toc_parser.parse_toc_structure(doc);
        debug!(
            "TextGenerator::parse_toc_structure returning {} entries",
            result.len()
        );
        result
    }

    pub fn process_chapter_content(
        &self,
        doc: &mut EpubDoc<BufReader<std::fs::File>>,
    ) -> Result<(String, Option<String>), String> {
        let content = doc
            .get_current_str()
            .map_err(|e| format!("Failed to get chapter content: {}", e))?;
        debug!("Raw content length: {} bytes", content.len());

        let chapter_title = self.extract_chapter_title(&content);
        let processed_text = self.clean_html_content(&content, &chapter_title);

        if processed_text.is_empty() {
            warn!("Converted text is empty");
            Ok((
                "No content available in this chapter.".to_string(),
                chapter_title,
            ))
        } else {
            debug!("Final text length: {} bytes", processed_text.len());
            let formatted_text = self.format_text_with_spacing(&processed_text);

            let mut final_text = formatted_text;
            if let Some(ref title) = chapter_title {
                let trimmed_content = final_text.trim_start();
                if trimmed_content.starts_with(title) {
                    final_text = trimmed_content[title.len()..].trim_start().to_string();
                }
            }

            Ok((final_text, chapter_title))
        }
    }

    fn clean_html_content(&self, content: &str, chapter_title: &Option<String>) -> String {
        let style_re = Regex::new(r"(?s)<style[^>]*>.*?</style>").unwrap();
        let script_re = Regex::new(r"(?s)<script[^>]*>.*?</script>").unwrap();
        let mut content = style_re.replace_all(content, "").into_owned();
        content = script_re.replace_all(&content, "").into_owned();

        if let Some(_title) = chapter_title {
            // Remove h1/h2 tags that contain the chapter title
            // This handles complex nested structures by removing the entire h1/h2 tag
            let title_removal_re = Regex::new(r"(?s)<h[12][^>]*>.*?</h[12]>").unwrap();
            content = title_removal_re.replace_all(&content, "").into_owned();

            // Also remove title tags since they can contain duplicate title text
            let title_tag_re = Regex::new(r"(?s)<title[^>]*>.*?</title>").unwrap();
            content = title_tag_re.replace_all(&content, "").into_owned();
        }

        // Process img tags into text placeholders
        content = self
            .img_tag_re
            .replace_all(&content, "\n\n[image src=\"$1\"]\n\n")
            .into_owned();

        let text = content
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&mdash;", "—")
            .replace("&ndash;", "–")
            .replace("&hellip;", "...")
            .replace("&ldquo;", "\u{201C}")
            .replace("&rdquo;", "\u{201D}")
            .replace("&lsquo;", "\u{2018}")
            .replace("&rsquo;", "\u{2019}");

        let text = self.p_tag_re.replace_all(&text, "").to_string();

        let text = text
            .replace("</p>", "\n\n")
            .replace("<br>", "\n")
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("<blockquote>", "\n    ")
            .replace("</blockquote>", "\n")
            .replace("<em>", "_")
            .replace("</em>", "_")
            .replace("<i>", "_")
            .replace("</i>", "_")
            .replace("<strong>", "**")
            .replace("</strong>", "**")
            .replace("<b>", "**")
            .replace("</b>", "**")
            .replace("<div>", "")
            .replace("</div>", "\n");

        let text = self.h_open_re.replace_all(&text, "\n\n").to_string();
        let text = self.h_close_re.replace_all(&text, "\n\n").to_string();
        let text = self.remaining_tags_re.replace_all(&text, "").to_string();

        let text = self.multi_space_re.replace_all(&text, " ").to_string();
        let text = self.multi_newline_re.replace_all(&text, "\n\n").to_string();
        let text = Regex::new(r"\n\s*\n")
            .unwrap()
            .replace_all(&text, "\n\n")
            .to_string();
        let text = self.multi_newline_re.replace_all(&text, "\n\n").to_string();
        let text = self.leading_space_re.replace_all(&text, "").to_string();
        let text = self
            .line_leading_space_re
            .replace_all(&text, "\n")
            .to_string();

        text.trim().to_string()
    }

    fn format_text_with_spacing(&self, text: &str) -> String {
        let mut formatted = String::new();
        let normalized_text = self.multi_newline_re.replace_all(text, "\n\n");
        let paragraphs: Vec<&str> = normalized_text.split("\n\n").collect();

        for (i, paragraph) in paragraphs.iter().enumerate() {
            if paragraph.trim().is_empty() {
                continue;
            }

            let lines: Vec<&str> = paragraph.lines().collect();
            for (j, line) in lines.iter().enumerate() {
                formatted.push_str(line);
                if j < lines.len() - 1 {
                    formatted.push('\n');
                }
            }

            if i < paragraphs.len() - 1 {
                formatted.push_str("\n\n");
            }
        }

        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_section_header_with_toc_structure() {
        let generator = TextGenerator::new();

        // Create a sample TOC structure with sections and chapters
        let toc_entries = vec![
            TocItem::Chapter {
                title: "Introduction".to_string(),
                href: "Text/intro.html".to_string(),
                index: 0, // No children - not a section header
            },
            TocItem::Section {
                title: "Chapter 1: Getting Started".to_string(),
                href: Some("Text/chapter1.html".to_string()),
                index: Some(1),
                children: vec![
                    // Has children - is a section header
                    TocItem::Chapter {
                        title: "1.1 Setup".to_string(),
                        href: "Text/chapter1_1.html".to_string(),
                        index: 2,
                    },
                    TocItem::Chapter {
                        title: "1.2 Configuration".to_string(),
                        href: "Text/chapter1_2.html".to_string(),
                        index: 3,
                    },
                ],
                is_expanded: true,
            },
        ];

        // Test that chapters without children are not section headers
        assert!(!generator.is_section_header("Text/intro.html", &toc_entries));
        assert!(!generator.is_section_header("Text/chapter1_1.html", &toc_entries));
        assert!(!generator.is_section_header("Text/chapter1_2.html", &toc_entries));

        // Test that chapters with children are section headers
        assert!(generator.is_section_header("Text/chapter1.html", &toc_entries));

        // Test href normalization (relative paths, OEBPS directory, and fragments)
        assert!(generator.is_section_header("../Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("./Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("OEBPS/Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("../OEBPS/Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("Text/chapter1.html#ch1", &toc_entries));
        assert!(generator.is_section_header("OEBPS/Text/chapter1.html#anchor", &toc_entries));

        // Test non-existent hrefs
        assert!(!generator.is_section_header("Text/nonexistent.html", &toc_entries));
    }

    #[test]
    fn test_duplicate_title_removal() {
        let generator = TextGenerator::new();

        // Simulate the HTML structure found in careless.epub
        // This reproduces the exact scenario where title appears twice:
        // 1. Title extraction finds "Simpleminded Hope" from h1 tag
        // 2. But after HTML processing, the h1 content still appears in text
        // 3. The duplicate removal logic fails because it tries to remove the h1 tag but content remains
        let html_content = r#"
<!DOCTYPE html>
<html>
<head>
    <title>1. Simpleminded Hope</title>
</head>
<body>
    <h1 class="CHAPTER" id="ch1">
        <a href="contents.xhtml#c_ch1"><span class="CN">1</span>
        <span class="CT">Simpleminded Hope</span></a>
    </h1>
    <p>This is the chapter content that should not be duplicated.</p>
</body>
</html>
        "#;

        let extracted_title = generator.extract_chapter_title(html_content);
        assert_eq!(extracted_title, Some("1. Simpleminded Hope".to_string()));

        let cleaned_content = generator.clean_html_content(html_content, &extracted_title);

        assert!(
            !cleaned_content.contains("Simpleminded Hope"),
            "DUPLICATE TITLE BUG: Content contains duplicate title text in: '{}'",
            cleaned_content
        );

        assert!(
            cleaned_content.contains("This is the chapter content"),
            "Content should contain the actual chapter text but found: '{}'",
            cleaned_content
        );
    }
}
