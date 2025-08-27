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
                let extension = path.extension()?.to_str()?;
                if extension == "epub" || extension == "html" || extension == "htm" {
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

    pub fn load_epub(&self, path: &str) -> Result<EpubDoc<BufReader<std::fs::File>>, String> {
        info!("Loading document from path: {}", path);

        // Verify the book exists in our managed list
        if !self.books.iter().any(|book| book.path == path) {
            return Err(format!("Book not found in managed list: {}", path));
        }

        if self.is_html_file(path) {
            // For HTML files, create a fake EPUB
            self.create_fake_epub_from_html(path)
        } else {
            // For real EPUB files
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
    }

    fn create_fake_epub_from_html(
        &self,
        path: &str,
    ) -> Result<EpubDoc<BufReader<std::fs::File>>, String> {
        // Read the HTML content
        let html_content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                error!("Failed to read HTML file {}: {}", path, e);
                return Err(format!("Failed to read HTML file: {}", e));
            }
        };

        // Create a minimal EPUB structure in memory
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

        // Extract title from HTML, or use full filename (with .html) as fallback
        let filename = Path::new(original_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("HTML Document");

        let title = self
            .extract_html_title(html_content)
            .unwrap_or_else(|| filename.to_string());

        // Create a temporary file to store the fake EPUB
        let temp_file =
            NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {}", e))?;

        let temp_path = temp_file.path().to_path_buf();

        {
            let file = std::fs::File::create(&temp_path)
                .map_err(|e| format!("Failed to create temp EPUB file: {}", e))?;

            let mut zip = ZipWriter::new(file);
            let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

            // Add mimetype file
            zip.start_file("mimetype", options)
                .map_err(|e| format!("Failed to add mimetype: {}", e))?;
            zip.write_all(b"application/epub+zip")
                .map_err(|e| format!("Failed to write mimetype: {}", e))?;

            // Add META-INF/container.xml
            zip.start_file("META-INF/container.xml", options)
                .map_err(|e| format!("Failed to add container.xml: {}", e))?;
            let container_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
</container>"#;
            zip.write_all(container_xml.as_bytes())
                .map_err(|e| format!("Failed to write container.xml: {}", e))?;

            // Add OEBPS/content.opf
            zip.start_file("OEBPS/content.opf", options)
                .map_err(|e| format!("Failed to add content.opf: {}", e))?;
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
                .map_err(|e| format!("Failed to write content.opf: {}", e))?;

            // Add OEBPS/toc.ncx (table of contents)
            zip.start_file("OEBPS/toc.ncx", options)
                .map_err(|e| format!("Failed to add toc.ncx: {}", e))?;
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
                .map_err(|e| format!("Failed to write toc.ncx: {}", e))?;

            // Add OEBPS/chapter1.xhtml with the HTML content
            zip.start_file("OEBPS/chapter1.xhtml", options)
                .map_err(|e| format!("Failed to add chapter1.xhtml: {}", e))?;

            // Wrap HTML content in proper XHTML structure if it's not already
            let xhtml_content = if html_content.contains("<!DOCTYPE") {
                html_content.to_string()
            } else {
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>{}</title>
</head>
<body>
{}
</body>
</html>"#,
                    title, html_content
                )
            };

            zip.write_all(xhtml_content.as_bytes())
                .map_err(|e| format!("Failed to write chapter1.xhtml: {}", e))?;

            zip.finish()
                .map_err(|e| format!("Failed to finish ZIP: {}", e))?;
        }

        // Now open the temporary EPUB file
        match EpubDoc::new(&temp_path) {
            Ok(mut doc) => {
                info!(
                    "Successfully created fake EPUB from HTML: {}",
                    original_path
                );
                // Move to the first (and only) chapter
                let _ = doc.set_current_page(0);
                Ok(doc)
            }
            Err(e) => {
                error!("Failed to open created EPUB: {}", e);
                Err(format!("Failed to open created EPUB: {}", e))
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
                    let clean_title = title.replace(|c: char| c == '<' || c == '>', "");
                    return Some(clean_title.trim().to_string());
                }
            }
        }

        None
    }

    pub fn refresh_books(&mut self) {
        info!("Refreshing book list");
        self.books = Self::discover_books_in_dir(&self.scan_directory);
    }

    pub fn find_book_index_by_path(&self, path: &str) -> Option<usize> {
        self.books.iter().position(|book| book.path == path)
    }

    pub fn contains_book(&self, path: &str) -> bool {
        self.books.iter().any(|book| book.path == path)
    }

    pub fn is_html_file(&self, path: &str) -> bool {
        let path = Path::new(path);
        match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) => ext == "html" || ext == "htm",
            None => false,
        }
    }
}
