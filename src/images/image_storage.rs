use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use roxmltree::Document;

/// Info about an extracted book's image directory
#[derive(Clone, Debug)]
struct BookDirInfo {
    /// The temp directory where images are extracted (e.g., temp_images/jung/)
    temp_dir: PathBuf,
    /// The content root directory from the EPUB's rootfile (e.g., "ops" or "OEBPS")
    content_root: Option<PathBuf>,
}

#[derive(Clone)]
pub struct ImageStorage {
    base_dir: PathBuf,
    book_dirs: Arc<Mutex<HashMap<String, BookDirInfo>>>,
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
                let content_root = content_root_from_epub(epub_path).ok().flatten();
                self.book_dirs.lock().unwrap().insert(
                    epub_path_str,
                    BookDirInfo {
                        temp_dir: book_dir,
                        content_root,
                    },
                );
                return Ok(());
            }
        }

        fs::create_dir_all(&book_dir)
            .with_context(|| format!("Failed to create book directory: {book_dir:?}"))?;

        let mut doc = if epub_path.is_dir() {
            create_epub_from_directory(epub_path)
                .with_context(|| format!("Failed to open exploded EPUB: {epub_path:?}"))?
        } else {
            let file = fs::File::open(epub_path)
                .with_context(|| format!("Failed to open EPUB file: {epub_path:?}"))?;
            EpubDoc::from_reader(BufReader::new(file))
                .with_context(|| format!("Failed to parse EPUB: {epub_path:?}"))?
        };

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

        // Extract content root directory from the epub's root_file path
        // e.g., "ops/vol_14_9781400850853.opf" -> "ops"
        let content_root = content_root_from_rootfile(&doc.root_file);

        info!("Extracted {image_count} images to {book_dir:?} (content_root: {content_root:?})");
        self.book_dirs.lock().unwrap().insert(
            epub_path_str,
            BookDirInfo {
                temp_dir: book_dir,
                content_root,
            },
        );

        Ok(())
    }

    pub fn resolve_image_path_with_context(
        &self,
        epub_path: &Path,
        image_href: &str,
        chapter_path: Option<&str>,
    ) -> Option<PathBuf> {
        let epub_path_str = epub_path.to_string_lossy().to_string();

        let BookDirInfo {
            temp_dir: book_dir,
            content_root,
        } = self
            .book_dirs
            .lock()
            .unwrap()
            .get(&epub_path_str)
            .cloned()?;

        let mut paths_to_try = Vec::new();
        let clean_href = image_href.trim_start_matches('/');
        let content_root = content_root.as_deref().unwrap_or_else(|| Path::new(""));

        // Resolve href against chapter path if present; otherwise resolve against content root.
        let mut resolved_rel = if let Some(chapter) = chapter_path {
            let chapter = chapter.trim_start_matches('/');
            let chapter_path = Path::new(chapter);
            let chapter_rel =
                if !content_root.as_os_str().is_empty() && chapter_path.starts_with(content_root) {
                    chapter_path
                        .strip_prefix(content_root)
                        .unwrap_or(chapter_path)
                } else {
                    chapter_path
                };
            let base_dir = chapter_rel.parent().unwrap_or_else(|| Path::new(""));
            normalize_path(&base_dir.join(clean_href))
        } else {
            normalize_path(Path::new(clean_href))
        };

        if chapter_path.is_none() {
            resolved_rel = strip_leading_parents(&resolved_rel);
        }

        // Primary, spec-aligned resolution: content root + resolved href.
        let resolved_from_root =
            if !content_root.as_os_str().is_empty() && resolved_rel.starts_with(content_root) {
                normalize_path(&resolved_rel)
            } else {
                normalize_path(&content_root.join(&resolved_rel))
            };
        paths_to_try.push(book_dir.join(&resolved_from_root));

        // Fallback: some older EPUBs hardcode OEBPS.
        if content_root != Path::new("OEBPS") {
            let resolved_oebps = if resolved_rel.starts_with("OEBPS") {
                normalize_path(&resolved_rel)
            } else {
                normalize_path(&Path::new("OEBPS").join(&resolved_rel))
            };
            paths_to_try.push(book_dir.join(resolved_oebps));
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

fn content_root_from_rootfile(root_file: &Path) -> Option<PathBuf> {
    root_file.parent().and_then(|parent| {
        if parent.as_os_str().is_empty() {
            None
        } else {
            Some(parent.to_path_buf())
        }
    })
}

fn content_root_from_epub(epub_path: &Path) -> Result<Option<PathBuf>> {
    if epub_path.is_dir() {
        let container_path = epub_path.join("META-INF").join("container.xml");
        if !container_path.is_file() {
            return Ok(None);
        }
        let xml = fs::read_to_string(&container_path)
            .with_context(|| format!("Failed to read container.xml: {container_path:?}"))?;
        let doc = Document::parse(&xml)
            .with_context(|| format!("Failed to parse container.xml: {container_path:?}"))?;
        let rootfile = doc
            .descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"));
        Ok(rootfile.and_then(|path| content_root_from_rootfile(Path::new(path))))
    } else {
        let file = fs::File::open(epub_path)
            .with_context(|| format!("Failed to open EPUB file: {epub_path:?}"))?;
        let doc = EpubDoc::from_reader(BufReader::new(file))
            .with_context(|| format!("Failed to parse EPUB: {epub_path:?}"))?;
        Ok(content_root_from_rootfile(&doc.root_file))
    }
}

fn create_epub_from_directory(dir: &Path) -> Result<EpubDoc<BufReader<std::fs::File>>> {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use walkdir::WalkDir;
    use zip::write::FileOptions;

    info!("Creating temporary EPUB from directory: {dir:?}");

    let temp_file = NamedTempFile::new().context("Failed to create temp file")?;
    let temp_path = temp_file.path().to_path_buf();

    {
        let file = std::fs::File::create(&temp_path)
            .with_context(|| format!("Failed to create temp EPUB file: {temp_path:?}"))?;
        let mut zip = zip::ZipWriter::new(file);

        let stored = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mimetype_path = dir.join("mimetype");
        if mimetype_path.is_file() {
            let data = std::fs::read(&mimetype_path)
                .with_context(|| format!("Failed to read mimetype: {mimetype_path:?}"))?;
            zip.start_file("mimetype", stored)
                .context("Failed to add mimetype to zip")?;
            zip.write_all(&data)
                .context("Failed to write mimetype to zip")?;
        } else {
            warn!("Exploded EPUB missing mimetype file: {mimetype_path:?}");
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                if entry.file_name() == ".DS_Store" {
                    continue;
                }

                let rel = entry
                    .path()
                    .strip_prefix(dir)
                    .context("Failed to strip prefix for EPUB directory entry")?;
                if rel == Path::new("mimetype") {
                    continue;
                }
                files.push(rel.to_path_buf());
            }
        }

        files.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
        for rel in files {
            let full_path = dir.join(&rel);
            let zip_path = rel.to_string_lossy().replace('\\', "/");
            zip.start_file(zip_path, deflated)
                .context("Failed to add file to EPUB zip")?;
            let data = std::fs::read(&full_path)
                .with_context(|| format!("Failed to read file {full_path:?}"))?;
            zip.write_all(&data)
                .with_context(|| format!("Failed to write file {full_path:?}"))?;
        }

        zip.finish().context("Failed to finish EPUB zip")?;
    }

    let file = std::fs::File::open(&temp_path)
        .with_context(|| format!("Failed to open temp EPUB file: {temp_path:?}"))?;
    EpubDoc::from_reader(BufReader::new(file)).context("Failed to parse temp EPUB")
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

fn strip_leading_parents(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    let mut skipping = true;

    for component in path.components() {
        if skipping && matches!(component, std::path::Component::ParentDir) {
            continue;
        }
        skipping = false;
        components.push(component);
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
    use walkdir::WalkDir;

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

    #[test]
    fn extract_images_from_exploded_epub() {
        let base_dir = TempDir::new().unwrap();
        let book_dir_root = TempDir::new().unwrap();
        let book_dir = book_dir_root.path().join("book.epub");

        fs::create_dir_all(&book_dir).unwrap();
        fs::write(book_dir.join("mimetype"), b"application/epub+zip").unwrap();

        let container_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
</container>"#;
        fs::create_dir_all(book_dir.join("META-INF")).unwrap();
        fs::write(book_dir.join("META-INF/container.xml"), container_xml).unwrap();

        let content_opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" version="2.0">
    <metadata>
        <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Exploded</dc:title>
        <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="bookid">exploded-2</dc:identifier>
        <dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language>
    </metadata>
    <manifest>
        <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
        <item id="cover" href="images/cover.jpg" media-type="image/jpeg"/>
        <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    </manifest>
    <spine toc="ncx">
        <itemref idref="chapter1"/>
    </spine>
</package>"#;
        fs::create_dir_all(book_dir.join("OEBPS/images")).unwrap();
        fs::write(book_dir.join("OEBPS/content.opf"), content_opf).unwrap();
        fs::write(
            book_dir.join("OEBPS/chapter1.xhtml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>Exploded</title>
</head>
<body>
    <p>Hi</p>
</body>
</html>"#,
        )
        .unwrap();
        fs::write(
            book_dir.join("OEBPS/toc.ncx"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
    <head>
        <meta name="dtb:uid" content="exploded-2"/>
        <meta name="dtb:depth" content="1"/>
        <meta name="dtb:totalPageCount" content="0"/>
        <meta name="dtb:maxPageNumber" content="0"/>
    </head>
    <docTitle>
        <text>Exploded</text>
    </docTitle>
    <navMap>
        <navPoint id="chapter1" playOrder="1">
            <navLabel>
                <text>Chapter 1</text>
            </navLabel>
            <content src="chapter1.xhtml"/>
        </navPoint>
    </navMap>
</ncx>"#,
        )
        .unwrap();
        fs::write(book_dir.join("OEBPS/images/cover.jpg"), b"fakejpg").unwrap();

        let storage = ImageStorage::new(base_dir.path().to_path_buf()).unwrap();
        storage.extract_images(&book_dir).unwrap();

        let mut found = false;
        for entry in WalkDir::new(base_dir.path()).into_iter().flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("jpg") {
                found = true;
                break;
            }
        }

        assert!(found, "expected extracted .jpg in image storage");
    }
}
