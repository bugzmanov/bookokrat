/// Simple fake book creator that writes minimal EPUB files directly to disk
use std::fs;
use std::path::Path;

/// Configuration for a fake book
#[derive(Debug, Clone)]
pub struct FakeBookConfig {
    pub title: String,
    pub chapter_count: usize,
    pub words_per_chapter: usize,
}

/// Creates a minimal, valid EPUB file at the given path
pub fn create_fake_epub_file<P: AsRef<Path>>(
    path: P,
    config: &FakeBookConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use zip::write::FileOptions;

    let file = fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);

    // mimetype file (uncompressed)
    zip.start_file(
        "mimetype",
        FileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    zip.write_all(b"application/epub+zip")?;

    // META-INF/container.xml
    zip.start_file("META-INF/container.xml", FileOptions::default())?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
</container>"#,
    )?;

    // OEBPS/content.opf
    let mut opf_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
    <metadata>
        <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">{}</dc:title>
        <dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/">Test Author</dc:creator>
        <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="BookId">fake-book-{}</dc:identifier>
        <dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language>
    </metadata>
    <manifest>
        <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
"#,
        config.title,
        config.title.to_lowercase().replace(' ', "-")
    );

    // Add chapter items to manifest
    for i in 0..config.chapter_count {
        opf_content.push_str(&format!(
            r#"        <item id="chapter{}" href="chapter{}.xhtml" media-type="application/xhtml+xml"/>
"#, i + 1, i + 1
        ));
    }

    opf_content.push_str(
        r#"    </manifest>
    <spine toc="ncx">
"#,
    );

    // Add chapter items to spine
    for i in 0..config.chapter_count {
        opf_content.push_str(&format!(
            r#"        <itemref idref="chapter{}"/>
"#,
            i + 1
        ));
    }

    opf_content.push_str(
        r#"    </spine>
</package>"#,
    );

    zip.start_file("OEBPS/content.opf", FileOptions::default())?;
    zip.write_all(opf_content.as_bytes())?;

    // OEBPS/toc.ncx
    let mut ncx_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
    <head>
        <meta name="dtb:uid" content="fake-book-{}"/>
        <meta name="dtb:depth" content="1"/>
        <meta name="dtb:totalPageCount" content="0"/>
        <meta name="dtb:maxPageNumber" content="0"/>
    </head>
    <docTitle>
        <text>{}</text>
    </docTitle>
    <navMap>
"#,
        config.title.to_lowercase().replace(' ', "-"),
        config.title
    );

    // Add TOC entries
    for i in 0..config.chapter_count {
        ncx_content.push_str(&format!(
            r#"        <navPoint id="navpoint{}" playOrder="{}">
            <navLabel>
                <text>Chapter {}</text>
            </navLabel>
            <content src="chapter{}.xhtml"/>
        </navPoint>
"#,
            i + 1,
            i + 1,
            i + 1,
            i + 1
        ));
    }

    ncx_content.push_str(
        r#"    </navMap>
</ncx>"#,
    );

    zip.start_file("OEBPS/toc.ncx", FileOptions::default())?;
    zip.write_all(ncx_content.as_bytes())?;

    // Generate chapter files
    for i in 0..config.chapter_count {
        let chapter_content = generate_chapter_content(i + 1, config.words_per_chapter);
        zip.start_file(
            format!("OEBPS/chapter{}.xhtml", i + 1),
            FileOptions::default(),
        )?;
        zip.write_all(chapter_content.as_bytes())?;
    }

    zip.finish()?;
    Ok(())
}

/// Generate predictable chapter content
fn generate_chapter_content(chapter_num: usize, word_count: usize) -> String {
    let mut content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
    <title>Chapter {chapter_num}</title>
</head>
<body>
    <h1>Chapter {chapter_num}: Test Chapter Title</h1>
"#
    );

    // Generate predictable text content
    let base_words = [
        "Lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "adipiscing",
        "elit",
        "sed",
        "do",
        "eiusmod",
        "tempor",
        "incididunt",
        "ut",
        "labore",
        "et",
        "dolore",
        "magna",
        "aliqua",
        "Ut",
        "enim",
        "ad",
        "minim",
        "veniam",
        "quis",
        "nostrud",
        "exercitation",
        "ullamco",
        "laboris",
        "nisi",
        "ut",
        "aliquip",
        "ex",
        "ea",
        "commodo",
        "consequat",
        "Duis",
        "aute",
        "irure",
        "dolor",
        "in",
        "reprehenderit",
        "in",
        "voluptate",
        "velit",
        "esse",
        "cillum",
        "dolore",
        "eu",
        "fugiat",
    ];

    content.push_str("    <p>");
    for i in 0..word_count {
        if i > 0 && i % 50 == 0 {
            content.push_str("</p>\n    <p>");
        }
        let word_index = (i + chapter_num * 17) % base_words.len(); // Predictable but varied
        content.push_str(base_words[word_index]);
        if i < word_count - 1 {
            content.push(' ');
        }
    }
    content.push_str("</p>\n</body>\n</html>");

    content
}

/// Create a single-chapter EPUB whose body is one `<pre>` block of `num_lines`
/// contiguous, zero-padded numbered lines (`L0000`, `L0001`, ...). The lines
/// render one-to-one with no blank separators or wrapping, which makes them
/// useful for tests that reason about exact rendered-line positions (e.g. the
/// dual-column page-grid layout, where the seam between pages is only visible
/// when consecutive line numbers are easy to spot).
pub fn create_numbered_lines_epub<P: AsRef<Path>>(
    path: P,
    num_lines: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use zip::write::FileOptions;

    let file = fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);

    zip.start_file(
        "mimetype",
        FileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    zip.write_all(b"application/epub+zip")?;

    zip.start_file("META-INF/container.xml", FileOptions::default())?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
</container>"#,
    )?;

    zip.start_file("OEBPS/content.opf", FileOptions::default())?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
    <metadata>
        <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Numbered Lines</dc:title>
        <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="BookId">numbered-lines</dc:identifier>
        <dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language>
    </metadata>
    <manifest>
        <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
        <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    </manifest>
    <spine toc="ncx">
        <itemref idref="chapter1"/>
    </spine>
</package>"#,
    )?;

    zip.start_file("OEBPS/toc.ncx", FileOptions::default())?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
    <head>
        <meta name="dtb:uid" content="numbered-lines"/>
        <meta name="dtb:depth" content="1"/>
    </head>
    <docTitle><text>Numbered Lines</text></docTitle>
    <navMap>
        <navPoint id="navpoint1" playOrder="1">
            <navLabel><text>Chapter 1</text></navLabel>
            <content src="chapter1.xhtml"/>
        </navPoint>
    </navMap>
</ncx>"#,
    )?;

    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Chapter 1</title></head>\
         <body><pre>",
    );
    for i in 0..num_lines {
        body.push_str(&format!("L{i:04}\n"));
    }
    body.push_str("</pre></body></html>");

    zip.start_file("OEBPS/chapter1.xhtml", FileOptions::default())?;
    zip.write_all(body.as_bytes())?;

    zip.finish()?;
    Ok(())
}

/// Create custom test books in a directory with specified configurations
pub fn create_custom_test_books_in_dir<P: AsRef<Path>>(
    dir: P,
    configs: &[FakeBookConfig],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;

    let mut paths = Vec::new();

    for (i, config) in configs.iter().enumerate() {
        let filename = format!("fake_book_{}.epub", i + 1);
        let path = dir.join(&filename);
        create_fake_epub_file(&path, config)?;
        paths.push(filename);
    }

    Ok(paths)
}

/// Create standard test books in a directory (for backward compatibility)
pub fn create_test_books_in_dir<P: AsRef<Path>>(
    dir: P,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let configs = vec![
        FakeBookConfig {
            title: "Digital Frontier".to_string(),
            chapter_count: 33,
            words_per_chapter: 150,
        },
        FakeBookConfig {
            title: "Seven Chapter Book".to_string(),
            chapter_count: 7,
            words_per_chapter: 200,
        },
    ];

    create_custom_test_books_in_dir(dir, &configs)
}
