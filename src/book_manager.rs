use epub::doc::EpubDoc;
use log::{error, info};
use std::io::BufReader;
use std::path::Path;

pub struct BookManager {
    pub books: Vec<BookInfo>,
    scan_directory: String,
}

#[derive(Clone)]
pub struct BookInfo {
    pub path: String,
    pub display_name: String,
}

impl BookManager {
    pub fn new() -> Self {
        Self::new_with_directory(".")
    }

    pub fn new_with_directory(directory: &str) -> Self {
        let scan_directory = directory.to_string();
        let mut books = Self::discover_books_in_dir(&scan_directory);
        books.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        Self {
            books,
            scan_directory,
        }
    }

    fn discover_books_in_dir(dir: &str) -> Vec<BookInfo> {
        std::fs::read_dir(dir)
            .unwrap_or_else(|e| {
                error!("Failed to read directory {}: {}", dir, e);
                panic!("Failed to read directory {}: {}", dir, e);
            })
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension()?.to_str()? == "epub" {
                    let path_str = path.to_str()?.to_string();
                    let display_name = Self::extract_display_name(&path_str);
                    Some(BookInfo {
                        path: path_str,
                        display_name,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn extract_display_name(file_path: &str) -> String {
        Path::new(file_path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }

    pub fn get_book_paths(&self) -> Vec<String> {
        self.books.iter().map(|book| book.path.clone()).collect()
    }

    pub fn get_book_info(&self, index: usize) -> Option<&BookInfo> {
        self.books.get(index)
    }

    pub fn find_book_by_path(&self, path: &str) -> Option<usize> {
        self.books.iter().position(|book| book.path == path)
    }

    pub fn load_epub(&self, path: &str) -> Result<EpubDoc<BufReader<std::fs::File>>, String> {
        info!("Loading EPUB from path: {}", path);

        // Verify the book exists in our managed list
        if !self.books.iter().any(|book| book.path == path) {
            return Err(format!("Book not found in managed list: {}", path));
        }

        match EpubDoc::new(path) {
            Ok(doc) => {
                info!("Successfully loaded EPUB: {}", path);
                Ok(doc)
            }
            Err(e) => {
                error!("Failed to load EPUB {}: {}", path, e);
                Err(format!("Failed to load EPUB: {}", e))
            }
        }
    }

    pub fn refresh_books(&mut self) {
        info!("Refreshing book list");
        self.books = Self::discover_books_in_dir(&self.scan_directory);
    }

    pub fn book_count(&self) -> usize {
        self.books.len()
    }

    pub fn find_book_index_by_path(&self, path: &str) -> Option<usize> {
        self.books.iter().position(|book| book.path == path)
    }

    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    pub fn contains_book(&self, path: &str) -> bool {
        self.books.iter().any(|book| book.path == path)
    }
}
