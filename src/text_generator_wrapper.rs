use crate::table_of_contents::TocItem;
use epub::doc::EpubDoc;
use std::io::BufReader;

// Simple enum to choose between implementations
pub enum TextGeneratorImpl {
    Regex(crate::text_generator::TextGenerator),
    Html5ever(crate::html5ever_text_generator::TextGenerator),
}

pub struct TextGeneratorWrapper {
    implementation: TextGeneratorImpl,
}

impl TextGeneratorWrapper {
    pub fn new_regex() -> Self {
        Self {
            implementation: TextGeneratorImpl::Regex(crate::text_generator::TextGenerator::new()),
        }
    }

    pub fn new_html5ever() -> Self {
        Self {
            implementation: TextGeneratorImpl::Html5ever(
                crate::html5ever_text_generator::TextGenerator::new(),
            ),
        }
    }

    pub fn new_default() -> Self {
        // For now, default to HTML5ever implementation for testing
        // Change this line to switch between implementations easily
        Self::new_html5ever()
        // Self::new_regex() // Uncomment this and comment above to use regex implementation
    }

    pub fn extract_chapter_title(&self, html_content: &str) -> Option<String> {
        match &self.implementation {
            TextGeneratorImpl::Regex(generator) => generator.extract_chapter_title(html_content),
            TextGeneratorImpl::Html5ever(generator) => {
                generator.extract_chapter_title(html_content)
            }
        }
    }

    pub fn normalize_href(&self, href: &str) -> String {
        match &self.implementation {
            TextGeneratorImpl::Regex(generator) => generator.normalize_href(href),
            TextGeneratorImpl::Html5ever(generator) => generator.normalize_href(href),
        }
    }

    pub fn parse_toc_structure(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Vec<TocItem> {
        match &self.implementation {
            TextGeneratorImpl::Regex(generator) => generator.parse_toc_structure(doc),
            TextGeneratorImpl::Html5ever(generator) => generator.parse_toc_structure(doc),
        }
    }

    pub fn process_chapter_content(
        &self,
        doc: &mut EpubDoc<BufReader<std::fs::File>>,
    ) -> Result<(String, Option<String>), String> {
        match &self.implementation {
            TextGeneratorImpl::Regex(generator) => generator.process_chapter_content(doc),
            TextGeneratorImpl::Html5ever(generator) => generator.process_chapter_content(doc),
        }
    }

    pub fn get_implementation_name(&self) -> &'static str {
        match &self.implementation {
            TextGeneratorImpl::Regex(_) => "Regex-based",
            TextGeneratorImpl::Html5ever(_) => "HTML5ever-based",
        }
    }
}

impl Default for TextGeneratorWrapper {
    fn default() -> Self {
        Self::new_default()
    }
}
