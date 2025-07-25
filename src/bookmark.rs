use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub chapter: usize,
    pub scroll_offset: usize,
    pub last_read: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bookmarks {
    books: HashMap<String, Bookmark>,
}

impl Bookmarks {
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
        }
    }

    pub fn load() -> anyhow::Result<Self> {
        Self::load_from_file("bookmarks.json")
    }

    pub fn load_from_file(file_path: &str) -> anyhow::Result<Self> {
        let path = Path::new(file_path);
        if path.exists() {
            let content = fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::new())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write("bookmarks.json", content)?;
        Ok(())
    }

    pub fn get_bookmark(&self, path: &str) -> Option<&Bookmark> {
        self.books.get(path)
    }

    pub fn get_most_recent(&self) -> Option<(String, &Bookmark)> {
        self.books
            .iter()
            .max_by_key(|(_, bookmark)| bookmark.last_read)
            .map(|(path, bookmark)| (path.clone(), bookmark))
    }

    pub fn update_bookmark(&mut self, path: &str, chapter: usize, scroll_offset: usize) {
        self.books.insert(
            path.to_string(),
            Bookmark {
                chapter,
                scroll_offset,
                last_read: chrono::Utc::now(),
            },
        );
        // Only try to save if we have at least one bookmark
        if !self.books.is_empty() {
            if let Err(e) = self.save() {
                log::error!("Failed to save bookmark: {}", e);
            }
        }
    }
} 