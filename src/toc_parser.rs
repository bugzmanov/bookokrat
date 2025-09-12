use crate::table_of_contents::TocItem;
use epub::doc::EpubDoc;
use log::debug;
use regex::Regex;
use std::io::BufReader;

pub struct TocParser;

impl TocParser {
    pub fn new() -> Self {
        Self
    }

    /// Split href into path and anchor components
    fn split_href_and_anchor(&self, href: &str) -> (String, Option<String>) {
        if let Some(hash_pos) = href.find('#') {
            let path = href[..hash_pos].to_string();
            let anchor = href[hash_pos + 1..].to_string();
            (path, Some(anchor))
        } else {
            (href.to_string(), None)
        }
    }

    /// Parse EPUB table of contents to get hierarchical structure
    pub fn parse_toc_structure(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Vec<TocItem> {
        // Try NCX document first (EPUB2 style) - more reliable parsing
        if let Some(ncx_id) = self.find_ncx_document(doc) {
            debug!("Found NCX document: {}", ncx_id);
            if let Ok(ncx_content) = doc.get_resource_str(&ncx_id) {
                debug!("NCX content length: {} chars", ncx_content.len());
                let ncx_entries = self.parse_ncx_document(&ncx_content);
                debug!("NCX parsing returned {} entries", ncx_entries.len());
                if !ncx_entries.is_empty() {
                    return ncx_entries;
                }
            }
        }

        // Fallback to navigation document (EPUB3 style)
        if let Some(nav_id) = self.find_nav_document(doc) {
            if let Ok(nav_content) = doc.get_resource_str(&nav_id) {
                return self.parse_nav_document(&nav_content);
            }
        }

        // If no TOC found, return empty structure
        Vec::new()
    }

    /// Find NCX document ID (EPUB2)
    fn find_ncx_document(&self, doc: &EpubDoc<BufReader<std::fs::File>>) -> Option<String> {
        for (id, (path, mimetype)) in &doc.resources {
            if mimetype.contains("application/x-dtbncx+xml")
                || path.to_string_lossy().ends_with(".ncx")
            {
                return Some(id.clone());
            }
        }
        None
    }

    /// Find navigation document ID (EPUB3)
    fn find_nav_document(&self, doc: &EpubDoc<BufReader<std::fs::File>>) -> Option<String> {
        for (id, (path, mimetype)) in &doc.resources {
            if mimetype.contains("application/xhtml+xml")
                && (path.to_string_lossy().contains("nav")
                    || path.to_string_lossy().contains("toc"))
            {
                return Some(id.clone());
            }
        }
        None
    }

    /// Parse NCX document (EPUB2)
    fn parse_ncx_document(&self, content: &str) -> Vec<TocItem> {
        let mut entries = Vec::new();

        // Parse navMap structure with multiline support
        if let Ok(navmap_regex) = Regex::new(r#"(?s)<navMap[^>]*>(.*?)</navMap>"#) {
            if let Some(captures) = navmap_regex.captures(content) {
                if let Some(navmap_content) = captures.get(1) {
                    debug!(
                        "Found navMap content, length: {} chars",
                        navmap_content.as_str().len()
                    );
                    entries.extend(self.parse_ncx_nav_points(navmap_content.as_str()));
                    debug!("Parsed {} entries from navMap", entries.len());
                } else {
                    debug!("navMap regex matched but no content captured");
                }
            } else {
                debug!("navMap regex did not match");
            }
        } else {
            debug!("Failed to compile navMap regex");
        }

        entries
    }

    /// Parse navigation document using DOM structure without hardcoded element names or classes
    fn parse_nav_document(&self, content: &str) -> Vec<TocItem> {
        // Parse based on DOM structure: any element containing <a> with potential nested elements
        self.parse_hierarchical_links(content)
    }

    /// Parse hierarchical link structure using DOM nesting patterns
    /// Pattern: <element><a href="...">title</a><nested-elements>...</nested-elements></element>
    fn parse_hierarchical_links(&self, content: &str) -> Vec<TocItem> {
        let mut entries = Vec::new();

        // Extract content from body if present
        let working_content = if let Ok(body_regex) = Regex::new(r#"(?s)<body[^>]*>(.*?)</body>"#) {
            body_regex
                .captures(content)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str())
                .unwrap_or(content)
        } else {
            content
        };

        // Use multiple regex patterns to match common HTML elements that can contain navigation links
        // This approach avoids the backreference issue and supports various HTML structures
        let element_patterns = [
            (r#"(?s)<section[^>]*>(.*?)</section>"#, "section"),
            (r#"(?s)<article[^>]*>(.*?)</article>"#, "article"),
            (r#"(?s)<div[^>]*>(.*?)</div>"#, "div"),
            (r#"(?s)<p[^>]*>(.*?)</p>"#, "p"),
            (r#"(?s)<span[^>]*>(.*?)</span>"#, "span"),
            (r#"(?s)<li[^>]*>(.*?)</li>"#, "li"),
        ];

        let mut potential_entries = Vec::new();

        for (pattern, tag_name) in &element_patterns {
            if let Ok(element_regex) = Regex::new(pattern) {
                let matches: Vec<_> = element_regex.find_iter(working_content).collect();

                // Process each element match
                for element_match in matches.iter() {
                    if let Some(captures) = element_regex.captures(element_match.as_str()) {
                        let element_name = tag_name; // We know the tag name from the pattern
                        let element_content = captures.get(1).unwrap().as_str(); // Content is first capture group
                        let element_start = element_match.start();
                        let element_end = element_match.end();

                        // Look for <a> tag anywhere in this element's content
                        if let Ok(link_regex) =
                            Regex::new(r#"<a[^>]*href="([^"]*)"[^>]*>([^<]+)</a>"#)
                        {
                            if let Some(link_captures) = link_regex.captures(element_content) {
                                let href = link_captures.get(1).unwrap().as_str().to_string();
                                let title =
                                    link_captures.get(2).unwrap().as_str().trim().to_string();
                                let link_start =
                                    element_start + link_captures.get(0).unwrap().start();
                                let link_end = element_start + link_captures.get(0).unwrap().end();

                                potential_entries.push((
                                    element_start,
                                    element_end,
                                    link_start,
                                    link_end,
                                    element_name.to_string(),
                                    title,
                                    href,
                                    element_content.to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Second pass: determine which entries are top-level vs nested based on DOM hierarchy
        for (
            i,
            (start, end, _link_start, _link_end, _element_name, title, href, _element_content),
        ) in potential_entries.iter().enumerate()
        {
            // Check if this element is nested within another element from our list
            let is_nested = potential_entries.iter().enumerate().any(
                |(j, (other_start, other_end, _, _, _, _, _, _))| {
                    i != j && start > other_start && end < other_end
                },
            );

            if !is_nested {
                // This is a top-level entry, now find its children
                let mut children = Vec::new();

                // Look for nested entries within this element
                for (j, (child_start, child_end, _, _, _, child_title, child_href, _)) in
                    potential_entries.iter().enumerate()
                {
                    if i != j && child_start > start && child_end < end {
                        // This is a child of the current entry
                        // Extract anchor from href if present
                        let (clean_href, anchor) = self.split_href_and_anchor(child_href);
                        // For nav documents, we'll create chapters without index (will be assigned later)
                        children.push(TocItem::Chapter {
                            title: child_title.clone(),
                            href: clean_href,
                            index: 0, // Will be mapped later when converting
                            anchor,
                        });
                    }
                }

                if children.is_empty() {
                    // No children, create a Chapter
                    let (clean_href, anchor) = self.split_href_and_anchor(href);
                    entries.push(TocItem::Chapter {
                        title: title.clone(),
                        href: clean_href,
                        index: 0, // Will be mapped later
                        anchor,
                    });
                } else {
                    // Has children, create a Section
                    let (clean_href, anchor) = self.split_href_and_anchor(href);
                    entries.push(TocItem::Section {
                        title: title.clone(),
                        href: Some(clean_href),
                        index: None, // Will be mapped later
                        anchor,
                        children,
                        is_expanded: true,
                    });
                }
            }
        }

        entries
    }

    /// Parse NCX navigation points
    fn parse_ncx_nav_points(&self, content: &str) -> Vec<TocItem> {
        let mut entries = Vec::new();

        // Find top-level navPoint elements (not nested within other navPoints)
        let mut current_pos = 0;
        while let Some(start) = content[current_pos..].find("<navPoint") {
            let absolute_start = current_pos + start;

            if let Some(navpoint_content) =
                self.extract_complete_navpoint(&content[absolute_start..])
            {
                if let Some(entry) = self.parse_single_ncx_navpoint(&navpoint_content) {
                    entries.push(entry);
                }
                current_pos = absolute_start + navpoint_content.len();
            } else {
                current_pos = absolute_start + 1;
            }
        }

        entries
    }

    /// Extract a complete navPoint element including all nested navPoints
    fn extract_complete_navpoint(&self, content: &str) -> Option<String> {
        if !content.starts_with("<navPoint") {
            return None;
        }

        let mut depth = 0;
        let mut pos = 0;
        let mut in_tag = false;
        let mut current_tag = String::new();

        for ch in content.chars() {
            match ch {
                '<' => {
                    in_tag = true;
                    current_tag.clear();
                    current_tag.push(ch);
                }
                '>' => {
                    if in_tag {
                        current_tag.push(ch);

                        if current_tag.starts_with("<navPoint") {
                            depth += 1;
                        } else if current_tag == "</navPoint>" {
                            depth -= 1;
                            if depth == 0 {
                                return Some(content[..pos + 1].to_string());
                            }
                        }

                        in_tag = false;
                    }
                }
                _ => {
                    if in_tag {
                        current_tag.push(ch);
                    }
                }
            }
            pos += ch.len_utf8();
        }

        None
    }

    /// Parse single NCX navigation point with proper nesting
    fn parse_single_ncx_navpoint(&self, content: &str) -> Option<TocItem> {
        debug!(
            "Parsing navPoint content ({}chars): {}...",
            content.len(),
            content.chars().take(200).collect::<String>()
        );

        // Extract navLabel and content with multiline support
        let title = if let Ok(label_regex) =
            Regex::new(r#"(?s)<navLabel[^>]*>.*?<text[^>]*>([^<]+)</text>.*?</navLabel>"#)
        {
            let captured_text = label_regex.captures(content)?.get(1)?.as_str();
            captured_text.trim().to_string()
        } else {
            debug!("Failed to extract title from navPoint");
            return None;
        };

        let href =
            if let Ok(content_regex) = Regex::new(r#"<content[^>]*src=["']([^"']*)["'][^>]*/?>"#) {
                content_regex
                    .captures(content)?
                    .get(1)?
                    .as_str()
                    .to_string()
            } else {
                return None;
            };

        // Find nested navPoints that are direct children
        let mut children = Vec::new();

        // Look for nested navPoint elements within this navPoint's content
        // We need to find the content after the first navLabel and content tags
        if let Some(label_end) = content.find("</navLabel>") {
            if let Some(content_end) = content[label_end..]
                .find("/>")
                .or_else(|| content[label_end..].find("</content>"))
            {
                let search_start = label_end + content_end;
                let remaining_content = &content[search_start..];

                debug!(
                    "Looking for children in remaining content ({}chars): {}...",
                    remaining_content.len(),
                    remaining_content.chars().take(100).collect::<String>()
                );

                // Parse any nested navPoints in the remaining content
                children.extend(self.parse_ncx_nav_points(remaining_content));
                debug!("Found {} children for '{}'", children.len(), title);
            }
        }

        if children.is_empty() {
            // No children, create a Chapter
            let (clean_href, anchor) = self.split_href_and_anchor(&href);
            Some(TocItem::Chapter {
                title,
                href: clean_href,
                index: 0, // Will be mapped later
                anchor,
            })
        } else {
            // Has children, create a Section
            let (clean_href, anchor) = self.split_href_and_anchor(&href);
            Some(TocItem::Section {
                title,
                href: Some(clean_href),
                index: None, // Will be mapped later
                anchor,
                children,
                is_expanded: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ncx_parsing_with_hierarchy() {
        let parser = TocParser::new();

        // Sample NCX content with proper nesting structure
        let ncx_content = r#"<?xml version="1.0" encoding="utf-8" ?>
<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">
<ncx version="2005-1" xmlns="http://www.daisy.org/z3986/2005/ncx/">
  <navMap>
    <navPoint id="navPoint-1" playOrder="1">
      <navLabel>
        <text>Главное за пять минут</text>
      </navLabel>
      <content src="Text/content2.html"/>
    </navPoint>
    <navPoint id="navPoint-2" playOrder="2">
      <navLabel>
        <text>Контекст</text>
      </navLabel>
      <content src="Text/Section0002.html"/>
      <navPoint id="navPoint-3" playOrder="3">
        <navLabel>
          <text>Как влияет контекст</text>
        </navLabel>
        <content src="Text/content9.html"/>
      </navPoint>
      <navPoint id="navPoint-4" playOrder="4">
        <navLabel>
          <text>Как это выглядит</text>
        </navLabel>
        <content src="Text/content11.html"/>
      </navPoint>
    </navPoint>
    <navPoint id="navPoint-5" playOrder="5">
      <navLabel>
        <text>Интерес</text>
      </navLabel>
      <content src="Text/content21.html"/>
      <navPoint id="navPoint-6" playOrder="6">
        <navLabel>
          <text>Великое школьное искажение</text>
        </navLabel>
        <content src="Text/content23.html"/>
      </navPoint>
    </navPoint>
  </navMap>
</ncx>"#;

        let entries = parser.parse_ncx_document(ncx_content);

        // Should have 3 top-level entries
        assert_eq!(entries.len(), 3);

        // First entry: standalone chapter
        match &entries[0] {
            TocItem::Chapter { title, href, .. } => {
                assert_eq!(title, "Главное за пять минут");
                assert_eq!(href, "Text/content2.html");
            }
            _ => panic!("Expected first entry to be a Chapter"),
        }

        // Second entry: section with children
        match &entries[1] {
            TocItem::Section {
                title,
                href,
                children,
                ..
            } => {
                assert_eq!(title, "Контекст");
                assert_eq!(href, &Some("Text/Section0002.html".to_string()));
                assert_eq!(children.len(), 2);

                match &children[0] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Как влияет контекст");
                        assert_eq!(href, "Text/content9.html");
                    }
                    _ => panic!("Expected first child to be a Chapter"),
                }

                match &children[1] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Как это выглядит");
                        assert_eq!(href, "Text/content11.html");
                    }
                    _ => panic!("Expected second child to be a Chapter"),
                }
            }
            _ => panic!("Expected second entry to be a Section"),
        }

        // Third entry: another section with children
        match &entries[2] {
            TocItem::Section {
                title,
                href,
                children,
                ..
            } => {
                assert_eq!(title, "Интерес");
                assert_eq!(href, &Some("Text/content21.html".to_string()));
                assert_eq!(children.len(), 1);

                match &children[0] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Великое школьное искажение");
                        assert_eq!(href, "Text/content23.html");
                    }
                    _ => panic!("Expected child to be a Chapter"),
                }
            }
            _ => panic!("Expected third entry to be a Section"),
        }
    }

    #[test]
    fn test_nav_document_parsing_without_hardcoded_classes() {
        let parser = TocParser::new();

        // Sample navigation content with different element types (no hardcoded class names)
        // This structure shows proper nesting: parent elements contain child elements
        let nav_content = r#"<body>
  <section>
    <a href="../Text/content2.html">Главное за пять минут</a>
  </section>

  <article>
    <a href="../Text/Section0002.html">Контекст</a>
    <div>
      <a href="../Text/content9.html">Как влияет контекст</a>
    </div>
    <p>
      <a href="../Text/content11.html">Как это выглядит</a>
    </p>
  </article>

  <div>
    <a href="../Text/content21.html">Интерес</a>
    <span>
      <a href="../Text/content23.html">Великое школьное искажение</a>
    </span>
  </div>
</body>"#;

        let entries = parser.parse_nav_document(nav_content);

        // Debug output to see what was parsed
        println!("Parsed {} entries:", entries.len());
        for (i, entry) in entries.iter().enumerate() {
            match entry {
                TocItem::Chapter { title, href, .. } => {
                    println!("  {}: Chapter '{}' -> {}", i, title, href);
                }
                TocItem::Section {
                    title,
                    href,
                    children,
                    ..
                } => {
                    let href_str = href.as_ref().map(|h| h.as_str()).unwrap_or("None");
                    println!(
                        "  {}: Section '{}' -> {} (children: {})",
                        i,
                        title,
                        href_str,
                        children.len()
                    );
                    for (j, child) in children.iter().enumerate() {
                        match child {
                            TocItem::Chapter { title, href, .. } => {
                                println!("    {}.{}: Chapter '{}' -> {}", i, j, title, href);
                            }
                            TocItem::Section { title, .. } => {
                                println!("    {}.{}: Section '{}'", i, j, title);
                            }
                        }
                    }
                }
            }
        }

        // Should find 3 top-level entries (section, article, div)
        assert_eq!(
            entries.len(),
            3,
            "Should find exactly 3 top-level entries, found: {}",
            entries.len()
        );

        // First entry: standalone chapter (no children)
        match &entries[0] {
            TocItem::Chapter { title, href, .. } => {
                assert_eq!(title, "Главное за пять минут");
                assert_eq!(href, "../Text/content2.html");
            }
            _ => panic!("Expected first entry to be a Chapter"),
        }

        // Second entry: section with nested chapters
        match &entries[1] {
            TocItem::Section {
                title,
                href,
                children,
                ..
            } => {
                assert_eq!(title, "Контекст");
                assert_eq!(href, &Some("../Text/Section0002.html".to_string()));
                assert_eq!(children.len(), 2, "Контекст section should have 2 children");

                // Validate nested chapters under Контекст
                match &children[0] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Как влияет контекст");
                        assert_eq!(href, "../Text/content9.html");
                    }
                    _ => panic!("Expected first child to be a Chapter"),
                }
                match &children[1] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Как это выглядит");
                        assert_eq!(href, "../Text/content11.html");
                    }
                    _ => panic!("Expected second child to be a Chapter"),
                }
            }
            _ => panic!("Expected second entry to be a Section"),
        }

        // Third entry: another section with nested chapter
        match &entries[2] {
            TocItem::Section {
                title,
                href,
                children,
                ..
            } => {
                assert_eq!(title, "Интерес");
                assert_eq!(href, &Some("../Text/content21.html".to_string()));
                assert_eq!(children.len(), 1, "Интерес section should have 1 child");

                // Validate nested chapter under Интерес
                match &children[0] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Великое школьное искажение");
                        assert_eq!(href, "../Text/content23.html");
                    }
                    _ => panic!("Expected child to be a Chapter"),
                }
            }
            _ => panic!("Expected third entry to be a Section"),
        }
    }

    #[test]
    fn test_careless_flat_ncx_structure() {
        let parser = TocParser::new();

        // Sample NCX content from careless.epub - flat structure with no nesting
        let ncx_content = r#"<?xml version="1.0" encoding="UTF-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1" xml:lang="en-US">
<head>
<meta name="dtb:uid" content="9781250391247"/>
<meta name="dtb:depth" content="1"/>
<meta name="dtb:totalPageCount" content="400"/>
<meta name="dtb:maxPageNumber" content="0"/>
</head>
<docTitle><text>Careless People</text></docTitle>
<docAuthor><text>Sarah Wynn-Williams</text></docAuthor>
<navMap>
<navPoint class="other" id="navpoint-1" playOrder="1"><navLabel><text>Cover</text></navLabel><content src="xhtml/cover.xhtml"/></navPoint>
<navPoint class="other" id="navpoint-2" playOrder="2"><navLabel><text>Title Page</text></navLabel><content src="xhtml/title.xhtml#tit"/></navPoint>
<navPoint class="other" id="navpoint-3" playOrder="3"><navLabel><text>Copyright Notice</text></navLabel><content src="xhtml/copyrightnotice.xhtml"/></navPoint>
<navPoint class="other" id="navpoint-4" playOrder="4"><navLabel><text>Dedication</text></navLabel><content src="xhtml/dedication.xhtml#ded"/></navPoint>
<navPoint class="other" id="navpoint-5" playOrder="5"><navLabel><text>Epigraph</text></navLabel><content src="xhtml/epigraph.xhtml#epi"/></navPoint>
<navPoint class="other" id="navpoint-6" playOrder="6"><navLabel><text>Prologue</text></navLabel><content src="xhtml/prologue.xhtml#pro"/></navPoint>
<navPoint class="other" id="navpoint-7" playOrder="7"><navLabel><text>1. Simpleminded Hope</text></navLabel><content src="xhtml/chapter1.xhtml#ch1"/></navPoint>
<navPoint class="other" id="navpoint-8" playOrder="8"><navLabel><text>2. Pitching the Revolution</text></navLabel><content src="xhtml/chapter2.xhtml#ch2"/></navPoint>
<navPoint class="other" id="navpoint-9" playOrder="9"><navLabel><text>3. This Is Going to Be Fun</text></navLabel><content src="xhtml/chapter3.xhtml#ch3"/></navPoint>
<navPoint class="other" id="navpoint-10" playOrder="10"><navLabel><text>4. Auf Wiedersehen to All That</text></navLabel><content src="xhtml/chapter4.xhtml#ch4"/></navPoint>
<navPoint class="other" id="navpoint-11" playOrder="11"><navLabel><text>5. The Little Red Book</text></navLabel><content src="xhtml/chapter5.xhtml#ch5"/></navPoint>
</navMap>
</ncx>"#;

        let entries = parser.parse_ncx_document(ncx_content);

        // Should have 11 flat entries (no hierarchy)
        assert_eq!(entries.len(), 11);

        // All entries should be chapters (flat structure)
        for entry in &entries {
            match entry {
                TocItem::Chapter { .. } => {
                    // Good, it's a chapter
                }
                TocItem::Section { title, .. } => {
                    panic!(
                        "Entry '{}' should be a Chapter in flat structure, not a Section",
                        title
                    );
                }
            }
        }

        // Verify specific entries
        match &entries[0] {
            TocItem::Chapter { title, href, .. } => {
                assert_eq!(title, "Cover");
                assert_eq!(href, "xhtml/cover.xhtml");
            }
            _ => panic!("Entry should be a Chapter"),
        }

        match &entries[1] {
            TocItem::Chapter {
                title,
                href,
                anchor,
                ..
            } => {
                assert_eq!(title, "Title Page");
                assert_eq!(href, "xhtml/title.xhtml");
                assert_eq!(anchor, &Some("tit".to_string()));
            }
            _ => panic!("Entry should be a Chapter"),
        }

        match &entries[2] {
            TocItem::Chapter { title, href, .. } => {
                assert_eq!(title, "Copyright Notice");
                assert_eq!(href, "xhtml/copyrightnotice.xhtml");
            }
            _ => panic!("Entry should be a Chapter"),
        }

        match &entries[6] {
            TocItem::Chapter {
                title,
                href,
                anchor,
                ..
            } => {
                assert_eq!(title, "1. Simpleminded Hope");
                assert_eq!(href, "xhtml/chapter1.xhtml");
                assert_eq!(anchor, &Some("ch1".to_string()));
            }
            _ => panic!("Entry should be a Chapter"),
        }

        match &entries[7] {
            TocItem::Chapter {
                title,
                href,
                anchor,
                ..
            } => {
                assert_eq!(title, "2. Pitching the Revolution");
                assert_eq!(href, "xhtml/chapter2.xhtml");
                assert_eq!(anchor, &Some("ch2".to_string()));
            }
            _ => panic!("Entry should be a Chapter"),
        }

        match &entries[10] {
            TocItem::Chapter {
                title,
                href,
                anchor,
                ..
            } => {
                assert_eq!(title, "5. The Little Red Book");
                assert_eq!(href, "xhtml/chapter5.xhtml");
                assert_eq!(anchor, &Some("ch5".to_string()));
            }
            _ => panic!("Entry should be a Chapter"),
        }
    }

    #[test]
    fn test_empty_content() {
        let parser = TocParser::new();

        let empty_ncx = r#"<?xml version="1.0" encoding="utf-8" ?>
<ncx version="2005-1">
  <navMap>
  </navMap>
</ncx>"#;

        let entries = parser.parse_ncx_document(empty_ncx);
        assert_eq!(entries.len(), 0);

        let empty_nav = "<body></body>";
        let entries = parser.parse_nav_document(empty_nav);
        assert_eq!(entries.len(), 0);
    }
}
