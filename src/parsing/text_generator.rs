use crate::parsing::html_to_markdown::extract_chapter_title as extract_parser_chapter_title;

pub struct TextGenerator {}

impl TextGenerator {
    pub fn extract_chapter_title(html_content: &str) -> Option<String> {
        extract_parser_chapter_title(html_content)
    }
}
