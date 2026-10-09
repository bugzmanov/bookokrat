use crate::parsing::html_to_markdown::{HtmlTitlePreference, extract_html_title};
use crate::settings::{BookSortOrder, RuntimeSettings};
use epub::doc::EpubDoc;
use log::{error, info};
use std::io::{self, BufReader};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LibraryMode {
    Standard,
    Calibre,
}

pub struct BookManager {
    pub books: Vec<BookInfo>,
    scan_directory: String,
    pub(crate) settings: RuntimeSettings,
    pub library_mode: LibraryMode,
    pending_scan: Option<Receiver<io::Result<Vec<BookInfo>>>>,
    #[cfg(feature = "pdf")]
    pub supports_graphics: bool,
}

/// Format of a book file
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookFormat {
    Epub,
    Html,
    #[cfg(feature = "pdf")]
    Pdf,
    #[cfg(feature = "pdf")]
    Djvu,
}

#[derive(Clone)]
pub struct BookInfo {
    pub path: String,
    pub display_name: String,
    pub format: BookFormat,
}

impl BookManager {
    pub fn new_with_directory(directory: &str, settings: RuntimeSettings) -> Self {
        let mut manager = Self::without_books(directory, settings);
        manager.books =
            Self::scan(&manager.scan_directory, manager.library_mode).unwrap_or_default();
        manager
    }

    /// Like `new_with_directory`, but scans on a background thread so a large
    /// library doesn't delay startup. `poll_scan` applies the result.
    pub fn new_with_directory_in_background(directory: &str, settings: RuntimeSettings) -> Self {
        let mut manager = Self::without_books(directory, settings);
        let (tx, rx) = mpsc::channel();
        let scan_directory = manager.scan_directory.clone();
        let library_mode = manager.library_mode;
        let spawned = std::thread::Builder::new()
            .name("library-scan".to_string())
            .spawn(move || {
                let _ = tx.send(Self::scan(&scan_directory, library_mode));
            });
        match spawned {
            Ok(_) => manager.pending_scan = Some(rx),
            Err(e) => {
                error!("Failed to spawn library scan thread, scanning synchronously: {e}");
                manager.books =
                    Self::scan(&manager.scan_directory, manager.library_mode).unwrap_or_default();
            }
        }
        manager
    }

    fn without_books(directory: &str, settings: RuntimeSettings) -> Self {
        let scan_directory = directory.to_string();
        let library_mode = if Self::is_calibre_library(&scan_directory) {
            info!("Detected Calibre library at {scan_directory}");
            LibraryMode::Calibre
        } else {
            LibraryMode::Standard
        };
        Self {
            books: Vec::new(),
            scan_directory,
            settings,
            library_mode,
            pending_scan: None,
            #[cfg(feature = "pdf")]
            supports_graphics: false,
        }
    }

    fn scan(dir: &str, library_mode: LibraryMode) -> io::Result<Vec<BookInfo>> {
        let mut books = match library_mode {
            LibraryMode::Calibre => Self::discover_books_in_calibre_library(dir),
            LibraryMode::Standard => Self::discover_books_in_dir(dir)?,
        };
        books.sort_by(|a, b| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        });
        Ok(books)
    }

    pub fn is_scanning(&self) -> bool {
        self.pending_scan.is_some()
    }

    /// Applies a finished background scan. Returns `None` while no result is available.
    pub fn poll_scan(&mut self) -> Option<io::Result<()>> {
        let result = match self.pending_scan.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(io::Error::other(
                "library scan thread exited without a result",
            )),
        };
        self.pending_scan = None;
        Some(result.map(|books| self.books = books))
    }

    fn is_calibre_library(dir: &str) -> bool {
        Path::new(dir).join("metadata.db").exists()
    }

    pub fn is_calibre_mode(&self) -> bool {
        self.library_mode == LibraryMode::Calibre
    }

    fn discover_books_in_dir(dir: &str) -> io::Result<Vec<BookInfo>> {
        let entries = std::fs::read_dir(dir).inspect_err(|e| {
            error!("Failed to read directory {dir}: {e}");
        })?;
        Ok(entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let path_str = path.to_str()?.to_string();
                let format = Self::detect_format(&path_str)?;
                let display_name = Self::extract_display_name(&path_str);
                Some(BookInfo {
                    path: path_str,
                    display_name,
                    format,
                })
            })
            .collect())
    }

    fn calibre_book_format(path: &str) -> Option<BookFormat> {
        Self::detect_format(path).filter(|format| *format != BookFormat::Html)
    }

    fn discover_books_in_calibre_library(dir: &str) -> Vec<BookInfo> {
        let start = std::time::Instant::now();
        let mut books = Vec::new();
        let mut files_visited: u64 = 0;

        // Calibre structure is always: Author/Book Title (id)/file.epub — depth 3 max.
        // Without a limit, WalkDir would descend into temp_images/, .git/, cloud-synced
        // dirs, etc., which can stall or take minutes on large filesystems.
        let mut last_log_time = start;
        for entry in WalkDir::new(dir)
            .max_depth(3)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            files_visited += 1;
            let now = std::time::Instant::now();
            if now.duration_since(last_log_time).as_secs() >= 5 {
                info!(
                    "Calibre scan in progress: {} books found so far, {} files visited ({:.1}s elapsed)",
                    books.len(),
                    files_visited,
                    now.duration_since(start).as_secs_f64()
                );
                last_log_time = now;
            }

            let path = entry.path();
            let path_str = match path.to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };

            let Some(format) = Self::calibre_book_format(&path_str) else {
                continue;
            };

            let display_name = path
                .parent()
                .and_then(Self::parse_calibre_opf)
                .unwrap_or_else(|| Self::extract_display_name(&path_str));

            books.push(BookInfo {
                path: path_str,
                display_name,
                format,
            });
        }

        info!(
            "Calibre library scan: {} books found, {} files visited in {:.2}s",
            books.len(),
            files_visited,
            start.elapsed().as_secs_f64()
        );

        books
    }

    fn parse_calibre_opf(book_dir: &Path) -> Option<String> {
        let opf_path = book_dir.join("metadata.opf");
        let content = std::fs::read_to_string(&opf_path).ok()?;
        let doc = roxmltree::Document::parse(&content).ok()?;

        let mut title: Option<String> = None;
        let mut author: Option<String> = None;

        for node in doc.descendants() {
            if node.tag_name().name() == "title" && title.is_none() {
                title = node.text().map(|s| s.trim().to_string());
            }
            if node.tag_name().name() == "creator" && author.is_none() {
                author = node.text().map(|s| s.trim().to_string());
            }
            if title.is_some() && author.is_some() {
                break;
            }
        }

        let title = title?;
        Some(match author {
            Some(a) if !a.is_empty() => format!("{title} - {a}"),
            _ => title,
        })
    }

    fn extract_display_name(file_path: &str) -> String {
        let path = Path::new(file_path);

        // For HTML files, preserve the full filename with extension
        if let Some(extension) = path.extension() {
            if extension == "html" || extension == "htm" {
                return path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
            }
        }

        // For other files (like EPUB), remove the extension
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }

    pub fn get_book_info(&self, index: usize) -> Option<&BookInfo> {
        self.books.get(index)
    }

    pub fn get_book_by_path(&self, path: &str) -> Option<&BookInfo> {
        self.books.iter().find(|book| book.path == path)
    }

    pub fn load_epub(&self, path: &str) -> Result<EpubDoc<BufReader<std::fs::File>>, String> {
        info!("Loading document from path: {path}");

        if self.is_html_file(path) {
            // For HTML files, create a fake EPUB
            self.create_fake_epub_from_html(path)
        } else if Self::is_epub_directory(Path::new(path)) {
            self.create_epub_from_directory(Path::new(path))
        } else {
            info!("Attempting to load EPUB file: {path}");
            match EpubDoc::new(path) {
                Ok(mut doc) => {
                    info!("Successfully created EpubDoc for: {path}");

                    let num_pages = doc.get_num_chapters();
                    let current_page = doc.get_current_chapter();
                    info!(
                        "EPUB spine details: {num_pages} pages, current position: {current_page}"
                    );

                    if let Some(title) = doc.mdata("title") {
                        info!("EPUB title: {value}", value = title.value);
                    }
                    if let Some(author) = doc.mdata("creator") {
                        info!("EPUB author: {value}", value = author.value);
                    }

                    match doc.get_current_str() {
                        Some((content, mime)) => {
                            info!(
                                "Initial content available at position 0, mime: {}, size: {} bytes",
                                mime,
                                content.len()
                            );
                        }
                        None => {
                            error!("WARNING: No content available at initial position 0");
                            info!("Attempting to get spine information...");
                            let spine = &doc.spine;
                            info!("Spine has {} items", spine.len());
                            for (i, spine_item) in spine.iter().take(5).enumerate() {
                                info!(
                                    "  Spine[{}]: idref={}, linear={}",
                                    i, spine_item.idref, spine_item.linear
                                );
                                // Check if this spine item exists in resources
                                if let Some(resource) = doc.resources.get(&spine_item.idref) {
                                    info!(
                                        "    -> Resource exists: {path:?} ({mime})",
                                        path = resource.path,
                                        mime = resource.mime
                                    );
                                } else {
                                    error!(
                                        "    -> Resource NOT FOUND in resources map for idref: {}",
                                        spine_item.idref
                                    );
                                }
                            }
                        }
                    }

                    Ok(doc)
                }
                Err(e) => {
                    error!("Failed to create EpubDoc for {path}: {e}");
                    Err(format!("Failed to load EPUB: {e}"))
                }
            }
        }
    }

    fn is_epub_directory(path: &Path) -> bool {
        if !path.is_dir() {
            return false;
        }

        match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) => ext == "epub",
            None => false,
        }
    }

    fn create_epub_from_directory(
        &self,
        dir: &Path,
    ) -> Result<EpubDoc<BufReader<std::fs::File>>, String> {
        use std::io::Write;
        use tempfile::NamedTempFile;
        use walkdir::WalkDir;
        use zip::write::FileOptions;

        info!("Creating temporary EPUB from directory: {dir:?}");

        let temp_file =
            NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {e}"))?;
        let temp_path = temp_file.path().to_path_buf();

        {
            let file = std::fs::File::create(&temp_path)
                .map_err(|e| format!("Failed to create temp EPUB file: {e}"))?;
            let mut zip = zip::ZipWriter::new(file);

            let stored = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            let deflated =
                FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            let mimetype_path = dir.join("mimetype");
            if mimetype_path.is_file() {
                let data = std::fs::read(&mimetype_path)
                    .map_err(|e| format!("Failed to read mimetype: {e}"))?;
                zip.start_file("mimetype", stored)
                    .map_err(|e| format!("Failed to add mimetype: {e}"))?;
                zip.write_all(&data)
                    .map_err(|e| format!("Failed to write mimetype: {e}"))?;
            } else {
                info!("Exploded EPUB missing mimetype file: {mimetype_path:?}");
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
                        .map_err(|e| format!("Failed to strip prefix: {e}"))?;
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
                    .map_err(|e| format!("Failed to add file to EPUB: {e}"))?;
                let data = std::fs::read(&full_path)
                    .map_err(|e| format!("Failed to read file {full_path:?}: {e}"))?;
                zip.write_all(&data)
                    .map_err(|e| format!("Failed to write file {full_path:?}: {e}"))?;
            }

            zip.finish()
                .map_err(|e| format!("Failed to finish EPUB ZIP: {e}"))?;
        }

        match EpubDoc::new(&temp_path) {
            Ok(mut doc) => {
                info!("Successfully created EPUB from directory: {dir:?}");
                let _ = doc.set_current_chapter(0);
                Ok(doc)
            }
            Err(e) => {
                error!("Failed to open created EPUB from directory: {e}");
                Err(format!("Failed to open created EPUB: {e}"))
            }
        }
    }

    fn create_fake_epub_from_html(
        &self,
        path: &str,
    ) -> Result<EpubDoc<BufReader<std::fs::File>>, String> {
        let html_content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                error!("Failed to read HTML file {path}: {e}");
                return Err(format!("Failed to read HTML file: {e}"));
            }
        };

        self.create_minimal_epub_from_html(&html_content, path)
    }

    fn create_minimal_epub_from_html(
        &self,
        html_content: &str,
        original_path: &str,
    ) -> Result<EpubDoc<BufReader<std::fs::File>>, String> {
        use std::io::Write;
        use tempfile::NamedTempFile;
        use zip::{ZipWriter, write::FileOptions};

        let filename = Path::new(original_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("HTML Document");

        let title = extract_html_title(html_content, HtmlTitlePreference::TitleThenH1)
            .unwrap_or_else(|| filename.to_string());

        let temp_file =
            NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {e}"))?;

        let temp_path = temp_file.path().to_path_buf();

        {
            let file = std::fs::File::create(&temp_path)
                .map_err(|e| format!("Failed to create temp EPUB file: {e}"))?;

            let mut zip = ZipWriter::new(file);
            let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

            zip.start_file("mimetype", options)
                .map_err(|e| format!("Failed to add mimetype: {e}"))?;
            zip.write_all(b"application/epub+zip")
                .map_err(|e| format!("Failed to write mimetype: {e}"))?;

            zip.start_file("META-INF/container.xml", options)
                .map_err(|e| format!("Failed to add container.xml: {e}"))?;
            let container_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
</container>"#;
            zip.write_all(container_xml.as_bytes())
                .map_err(|e| format!("Failed to write container.xml: {e}"))?;

            zip.start_file("OEBPS/content.opf", options)
                .map_err(|e| format!("Failed to add content.opf: {e}"))?;
            let content_opf = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" version="2.0">
    <metadata>
        <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">{}</dc:title>
        <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="bookid">html-{}</dc:identifier>
        <dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language>
    </metadata>
    <manifest>
        <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
        <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    </manifest>
    <spine toc="ncx">
        <itemref idref="chapter1"/>
    </spine>
</package>"#,
                title,
                original_path.replace('/', "_")
            );
            zip.write_all(content_opf.as_bytes())
                .map_err(|e| format!("Failed to write content.opf: {e}"))?;

            zip.start_file("OEBPS/toc.ncx", options)
                .map_err(|e| format!("Failed to add toc.ncx: {e}"))?;
            let toc_ncx = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
    <head>
        <meta name="dtb:uid" content="html-{}"/>
        <meta name="dtb:depth" content="1"/>
        <meta name="dtb:totalPageCount" content="0"/>
        <meta name="dtb:maxPageNumber" content="0"/>
    </head>
    <docTitle>
        <text>{}</text>
    </docTitle>
    <navMap>
        <navPoint id="chapter1" playOrder="1">
            <navLabel>
                <text>{}</text>
            </navLabel>
            <content src="chapter1.xhtml"/>
        </navPoint>
    </navMap>
</ncx>"#,
                original_path.replace('/', "_"),
                title,
                filename
            );
            zip.write_all(toc_ncx.as_bytes())
                .map_err(|e| format!("Failed to write toc.ncx: {e}"))?;

            zip.start_file("OEBPS/chapter1.xhtml", options)
                .map_err(|e| format!("Failed to add chapter1.xhtml: {e}"))?;

            let xhtml_content = if html_content.contains("<!DOCTYPE") {
                html_content.to_string()
            } else {
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>{title}</title>
</head>
<body>
{html_content}
</body>
</html>"#
                )
            };

            zip.write_all(xhtml_content.as_bytes())
                .map_err(|e| format!("Failed to write chapter1.xhtml: {e}"))?;

            zip.finish()
                .map_err(|e| format!("Failed to finish ZIP: {e}"))?;
        }

        match EpubDoc::new(&temp_path) {
            Ok(mut doc) => {
                info!("Successfully created fake EPUB from HTML: {original_path}");
                let _ = doc.set_current_chapter(0);
                Ok(doc)
            }
            Err(e) => {
                error!("Failed to open created EPUB: {e}");
                Err(format!("Failed to open created EPUB: {e}"))
            }
        }
    }

    pub fn refresh_books(&mut self) {
        // A synchronous rescan supersedes a background scan still in flight.
        self.pending_scan = None;
        if let Ok(books) = Self::scan(&self.scan_directory, self.library_mode) {
            self.books = books;
        }
    }

    /// Refresh and get filtered books list
    pub fn refresh(&mut self) {
        self.refresh_books();
    }

    /// Get books filtered by current settings (e.g., PDF enabled/disabled)
    pub fn get_books(&self) -> Vec<BookInfo> {
        let settings = self.settings.load();
        let mut books: Vec<BookInfo>;
        #[cfg(feature = "pdf")]
        {
            if !settings.pdf_enabled || !self.supports_graphics {
                books = self
                    .books
                    .iter()
                    .filter(|book| {
                        book.format != BookFormat::Pdf && book.format != BookFormat::Djvu
                    })
                    .cloned()
                    .collect();
            } else {
                books = self.books.clone();
            }
        }
        #[cfg(not(feature = "pdf"))]
        {
            books = self.books.clone();
        }

        if settings.book_sort_order == BookSortOrder::ByType {
            books.sort_by(|a, b| {
                let type_order = |f: &BookFormat| -> u8 {
                    match f {
                        #[cfg(feature = "pdf")]
                        BookFormat::Pdf => 0,
                        #[cfg(feature = "pdf")]
                        BookFormat::Djvu => 0,
                        BookFormat::Epub => 1,
                        BookFormat::Html => 2,
                    }
                };
                type_order(&a.format)
                    .cmp(&type_order(&b.format))
                    .then_with(|| {
                        a.display_name
                            .to_lowercase()
                            .cmp(&b.display_name.to_lowercase())
                    })
            });
        }

        books
    }

    pub fn find_book_index_by_path(&self, path: &str) -> Option<usize> {
        self.books.iter().position(|book| book.path == path)
    }

    /// Whether the library lists `path`. While a background scan is running,
    /// the scan's own rules are applied to `path` instead of waiting for it.
    pub fn contains_book(&self, path: &str) -> bool {
        if !self.is_scanning() {
            return self.books.iter().any(|book| book.path == path);
        }
        let scan_dir = Path::new(&self.scan_directory);
        let candidate = Path::new(path);
        match self.library_mode {
            LibraryMode::Standard => {
                Self::detect_format(path).is_some()
                    && candidate
                        .file_name()
                        .is_some_and(|name| scan_dir.join(name).to_str() == Some(path))
                    && candidate.symlink_metadata().is_ok()
            }
            LibraryMode::Calibre => {
                Self::calibre_book_format(path).is_some()
                    && candidate.strip_prefix(scan_dir).is_ok_and(|relative| {
                        (1..=3).contains(&relative.components().count())
                            && scan_dir.join(relative).to_str() == Some(path)
                    })
                    && candidate
                        .symlink_metadata()
                        .is_ok_and(|metadata| metadata.is_file())
            }
        }
    }

    /// Detect format from file extension (for files not in the managed list)
    pub fn detect_format(path: &str) -> Option<BookFormat> {
        let path = Path::new(path);
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "epub" => Some(BookFormat::Epub),
            "html" | "htm" => Some(BookFormat::Html),
            #[cfg(feature = "pdf")]
            "pdf" => Some(BookFormat::Pdf),
            #[cfg(feature = "pdf")]
            "djvu" | "djv" => Some(BookFormat::Djvu),
            _ => None,
        }
    }

    pub fn is_html_file(&self, path: &str) -> bool {
        Self::detect_format(path) == Some(BookFormat::Html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents.as_bytes()).unwrap();
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn get_books_filters_pdf_and_djvu_when_pdf_disabled() {
        let temp_dir = TempDir::new().unwrap();
        // Create dummy files so discover_books_in_dir picks them up
        fs::write(temp_dir.path().join("novel.epub"), b"fake").unwrap();
        fs::write(temp_dir.path().join("paper.pdf"), b"fake").unwrap();
        fs::write(temp_dir.path().join("scan.djvu"), b"fake").unwrap();

        let settings = RuntimeSettings::in_memory(crate::settings::Settings {
            pdf_enabled: false,
            ..crate::settings::Settings::default()
        });
        let manager = BookManager::new_with_directory(temp_dir.path().to_str().unwrap(), settings);
        assert_eq!(manager.books.len(), 3, "all 3 files should be discovered");

        let filtered = manager.get_books();
        let formats: Vec<_> = filtered.iter().map(|b| b.format).collect();

        assert!(
            !formats.contains(&BookFormat::Pdf),
            "PDF books should be filtered out when pdf is disabled"
        );
        assert!(
            !formats.contains(&BookFormat::Djvu),
            "DJVU books should be filtered out when pdf is disabled"
        );
        assert_eq!(filtered.len(), 1, "only the epub should remain");
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn get_books_filters_pdf_and_djvu_when_no_graphics_support() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("novel.epub"), b"fake").unwrap();
        fs::write(temp_dir.path().join("paper.pdf"), b"fake").unwrap();
        fs::write(temp_dir.path().join("scan.djvu"), b"fake").unwrap();

        let settings = RuntimeSettings::in_memory(crate::settings::Settings::default());
        let manager = BookManager::new_with_directory(temp_dir.path().to_str().unwrap(), settings);
        assert_eq!(manager.books.len(), 3, "all 3 files should be discovered");

        // pdf_enabled is true (default) — simulating a user who has never toggled the setting.
        // But the terminal doesn't support graphics, so PDFs/DJVUs should still be hidden.
        let filtered = manager.get_books();
        let formats: Vec<_> = filtered.iter().map(|b| b.format).collect();

        // This must hold regardless of the pdf_enabled setting:
        // a terminal without graphics cannot render PDFs/DJVUs.
        assert!(
            !formats.contains(&BookFormat::Pdf),
            "PDF books should be filtered out when terminal has no graphics support"
        );
        assert!(
            !formats.contains(&BookFormat::Djvu),
            "DJVU books should be filtered out when terminal has no graphics support"
        );
        assert_eq!(filtered.len(), 1, "only the epub should remain");
    }

    #[test]
    fn load_exploded_epub_directory() {
        let temp_dir = TempDir::new().unwrap();
        let book_dir = temp_dir.path().join("book.epub");

        fs::create_dir_all(&book_dir).unwrap();

        write_file(book_dir.join("mimetype").as_path(), "application/epub+zip");

        write_file(
            book_dir.join("META-INF/container.xml").as_path(),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
</container>"#,
        );

        write_file(
            book_dir.join("OEBPS/content.opf").as_path(),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" version="2.0">
    <metadata>
        <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Exploded</dc:title>
        <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="bookid">exploded-1</dc:identifier>
        <dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language>
    </metadata>
    <manifest>
        <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
        <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    </manifest>
    <spine toc="ncx">
        <itemref idref="chapter1"/>
    </spine>
</package>"#,
        );

        write_file(
            book_dir.join("OEBPS/toc.ncx").as_path(),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
    <head>
        <meta name="dtb:uid" content="exploded-1"/>
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
        );

        write_file(
            book_dir.join("OEBPS/chapter1.xhtml").as_path(),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>Exploded</title>
</head>
<body>
    <p>Hello</p>
</body>
</html>"#,
        );

        let manager = BookManager::new_with_directory(
            temp_dir.path().to_str().unwrap(),
            RuntimeSettings::in_memory(crate::settings::Settings::default()),
        );
        let book_path = book_dir.to_str().unwrap();
        let mut doc = manager.load_epub(book_path).unwrap();

        assert!(doc.get_num_chapters() >= 1);
        assert!(doc.get_current_str().is_some());
    }
}
