#[derive(Debug, Clone)]
pub struct LinkInfo {
    pub text: String,
    pub url: String,
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub link_type: crate::markdown::LinkType,
    pub target_chapter: Option<String>,
    pub target_anchor: Option<String>,
}

impl LinkInfo {
    pub fn from_url(url: String) -> Self {
        let (link_type, target_chapter, target_anchor) = crate::markdown::classify_link_href(&url);

        Self {
            text: url.clone(),
            url,
            line: 0, // Not needed for navigation
            start_col: 0,
            end_col: 0,
            link_type,
            target_chapter,
            target_anchor,
        }
    }
}
