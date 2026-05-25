use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use regex::Regex;
use serde::Serialize;

use bookokrat::markdown::{Block, Document, Node, Text, TextOrInline};
use bookokrat::parsing::html_to_markdown::HtmlToMarkdownConverter;
use bookokrat::parsing::markdown_renderer::MarkdownRenderer;
use bookokrat::parsing::toc_parser::TocParser;
use bookokrat::widget::navigation_panel::table_of_contents::TocItem;

#[derive(Serialize)]
struct BookInfo {
    title: Option<String>,
    creator: Option<String>,
    language: Option<String>,
    date: Option<String>,
    publisher: Option<String>,
    description: Option<String>,
    identifier: Option<String>,
    chapter_count: usize,
}

#[derive(Serialize)]
struct TocEntryOut {
    /// 1-indexed spine chapter number this entry points at, or `null` if the
    /// ToC entry could not be matched to a spine chapter.
    chapter_index: Option<usize>,
    title: String,
    level: usize,
    /// Fragment id within the chapter, if the ToC entry references one.
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor: Option<String>,
}

pub fn cmd_extract(file: &str, output: &str) -> Result<()> {
    if !Path::new(file).exists() {
        eprintln!("Error: file not found: {file}");
        std::process::exit(1);
    }
    match bookokrat::book_manager::BookManager::detect_format(file) {
        Some(bookokrat::book_manager::BookFormat::Epub) => {}
        Some(_) => {
            eprintln!("Error: extract currently supports only EPUB files");
            std::process::exit(1);
        }
        None => {
            eprintln!("Error: unsupported file format: {file}");
            std::process::exit(1);
        }
    }

    let output_dir = PathBuf::from(output);
    let chapters_dir = output_dir.join("chapters");
    let images_dir = output_dir.join("Images");
    fs::create_dir_all(&chapters_dir)
        .with_context(|| format!("Failed to create chapters dir: {chapters_dir:?}"))?;
    fs::create_dir_all(&images_dir)
        .with_context(|| format!("Failed to create images dir: {images_dir:?}"))?;

    let data = fs::read(file).with_context(|| format!("Failed to read EPUB: {file}"))?;
    let mut doc = EpubDoc::from_reader(Cursor::new(data))
        .map_err(|e| anyhow::anyhow!("Failed to open EPUB: {e}"))?;

    let num_chapters = doc.get_num_chapters();

    // Extract images: full epub-internal resource path -> chosen flat basename
    // under <output>/Images/. Collisions get _N suffixes so refs stay unique.
    let mut resource_to_target: HashMap<PathBuf, String> = HashMap::new();
    {
        let resources = doc.resources.clone();
        let mut used_names: HashSet<String> = HashSet::new();
        // Sort for stable collision ordering across runs.
        let mut sorted: Vec<_> = resources.iter().collect();
        sorted.sort_by(|a, b| a.1.path.cmp(&b.1.path));
        for (id, res) in sorted {
            if !is_image_mime(&res.mime) {
                continue;
            }
            let basename = res
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("img_{id}"));
            let target = uniquify(&basename, &used_names);
            used_names.insert(target.clone());

            let Some((bytes, _mime)) = doc.get_resource(id) else {
                eprintln!("Warning: failed to read image resource: {id}");
                continue;
            };
            let out_path = images_dir.join(&target);
            fs::write(&out_path, &bytes)
                .with_context(|| format!("Failed to write image: {out_path:?}"))?;
            resource_to_target.insert(res.path.clone(), target);
        }
    }

    // Resolve ToC entries -> spine chapter index and flatten with levels.
    let toc_items = TocParser::parse_toc_structure(&doc);
    let mut toc_out: Vec<TocEntryOut> = Vec::new();
    // chapter index (0-based) -> first ToC title seen for that chapter
    let mut chapter_titles_from_toc: HashMap<usize, String> = HashMap::new();
    flatten_toc(&toc_items, 0, &mut |entry| {
        let chapter_index = entry.href.as_ref().and_then(|h| {
            let resolved = resolve_toc_href(h, &doc.root_file);
            doc.resource_uri_to_chapter(&resolved)
        });
        if let Some(idx) = chapter_index {
            chapter_titles_from_toc
                .entry(idx)
                .or_insert_with(|| entry.title.clone());
        }
        toc_out.push(TocEntryOut {
            chapter_index: chapter_index.map(|i| i + 1),
            title: entry.title.clone(),
            level: entry.level,
            anchor: entry.anchor.clone(),
        });
    });

    // Render chapters.
    let image_ref_re =
        Regex::new(r#"\[image src="([^"]*)"\]"#).context("Failed to compile image ref regex")?;
    let mut converter = HtmlToMarkdownConverter::new();
    let renderer = MarkdownRenderer::new();
    for i in 0..num_chapters {
        doc.set_current_chapter(i);
        let chapter_path = doc.get_current_path();
        let (html, _mime) = doc.get_current_str().unwrap_or_default();
        if html.is_empty() {
            // Still produce a placeholder file so chapter indices stay aligned.
            write_chapter(&chapters_dir, i, &format!("Chapter {}", i + 1), "")?;
            continue;
        }
        let md_doc = converter.convert(&html);
        let body = renderer.render(&md_doc);

        let title = chapter_titles_from_toc
            .get(&i)
            .cloned()
            .or_else(|| first_heading_text(&md_doc, &renderer))
            .unwrap_or_else(|| format!("Chapter {}", i + 1));

        let body_rewritten = rewrite_image_refs(
            &body,
            chapter_path.as_deref(),
            &resource_to_target,
            &image_ref_re,
        );

        write_chapter(&chapters_dir, i, &title, body_rewritten.trim())?;
    }

    // Write metadata.
    let info = BookInfo {
        title: doc.mdata("title").map(|m| m.value.clone()),
        creator: doc.mdata("creator").map(|m| m.value.clone()),
        language: doc.mdata("language").map(|m| m.value.clone()),
        date: doc.mdata("date").map(|m| m.value.clone()),
        publisher: doc.mdata("publisher").map(|m| m.value.clone()),
        description: doc.mdata("description").map(|m| m.value.clone()),
        identifier: doc.mdata("identifier").map(|m| m.value.clone()),
        chapter_count: num_chapters,
    };
    fs::write(
        output_dir.join("info.json"),
        serde_json::to_string_pretty(&info).context("Failed to serialize info.json")?,
    )
    .context("Failed to write info.json")?;

    fs::write(
        output_dir.join("toc.json"),
        serde_json::to_string_pretty(&toc_out).context("Failed to serialize toc.json")?,
    )
    .context("Failed to write toc.json")?;

    println!(
        "Extracted {chapters} chapter(s) and {images} image(s) to {dir}",
        chapters = num_chapters,
        images = resource_to_target.len(),
        dir = output_dir.display()
    );

    Ok(())
}

struct FlatTocEntry {
    title: String,
    href: Option<String>,
    anchor: Option<String>,
    level: usize,
}

fn flatten_toc<F>(items: &[TocItem], level: usize, f: &mut F)
where
    F: FnMut(&FlatTocEntry),
{
    for item in items {
        match item {
            TocItem::Chapter {
                title,
                href,
                anchor,
            } => {
                f(&FlatTocEntry {
                    title: title.clone(),
                    href: Some(href.clone()),
                    anchor: anchor.clone(),
                    level,
                });
            }
            TocItem::Section {
                title,
                href,
                anchor,
                children,
                ..
            } => {
                f(&FlatTocEntry {
                    title: title.clone(),
                    href: href.clone(),
                    anchor: anchor.clone(),
                    level,
                });
                flatten_toc(children, level + 1, f);
            }
        }
    }
}

fn resolve_toc_href(href: &str, root_file: &Path) -> PathBuf {
    // ToC parser already split off the anchor. Hrefs are stored as
    // root_base.join(src) by the epub crate (see get_navpoints), so they are
    // already epub-root-relative paths. Normalize defensively.
    let base = root_file.parent().unwrap_or_else(|| Path::new(""));
    let p = Path::new(href);
    let combined = if p.is_absolute() || p.starts_with(base) {
        p.to_path_buf()
    } else {
        // Should not happen for well-formed NCX, but handle just in case.
        base.join(p)
    };
    normalize_path(&combined)
}

fn first_heading_text(doc: &Document, renderer: &MarkdownRenderer) -> Option<String> {
    for node in &doc.blocks {
        if let Node {
            block: Block::Heading { content, .. },
            ..
        } = node
        {
            let s = renderer.render_text(content);
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        // Some books wrap headings in other constructs; stop at first paragraph
        // so we don't pick a heading buried five sections down.
        if matches!(node.block, Block::Paragraph { .. }) {
            break;
        }
        // Treat lone images as non-blocking — skip and keep looking briefly.
        let _ = std::convert::identity::<&Text>;
        let _ = TextOrInline::Text(Default::default());
    }
    None
}

fn rewrite_image_refs(
    body: &str,
    chapter_path: Option<&Path>,
    resource_to_target: &HashMap<PathBuf, String>,
    re: &Regex,
) -> String {
    re.replace_all(body, |caps: &regex::Captures| {
        let src = &caps[1];
        let resolved = resolve_image_src(src, chapter_path);
        if let Some(name) = resource_to_target.get(&resolved) {
            format!("[image src=\"../Images/{name}\"]")
        } else {
            // Unknown ref — leave the original src so the breakage is visible
            // instead of silently dropping the image.
            caps[0].to_string()
        }
    })
    .into_owned()
}

fn resolve_image_src(src: &str, chapter_path: Option<&Path>) -> PathBuf {
    let src_path = Path::new(src.trim_start_matches('/'));
    let base = chapter_path
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new(""));
    normalize_path(&base.join(src_path))
}

fn normalize_path(p: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                // Pop only normal components; preserve any leading root/prefix.
                if matches!(out.last(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(Component::ParentDir);
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.iter().collect()
}

fn uniquify(base: &str, used: &HashSet<String>) -> String {
    if !used.contains(base) {
        return base.to_string();
    }
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (base.to_string(), String::new()),
    };
    let mut n = 1usize;
    loop {
        let candidate = format!("{stem}_{n}{ext}");
        if !used.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
        || matches!(
            mime,
            "application/x-png" | "application/x-jpg" | "application/x-jpeg"
        )
}

fn write_chapter(chapters_dir: &Path, idx: usize, title: &str, body: &str) -> Result<()> {
    let path = chapters_dir.join(format!("{:04}.md", idx + 1));
    let mut contents = String::with_capacity(body.len() + title.len() + 8);
    contents.push_str("# ");
    contents.push_str(title.trim());
    contents.push_str("\n\n");
    contents.push_str(body);
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    fs::write(&path, contents).with_context(|| format!("Failed to write chapter: {path:?}"))?;
    Ok(())
}
