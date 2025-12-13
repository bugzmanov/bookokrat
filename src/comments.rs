use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "target_kind", rename_all = "snake_case")]
pub enum CommentTarget {
    Paragraph {
        paragraph_index: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        word_range: Option<(usize, usize)>,
    },
    CodeBlock {
        paragraph_index: usize,
        /// Inclusive line range within the code block.
        line_range: (usize, usize),
    },
}

impl CommentTarget {
    pub fn node_index(&self) -> usize {
        match self {
            CommentTarget::Paragraph {
                paragraph_index, ..
            }
            | CommentTarget::CodeBlock {
                paragraph_index, ..
            } => *paragraph_index,
        }
    }

    pub fn word_range(&self) -> Option<(usize, usize)> {
        match self {
            CommentTarget::Paragraph { word_range, .. } => *word_range,
            CommentTarget::CodeBlock { .. } => None,
        }
    }

    pub fn line_range(&self) -> Option<(usize, usize)> {
        match self {
            CommentTarget::Paragraph { .. } => None,
            CommentTarget::CodeBlock { line_range, .. } => Some(*line_range),
        }
    }

    pub fn kind_order(&self) -> u8 {
        match self {
            CommentTarget::Paragraph { .. } => 0,
            CommentTarget::CodeBlock { .. } => 1,
        }
    }

    pub fn secondary_sort_key(&self) -> (usize, usize) {
        match self {
            CommentTarget::Paragraph { word_range, .. } => word_range
                .map(|(start, end)| (start, end))
                .unwrap_or((0, 0)),
            CommentTarget::CodeBlock { line_range, .. } => *line_range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub chapter_href: String,
    pub target: CommentTarget,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
struct CommentModernSerde {
    pub chapter_href: String,
    #[serde(flatten)]
    pub target: CommentTarget,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentLegacySerde {
    pub chapter_href: String,
    pub paragraph_index: usize,
    #[serde(default)]
    pub word_range: Option<(usize, usize)>,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum CommentSerde {
    Modern(CommentModernSerde),
    Legacy(CommentLegacySerde),
}

impl From<CommentLegacySerde> for Comment {
    fn from(legacy: CommentLegacySerde) -> Self {
        Comment {
            chapter_href: legacy.chapter_href,
            target: CommentTarget::Paragraph {
                paragraph_index: legacy.paragraph_index,
                word_range: legacy.word_range,
            },
            content: legacy.content,
            updated_at: legacy.updated_at,
        }
    }
}

impl From<CommentModernSerde> for Comment {
    fn from(modern: CommentModernSerde) -> Self {
        Comment {
            chapter_href: modern.chapter_href,
            target: modern.target,
            content: modern.content,
            updated_at: modern.updated_at,
        }
    }
}

impl From<&Comment> for CommentModernSerde {
    fn from(comment: &Comment) -> Self {
        CommentModernSerde {
            chapter_href: comment.chapter_href.clone(),
            target: comment.target.clone(),
            content: comment.content.clone(),
            updated_at: comment.updated_at,
        }
    }
}

impl Serialize for Comment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CommentModernSerde::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Comment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match CommentSerde::deserialize(deserializer)? {
            CommentSerde::Legacy(legacy) => Ok(Comment::from(legacy)),
            CommentSerde::Modern(modern) => Ok(Comment::from(modern)),
        }
    }
}

impl Comment {
    pub fn node_index(&self) -> usize {
        self.target.node_index()
    }

    pub fn is_paragraph_comment(&self) -> bool {
        matches!(self.target, CommentTarget::Paragraph { .. })
    }

    pub fn matches_location(&self, chapter_href: &str, target: &CommentTarget) -> bool {
        self.chapter_href == chapter_href && self.target == *target
    }
}

pub struct BookComments {
    pub file_path: PathBuf,
    comments: Vec<Comment>,
    // chapter_href -> node_index -> comment indices
    comments_by_location: HashMap<String, HashMap<usize, Vec<usize>>>,
}

impl BookComments {
    pub fn new(book_path: &Path) -> Result<Self> {
        let book_hash = Self::compute_book_hash(book_path);
        let comments_dir = Self::get_comments_dir()?;
        let file_path = comments_dir.join(format!("book_{book_hash}.yaml"));
        Self::new_with_path(file_path)
    }

    #[cfg(test)]
    pub fn new_with_custom_dir(book_path: &Path, comments_dir: &Path) -> Result<Self> {
        let book_hash = Self::compute_book_hash(book_path);
        if !comments_dir.exists() {
            fs::create_dir_all(comments_dir)?;
        }
        let file_path = comments_dir.join(format!("book_{book_hash}.yaml"));
        Self::new_with_path(file_path)
    }

    fn new_with_path(file_path: PathBuf) -> Result<Self> {
        let comments = if file_path.exists() {
            Self::load_from_file(&file_path)?
        } else {
            Vec::new()
        };

        let mut book_comments = Self {
            file_path,
            comments: Vec::new(),
            comments_by_location: HashMap::new(),
        };

        for comment in comments {
            book_comments.add_to_indices(&comment);
            book_comments.comments.push(comment);
        }

        Ok(book_comments)
    }

    pub fn add_comment(&mut self, comment: Comment) -> Result<()> {
        if matches!(comment.target, CommentTarget::Paragraph { .. }) {
            if let Some(existing_idx) =
                self.find_comment_index(&comment.chapter_href, &comment.target)
            {
                self.comments[existing_idx] = comment.clone();
                self.sort_comments();
                return self.save_to_disk();
            }
        }

        self.add_to_indices(&comment);
        self.comments.push(comment);

        self.sort_comments();
        self.save_to_disk()
    }

    pub fn update_comment(
        &mut self,
        chapter_href: &str,
        target: &CommentTarget,
        new_content: String,
    ) -> Result<()> {
        let idx = self
            .find_comment_index(chapter_href, target)
            .context("Comment not found")?;

        self.comments[idx].content = new_content;
        self.comments[idx].updated_at = Utc::now();

        self.save_to_disk()
    }

    pub fn delete_comment(&mut self, chapter_href: &str, target: &CommentTarget) -> Result<()> {
        let idx = self
            .find_comment_index(chapter_href, target)
            .context("Comment not found")?;

        let _comment = self.comments.remove(idx);

        self.rebuild_indices();

        self.save_to_disk()
    }

    /// Efficiently get comments for a specific AST node in a chapter
    pub fn get_node_comments(&self, chapter_href: &str, node_index: usize) -> Vec<&Comment> {
        self.comments_by_location
            .get(chapter_href)
            .and_then(|chapter_map| chapter_map.get(&node_index))
            .map(|indices| indices.iter().map(|&i| &self.comments[i]).collect())
            .unwrap_or_default()
    }

    pub fn get_chapter_comments(&self, chapter_href: &str) -> Vec<&Comment> {
        self.comments_by_location
            .get(chapter_href)
            .map(|chapter_map| {
                chapter_map
                    .values()
                    .flat_map(|indices| indices.iter().map(|&i| &self.comments[i]))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_all_comments(&self) -> &[Comment] {
        &self.comments
    }

    fn compute_book_hash(book_path: &Path) -> String {
        let filename = book_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| {
                // Fallback: use the full path if we can't get the filename
                book_path.to_str().unwrap_or("unknown")
            });

        let digest = md5::compute(filename.as_bytes());
        format!("{digest:x}")
    }

    fn get_comments_dir() -> Result<PathBuf> {
        let comments_dir = if let Ok(custom_dir) = std::env::var("BOOKOKRAT_COMMENTS_DIR") {
            PathBuf::from(custom_dir)
        } else {
            std::env::current_dir()
                .context("Could not determine current directory")?
                .join(".bookokrat_comments")
        };

        if !comments_dir.exists() {
            fs::create_dir_all(&comments_dir).context("Failed to create comments directory")?;
        }

        Ok(comments_dir)
    }

    fn load_from_file(file_path: &Path) -> Result<Vec<Comment>> {
        let content = fs::read_to_string(file_path).context("Failed to read comments file")?;

        if content.trim().is_empty() {
            return Ok(Vec::new());
        }

        serde_yaml::from_str(&content).context("Failed to parse comments YAML")
    }

    fn save_to_disk(&self) -> Result<()> {
        let yaml = serde_yaml::to_string(&self.comments).context("Failed to serialize comments")?;

        fs::write(&self.file_path, yaml).context("Failed to write comments file")?;

        Ok(())
    }

    fn find_comment_index(&self, chapter_href: &str, target: &CommentTarget) -> Option<usize> {
        self.comments
            .iter()
            .position(|c| c.matches_location(chapter_href, target))
    }

    fn add_to_indices(&mut self, comment: &Comment) {
        let idx = self.comments.len();
        self.comments_by_location
            .entry(comment.chapter_href.clone())
            .or_default()
            .entry(comment.node_index())
            .or_default()
            .push(idx);
    }

    fn rebuild_indices(&mut self) {
        self.comments_by_location.clear();
        for (idx, comment) in self.comments.iter().enumerate() {
            self.comments_by_location
                .entry(comment.chapter_href.clone())
                .or_default()
                .entry(comment.node_index())
                .or_default()
                .push(idx);
        }
    }

    fn sort_comments(&mut self) {
        self.comments.sort_by(|a, b| {
            a.chapter_href
                .cmp(&b.chapter_href)
                .then(a.node_index().cmp(&b.node_index()))
                .then(a.target.kind_order().cmp(&b.target.kind_order()))
                .then(
                    a.target
                        .secondary_sort_key()
                        .cmp(&b.target.secondary_sort_key()),
                )
                .then(a.updated_at.cmp(&b.updated_at))
        });

        self.rebuild_indices();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_env() -> (TempDir, PathBuf, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let book_path = temp_dir.path().join("test_book.epub");
        fs::write(&book_path, "fake epub content").unwrap();

        let comments_dir = temp_dir.path().join("comments");
        fs::create_dir_all(&comments_dir).unwrap();

        (temp_dir, book_path, comments_dir)
    }

    fn create_paragraph_comment(chapter: &str, node: usize, content: &str) -> Comment {
        Comment {
            chapter_href: chapter.to_string(),
            target: CommentTarget::Paragraph {
                paragraph_index: node,
                word_range: None,
            },
            content: content.to_string(),
            updated_at: Utc::now(),
        }
    }

    fn create_code_comment(
        chapter: &str,
        node: usize,
        line_range: (usize, usize),
        content: &str,
    ) -> Comment {
        Comment {
            chapter_href: chapter.to_string(),
            target: CommentTarget::CodeBlock {
                paragraph_index: node,
                line_range,
            },
            content: content.to_string(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_add_and_get_comments() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments =
            BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let comment = create_paragraph_comment("chapter1.xhtml", 3, "Nice paragraph");
        book_comments.add_comment(comment.clone()).unwrap();

        let comments = book_comments.get_node_comments("chapter1.xhtml", 3);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, comment.content);
    }

    #[test]
    fn test_update_comment() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments =
            BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let comment = create_paragraph_comment("chapter1.xhtml", 1, "Old text");
        book_comments.add_comment(comment.clone()).unwrap();

        let new_content = "Updated text".to_string();
        book_comments
            .update_comment("chapter1.xhtml", &comment.target, new_content.clone())
            .unwrap();

        let comments = book_comments.get_node_comments("chapter1.xhtml", 1);
        assert_eq!(comments[0].content, new_content);
    }

    #[test]
    fn test_delete_comment() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments =
            BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let comment = create_paragraph_comment("chapter1.xhtml", 2, "Delete me");
        book_comments.add_comment(comment.clone()).unwrap();

        book_comments
            .delete_comment("chapter1.xhtml", &comment.target)
            .unwrap();

        let comments = book_comments.get_node_comments("chapter1.xhtml", 2);
        assert!(comments.is_empty());
    }

    #[test]
    fn test_code_block_comments() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments =
            BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let comment = create_code_comment("chapter2.xhtml", 5, (1, 3), "Highlight lines");
        book_comments.add_comment(comment.clone()).unwrap();

        let comments = book_comments.get_node_comments("chapter2.xhtml", 5);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].target.line_range(), Some((1, 3)));
    }

    #[test]
    fn test_multiple_code_comments_same_line_range() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments =
            BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let comment_a = create_code_comment("chapter.xhtml", 2, (0, 0), "First note");
        let comment_b = create_code_comment("chapter.xhtml", 2, (0, 0), "Second note");

        book_comments.add_comment(comment_a.clone()).unwrap();
        book_comments.add_comment(comment_b.clone()).unwrap();

        let comments = book_comments.get_node_comments("chapter.xhtml", 2);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].content, "First note");
        assert_eq!(comments[1].content, "Second note");
    }

    #[test]
    fn test_modern_code_comment_serialization_roundtrip() {
        let comment = create_code_comment("chapter.xhtml", 3, (2, 4), "inline");
        let yaml = serde_yaml::to_string(&vec![comment.clone()]).unwrap();

        let parsed: Vec<Comment> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].target, comment.target);
    }

    #[test]
    fn test_legacy_comment_deserialize() {
        let legacy_yaml = r#"
- chapter_href: ch.xhtml
  paragraph_index: 5
  content: legacy
  updated_at: "2024-01-01T12:00:00Z"
"#;
        let parsed: Vec<Comment> = serde_yaml::from_str(legacy_yaml).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0].target,
            CommentTarget::Paragraph {
                paragraph_index: 5,
                ..
            }
        ));
    }

    #[test]
    fn test_sorting_respects_targets() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments =
            BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let comment_a = create_paragraph_comment("chapter.xhtml", 1, "A");
        let comment_b = create_code_comment("chapter.xhtml", 1, (2, 4), "B");
        let comment_c = create_paragraph_comment("chapter.xhtml", 0, "C");

        book_comments.add_comment(comment_a).unwrap();
        book_comments.add_comment(comment_b).unwrap();
        book_comments.add_comment(comment_c).unwrap();

        let all = book_comments.get_all_comments();
        assert_eq!(all[0].node_index(), 0);
        assert_eq!(all[1].node_index(), 1);
        assert!(all[1].is_paragraph_comment());
        assert!(matches!(all[2].target, CommentTarget::CodeBlock { .. }));
    }
}
