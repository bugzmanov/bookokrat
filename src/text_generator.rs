use regex::Regex;
use epub::doc::EpubDoc;
use std::io::BufReader;
use log::{debug, warn};

pub struct TextGenerator {
    p_tag_re: Regex,
    h_open_re: Regex,
    h_close_re: Regex,
    remaining_tags_re: Regex,
    multi_space_re: Regex,
    multi_newline_re: Regex,
    leading_space_re: Regex,
    line_leading_space_re: Regex,
}

impl TextGenerator {
    pub fn new() -> Self {
        Self {
            p_tag_re: Regex::new(r"<p[^>]*>").expect("Failed to compile paragraph tag regex"),
            h_open_re: Regex::new(r"<h[1-6][^>]*>").expect("Failed to compile header open tag regex"),
            h_close_re: Regex::new(r"</h[1-6]>").expect("Failed to compile header close tag regex"),
            remaining_tags_re: Regex::new(r"<[^>]*>").expect("Failed to compile remaining tags regex"),
            multi_space_re: Regex::new(r" +").expect("Failed to compile multi space regex"),
            multi_newline_re: Regex::new(r"\n{3,}").expect("Failed to compile multi newline regex"),
            leading_space_re: Regex::new(r"^ +").expect("Failed to compile leading space regex"),
            line_leading_space_re: Regex::new(r"\n +").expect("Failed to compile line leading space regex"),
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

    pub fn process_chapter_content(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Result<(String, Option<String>), String> {
        let content = doc.get_current_str().map_err(|e| format!("Failed to get chapter content: {}", e))?;
        debug!("Raw content length: {} bytes", content.len());
        
        let chapter_title = self.extract_chapter_title(&content);
        let processed_text = self.clean_html_content(&content, &chapter_title);
        
        if processed_text.is_empty() {
            warn!("Converted text is empty");
            Ok(("No content available in this chapter.".to_string(), chapter_title))
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
        
        if let Some(ref title) = chapter_title {
            let title_removal_re = Regex::new(&format!(r"<h[12][^>]*>\s*{}\s*</h[12]>", regex::escape(title))).unwrap();
            content = title_removal_re.replace_all(&content, "").into_owned();
        }
        
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
        let text = Regex::new(r"\n\s*\n").unwrap().replace_all(&text, "\n\n").to_string();
        let text = self.multi_newline_re.replace_all(&text, "\n\n").to_string();
        let text = self.leading_space_re.replace_all(&text, "").to_string();
        let text = self.line_leading_space_re.replace_all(&text, "\n").to_string();
        
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
            
            let trimmed = paragraph.trim();
            let is_header = trimmed.len() > 0 && trimmed.len() < 60 && 
                           (trimmed.chars().filter(|c| c.is_alphabetic()).count() > 0) &&
                           (trimmed.chars().filter(|c| c.is_uppercase()).count() as f32 / 
                            trimmed.chars().filter(|c| c.is_alphabetic()).count() as f32 > 0.7);
            
            if !is_header && !trimmed.starts_with("    ") {
                formatted.push_str("    ");
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