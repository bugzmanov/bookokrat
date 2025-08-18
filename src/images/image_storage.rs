use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use log::{debug, warn};
use std::collections::HashMap;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ImageStorage {
    base_dir: PathBuf,
    book_dirs: Arc<Mutex<HashMap<String, PathBuf>>>,
}

impl ImageStorage {
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_dir)
            .with_context(|| format!("Failed to create base directory: {:?}", base_dir))?;

        Ok(Self {
            base_dir,
            book_dirs: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn new_in_project_temp() -> Result<Self> {
        let base_dir = PathBuf::from("temp_images");
        Self::new(base_dir)
    }

    pub fn extract_images(&self, epub_path: &Path) -> Result<()> {
        let epub_path_str = epub_path.to_string_lossy().to_string();

        if self.book_dirs.lock().unwrap().contains_key(&epub_path_str) {
            debug!("Images already extracted for: {}", epub_path_str);
            return Ok(());
        }

        let book_name = epub_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let safe_book_name = sanitize_filename(book_name);
        let book_dir = self.base_dir.join(&safe_book_name);

        // Check if directory exists and already contains images
        if book_dir.exists() {
            let mut has_images = false;
            if let Ok(entries) = fs::read_dir(&book_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_file() {
                        if let Some(ext) = entry.path().extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if matches!(
                                ext_str.as_str(),
                                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp"
                            ) {
                                has_images = true;
                                break;
                            }
                        }
                    }
                }
            }

            // Also check subdirectories for images
            if !has_images {
                let mut images = Vec::new();
                if collect_images_recursive(&book_dir, &mut images).is_ok() && !images.is_empty() {
                    has_images = true;
                }
            }

            if has_images {
                debug!(
                    "Book directory already exists with images, skipping extraction: {:?}",
                    book_dir
                );
                self.book_dirs
                    .lock()
                    .unwrap()
                    .insert(epub_path_str, book_dir);
                return Ok(());
            }
        }

        fs::create_dir_all(&book_dir)
            .with_context(|| format!("Failed to create book directory: {:?}", book_dir))?;

        let file = fs::File::open(epub_path)
            .with_context(|| format!("Failed to open EPUB file: {:?}", epub_path))?;
        let mut doc = EpubDoc::from_reader(BufReader::new(file))
            .with_context(|| format!("Failed to parse EPUB: {:?}", epub_path))?;

        let resources = doc.resources.clone();
        let mut extracted_count = 0;

        for (id, (path, mime_type)) in resources.iter() {
            if is_image_mime_type(mime_type) {
                debug!("Found image resource: {} ({})", id, mime_type);

                if let Ok(data) = doc.get_resource(id) {
                    let image_path = book_dir.join(&path);

                    if let Some(parent) = image_path.parent() {
                        fs::create_dir_all(parent)
                            .with_context(|| format!("Failed to create directory: {:?}", parent))?;
                    }

                    fs::write(&image_path, &data)
                        .with_context(|| format!("Failed to write image: {:?}", image_path))?;

                    debug!("Extracted image: {:?}", image_path);
                    extracted_count += 1;
                } else {
                    warn!("Failed to extract resource: {}", id);
                }
            }
        }

        debug!(
            "Extracted {} images from {}",
            extracted_count, epub_path_str
        );
        self.book_dirs
            .lock()
            .unwrap()
            .insert(epub_path_str, book_dir);

        Ok(())
    }

    pub fn resolve_image_path(&self, epub_path: &Path, image_href: &str) -> Option<PathBuf> {
        let epub_path_str = epub_path.to_string_lossy().to_string();

        let book_dir = self
            .book_dirs
            .lock()
            .unwrap()
            .get(&epub_path_str)
            .cloned()?;

        // Try multiple strategies to resolve the image path
        let mut paths_to_try = Vec::new();

        // Clean the href
        let clean_href = image_href.trim_start_matches('/');

        // Strategy 1: Direct path from book root
        paths_to_try.push(book_dir.join(clean_href));

        // Strategy 2: Remove OEBPS prefix if present
        let without_oebps = clean_href.strip_prefix("OEBPS/").unwrap_or(clean_href);
        paths_to_try.push(book_dir.join(without_oebps));

        // Strategy 3: If it's a relative path with ../, resolve it from OEBPS/text or similar
        if clean_href.starts_with("../") {
            // Remove the ../ prefix
            let without_parent = clean_href.strip_prefix("../").unwrap_or(clean_href);
            // Try from OEBPS directory
            paths_to_try.push(book_dir.join("OEBPS").join(without_parent));
        }

        // Strategy 4: Try adding OEBPS prefix if not present
        if !clean_href.starts_with("OEBPS/") && !clean_href.starts_with("../") {
            paths_to_try.push(book_dir.join("OEBPS").join(clean_href));
        }

        // Try each path in order
        for path in &paths_to_try {
            if path.exists() {
                debug!("Resolved image '{}' to '{:?}'", image_href, path);
                return Some(path.clone());
            }
        }

        // If none found, log all attempts
        warn!(
            "Image not found: '{}' (tried: {:?})",
            image_href, paths_to_try
        );
        None
    }

    pub fn get_book_images(&self, epub_path: &Path) -> Option<Vec<PathBuf>> {
        let epub_path_str = epub_path.to_string_lossy().to_string();
        let book_dir = self
            .book_dirs
            .lock()
            .unwrap()
            .get(&epub_path_str)
            .cloned()?;

        let mut images = Vec::new();
        collect_images_recursive(&book_dir, &mut images).ok()?;

        Some(images)
    }

    pub fn cleanup_book(&self, epub_path: &Path) -> Result<()> {
        let epub_path_str = epub_path.to_string_lossy().to_string();

        if let Some(book_dir) = self.book_dirs.lock().unwrap().remove(&epub_path_str) {
            if book_dir.exists() {
                fs::remove_dir_all(&book_dir)
                    .with_context(|| format!("Failed to remove book directory: {:?}", book_dir))?;
                debug!("Cleaned up images for: {}", epub_path_str);
            }
        }

        Ok(())
    }

    pub fn cleanup_all(&self) -> Result<()> {
        let book_dirs: Vec<PathBuf> = self
            .book_dirs
            .lock()
            .unwrap()
            .drain()
            .map(|(_, v)| v)
            .collect();
        for book_dir in book_dirs {
            if book_dir.exists() {
                fs::remove_dir_all(&book_dir)
                    .with_context(|| format!("Failed to remove book directory: {:?}", book_dir))?;
            }
        }

        if self.base_dir.exists() {
            if fs::read_dir(&self.base_dir)?.next().is_none() {
                fs::remove_dir(&self.base_dir).with_context(|| {
                    format!("Failed to remove base directory: {:?}", self.base_dir)
                })?;
            }
        }

        Ok(())
    }
}

fn is_image_mime_type(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
        || matches!(
            mime_type,
            "application/x-png" | "application/x-jpg" | "application/x-jpeg"
        )
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

fn collect_images_recursive(dir: &Path, images: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_images_recursive(&path, images)?;
        } else if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if matches!(
                ext.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp"
            ) {
                images.push(path);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_image_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();
        assert!(temp_dir.path().exists());
        drop(storage);
    }

    #[test]
    fn test_extract_images_from_digital_frontier() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");

        storage.extract_images(epub_path).unwrap();

        let images = storage.get_book_images(epub_path).unwrap();
        assert!(!images.is_empty(), "No images were extracted");

        let expected_images = ["tech_lab.svg", "network.svg", "cover.svg"];
        for expected in &expected_images {
            let found = images.iter().any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name == *expected)
                    .unwrap_or(false)
            });
            assert!(found, "Expected image {} not found", expected);
        }
    }

    #[test]
    fn test_resolve_image_path() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");
        storage.extract_images(epub_path).unwrap();

        let test_cases = vec![
            "OEBPS/images/tech_lab.svg",
            "images/tech_lab.svg",
            "/OEBPS/images/network.svg",
            "../images/cover.svg",
        ];

        for href in test_cases {
            if let Some(resolved) = storage.resolve_image_path(epub_path, href) {
                assert!(
                    resolved.exists(),
                    "Resolved path doesn't exist: {:?}",
                    resolved
                );
            }
        }
    }

    #[test]
    fn test_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");
        storage.extract_images(epub_path).unwrap();

        let images = storage.get_book_images(epub_path).unwrap();
        assert!(!images.is_empty());

        storage.cleanup_book(epub_path).unwrap();

        assert!(storage.get_book_images(epub_path).is_none());
    }

    #[test]
    fn test_mime_type_detection() {
        assert!(is_image_mime_type("image/png"));
        assert!(is_image_mime_type("image/jpeg"));
        assert!(is_image_mime_type("image/svg+xml"));
        assert!(is_image_mime_type("application/x-png"));
        assert!(!is_image_mime_type("text/html"));
        assert!(!is_image_mime_type("application/javascript"));
    }

    #[test]
    fn test_filename_sanitization() {
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
        assert_eq!(sanitize_filename("name/with\\slashes"), "name_with_slashes");
        assert_eq!(
            sanitize_filename("name:with*special?chars"),
            "name_with_special_chars"
        );
    }

    #[test]
    fn test_skip_extraction_if_images_exist() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");

        // First extraction
        storage.extract_images(epub_path).unwrap();
        let images_first = storage.get_book_images(epub_path).unwrap();
        assert!(!images_first.is_empty());

        // Get modification time of first image
        let first_image_path = &images_first[0];
        let first_mod_time = fs::metadata(first_image_path).unwrap().modified().unwrap();

        // Small delay to ensure filesystem timestamps differ
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Create a new storage instance to simulate app restart
        let mut storage2 = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();

        // Second extraction should skip due to existing images
        storage2.extract_images(epub_path).unwrap();
        let images_second = storage2.get_book_images(epub_path).unwrap();

        // Verify same number of images
        assert_eq!(images_first.len(), images_second.len());

        // Verify the first image wasn't re-extracted (same modification time)
        let second_mod_time = fs::metadata(first_image_path).unwrap().modified().unwrap();
        assert_eq!(
            first_mod_time, second_mod_time,
            "Image was re-extracted when it shouldn't have been"
        );
    }

    #[test]
    fn test_extract_after_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();

        let epub_path = Path::new("tests/testdata/digital_frontier.epub");

        // First extraction
        storage.extract_images(epub_path).unwrap();
        let images_first = storage.get_book_images(epub_path).unwrap();
        assert_eq!(images_first.len(), 3);

        // Clean up
        storage.cleanup_book(epub_path).unwrap();
        assert!(storage.get_book_images(epub_path).is_none());

        // Extract again - should work since directory was cleaned
        storage.extract_images(epub_path).unwrap();
        let images_second = storage.get_book_images(epub_path).unwrap();
        assert_eq!(images_second.len(), 3);
    }

    #[test]
    fn test_skip_extraction_with_existing_directory_structure() {
        let temp_dir = TempDir::new().unwrap();

        // Manually create the expected directory structure with an image
        let book_dir = temp_dir
            .path()
            .join("digital_frontier")
            .join("OEBPS")
            .join("images");
        fs::create_dir_all(&book_dir).unwrap();

        // Create a dummy image file
        let dummy_image = book_dir.join("test.svg");
        fs::write(&dummy_image, "<svg></svg>").unwrap();

        // Now create storage and try to extract
        let mut storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let epub_path = Path::new("tests/testdata/digital_frontier.epub");

        // Record the modification time of our dummy image
        let original_mod_time = fs::metadata(&dummy_image).unwrap().modified().unwrap();

        // Extract should skip because images already exist
        storage.extract_images(epub_path).unwrap();

        // Verify our dummy image wasn't touched
        assert!(dummy_image.exists());
        let new_mod_time = fs::metadata(&dummy_image).unwrap().modified().unwrap();
        assert_eq!(original_mod_time, new_mod_time);

        // Storage should still report the book as available
        assert!(storage.get_book_images(epub_path).is_some());
    }

    #[test]
    fn test_extract_with_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Create empty directory structure
        let book_dir = temp_dir.path().join("digital_frontier");
        fs::create_dir_all(&book_dir).unwrap();

        // Create storage and extract - should proceed with extraction
        let mut storage = ImageStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let epub_path = Path::new("tests/testdata/digital_frontier.epub");

        storage.extract_images(epub_path).unwrap();

        // Verify images were extracted
        let images = storage.get_book_images(epub_path).unwrap();
        assert_eq!(
            images.len(),
            3,
            "Should extract all 3 images even with existing empty directory"
        );
    }
}
