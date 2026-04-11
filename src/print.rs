use std::io::Cursor;
use std::path::Path;

use anyhow::Result;
use epub::doc::EpubDoc;

use bookokrat::book_manager::BookManager;
use bookokrat::parsing::html_to_markdown::HtmlToMarkdownConverter;
use bookokrat::parsing::markdown_renderer::MarkdownRenderer;
use bookokrat::parsing::toc_parser::TocParser;
use bookokrat::widget::navigation_panel::table_of_contents::TocItem;

pub fn cmd_print(
    file: &str,
    toc: bool,
    info: bool,
    chapter: Option<usize>,
    pages: Option<usize>,
) -> Result<()> {
    if !Path::new(file).exists() {
        eprintln!("Error: file not found: {file}");
        std::process::exit(1);
    }

    let format = BookManager::detect_format(file);
    if format.is_none() {
        eprintln!("Error: unsupported file format: {file}");
        std::process::exit(1);
    }

    if toc {
        cmd_print_toc(file, &format)
    } else if info {
        cmd_print_info(file, &format)
    } else {
        cmd_print_content(file, &format, chapter, pages)
    }
}

fn cmd_print_toc(file: &str, format: &Option<bookokrat::book_manager::BookFormat>) -> Result<()> {
    match format {
        Some(bookokrat::book_manager::BookFormat::Epub) => cmd_print_toc_epub(file),
        #[cfg(feature = "pdf")]
        Some(bookokrat::book_manager::BookFormat::Pdf) => cmd_print_toc_pdf(file),
        _ => {
            eprintln!("Error: TOC not supported for this format");
            std::process::exit(1);
        }
    }
}

fn cmd_print_toc_epub(file: &str) -> Result<()> {
    let doc = EpubDoc::new(file).map_err(|e| anyhow::anyhow!("Failed to open EPUB: {e}"))?;
    let toc = TocParser::parse_toc_structure(&doc);
    if toc.is_empty() {
        println!("No table of contents found.");
        return Ok(());
    }
    print_toc_items(&toc, 0);
    Ok(())
}

fn print_toc_items(items: &[TocItem], depth: usize) {
    let indent = "  ".repeat(depth);
    for item in items {
        println!("{}{}", indent, item.title());
        if let TocItem::Section { children, .. } = item {
            print_toc_items(children, depth + 1);
        }
    }
}

#[cfg(feature = "pdf")]
fn cmd_print_toc_pdf(file: &str) -> Result<()> {
    let doc =
        mupdf::Document::open(file).map_err(|e| anyhow::anyhow!("Failed to open PDF: {e}"))?;
    let page_count = doc
        .page_count()
        .map_err(|e| anyhow::anyhow!("Failed to get page count: {e}"))?;
    let toc = bookokrat::pdf::extract_toc(&doc, page_count as usize);
    if toc.is_empty() {
        println!("No table of contents found.");
        return Ok(());
    }
    for entry in &toc {
        let indent = "  ".repeat(entry.level);
        if let bookokrat::pdf::TocTarget::InternalPage(page) = &entry.target {
            println!("{}{} (p. {})", indent, entry.title, page + 1);
        } else {
            println!("{}{}", indent, entry.title);
        }
    }
    Ok(())
}

fn cmd_print_info(file: &str, format: &Option<bookokrat::book_manager::BookFormat>) -> Result<()> {
    match format {
        Some(bookokrat::book_manager::BookFormat::Epub) => cmd_print_info_epub(file),
        #[cfg(feature = "pdf")]
        Some(bookokrat::book_manager::BookFormat::Pdf) => cmd_print_info_pdf(file),
        _ => {
            eprintln!("Error: info not supported for this format");
            std::process::exit(1);
        }
    }
}

fn cmd_print_info_epub(file: &str) -> Result<()> {
    let doc = EpubDoc::new(file).map_err(|e| anyhow::anyhow!("Failed to open EPUB: {e}"))?;
    let fields = [
        "title",
        "creator",
        "language",
        "publisher",
        "date",
        "description",
        "subject",
        "identifier",
    ];
    for field in &fields {
        if let Some(item) = doc.mdata(field) {
            println!("{}: {}", field, item.value);
        }
    }
    println!("chapters: {}", doc.get_num_chapters());
    Ok(())
}

#[cfg(feature = "pdf")]
fn cmd_print_info_pdf(file: &str) -> Result<()> {
    let doc =
        mupdf::Document::open(file).map_err(|e| anyhow::anyhow!("Failed to open PDF: {e}"))?;
    let fields = [
        ("title", mupdf::MetadataName::Title),
        ("author", mupdf::MetadataName::Author),
        ("subject", mupdf::MetadataName::Subject),
        ("creator", mupdf::MetadataName::Creator),
        ("producer", mupdf::MetadataName::Producer),
        ("creation_date", mupdf::MetadataName::CreationDate),
        ("modification_date", mupdf::MetadataName::ModDate),
    ];
    for (label, key) in &fields {
        if let Ok(value) = doc.metadata(*key) {
            if !value.is_empty() {
                println!("{}: {}", label, value);
            }
        }
    }
    let page_count = doc.page_count().unwrap_or(0);
    println!("pages: {}", page_count);
    Ok(())
}

fn cmd_print_content(
    file: &str,
    format: &Option<bookokrat::book_manager::BookFormat>,
    chapter: Option<usize>,
    pages: Option<usize>,
) -> Result<()> {
    match format {
        Some(bookokrat::book_manager::BookFormat::Epub) => cmd_print_content_epub(file, chapter),
        #[cfg(feature = "pdf")]
        Some(bookokrat::book_manager::BookFormat::Pdf) => cmd_print_content_pdf(file, pages),
        _ => {
            eprintln!("Error: content extraction not supported for this format");
            std::process::exit(1);
        }
    }
}

fn cmd_print_content_epub(file: &str, chapter: Option<usize>) -> Result<()> {
    let data = std::fs::read(file)?;
    let mut doc = EpubDoc::from_reader(Cursor::new(data))
        .map_err(|e| anyhow::anyhow!("Failed to open EPUB: {e}"))?;

    let num_chapters = doc.get_num_chapters();
    let mut converter = HtmlToMarkdownConverter::new();
    let renderer = MarkdownRenderer::new();

    let range = if let Some(ch) = chapter {
        if ch == 0 || ch > num_chapters {
            eprintln!("Error: chapter {ch} out of range, book has {num_chapters} chapters");
            std::process::exit(1);
        }
        (ch - 1)..ch
    } else {
        0..num_chapters
    };

    let mut first = true;
    for i in range {
        doc.set_current_chapter(i);
        if let Some((html, _mime)) = doc.get_current_str() {
            let md_doc = converter.convert(&html);
            let text = renderer.render(&md_doc);
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                if !first {
                    println!("\n---\n");
                }
                println!("{trimmed}");
                first = false;
            }
        }
    }

    Ok(())
}

#[cfg(feature = "pdf")]
fn cmd_print_content_pdf(file: &str, pages: Option<usize>) -> Result<()> {
    let doc =
        mupdf::Document::open(file).map_err(|e| anyhow::anyhow!("Failed to open PDF: {e}"))?;
    let page_count =
        doc.page_count()
            .map_err(|e| anyhow::anyhow!("Failed to get page count: {e}"))? as usize;

    let range = if let Some(pg) = pages {
        if pg == 0 || pg > page_count {
            eprintln!("Error: page {pg} out of range, document has {page_count} pages");
            std::process::exit(1);
        }
        (pg - 1)..pg
    } else {
        0..page_count
    };

    let mut first = true;
    for i in range {
        let page = doc
            .load_page(i as i32)
            .map_err(|e| anyhow::anyhow!("Failed to load page {}: {e}", i + 1))?;
        let text_page = page
            .to_text_page(mupdf::TextPageFlags::empty())
            .map_err(|e| anyhow::anyhow!("Failed to extract text from page {}: {e}", i + 1))?;
        let text = text_page
            .to_text()
            .map_err(|e| anyhow::anyhow!("Failed to render text from page {}: {e}", i + 1))?;
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            if !first {
                println!("\n---\n");
            }
            println!("{trimmed}");
            first = false;
        }
    }

    Ok(())
}
