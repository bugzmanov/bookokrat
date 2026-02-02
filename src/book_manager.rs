#[cfg(feature = "pdf")]
use crate::settings::is_pdf_enabled;
use epub::doc::EpubDoc;
use log::{error, info};
use std::io::BufReader;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LibraryMode {
    Standard,
    Calibre,
}

pub struct BookManager {
    pub books: Vec<BookInfo>,
    scan_directory: String,
    pub library_mode: LibraryMode,
}

/// Format of a book file
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookFormat {
    Epub,
    Html,
    #[cfg(feature = "pdf")]
    Pdf,
}

#[derive(Clone)]
pub struct BookInfo {
    pub path: String,
    pub display_name: String,
    pub format: BookFormat,
}

impl Default for BookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BookManager {
    pub fn new() -> Self {
        Self::new_with_directory(".")
    }

    pub fn new_with_directory(directory: &str) -> Self {
        let scan_directory = directory.to_string();
        let library_mode = if Self::is_calibre_library(&scan_directory) {
            info!("Detected Calibre library at {scan_directory}");
            LibraryMode::Calibre
        } else {
            LibraryMode::Standard
        };

        let mut books = match library_mode {
            LibraryMode::Calibre => Self::discover_books_in_calibre_library(&scan_directory),
            LibraryMode::Standard => Self::discover_books_in_dir(&scan_directory),
        };
        books.sort_by(|a, b| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        });
        Self {
            books,
            scan_directory,
            library_mode,
        }
    }

    fn is_calibre_library(dir: &str) -> bool {
        Path::new(dir).join("metadata.db").exists()
    }

    pub fn is_calibre_mode(&self) -> bool {
        self.library_mode == LibraryMode::Calibre
    }

    fn discover_books_in_dir(dir: &str) -> Vec<BookInfo> {
        std::fs::read_dir(dir)
            .unwrap_or_else(|e| {
                error!("Failed to read directory {dir}: {e}");
                panic!("Failed to read directory {dir}: {e}");
            })
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                let extension = path.extension()?.to_str()?.to_lowercase();
                let format = match extension.as_str() {
                    "epub" => Some(BookFormat::Epub),
                    "html" | "htm" => Some(BookFormat::Html),
                    #[cfg(feature = "pdf")]
                    "pdf" => Some(BookFormat::Pdf),
                    _ => None,
                }?;
                let path_str = path.to_str()?.to_string();
                let display_name = Self::extract_display_name(&path_str);
                Some(BookInfo {
                    path: path_str,
                    display_name,
                    format,
                })
            })
            .collect()
    }

    fn discover_books_in_calibre_library(dir: &str) -> Vec<BookInfo> {
        let mut books = Vec::new();

        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let path_str = match path.to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };

            let format = match Self::detect_format(&path_str) {
                Some(BookFormat::Epub) => Some(BookFormat::Epub),
                #[cfg(feature = "pdf")]
                Some(BookFormat::Pdf) => Some(BookFormat::Pdf),
                _ => None,
            };

            let Some(format) = format else {
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

        if !self.books.iter().any(|book| book.path == path) {
            return Err(format!("Book not found in managed list: {path}"));
        }

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

        let title = self
            .extract_html_title(html_content)
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

    fn extract_html_title(&self, content: &str) -> Option<String> {
        // Try to extract title from <title> tag or <h1> tag
        if let Some(start) = content.find("<title>") {
            if let Some(end) = content[start + 7..].find("</title>") {
                let title = &content[start + 7..start + 7 + end];
                return Some(title.trim().to_string());
            }
        }

        if let Some(start) = content.find("<h1") {
            if let Some(tag_end) = content[start..].find('>') {
                let content_start = start + tag_end + 1;
                if let Some(end) = content[content_start..].find("</h1>") {
                    let title = &content[content_start..content_start + end];
                    // Remove any HTML tags from the title
                    let clean_title = title.replace(['<', '>'], "");
                    return Some(clean_title.trim().to_string());
                }
            }
        }

        None
    }

    pub fn refresh_books(&mut self) {
        self.books = match self.library_mode {
            LibraryMode::Calibre => Self::discover_books_in_calibre_library(&self.scan_directory),
            LibraryMode::Standard => Self::discover_books_in_dir(&self.scan_directory),
        };
        self.books.sort_by(|a, b| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        });
    }

    /// Refresh and get filtered books list
    pub fn refresh(&mut self) {
        self.refresh_books();
    }

    /// Get books filtered by current settings (e.g., PDF enabled/disabled)
    pub fn get_books(&self) -> Vec<BookInfo> {
        #[cfg(feature = "pdf")]
        {
            if !is_pdf_enabled() {
                return self
                    .books
                    .iter()
                    .filter(|book| book.format != BookFormat::Pdf)
                    .cloned()
                    .collect();
            }
        }
        self.books.clone()
    }

    pub fn find_book_index_by_path(&self, path: &str) -> Option<usize> {
        self.books.iter().position(|book| book.path == path)
    }

    pub fn contains_book(&self, path: &str) -> bool {
        self.books.iter().any(|book| book.path == path)
    }

    /// Get the format of a book by path
    pub fn get_format(&self, path: &str) -> Option<BookFormat> {
        self.books
            .iter()
            .find(|book| book.path == path)
            .map(|book| book.format)
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
            _ => None,
        }
    }

    pub fn is_html_file(&self, path: &str) -> bool {
        Self::detect_format(path) == Some(BookFormat::Html)
    }

    #[cfg(feature = "pdf")]
    pub fn is_pdf_file(&self, path: &str) -> bool {
        Self::detect_format(path) == Some(BookFormat::Pdf)
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

        let manager = BookManager::new_with_directory(temp_dir.path().to_str().unwrap());
        let book_path = book_dir.to_str().unwrap();
        let mut doc = manager.load_epub(book_path).unwrap();

        assert!(doc.get_num_chapters() >= 1);
        assert!(doc.get_current_str().is_some());
    }
}
