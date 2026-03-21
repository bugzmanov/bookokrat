use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Bookmark {
    pub chapter_href: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_index: Option<usize>,

    pub last_read: chrono::DateTime<chrono::Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter_index: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_chapters: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_page: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_zoom: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pdf_pan: Option<u16>,

    #[cfg(feature = "pdf")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pdf_invert_images: Option<bool>,

    #[cfg(feature = "pdf")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pdf_themed_rendering: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub book_progress: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_nodes: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub book_title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub book_author: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub absolute_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bookmarks {
    books: HashMap<String, Bookmark>,

    #[serde(skip)]
    file_path: Option<String>,
}

impl Bookmarks {
    fn normalize_key(path: &str) -> String {
        let mut cleaned = std::path::PathBuf::new();
        for component in Path::new(path).components() {
            match component {
                std::path::Component::CurDir => {}
                _ => cleaned.push(component.as_os_str()),
            }
        }
        cleaned.to_string_lossy().to_string()
    }

    fn candidate_keys(path: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        candidates.push(path.to_string());

        let normalized = Self::normalize_key(path);
        if normalized != path {
            candidates.push(normalized.clone());
        }

        if !normalized.starts_with("./") {
            candidates.push(format!("./{normalized}"));
        }

        if let Ok(cwd) = std::env::current_dir() {
            let path_buf = Path::new(&normalized);
            if path_buf.is_absolute() {
                if let Ok(rel) = path_buf.strip_prefix(&cwd) {
                    let rel_str = rel.to_string_lossy().to_string();
                    candidates.push(rel_str.clone());
                    if !rel_str.starts_with("./") {
                        candidates.push(format!("./{rel_str}"));
                    }
                }
            } else {
                let abs = cwd.join(path_buf).to_string_lossy().to_string();
                candidates.push(abs);
            }
        }

        candidates
    }

    fn resolve_existing_key(&self, path: &str) -> Option<String> {
        Self::candidate_keys(path)
            .into_iter()
            .find(|candidate| self.books.contains_key(candidate))
    }

    pub fn ephemeral() -> Self {
        Self {
            books: HashMap::new(),
            file_path: None,
        }
    }

    pub fn with_file(file_path: &str) -> Self {
        Self {
            books: HashMap::new(),
            file_path: Some(file_path.to_string()),
        }
    }

    pub fn load_or_ephemeral(file_path: Option<&str>) -> Self {
        match file_path {
            Some(path) => Self::load_from_file(path).unwrap_or_else(|e| {
                log::error!("Failed to load bookmarks from {path}: {e}");
                Self::with_file(path)
            }),
            None => Self::ephemeral(),
        }
    }

    pub fn load_from_file(file_path: &str) -> anyhow::Result<Self> {
        let path = Path::new(file_path);
        if path.exists() {
            let content = fs::read_to_string(path)?;

            match serde_json::from_str::<Self>(&content) {
                Ok(mut bookmarks) => {
                    bookmarks.file_path = Some(file_path.to_string());
                    Ok(bookmarks)
                }
                Err(e) => {
                    log::error!("Failed to parse bookmarks file: {e}");
                    Err(anyhow::anyhow!("Failed to parse bookmarks: {}", e))
                }
            }
        } else {
            Ok(Self::with_file(file_path))
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        match &self.file_path {
            Some(path) => {
                if let Some(parent) = Path::new(path).parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)?;
                    }
                }
                let content = serde_json::to_string_pretty(self)?;
                fs::write(path, content)?;
                Ok(())
            }
            None => Ok(()),
        }
    }

    pub fn get_bookmark(&self, path: &str) -> Option<&Bookmark> {
        for candidate in Self::candidate_keys(path) {
            if let Some(bookmark) = self.books.get(&candidate) {
                return Some(bookmark);
            }
        }
        None
    }

    pub fn get_most_recent(&self) -> Option<(String, &Bookmark)> {
        self.books
            .iter()
            .max_by_key(|(_, bookmark)| &bookmark.last_read)
            .map(|(path, bookmark)| (path.clone(), bookmark))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_bookmark(
        &mut self,
        path: &str,
        chapter_href: String,
        node_index: Option<usize>,
        chapter_index: Option<usize>,
        total_chapters: Option<usize>,
        pdf_page: Option<usize>,
        pdf_zoom: Option<f32>,
        pdf_pan: Option<u16>,
        book_progress: Option<f32>,
        total_nodes: Option<usize>,
    ) {
        self.update_bookmark_internal(
            path,
            chapter_href,
            node_index,
            chapter_index,
            total_chapters,
            pdf_page,
            pdf_zoom,
            pdf_pan,
            None,
            None,
            book_progress,
            total_nodes,
        );
    }

    #[cfg(feature = "pdf")]
    #[allow(clippy::too_many_arguments)]
    pub fn update_bookmark_pdf(
        &mut self,
        path: &str,
        chapter_href: String,
        node_index: Option<usize>,
        chapter_index: Option<usize>,
        total_chapters: Option<usize>,
        pdf_page: Option<usize>,
        pdf_zoom: Option<f32>,
        pdf_pan: Option<u16>,
        pdf_invert_images: Option<bool>,
        pdf_themed_rendering: Option<bool>,
        book_progress: Option<f32>,
    ) {
        self.update_bookmark_internal(
            path,
            chapter_href,
            node_index,
            chapter_index,
            total_chapters,
            pdf_page,
            pdf_zoom,
            pdf_pan,
            pdf_invert_images,
            pdf_themed_rendering,
            book_progress,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn update_bookmark_internal(
        &mut self,
        path: &str,
        chapter_href: String,
        node_index: Option<usize>,
        chapter_index: Option<usize>,
        total_chapters: Option<usize>,
        pdf_page: Option<usize>,
        pdf_zoom: Option<f32>,
        pdf_pan: Option<u16>,
        #[cfg(feature = "pdf")] pdf_invert_images: Option<bool>,
        #[cfg(feature = "pdf")] pdf_themed_rendering: Option<bool>,
        #[cfg(not(feature = "pdf"))] _pdf_invert_images: Option<bool>,
        #[cfg(not(feature = "pdf"))] _pdf_themed_rendering: Option<bool>,
        book_progress: Option<f32>,
        total_nodes: Option<usize>,
    ) {
        let key = self
            .resolve_existing_key(path)
            .unwrap_or_else(|| path.to_string());

        let existing = self.books.get(&key);
        let book_title = existing.and_then(|b| b.book_title.clone());
        let book_author = existing.and_then(|b| b.book_author.clone());
        let absolute_path = existing.and_then(|b| b.absolute_path.clone());

        self.books.insert(
            key,
            Bookmark {
                chapter_href,
                node_index,
                last_read: chrono::Utc::now(),
                chapter_index,
                total_chapters,
                pdf_page,
                pdf_zoom,
                pdf_pan,
                #[cfg(feature = "pdf")]
                pdf_invert_images,
                #[cfg(feature = "pdf")]
                pdf_themed_rendering,
                book_progress,
                total_nodes,
                book_title,
                book_author,
                absolute_path,
            },
        );

        if !self.books.is_empty() && self.file_path.is_some() {
            if let Err(e) = self.save() {
                log::error!("Failed to save bookmark: {e}");
            }
        }
    }

    pub fn set_metadata(
        &mut self,
        path: &str,
        title: Option<String>,
        author: Option<String>,
        abs_path: Option<String>,
    ) {
        let key = self
            .resolve_existing_key(path)
            .unwrap_or_else(|| path.to_string());
        match self.books.get_mut(&key) {
            Some(bookmark) => {
                bookmark.book_title = title;
                bookmark.book_author = author;
                bookmark.absolute_path = abs_path;
            }
            None => {
                self.books.insert(
                    key,
                    Bookmark {
                        chapter_href: String::new(),
                        node_index: None,
                        last_read: chrono::Utc::now(),
                        chapter_index: None,
                        total_chapters: None,
                        pdf_page: None,
                        pdf_zoom: None,
                        pdf_pan: None,
                        #[cfg(feature = "pdf")]
                        pdf_invert_images: None,
                        #[cfg(feature = "pdf")]
                        pdf_themed_rendering: None,
                        book_progress: None,
                        total_nodes: None,
                        book_title: title,
                        book_author: author,
                        absolute_path: abs_path,
                    },
                );
            }
        }
        if let Err(e) = self.save() {
            log::error!("Failed to save bookmark metadata: {e}");
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Bookmark)> {
        self.books.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_metadata_before_bookmark_exists() {
        let mut bookmarks = Bookmarks::ephemeral();
        let path = "./book.pdf";

        // This is what happens in open_pdf: set_metadata is called before save_bookmark
        bookmarks.set_metadata(
            path,
            Some("My Book".to_string()),
            Some("Author".to_string()),
            Some("/abs/path/book.pdf".to_string()),
        );

        // Then save_bookmark is called later on scroll/navigate
        bookmarks.update_bookmark(
            path,
            "1".to_string(),
            Some(0),
            Some(0),
            Some(100),
            Some(0),
            None,
            None,
            Some(0.05),
            None,
        );

        let bookmark = bookmarks.get_bookmark(path).unwrap();
        assert_eq!(bookmark.book_title.as_deref(), Some("My Book"));
        assert_eq!(bookmark.book_author.as_deref(), Some("Author"));
        assert_eq!(
            bookmark.absolute_path.as_deref(),
            Some("/abs/path/book.pdf")
        );
    }

    #[test]
    fn set_metadata_after_bookmark_exists() {
        let mut bookmarks = Bookmarks::ephemeral();
        let path = "./book.epub";

        // Bookmark saved first
        bookmarks.update_bookmark(
            path,
            "ch1".to_string(),
            Some(0),
            Some(0),
            Some(10),
            None,
            None,
            None,
            Some(0.1),
            None,
        );

        // Then metadata set
        bookmarks.set_metadata(
            path,
            Some("Title".to_string()),
            Some("Author".to_string()),
            Some("/abs/book.epub".to_string()),
        );

        let bookmark = bookmarks.get_bookmark(path).unwrap();
        assert_eq!(bookmark.book_title.as_deref(), Some("Title"));
        assert_eq!(bookmark.absolute_path.as_deref(), Some("/abs/book.epub"));
    }
}
