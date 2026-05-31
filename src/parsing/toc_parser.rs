use crate::parsing::html_to_markdown::fix_html_for_parser;
use crate::table_of_contents::TocItem;
use epub::doc::{EpubDoc, NavPoint};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Node, NodeData, RcDom};
use std::io::{Read, Seek};
use std::rc::Rc;

pub struct TocParser;

// todo all methods needs to be static
impl TocParser {
    /// Split href into path and anchor components
    fn split_href_and_anchor(href: &str) -> (String, Option<String>) {
        if let Some(hash_pos) = href.find('#') {
            let path = href[..hash_pos].to_string();
            let anchor = href[hash_pos + 1..].to_string();
            (path, Some(anchor))
        } else {
            (href.to_string(), None)
        }
    }

    pub fn parse_toc_structure<R: Read + Seek>(doc: &mut EpubDoc<R>) -> Vec<TocItem> {
        // EPUB2 / books that ship an NCX: the `epub` crate fills `doc.toc`.
        if !doc.toc.is_empty() {
            return Self::convert_navpoints_to_toc_items(&doc.toc);
        }
        // EPUB3-only books expose their ToC through the XHTML navigation
        // document, which the `epub` crate does not parse. Fall back to it.
        Self::parse_nav_toc(doc).unwrap_or_default()
    }

    /// Parse the EPUB3 navigation document (`properties="nav"`) into TocItems.
    fn parse_nav_toc<R: Read + Seek>(doc: &mut EpubDoc<R>) -> Option<Vec<TocItem>> {
        let nav_id = doc.get_nav_id()?;
        let (content, _mime) = doc.get_resource_str(&nav_id)?;
        let items = Self::parse_nav_html(&content);
        if items.is_empty() { None } else { Some(items) }
    }

    /// Parse the `<nav epub:type="toc">` ordered list out of a nav document.
    fn parse_nav_html(html: &str) -> Vec<TocItem> {
        let preprocessed = fix_html_for_parser(html);
        let Ok(dom) = parse_document(RcDom::default(), Default::default())
            .from_utf8()
            .read_from(&mut preprocessed.as_bytes())
        else {
            return Vec::new();
        };

        // Prefer the toc nav; otherwise fall back to any nav, then any <ol>.
        let ol = Self::find_toc_nav(&dom.document)
            .as_ref()
            .and_then(Self::find_descendant_ol)
            .or_else(|| Self::find_descendant_ol(&dom.document));

        match ol {
            Some(ol) => Self::parse_ol(&ol),
            None => Vec::new(),
        }
    }

    /// Find `<nav epub:type="toc">` (matching the `toc` token in the type list).
    fn find_toc_nav(node: &Rc<Node>) -> Option<Rc<Node>> {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if name.local.as_ref().eq_ignore_ascii_case("nav") {
                let is_toc = attrs.borrow().iter().any(|a| {
                    let n = a.name.local.as_ref();
                    (n == "epub:type" || n.eq_ignore_ascii_case("type"))
                        && a.value
                            .split_ascii_whitespace()
                            .any(|t| t.eq_ignore_ascii_case("toc"))
                });
                if is_toc {
                    return Some(node.clone());
                }
            }
        }
        for child in node.children.borrow().iter() {
            if let Some(found) = Self::find_toc_nav(child) {
                return Some(found);
            }
        }
        None
    }

    /// Find the first `<ol>` descendant of a node.
    fn find_descendant_ol(node: &Rc<Node>) -> Option<Rc<Node>> {
        for child in node.children.borrow().iter() {
            if let NodeData::Element { name, .. } = &child.data {
                if name.local.as_ref().eq_ignore_ascii_case("ol") {
                    return Some(child.clone());
                }
            }
            if let Some(found) = Self::find_descendant_ol(child) {
                return Some(found);
            }
        }
        None
    }

    /// Convert an `<ol>` node into a list of TocItems (one per `<li>`).
    fn parse_ol(ol: &Rc<Node>) -> Vec<TocItem> {
        let mut items = Vec::new();
        for li in Self::child_elements(ol, "li") {
            // The label/link is the first <a> (or <span>) inside the <li>.
            let anchor_node = Self::find_descendant_element(&li, &["a", "span"]);
            let title = anchor_node
                .as_ref()
                .map(|n| {
                    let mut text = String::new();
                    Self::collect_text(n, &mut text);
                    text.split_whitespace().collect::<Vec<_>>().join(" ")
                })
                .unwrap_or_default();
            let raw_href = anchor_node
                .as_ref()
                .and_then(|n| Self::element_attr(n, "href"));

            // A nested <ol> (direct child) makes this li a Section.
            let nested_ol = Self::child_elements(&li, "ol").into_iter().next();

            match nested_ol {
                Some(nested) => {
                    let children = Self::parse_ol(&nested);
                    let (clean_href, anchor) = match raw_href {
                        Some(h) => {
                            let (c, a) = Self::split_href_and_anchor(&h);
                            (Some(c), a)
                        }
                        None => (None, None),
                    };
                    items.push(TocItem::Section {
                        title,
                        href: clean_href,
                        anchor,
                        children,
                        is_expanded: false,
                    });
                }
                None => {
                    let Some(h) = raw_href else { continue };
                    let (clean_href, anchor) = Self::split_href_and_anchor(&h);
                    items.push(TocItem::Chapter {
                        title,
                        href: clean_href,
                        anchor,
                    });
                }
            }
        }
        items
    }

    /// Direct child elements of `node` matching `tag`.
    fn child_elements(node: &Rc<Node>, tag: &str) -> Vec<Rc<Node>> {
        node.children
            .borrow()
            .iter()
            .filter(|child| {
                matches!(&child.data, NodeData::Element { name, .. }
                    if name.local.as_ref().eq_ignore_ascii_case(tag))
            })
            .cloned()
            .collect()
    }

    /// First descendant element whose tag is in `tags`.
    fn find_descendant_element(node: &Rc<Node>, tags: &[&str]) -> Option<Rc<Node>> {
        for child in node.children.borrow().iter() {
            if let NodeData::Element { name, .. } = &child.data {
                let local = name.local.as_ref();
                if tags.iter().any(|t| local.eq_ignore_ascii_case(t)) {
                    return Some(child.clone());
                }
            }
            if let Some(found) = Self::find_descendant_element(child, tags) {
                return Some(found);
            }
        }
        None
    }

    fn element_attr(node: &Rc<Node>, attr: &str) -> Option<String> {
        if let NodeData::Element { attrs, .. } = &node.data {
            for a in attrs.borrow().iter() {
                if a.name.local.as_ref().eq_ignore_ascii_case(attr) {
                    return Some(a.value.to_string());
                }
            }
        }
        None
    }

    fn collect_text(node: &Rc<Node>, output: &mut String) {
        match &node.data {
            NodeData::Text { contents } => output.push_str(&contents.borrow()),
            _ => {
                for child in node.children.borrow().iter() {
                    Self::collect_text(child, output);
                }
            }
        }
    }

    /// Convert NavPoint structure to TocItem structure
    fn convert_navpoints_to_toc_items(navpoints: &[NavPoint]) -> Vec<TocItem> {
        navpoints
            .iter()
            .map(Self::convert_navpoint_to_toc_item)
            .collect()
    }

    /// Convert a single NavPoint to TocItem
    fn convert_navpoint_to_toc_item(navpoint: &NavPoint) -> TocItem {
        let href = navpoint.content.to_string_lossy().to_string();
        let (clean_href, anchor) = Self::split_href_and_anchor(&href);

        if navpoint.children.is_empty() {
            // No children, create a Chapter
            TocItem::Chapter {
                title: navpoint.label.clone(),
                href: clean_href,
                anchor,
            }
        } else {
            // Has children, create a Section
            let children = Self::convert_navpoints_to_toc_items(&navpoint.children);
            TocItem::Section {
                title: navpoint.label.clone(),
                href: Some(clean_href),
                anchor,
                children,
                is_expanded: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_navpoint(label: &str, content: &str, children: Vec<NavPoint>) -> NavPoint {
        NavPoint {
            label: label.to_string(),
            content: PathBuf::from(content),
            children,
            play_order: Some(0),
        }
    }

    #[test]
    fn test_convert_flat_navpoints() {
        let navpoints = vec![
            create_test_navpoint("Chapter 1", "ch1.xhtml", vec![]),
            create_test_navpoint("Chapter 2", "ch2.xhtml#section", vec![]),
            create_test_navpoint("Chapter 3", "ch3.xhtml", vec![]),
        ];

        let toc_items = TocParser::convert_navpoints_to_toc_items(&navpoints);

        assert_eq!(toc_items.len(), 3);

        match &toc_items[0] {
            TocItem::Chapter {
                title,
                href,
                anchor,
                ..
            } => {
                assert_eq!(title, "Chapter 1");
                assert_eq!(href, "ch1.xhtml");
                assert_eq!(anchor, &None);
            }
            _ => panic!("Expected Chapter"),
        }

        match &toc_items[1] {
            TocItem::Chapter {
                title,
                href,
                anchor,
                ..
            } => {
                assert_eq!(title, "Chapter 2");
                assert_eq!(href, "ch2.xhtml");
                assert_eq!(anchor, &Some("section".to_string()));
            }
            _ => panic!("Expected Chapter"),
        }
    }

    #[test]
    fn test_convert_hierarchical_navpoints() {
        let navpoints = vec![
            create_test_navpoint(
                "Part 1",
                "part1.xhtml",
                vec![
                    create_test_navpoint("Chapter 1.1", "ch1_1.xhtml", vec![]),
                    create_test_navpoint("Chapter 1.2", "ch1_2.xhtml", vec![]),
                ],
            ),
            create_test_navpoint(
                "Part 2",
                "part2.xhtml",
                vec![create_test_navpoint("Chapter 2.1", "ch2_1.xhtml", vec![])],
            ),
            create_test_navpoint("Epilogue", "epilogue.xhtml", vec![]),
        ];

        let toc_items = TocParser::convert_navpoints_to_toc_items(&navpoints);

        assert_eq!(toc_items.len(), 3);

        match &toc_items[0] {
            TocItem::Section {
                title,
                href,
                children,
                ..
            } => {
                assert_eq!(title, "Part 1");
                assert_eq!(href, &Some("part1.xhtml".to_string()));
                assert_eq!(children.len(), 2);

                match &children[0] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Chapter 1.1");
                        assert_eq!(href, "ch1_1.xhtml");
                    }
                    _ => panic!("Expected Chapter"),
                }
            }
            _ => panic!("Expected Section"),
        }

        match &toc_items[2] {
            TocItem::Chapter { title, href, .. } => {
                assert_eq!(title, "Epilogue");
                assert_eq!(href, "epilogue.xhtml");
            }
            _ => panic!("Expected Chapter"),
        }
    }

    #[test]
    fn test_parse_nav_html_flat() {
        // EPUB3 nav using a flat <ol> with class="indent" markers (no nesting).
        let html = r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body>
            <nav epub:type="toc"><ol>
                <li><a href="01.00-intro.xhtml">1. Introduction</a></li>
                <li class="indent"><a href="01.01-prereq.xhtml#sec">1.1. Prerequisites</a></li>
            </ol></nav></body></html>"#;

        let items = TocParser::parse_nav_html(html);
        assert_eq!(items.len(), 2);
        match &items[0] {
            TocItem::Chapter { title, href, .. } => {
                assert_eq!(title, "1. Introduction");
                assert_eq!(href, "01.00-intro.xhtml");
            }
            _ => panic!("Expected Chapter"),
        }
        match &items[1] {
            TocItem::Chapter {
                title,
                href,
                anchor,
            } => {
                assert_eq!(title, "1.1. Prerequisites");
                assert_eq!(href, "01.01-prereq.xhtml");
                assert_eq!(anchor, &Some("sec".to_string()));
            }
            _ => panic!("Expected Chapter"),
        }
    }

    #[test]
    fn test_parse_nav_html_nested() {
        // EPUB3 nav using nested <ol> for hierarchy.
        let html = r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body>
            <nav epub:type="toc"><ol>
                <li><a href="part1.xhtml">Part 1</a>
                    <ol>
                        <li><a href="ch1.xhtml">Chapter 1</a></li>
                    </ol>
                </li>
            </ol></nav></body></html>"#;

        let items = TocParser::parse_nav_html(html);
        assert_eq!(items.len(), 1);
        match &items[0] {
            TocItem::Section {
                title,
                href,
                children,
                ..
            } => {
                assert_eq!(title, "Part 1");
                assert_eq!(href, &Some("part1.xhtml".to_string()));
                assert_eq!(children.len(), 1);
                match &children[0] {
                    TocItem::Chapter { title, href, .. } => {
                        assert_eq!(title, "Chapter 1");
                        assert_eq!(href, "ch1.xhtml");
                    }
                    _ => panic!("Expected Chapter"),
                }
            }
            _ => panic!("Expected Section"),
        }
    }

    #[test]
    fn test_split_href_and_anchor() {
        let (href, anchor) = TocParser::split_href_and_anchor("chapter.xhtml#section1");
        assert_eq!(href, "chapter.xhtml");
        assert_eq!(anchor, Some("section1".to_string()));

        let (href, anchor) = TocParser::split_href_and_anchor("chapter.xhtml");
        assert_eq!(href, "chapter.xhtml");
        assert_eq!(anchor, None);
    }
}
