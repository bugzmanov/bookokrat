use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comment {
    pub chapter_href: String,
    pub paragraph_index: usize,
    pub word_range: Option<(usize, usize)>,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

impl Comment {
    fn matches_location(
        &self,
        chapter_href: &str,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> bool {
        self.chapter_href == chapter_href
            && self.paragraph_index == paragraph_index
            && self.word_range == word_range
    }
}

pub struct BookComments {
    pub file_path: PathBuf,
    comments: Vec<Comment>,
    // Efficient two-level lookup: chapter_href -> paragraph_index -> comment indices
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
        if let Some(existing_idx) = self.find_comment_index(
            &comment.chapter_href,
            comment.paragraph_index,
            comment.word_range,
        ) {
            self.comments[existing_idx] = comment.clone();
        } else {
            self.add_to_indices(&comment);
            self.comments.push(comment);
        }

        self.sort_comments();
        self.save_to_disk()
    }

    pub fn update_comment(
        &mut self,
        chapter_href: &str,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
        new_content: String,
    ) -> Result<()> {
        let idx = self
            .find_comment_index(chapter_href, paragraph_index, word_range)
            .context("Comment not found")?;

        self.comments[idx].content = new_content;
        self.comments[idx].updated_at = Utc::now();

        self.save_to_disk()
    }

    pub fn delete_comment(
        &mut self,
        chapter_href: &str,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> Result<()> {
        let idx = self
            .find_comment_index(chapter_href, paragraph_index, word_range)
            .context("Comment not found")?;

        let _comment = self.comments.remove(idx);

        self.rebuild_indices();

        self.save_to_disk()
    }

    /// Efficiently get comments for a specific paragraph in a chapter
    pub fn get_paragraph_comments(
        &self,
        chapter_href: &str,
        paragraph_index: usize,
    ) -> Vec<&Comment> {
        self.comments_by_location
            .get(chapter_href)
            .and_then(|chapter_map| chapter_map.get(&paragraph_index))
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
        // Use only the filename, not the full path
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
        // Use current directory instead of config directory
        let comments_dir = std::env::current_dir()
            .context("Could not determine current directory")?
            .join(".bookrat_comments");

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

    fn find_comment_index(
        &self,
        chapter_href: &str,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> Option<usize> {
        self.comments
            .iter()
            .position(|c| c.matches_location(chapter_href, paragraph_index, word_range))
    }

    fn add_to_indices(&mut self, comment: &Comment) {
        let idx = self.comments.len();
        self.comments_by_location
            .entry(comment.chapter_href.clone())
            .or_default()
            .entry(comment.paragraph_index)
            .or_default()
            .push(idx);
    }

    fn rebuild_indices(&mut self) {
        self.comments_by_location.clear();
        for (idx, comment) in self.comments.iter().enumerate() {
            self.comments_by_location
                .entry(comment.chapter_href.clone())
                .or_default()
                .entry(comment.paragraph_index)
                .or_default()
                .push(idx);
        }
    }

    fn sort_comments(&mut self) {
        self.comments.sort_by(|a, b| {
            a.chapter_href
                .cmp(&b.chapter_href)
                .then(a.paragraph_index.cmp(&b.paragraph_index))
                .then(a.word_range.cmp(&b.word_range))
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

    fn create_test_comment(chapter: &str, para: usize, content: &str) -> Comment {
        Comment {
            chapter_href: chapter.to_string(),
            paragraph_index: para,
            word_range: None,
            content: content.to_string(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_new_book_comments_creates_empty() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        assert_eq!(comments.get_all_comments().len(), 0);
        assert!(comments.file_path.parent().unwrap().exists());
    }

    #[test]
    fn test_add_comment_and_persist() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let mut comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();
        let comment = create_test_comment("chapter1.xhtml", 5, "Test comment");

        comments.add_comment(comment.clone()).unwrap();

        assert_eq!(comments.get_all_comments().len(), 1);
        assert_eq!(comments.get_all_comments()[0].content, "Test comment");

        let comments2 = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();
        assert_eq!(comments2.get_all_comments().len(), 1);
        assert_eq!(comments2.get_all_comments()[0].content, "Test comment");
    }

    #[test]
    fn test_update_comment() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let mut comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();
        let comment = create_test_comment("chapter1.xhtml", 5, "Original");

        comments.add_comment(comment).unwrap();

        comments
            .update_comment("chapter1.xhtml", 5, None, "Updated content".to_string())
            .unwrap();

        let updated = &comments.get_all_comments()[0];
        assert_eq!(updated.content, "Updated content");

        let comments2 = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();
        assert_eq!(comments2.get_all_comments()[0].content, "Updated content");
    }

    #[test]
    fn test_delete_comment() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let mut comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();
        let comment1 = create_test_comment("chapter1.xhtml", 5, "Comment 1");
        let comment2 = create_test_comment("chapter2.xhtml", 3, "Comment 2");

        comments.add_comment(comment1).unwrap();
        comments.add_comment(comment2).unwrap();
        assert_eq!(comments.get_all_comments().len(), 2);

        comments.delete_comment("chapter1.xhtml", 5, None).unwrap();

        assert_eq!(comments.get_all_comments().len(), 1);
        assert_eq!(comments.get_all_comments()[0].content, "Comment 2");

        let comments2 = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();
        assert_eq!(comments2.get_all_comments().len(), 1);
    }

    #[test]
    fn test_get_chapter_comments() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let mut comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        comments
            .add_comment(create_test_comment("chapter1.xhtml", 1, "C1P1"))
            .unwrap();
        comments
            .add_comment(create_test_comment("chapter1.xhtml", 5, "C1P5"))
            .unwrap();
        comments
            .add_comment(create_test_comment("chapter2.xhtml", 2, "C2P2"))
            .unwrap();

        let chapter1_comments = comments.get_chapter_comments("chapter1.xhtml");
        assert_eq!(chapter1_comments.len(), 2);
        assert_eq!(chapter1_comments[0].content, "C1P1");
        assert_eq!(chapter1_comments[1].content, "C1P5");

        let chapter2_comments = comments.get_chapter_comments("chapter2.xhtml");
        assert_eq!(chapter2_comments.len(), 1);
        assert_eq!(chapter2_comments[0].content, "C2P2");
    }

    #[test]
    fn test_comments_with_word_ranges() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let mut comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let mut comment1 = create_test_comment("chapter1.xhtml", 5, "Word range comment");
        comment1.word_range = Some((10, 20));

        let comment2 = create_test_comment("chapter1.xhtml", 5, "Full paragraph");

        comments.add_comment(comment1.clone()).unwrap();
        comments.add_comment(comment2).unwrap();

        assert_eq!(comments.get_all_comments().len(), 2);

        comments
            .update_comment(
                "chapter1.xhtml",
                5,
                Some((10, 20)),
                "Updated word range".to_string(),
            )
            .unwrap();

        let all = comments.get_all_comments();
        let word_range_comment = all.iter().find(|c| c.word_range.is_some()).unwrap();
        assert_eq!(word_range_comment.content, "Updated word range");
    }

    #[test]
    fn test_comment_sorting() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let mut comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        comments
            .add_comment(create_test_comment("chapter2.xhtml", 5, "C2P5"))
            .unwrap();
        comments
            .add_comment(create_test_comment("chapter1.xhtml", 10, "C1P10"))
            .unwrap();
        comments
            .add_comment(create_test_comment("chapter1.xhtml", 3, "C1P3"))
            .unwrap();

        let all = comments.get_all_comments();
        assert_eq!(all[0].chapter_href, "chapter1.xhtml");
        assert_eq!(all[0].paragraph_index, 3);
        assert_eq!(all[1].chapter_href, "chapter1.xhtml");
        assert_eq!(all[1].paragraph_index, 10);
        assert_eq!(all[2].chapter_href, "chapter2.xhtml");
    }

    #[test]
    fn test_replace_comment_at_same_location() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();

        let mut comments = BookComments::new_with_custom_dir(&book_path, &comments_dir).unwrap();

        let comment1 = create_test_comment("chapter1.xhtml", 5, "First version");
        let mut comment2 = create_test_comment("chapter1.xhtml", 5, "Second version");
        comment2.updated_at = Utc::now();

        comments.add_comment(comment1).unwrap();
        assert_eq!(comments.get_all_comments().len(), 1);

        comments.add_comment(comment2).unwrap();
        assert_eq!(comments.get_all_comments().len(), 1);
        assert_eq!(comments.get_all_comments()[0].content, "Second version");
    }
}
