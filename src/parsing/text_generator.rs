use regex::Regex;

pub struct TextGenerator {}

//todo: this needs to be removed
impl TextGenerator {
    pub fn extract_chapter_title(html_content: &str) -> Option<String> {
        let h1_pattern = Regex::new(r"(?s)<h1[^>]*>(.*?)</h1>").ok()?;
        let h2_pattern = Regex::new(r"(?s)<h2[^>]*>(.*?)</h2>").ok()?;
        let h3_pattern = Regex::new(r"(?s)<h3[^>]*>(.*?)</h3>").ok()?;
        let title_pattern = Regex::new(r"(?s)<title[^>]*>(.*?)</title>").ok()?;

        for re in [h1_pattern, h2_pattern, h3_pattern, title_pattern] {
            if let Some(captures) = re.captures(html_content) {
                if let Some(title_match) = captures.get(1) {
                    let title = Self::extract_text_from_html(title_match.as_str());
                    if !title.is_empty() && title.len() < 100 {
                        return Some(title);
                    }
                }
            }
        }

        None
    }

    /// Helper function to extract plain text from HTML, removing tags but keeping content
    fn extract_text_from_html(html: &str) -> String {
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let text = tag_re.replace_all(html, " ");

        let whitespace_re = Regex::new(r"\s+").unwrap();
        let cleaned = whitespace_re.replace_all(&text, " ");

        cleaned.trim().to_string()
    }
}
