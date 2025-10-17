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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bookmarks {
    books: HashMap<String, Bookmark>,

    #[serde(skip)]
    file_path: Option<String>,
}

impl Bookmarks {
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
                let content = serde_json::to_string_pretty(self)?;
                fs::write(path, content)?;
                Ok(())
            }
            None => Ok(()),
        }
    }

    pub fn get_bookmark(&self, path: &str) -> Option<&Bookmark> {
        self.books.get(path)
    }

    pub fn get_most_recent(&self) -> Option<(String, &Bookmark)> {
        self.books
            .iter()
            .max_by_key(|(_, bookmark)| &bookmark.last_read)
            .map(|(path, bookmark)| (path.clone(), bookmark))
    }

    pub fn update_bookmark(
        &mut self,
        path: &str,
        chapter_href: String,
        node_index: Option<usize>,
        chapter_index: Option<usize>,
        total_chapters: Option<usize>,
    ) {
        self.books.insert(
            path.to_string(),
            Bookmark {
                chapter_href,
                node_index,
                last_read: chrono::Utc::now(),
                chapter_index,
                total_chapters,
            },
        );

        if !self.books.is_empty() && self.file_path.is_some() {
            if let Err(e) = self.save() {
                log::error!("Failed to save bookmark: {e}");
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Bookmark)> {
        self.books.iter()
    }
}
