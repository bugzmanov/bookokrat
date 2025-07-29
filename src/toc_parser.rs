use crate::book_list::TocEntry;
use epub::doc::EpubDoc;
use regex::Regex;
use std::io::BufReader;

pub struct TocParser;

impl TocParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse EPUB table of contents to get hierarchical structure
    pub fn parse_toc_structure(
        &self,
        doc: &mut EpubDoc<BufReader<std::fs::File>>,
    ) -> Vec<TocEntry> {
        // Try NCX document first (EPUB2 style) - more reliable parsing
        if let Some(ncx_id) = self.find_ncx_document(doc) {
            if let Ok(ncx_content) = doc.get_resource_str(&ncx_id) {
                let ncx_entries = self.parse_ncx_document(&ncx_content);
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
    fn parse_ncx_document(&self, content: &str) -> Vec<TocEntry> {
        let mut entries = Vec::new();

        // Parse navMap structure with multiline support
        if let Ok(navmap_regex) = Regex::new(r#"(?s)<navMap[^>]*>(.*?)</navMap>"#) {
            if let Some(captures) = navmap_regex.captures(content) {
                if let Some(navmap_content) = captures.get(1) {
                    entries.extend(self.parse_ncx_nav_points(navmap_content.as_str()));
                }
            }
        }

        entries
    }

    /// Parse navigation document using DOM structure without hardcoded element names or classes
    fn parse_nav_document(&self, content: &str) -> Vec<TocEntry> {
        // Parse based on DOM structure: any element containing <a> with potential nested elements
        self.parse_hierarchical_links(content)
    }

    /// Parse hierarchical link structure using DOM nesting patterns
    /// Pattern: <element><a href="...">title</a><nested-elements>...</nested-elements></element>
    fn parse_hierarchical_links(&self, content: &str) -> Vec<TocEntry> {
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
                        children.push(TocEntry {
                            title: child_title.clone(),
                            href: child_href.clone(),
                            children: Vec::new(), // For now, only support one level of nesting
                        });
                    }
                }

                entries.push(TocEntry {
                    title: title.clone(),
                    href: href.clone(),
                    children,
                });
            }
        }

        entries
    }

    /// Parse NCX navigation points
    fn parse_ncx_nav_points(&self, content: &str) -> Vec<TocEntry> {
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
    fn parse_single_ncx_navpoint(&self, content: &str) -> Option<TocEntry> {
        // Extract navLabel and content with multiline support
        let title = if let Ok(label_regex) =
            Regex::new(r#"(?s)<navLabel[^>]*>.*?<text[^>]*>([^<]+)</text>.*?</navLabel>"#)
        {
            label_regex
                .captures(content)?
                .get(1)?
                .as_str()
                .trim()
                .to_string()
        } else {
            return None;
        };

        let href = if let Ok(content_regex) = Regex::new(r#"<content[^>]*src="([^"]*)"[^>]*/?>"#) {
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

                // Parse any nested navPoints in the remaining content
                children.extend(self.parse_ncx_nav_points(remaining_content));
            }
        }

        Some(TocEntry {
            title,
            href,
            children,
        })
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
        assert_eq!(entries[0].title, "Главное за пять минут");
        assert_eq!(entries[0].href, "Text/content2.html");
        assert_eq!(entries[0].children.len(), 0);

        // Second entry: section with children
        assert_eq!(entries[1].title, "Контекст");
        assert_eq!(entries[1].href, "Text/Section0002.html");
        assert_eq!(entries[1].children.len(), 2);
        assert_eq!(entries[1].children[0].title, "Как влияет контекст");
        assert_eq!(entries[1].children[0].href, "Text/content9.html");
        assert_eq!(entries[1].children[1].title, "Как это выглядит");
        assert_eq!(entries[1].children[1].href, "Text/content11.html");

        // Third entry: another section with children
        assert_eq!(entries[2].title, "Интерес");
        assert_eq!(entries[2].href, "Text/content21.html");
        assert_eq!(entries[2].children.len(), 1);
        assert_eq!(entries[2].children[0].title, "Великое школьное искажение");
        assert_eq!(entries[2].children[0].href, "Text/content23.html");
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
            println!(
                "  {}: {} -> {} (children: {})",
                i,
                entry.title,
                entry.href,
                entry.children.len()
            );
            for (j, child) in entry.children.iter().enumerate() {
                println!("    {}.{}: {} -> {}", i, j, child.title, child.href);
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
        assert_eq!(entries[0].title, "Главное за пять минут");
        assert_eq!(entries[0].href, "../Text/content2.html");
        assert_eq!(
            entries[0].children.len(),
            0,
            "First entry should have no children"
        );

        // Second entry: section with nested chapters
        assert_eq!(entries[1].title, "Контекст");
        assert_eq!(entries[1].href, "../Text/Section0002.html");
        assert_eq!(
            entries[1].children.len(),
            2,
            "Контекст section should have 2 children"
        );

        // Validate nested chapters under Контекст
        assert_eq!(entries[1].children[0].title, "Как влияет контекст");
        assert_eq!(entries[1].children[0].href, "../Text/content9.html");
        assert_eq!(entries[1].children[1].title, "Как это выглядит");
        assert_eq!(entries[1].children[1].href, "../Text/content11.html");

        // Third entry: another section with nested chapter
        assert_eq!(entries[2].title, "Интерес");
        assert_eq!(entries[2].href, "../Text/content21.html");
        assert_eq!(
            entries[2].children.len(),
            1,
            "Интерес section should have 1 child"
        );

        // Validate nested chapter under Интерес
        assert_eq!(entries[2].children[0].title, "Великое школьное искажение");
        assert_eq!(entries[2].children[0].href, "../Text/content23.html");
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
