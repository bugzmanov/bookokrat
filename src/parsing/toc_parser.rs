use crate::table_of_contents::TocItem;
use epub::doc::{EpubDoc, NavPoint};
use std::io::{Read, Seek};

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

    pub fn parse_toc_structure<R: Read + Seek>(doc: &EpubDoc<R>) -> Vec<TocItem> {
        Self::convert_navpoints_to_toc_items(&doc.toc)
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
    fn test_split_href_and_anchor() {
        let (href, anchor) = TocParser::split_href_and_anchor("chapter.xhtml#section1");
        assert_eq!(href, "chapter.xhtml");
        assert_eq!(anchor, Some("section1".to_string()));

        let (href, anchor) = TocParser::split_href_and_anchor("chapter.xhtml");
        assert_eq!(href, "chapter.xhtml");
        assert_eq!(anchor, None);
    }
}
