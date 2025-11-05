use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use log::{debug, info, warn};
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
            .with_context(|| format!("Failed to create base directory: {base_dir:?}"))?;

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
        info!("Starting image extraction for: {epub_path_str}");

        if self.book_dirs.lock().unwrap().contains_key(&epub_path_str) {
            info!("Images already extracted for this book");
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
                info!("Found existing images in directory: {book_dir:?}");
                self.book_dirs
                    .lock()
                    .unwrap()
                    .insert(epub_path_str, book_dir);
                return Ok(());
            }
        }

        fs::create_dir_all(&book_dir)
            .with_context(|| format!("Failed to create book directory: {book_dir:?}"))?;

        let file = fs::File::open(epub_path)
            .with_context(|| format!("Failed to open EPUB file: {epub_path:?}"))?;
        let mut doc = EpubDoc::from_reader(BufReader::new(file))
            .with_context(|| format!("Failed to parse EPUB: {epub_path:?}"))?;

        let resources = doc.resources.clone();
        info!("Found {} resources in EPUB", resources.len());

        let mut image_count = 0;
        for (id, resource) in resources.iter() {
            if is_image_mime_type(&resource.mime) {
                image_count += 1;
                debug!(
                    "Extracting image {id}: {path:?} ({mime})",
                    path = resource.path,
                    mime = resource.mime
                );
                if let Some((data, _mime)) = doc.get_resource(id) {
                    let image_path = book_dir.join(&resource.path);

                    if let Some(parent) = image_path.parent() {
                        fs::create_dir_all(parent)
                            .with_context(|| format!("Failed to create directory: {parent:?}"))?;
                    }

                    fs::write(&image_path, &data)
                        .with_context(|| format!("Failed to write image: {image_path:?}"))?;
                } else {
                    warn!("Failed to extract resource: {id}");
                }
            }
        }

        info!("Extracted {image_count} images to {book_dir:?}");
        self.book_dirs
            .lock()
            .unwrap()
            .insert(epub_path_str, book_dir);

        Ok(())
    }

    pub fn resolve_image_path_with_context(
        &self,
        epub_path: &Path,
        image_href: &str,
        chapter_path: Option<&str>,
    ) -> Option<PathBuf> {
        let epub_path_str = epub_path.to_string_lossy().to_string();

        let book_dir = self
            .book_dirs
            .lock()
            .unwrap()
            .get(&epub_path_str)
            .cloned()?;

        let mut paths_to_try = Vec::new();
        let clean_href = image_href.trim_start_matches('/');
        if let Some(chapter) = chapter_path {
            if clean_href.starts_with("../") {
                let chapter_path = Path::new(chapter);
                if let Some(chapter_dir) = chapter_path.parent() {
                    let resolved = chapter_dir.join(clean_href);

                    let normalized = normalize_path(&resolved);
                    paths_to_try.push(book_dir.join(&normalized));

                    if let Ok(stripped) = normalized.strip_prefix("OEBPS/") {
                        paths_to_try.push(book_dir.join(stripped));
                    }
                }
            }
        }

        // TODO: this is garbage of an approach
        //
        // Strategy 1: Direct path from book root
        paths_to_try.push(book_dir.join(clean_href));

        // Strategy 2: Remove OEBPS prefix if present
        let without_oebps = clean_href.strip_prefix("OEBPS/").unwrap_or(clean_href);
        paths_to_try.push(book_dir.join(without_oebps));

        // Strategy 3: If it's a relative path with ../, resolve it from common directories
        if clean_href.starts_with("../") {
            // Remove the ../ prefix
            let without_parent = clean_href.strip_prefix("../").unwrap_or(clean_href);
            // Try from OEBPS directory
            paths_to_try.push(book_dir.join("OEBPS").join(without_parent));
            // Try directly from book root (for case where images are at root level)
            paths_to_try.push(book_dir.join(without_parent));
        }

        // Strategy 4: Try adding OEBPS prefix if not present
        if !clean_href.starts_with("OEBPS/") && !clean_href.starts_with("../") {
            paths_to_try.push(book_dir.join("OEBPS").join(clean_href));
        }

        // Try each path in order
        for path in &paths_to_try {
            if path.exists() {
                debug!("Resolved image '{image_href}' to '{path:?}'");
                return Some(path.clone());
            }
        }

        warn!(
            "Image not found: '{image_href}' with chapter context {chapter_path:?} (tried: {paths_to_try:?})"
        );
        None
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

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                // Remove the last component if it exists and isn't also a ".."
                if !components.is_empty() {
                    if let Some(last) = components.last() {
                        if !matches!(last, std::path::Component::ParentDir) {
                            components.pop();
                            continue;
                        }
                    }
                }
                components.push(component);
            }
            std::path::Component::CurDir => {
                // Skip "." components
                continue;
            }
            _ => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
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
}
