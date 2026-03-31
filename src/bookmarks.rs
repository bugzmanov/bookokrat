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

#[derive(Debug, Serialize, Deserialize, Clone)]
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
        if let Some(key) = Self::candidate_keys(path)
            .into_iter()
            .find(|candidate| self.books.contains_key(candidate))
        {
            return Some(key);
        }
        // Also match by absolute_path field for cross-library lookups
        self.books
            .iter()
            .find(|(_, b)| b.absolute_path.as_deref() == Some(path))
            .map(|(k, _)| k.clone())
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
        if let Some(key) = self.resolve_existing_key(path) {
            return self.books.get(&key);
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

    #[allow(clippy::too_many_arguments)]
    pub fn save_initial_bookmark(
        &mut self,
        path: &str,
        chapter_href: String,
        chapter_index: Option<usize>,
        total_chapters: Option<usize>,
        pdf_page: Option<usize>,
        book_title: Option<String>,
        book_author: Option<String>,
        absolute_path: Option<String>,
    ) {
        let key = self
            .resolve_existing_key(path)
            .unwrap_or_else(|| path.to_string());

        if let Some(bookmark) = self.books.get_mut(&key) {
            bookmark.book_title = book_title;
            bookmark.book_author = book_author;
            bookmark.absolute_path = absolute_path;
            bookmark.last_read = chrono::Utc::now();
            if total_chapters.is_some() {
                bookmark.total_chapters = total_chapters;
            }
        } else {
            self.books.insert(
                key,
                Bookmark {
                    chapter_href,
                    node_index: None,
                    last_read: chrono::Utc::now(),
                    chapter_index,
                    total_chapters,
                    pdf_page,
                    pdf_zoom: None,
                    pdf_pan: None,
                    #[cfg(feature = "pdf")]
                    pdf_invert_images: None,
                    #[cfg(feature = "pdf")]
                    pdf_themed_rendering: None,
                    book_progress: None,
                    total_nodes: None,
                    book_title,
                    book_author,
                    absolute_path,
                },
            );
        }

        if let Err(e) = self.save() {
            log::error!("Failed to save initial bookmark: {e}");
        }
    }

    pub fn set_metadata(
        &mut self,
        path: &str,
        title: Option<String>,
        author: Option<String>,
        abs_path: Option<String>,
    ) {
        let key = match self.resolve_existing_key(path) {
            Some(k) => k,
            None => return,
        };
        if let Some(bookmark) = self.books.get_mut(&key) {
            bookmark.book_title = title;
            bookmark.book_author = author;
            bookmark.absolute_path = abs_path;
            if let Err(e) = self.save() {
                log::error!("Failed to save bookmark metadata: {e}");
            }
        }
    }

    pub fn remove_bookmark(&mut self, path: &str) -> bool {
        let key = match self.resolve_existing_key(path) {
            Some(k) => k,
            None => return false,
        };
        if self.books.remove(&key).is_some() {
            if let Err(e) = self.save() {
                log::error!("Failed to save after removing bookmark: {e}");
            }
            true
        } else {
            false
        }
    }

    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Bookmark)> {
        self.books.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_initial_bookmark_creates_entry_with_metadata() {
        let mut bookmarks = Bookmarks::ephemeral();
        let path = "./book.pdf";

        // save_initial_bookmark creates the entry with metadata in one shot
        bookmarks.save_initial_bookmark(
            path,
            "0".to_string(),
            None,
            Some(100),
            Some(0),
            Some("My Book".to_string()),
            Some("Author".to_string()),
            Some("/abs/path/book.pdf".to_string()),
        );

        let bookmark = bookmarks.get_bookmark(path).unwrap();
        assert_eq!(bookmark.book_title.as_deref(), Some("My Book"));
        assert_eq!(bookmark.book_author.as_deref(), Some("Author"));
        assert_eq!(
            bookmark.absolute_path.as_deref(),
            Some("/abs/path/book.pdf")
        );

        // Subsequent update_bookmark preserves metadata
        bookmarks.update_bookmark(
            path,
            "5".to_string(),
            Some(10),
            Some(4),
            Some(100),
            Some(4),
            None,
            None,
            Some(0.05),
            None,
        );

        let bookmark = bookmarks.get_bookmark(path).unwrap();
        assert_eq!(bookmark.book_title.as_deref(), Some("My Book"));
        assert_eq!(
            bookmark.absolute_path.as_deref(),
            Some("/abs/path/book.pdf")
        );
        assert_eq!(bookmark.chapter_index, Some(4));
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

    #[test]
    fn save_initial_bookmark_refreshes_last_read_for_existing_entry() {
        let mut bookmarks = Bookmarks::ephemeral();
        let path = "./book.epub";

        bookmarks.save_initial_bookmark(
            path,
            "ch1".to_string(),
            Some(0),
            Some(10),
            None,
            Some("Title".to_string()),
            None,
            Some("/abs/book.epub".to_string()),
        );

        let first_last_read = bookmarks.get_bookmark(path).unwrap().last_read;
        let refreshed_last_read = loop {
            bookmarks.save_initial_bookmark(
                path,
                "ch2".to_string(),
                Some(1),
                Some(10),
                None,
                Some("Updated Title".to_string()),
                None,
                Some("/abs/book.epub".to_string()),
            );

            let bookmark = bookmarks.get_bookmark(path).unwrap();
            if bookmark.last_read > first_last_read {
                break bookmark.last_read;
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
        };

        let bookmark = bookmarks.get_bookmark(path).unwrap();
        assert!(refreshed_last_read > first_last_read);
        assert_eq!(bookmark.book_title.as_deref(), Some("Updated Title"));
        assert_eq!(bookmark.chapter_index, Some(0));
    }

    /// Opening a book from "All Libraries" must NOT create a bookmark entry
    /// in the current library. This simulates the exact main_app.rs flow:
    /// open_book_for_reading_by_path calls set_metadata which creates an entry.
    /// If bookmarks haven't been switched, this entry goes to the current library.
    #[test]
    fn opening_cross_library_book_must_not_leak_into_current() {
        let dir = tempfile::TempDir::new().unwrap();
        let current_path = dir.path().join("current.json");
        let other_path = dir.path().join("other.json");

        // Current library: user has book_a
        let mut current = Bookmarks::with_file(current_path.to_str().unwrap());
        current.update_bookmark(
            "./book_a.epub",
            "ch1".into(),
            Some(0),
            Some(0),
            Some(10),
            None,
            None,
            None,
            Some(0.5),
            None,
        );

        // Other library has book_b
        let mut other = Bookmarks::with_file(other_path.to_str().unwrap());
        other.update_bookmark(
            "./book_b.pdf",
            "1".into(),
            Some(0),
            Some(0),
            Some(100),
            Some(0),
            None,
            None,
            Some(0.1),
            None,
        );

        // BUG SCENARIO: if we DON'T switch bookmarks and just open the book,
        // set_metadata creates book_b entry in current library
        current.set_metadata(
            "/other_lib/book_b.pdf",
            Some("Book B".into()),
            Some("Author B".into()),
            Some("/other_lib/book_b.pdf".into()),
        );

        // This is the bug: book_b now exists in current library
        let reloaded = Bookmarks::load_from_file(current_path.to_str().unwrap()).unwrap();
        assert!(
            reloaded.get_bookmark("/other_lib/book_b.pdf").is_none(),
            "book_b must NOT appear in current library's bookmarks"
        );
    }

    /// Simulates the full flow when opening a book from "All Libraries":
    /// The bug was: after switching bookmarks, open_book_for_reading_by_path
    /// calls save_bookmark_with_throttle which would write the OLD book (book_a)
    /// into the OTHER library's bookmarks file. The fix clears current_book
    /// before switching so save_bookmark is a no-op.
    #[test]
    fn cross_library_open_does_not_pollute_either_library() {
        let dir = tempfile::TempDir::new().unwrap();

        let current_lib_path = dir.path().join("current_bookmarks.json");
        let other_lib_path = dir.path().join("other_bookmarks.json");

        // Setup: current library has book_a
        let mut current_bookmarks = Bookmarks::with_file(current_lib_path.to_str().unwrap());
        current_bookmarks.update_bookmark(
            "./book_a.epub",
            "ch1".to_string(),
            Some(0),
            Some(0),
            Some(10),
            None,
            None,
            None,
            Some(0.5),
            None,
        );

        // Setup: other library has book_b
        let mut other_bookmarks = Bookmarks::with_file(other_lib_path.to_str().unwrap());
        other_bookmarks.update_bookmark(
            "./book_b.epub",
            "ch3".to_string(),
            Some(10),
            Some(2),
            Some(20),
            None,
            None,
            None,
            Some(0.3),
            None,
        );

        // --- Simulate the OpenBookAbsolute handler flow ---

        // Step 1: save_bookmark_with_throttle saves book_a to current library
        current_bookmarks.update_bookmark(
            "./book_a.epub",
            "ch2".to_string(),
            Some(5),
            Some(1),
            Some(10),
            None,
            None,
            None,
            Some(0.6),
            None,
        );

        // Step 2: clear current_book (simulated by not having book_a state anymore)
        // Step 3: switch_bookmarks_file loads other library
        let mut active_bookmarks =
            Bookmarks::load_from_file(other_lib_path.to_str().unwrap()).unwrap();

        // Step 4: open_book_for_reading_by_path calls save_bookmark_with_throttle
        // With book state cleared, this is a no-op. If NOT cleared, it would do:
        //   active_bookmarks.update_bookmark("./book_a.epub", ...) <-- THE BUG
        // We verify the no-op by NOT calling update_bookmark for book_a here.

        // Step 5: set_metadata + reading updates for book_b
        active_bookmarks.set_metadata(
            "./book_b.epub",
            Some("Book B".to_string()),
            None,
            Some("/lib2/book_b.epub".to_string()),
        );
        active_bookmarks.update_bookmark(
            "./book_b.epub",
            "ch5".to_string(),
            Some(20),
            Some(4),
            Some(20),
            None,
            None,
            None,
            Some(0.5),
            None,
        );

        // --- Verify no cross-contamination ---

        // book_a must NOT be in other library
        assert!(
            active_bookmarks.get_bookmark("./book_a.epub").is_none(),
            "book_a should NOT leak into other library's bookmarks"
        );

        // book_b must NOT be in current library
        let reloaded_current =
            Bookmarks::load_from_file(current_lib_path.to_str().unwrap()).unwrap();
        assert!(
            reloaded_current.get_bookmark("./book_b.epub").is_none(),
            "book_b should NOT exist in current library's bookmarks"
        );

        // book_a still in current library with updated progress
        let book_a = reloaded_current.get_bookmark("./book_a.epub").unwrap();
        assert_eq!(book_a.chapter_index, Some(1));
        assert_eq!(book_a.book_progress, Some(0.6));

        // book_b updated correctly in other library
        let book_b = active_bookmarks.get_bookmark("./book_b.epub").unwrap();
        assert_eq!(book_b.chapter_index, Some(4));
        assert_eq!(book_b.book_progress, Some(0.5));
    }
}
