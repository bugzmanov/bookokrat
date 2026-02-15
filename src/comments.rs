use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// PDF selection rectangle (pixel coordinates relative to page)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfSelectionRect {
    pub page: usize,
    pub topleft_x: u32,
    pub topleft_y: u32,
    pub bottomright_x: u32,
    pub bottomright_y: u32,
}

/// Represents the specific sub-element within a block that a comment targets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "subtarget_kind", rename_all = "snake_case")]
pub enum BlockSubtarget {
    /// A regular paragraph or heading - targets the whole block or a word range within it.
    Paragraph {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        word_range: Option<(usize, usize)>,
    },
    /// A specific item within a list block.
    ListItem {
        item_index: usize,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        list_path: Vec<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        word_range: Option<(usize, usize)>,
    },
    /// A specific paragraph within a blockquote.
    QuoteParagraph {
        paragraph_index: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        word_range: Option<(usize, usize)>,
    },
    /// A specific item within a definition list (term or definition).
    DefinitionItem {
        item_index: usize,
        is_term: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        word_range: Option<(usize, usize)>,
    },
    /// A range of lines within a code block.
    CodeLines { line_range: (usize, usize) },
}

impl BlockSubtarget {
    pub fn word_range(&self) -> Option<(usize, usize)> {
        match self {
            BlockSubtarget::Paragraph { word_range } => *word_range,
            BlockSubtarget::ListItem { word_range, .. } => *word_range,
            BlockSubtarget::QuoteParagraph { word_range, .. } => *word_range,
            BlockSubtarget::DefinitionItem { word_range, .. } => *word_range,
            BlockSubtarget::CodeLines { .. } => None,
        }
    }

    pub fn line_range(&self) -> Option<(usize, usize)> {
        match self {
            BlockSubtarget::CodeLines { line_range } => Some(*line_range),
            _ => None,
        }
    }

    pub fn list_item_index(&self) -> Option<usize> {
        match self {
            BlockSubtarget::ListItem { item_index, .. } => Some(*item_index),
            _ => None,
        }
    }

    pub fn list_path(&self) -> Option<&[usize]> {
        match self {
            BlockSubtarget::ListItem { list_path, .. } => Some(list_path.as_slice()),
            _ => None,
        }
    }

    pub fn definition_item_index(&self) -> Option<usize> {
        match self {
            BlockSubtarget::DefinitionItem { item_index, .. } => Some(*item_index),
            _ => None,
        }
    }

    pub fn quote_paragraph_index(&self) -> Option<usize> {
        match self {
            BlockSubtarget::QuoteParagraph {
                paragraph_index, ..
            } => Some(*paragraph_index),
            _ => None,
        }
    }

    pub fn is_code(&self) -> bool {
        matches!(self, BlockSubtarget::CodeLines { .. })
    }

    pub fn kind_order(&self) -> u8 {
        match self {
            BlockSubtarget::Paragraph { .. } => 0,
            BlockSubtarget::ListItem { .. } => 1,
            BlockSubtarget::QuoteParagraph { .. } => 2,
            BlockSubtarget::DefinitionItem { .. } => 3,
            BlockSubtarget::CodeLines { .. } => 4,
        }
    }

    pub fn secondary_sort_key(&self) -> (usize, usize) {
        match self {
            BlockSubtarget::Paragraph { word_range } => word_range.unwrap_or((0, 0)),
            BlockSubtarget::ListItem {
                item_index,
                list_path: _list_path,
                word_range,
            } => (*item_index, word_range.map(|(s, _)| s).unwrap_or(0)),
            BlockSubtarget::QuoteParagraph {
                paragraph_index,
                word_range,
            } => (*paragraph_index, word_range.map(|(s, _)| s).unwrap_or(0)),
            BlockSubtarget::DefinitionItem {
                item_index,
                word_range,
                ..
            } => (*item_index, word_range.map(|(s, _)| s).unwrap_or(0)),
            BlockSubtarget::CodeLines { line_range } => *line_range,
        }
    }
}

/// Identifies the location of a comment within the document.
/// Supports both EPUB text-based targeting and PDF pixel-based targeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentTarget {
    /// EPUB text-based targeting (original format)
    Text {
        node_index: usize,
        subtarget: BlockSubtarget,
    },
    /// PDF pixel-based targeting
    Pdf {
        page: usize,
        rects: Vec<PdfSelectionRect>,
    },
}

impl CommentTarget {
    /// Create a Text target for EPUB paragraph
    pub fn paragraph(node_index: usize, word_range: Option<(usize, usize)>) -> Self {
        Self::Text {
            node_index,
            subtarget: BlockSubtarget::Paragraph { word_range },
        }
    }

    /// Create a Text target for EPUB list item
    pub fn list_item(
        node_index: usize,
        item_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        Self::Text {
            node_index,
            subtarget: BlockSubtarget::ListItem {
                item_index,
                list_path: Vec::new(),
                word_range,
            },
        }
    }

    /// Create a Text target for EPUB list item with path
    pub fn list_item_with_path(
        node_index: usize,
        list_path: Vec<usize>,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        let item_index = list_path.last().copied().unwrap_or(0);
        Self::Text {
            node_index,
            subtarget: BlockSubtarget::ListItem {
                item_index,
                list_path,
                word_range,
            },
        }
    }

    /// Create a Text target for EPUB quote paragraph
    pub fn quote_paragraph(
        node_index: usize,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        Self::Text {
            node_index,
            subtarget: BlockSubtarget::QuoteParagraph {
                paragraph_index,
                word_range,
            },
        }
    }

    /// Create a Text target for EPUB definition item
    pub fn definition_item(
        node_index: usize,
        item_index: usize,
        is_term: bool,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        Self::Text {
            node_index,
            subtarget: BlockSubtarget::DefinitionItem {
                item_index,
                is_term,
                word_range,
            },
        }
    }

    /// Create a Text target for EPUB code block
    pub fn code_block(node_index: usize, line_range: (usize, usize)) -> Self {
        Self::Text {
            node_index,
            subtarget: BlockSubtarget::CodeLines { line_range },
        }
    }

    /// Create a Pdf target for PDF selection
    pub fn pdf(page: usize, rects: Vec<PdfSelectionRect>) -> Self {
        Self::Pdf { page, rects }
    }

    /// Returns the node index for Text targets, or None for Pdf targets
    pub fn node_index(&self) -> Option<usize> {
        match self {
            Self::Text { node_index, .. } => Some(*node_index),
            Self::Pdf { .. } => None,
        }
    }

    /// Returns the page for Pdf targets, or None for Text targets
    pub fn page(&self) -> Option<usize> {
        match self {
            Self::Text { .. } => None,
            Self::Pdf { page, .. } => Some(*page),
        }
    }

    /// Returns true if this is a Text (EPUB) target
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    /// Returns true if this is a Pdf target
    pub fn is_pdf(&self) -> bool {
        matches!(self, Self::Pdf { .. })
    }

    pub fn word_range(&self) -> Option<(usize, usize)> {
        match self {
            Self::Text { subtarget, .. } => subtarget.word_range(),
            Self::Pdf { .. } => None,
        }
    }

    pub fn list_item_index(&self) -> Option<usize> {
        match self {
            Self::Text { subtarget, .. } => subtarget.list_item_index(),
            Self::Pdf { .. } => None,
        }
    }

    pub fn definition_item_index(&self) -> Option<usize> {
        match self {
            Self::Text { subtarget, .. } => subtarget.definition_item_index(),
            Self::Pdf { .. } => None,
        }
    }

    pub fn quote_paragraph_index(&self) -> Option<usize> {
        match self {
            Self::Text { subtarget, .. } => subtarget.quote_paragraph_index(),
            Self::Pdf { .. } => None,
        }
    }

    pub fn line_range(&self) -> Option<(usize, usize)> {
        match self {
            Self::Text { subtarget, .. } => subtarget.line_range(),
            Self::Pdf { .. } => None,
        }
    }

    pub fn kind_order(&self) -> u8 {
        match self {
            Self::Text { subtarget, .. } => subtarget.kind_order(),
            Self::Pdf { .. } => 10, // PDF comments sort after text comments
        }
    }

    pub fn secondary_sort_key(&self) -> (usize, usize) {
        match self {
            Self::Text { subtarget, .. } => subtarget.secondary_sort_key(),
            Self::Pdf { page, rects } => {
                let y = rects.first().map(|r| r.topleft_y as usize).unwrap_or(0);
                (*page, y)
            }
        }
    }

    pub fn is_code_block(&self) -> bool {
        match self {
            Self::Text { subtarget, .. } => subtarget.is_code(),
            Self::Pdf { .. } => false,
        }
    }

    /// Returns the subtarget for Text targets, or None for Pdf targets
    pub fn subtarget(&self) -> Option<&BlockSubtarget> {
        match self {
            Self::Text { subtarget, .. } => Some(subtarget),
            Self::Pdf { .. } => None,
        }
    }

    /// Returns the list path for Text list item targets, or None otherwise
    pub fn list_path(&self) -> Option<&[usize]> {
        match self {
            Self::Text { subtarget, .. } => subtarget.list_path(),
            Self::Pdf { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub chapter_href: String,
    pub target: CommentTarget,
    pub content: String,
    pub updated_at: DateTime<Utc>,
    /// The quoted/selected text that this comment refers to (primarily for PDF)
    pub quoted_text: Option<String>,
}

/// Serde representation for Text CommentTarget (EPUB format with node_index + subtarget)
#[derive(Serialize, Deserialize)]
struct CommentTextTargetSerde {
    node_index: usize,
    #[serde(flatten)]
    subtarget: BlockSubtarget,
}

/// Modern format for EPUB text comments
#[derive(Serialize, Deserialize)]
struct CommentTextSerde {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub chapter_href: String,
    pub target_type: String, // "text"
    #[serde(flatten)]
    pub target: CommentTextTargetSerde,
    pub content: String,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quoted_text: Option<String>,
}

/// Modern format for PDF comments
#[derive(Serialize, Deserialize)]
struct CommentPdfSerde {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub chapter_href: String,
    pub target_type: String, // "pdf"
    pub page: usize,
    pub rects: Vec<PdfSelectionRect>,
    pub content: String,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quoted_text: Option<String>,
}

/// Legacy modern format (has node_index but no target_type, assumed to be Text)
#[derive(Serialize, Deserialize)]
struct CommentModernLegacySerde {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub chapter_href: String,
    #[serde(flatten)]
    pub target: CommentTextTargetSerde,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

/// Legacy format: paragraph comments with optional fields (pre-refactor)
#[derive(Serialize, Deserialize)]
struct CommentLegacyParagraphSerde {
    pub chapter_href: String,
    pub paragraph_index: usize,
    #[serde(default)]
    pub word_range: Option<(usize, usize)>,
    #[serde(default)]
    pub list_item_index: Option<usize>,
    #[serde(default)]
    pub definition_item_index: Option<usize>,
    #[serde(default)]
    pub quote_paragraph_index: Option<usize>,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

/// Legacy format: code block comments (pre-refactor)
#[derive(Serialize, Deserialize)]
struct CommentLegacyCodeBlockSerde {
    pub chapter_href: String,
    pub paragraph_index: usize,
    pub line_range: (usize, usize),
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum CommentSerde {
    Text(CommentTextSerde),
    Pdf(CommentPdfSerde),
    ModernLegacy(CommentModernLegacySerde),
    LegacyCodeBlock(CommentLegacyCodeBlockSerde),
    LegacyParagraph(CommentLegacyParagraphSerde),
}

impl From<CommentLegacyParagraphSerde> for Comment {
    fn from(legacy: CommentLegacyParagraphSerde) -> Self {
        let subtarget = if let Some(item_index) = legacy.list_item_index {
            BlockSubtarget::ListItem {
                item_index,
                list_path: Vec::new(),
                word_range: legacy.word_range,
            }
        } else if let Some(paragraph_index) = legacy.quote_paragraph_index {
            BlockSubtarget::QuoteParagraph {
                paragraph_index,
                word_range: legacy.word_range,
            }
        } else if let Some(item_index) = legacy.definition_item_index {
            BlockSubtarget::DefinitionItem {
                item_index,
                is_term: false, // Legacy doesn't have this field
                word_range: legacy.word_range,
            }
        } else {
            BlockSubtarget::Paragraph {
                word_range: legacy.word_range,
            }
        };

        Comment {
            id: generate_comment_id(),
            chapter_href: legacy.chapter_href,
            target: CommentTarget::Text {
                node_index: legacy.paragraph_index,
                subtarget,
            },
            content: legacy.content,
            updated_at: legacy.updated_at,
            quoted_text: None,
        }
    }
}

impl From<CommentLegacyCodeBlockSerde> for Comment {
    fn from(legacy: CommentLegacyCodeBlockSerde) -> Self {
        Comment {
            id: generate_comment_id(),
            chapter_href: legacy.chapter_href,
            target: CommentTarget::code_block(legacy.paragraph_index, legacy.line_range),
            content: legacy.content,
            updated_at: legacy.updated_at,
            quoted_text: None,
        }
    }
}

impl From<CommentModernLegacySerde> for Comment {
    fn from(modern: CommentModernLegacySerde) -> Self {
        Comment {
            id: modern.id.unwrap_or_else(generate_comment_id),
            chapter_href: modern.chapter_href,
            target: CommentTarget::Text {
                node_index: modern.target.node_index,
                subtarget: modern.target.subtarget,
            },
            content: modern.content,
            updated_at: modern.updated_at,
            quoted_text: None,
        }
    }
}

impl From<CommentTextSerde> for Comment {
    fn from(text: CommentTextSerde) -> Self {
        Comment {
            id: text.id.unwrap_or_else(generate_comment_id),
            chapter_href: text.chapter_href,
            target: CommentTarget::Text {
                node_index: text.target.node_index,
                subtarget: text.target.subtarget,
            },
            content: text.content,
            updated_at: text.updated_at,
            quoted_text: text.quoted_text,
        }
    }
}

impl From<CommentPdfSerde> for Comment {
    fn from(pdf: CommentPdfSerde) -> Self {
        Comment {
            id: pdf.id.unwrap_or_else(generate_comment_id),
            chapter_href: pdf.chapter_href,
            target: CommentTarget::Pdf {
                page: pdf.page,
                rects: pdf.rects,
            },
            content: pdf.content,
            updated_at: pdf.updated_at,
            quoted_text: pdf.quoted_text,
        }
    }
}

impl Serialize for Comment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.target {
            CommentTarget::Text {
                node_index,
                subtarget,
            } => {
                let serde = CommentTextSerde {
                    id: Some(self.id.clone()),
                    chapter_href: self.chapter_href.clone(),
                    target_type: "text".to_string(),
                    target: CommentTextTargetSerde {
                        node_index: *node_index,
                        subtarget: subtarget.clone(),
                    },
                    content: self.content.clone(),
                    updated_at: self.updated_at,
                    quoted_text: self.quoted_text.clone(),
                };
                serde.serialize(serializer)
            }
            CommentTarget::Pdf { page, rects } => {
                let serde = CommentPdfSerde {
                    id: Some(self.id.clone()),
                    chapter_href: self.chapter_href.clone(),
                    target_type: "pdf".to_string(),
                    page: *page,
                    rects: rects.clone(),
                    content: self.content.clone(),
                    updated_at: self.updated_at,
                    quoted_text: self.quoted_text.clone(),
                };
                serde.serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for Comment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match CommentSerde::deserialize(deserializer)? {
            CommentSerde::Text(text) => Ok(Comment::from(text)),
            CommentSerde::Pdf(pdf) => Ok(Comment::from(pdf)),
            CommentSerde::ModernLegacy(modern) => Ok(Comment::from(modern)),
            CommentSerde::LegacyParagraph(legacy) => Ok(Comment::from(legacy)),
            CommentSerde::LegacyCodeBlock(legacy) => Ok(Comment::from(legacy)),
        }
    }
}

impl Comment {
    pub fn new(
        chapter_href: String,
        target: CommentTarget,
        content: String,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: generate_comment_id(),
            chapter_href,
            target,
            content,
            updated_at,
            quoted_text: None,
        }
    }

    pub fn with_quoted_text(
        chapter_href: String,
        target: CommentTarget,
        content: String,
        updated_at: DateTime<Utc>,
        quoted_text: Option<String>,
    ) -> Self {
        Self {
            id: generate_comment_id(),
            chapter_href,
            target,
            content,
            updated_at,
            quoted_text,
        }
    }

    /// Returns the node index for Text targets, or None for Pdf targets
    pub fn node_index(&self) -> Option<usize> {
        self.target.node_index()
    }

    /// Returns the page for Pdf targets, or None for Text targets
    pub fn page(&self) -> Option<usize> {
        self.target.page()
    }

    /// Returns the index key used for storage lookup (node_index for Text, page for Pdf)
    pub fn index_key(&self) -> usize {
        match &self.target {
            CommentTarget::Text { node_index, .. } => *node_index,
            CommentTarget::Pdf { page, .. } => *page,
        }
    }

    /// Returns true if this is a Text (EPUB) comment
    pub fn is_text(&self) -> bool {
        self.target.is_text()
    }

    /// Returns true if this is a Pdf comment
    pub fn is_pdf(&self) -> bool {
        self.target.is_pdf()
    }

    /// Returns true if this comment is NOT a code block comment (i.e., targets text content)
    pub fn is_paragraph_comment(&self) -> bool {
        !self.target.is_code_block()
    }

    pub fn matches_location(&self, chapter_href: &str, target: &CommentTarget) -> bool {
        self.chapter_href == chapter_href && self.target == *target
    }
}

fn generate_comment_id() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct BookComments {
    pub file_path: PathBuf,
    comments: Vec<Comment>,
    // chapter_href -> node_index -> comment indices
    comments_by_location: HashMap<String, HashMap<usize, Vec<usize>>>,
    comments_by_id: HashMap<String, usize>,
}

impl BookComments {
    pub fn new(book_path: &Path, comments_dir: Option<&Path>) -> Result<Self> {
        let book_hash = Self::compute_book_hash(book_path);
        let resolved_dir = match comments_dir {
            Some(dir) => {
                if !dir.exists() {
                    fs::create_dir_all(dir)?;
                }
                dir.to_path_buf()
            }
            None => Self::get_comments_dir()?,
        };
        let file_path = resolved_dir.join(format!("book_{book_hash}.yaml"));
        Self::new_with_path(file_path)
    }

    /// Create an empty BookComments that doesn't load from or save to disk.
    /// Used in test mode for reproducible testing.
    pub fn new_empty() -> Self {
        Self {
            file_path: PathBuf::new(),
            comments: Vec::new(),
            comments_by_location: HashMap::new(),
            comments_by_id: HashMap::new(),
        }
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
            comments_by_id: HashMap::new(),
        };

        for comment in comments {
            book_comments.add_to_indices(&comment);
            book_comments.comments.push(comment);
        }

        Ok(book_comments)
    }

    pub fn add_comment(&mut self, comment: Comment) -> Result<()> {
        let mut comment = comment;
        if self.comments_by_id.contains_key(&comment.id) {
            comment.id = generate_comment_id();
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

    pub fn update_comment_by_id(&mut self, comment_id: &str, new_content: String) -> Result<()> {
        let idx = self
            .find_comment_index_by_id(comment_id)
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

    pub fn delete_comment_by_id(&mut self, comment_id: &str) -> Result<()> {
        let idx = self
            .find_comment_index_by_id(comment_id)
            .context("Comment not found")?;

        let _comment = self.comments.remove(idx);

        self.rebuild_indices();

        self.save_to_disk()
    }

    /// Efficiently get comments for a specific AST node in a chapter (EPUB Text comments)
    pub fn get_node_comments(&self, chapter_href: &str, node_index: usize) -> Vec<&Comment> {
        self.comments_by_location
            .get(chapter_href)
            .and_then(|chapter_map| chapter_map.get(&node_index))
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|&i| {
                        let c = self.comments.get(i)?;
                        if c.is_text() { Some(c) } else { None }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get comments for a specific page in a PDF document
    pub fn get_page_comments(&self, doc_id: &str, page: usize) -> Vec<&Comment> {
        self.comments_by_location
            .get(doc_id)
            .and_then(|doc_map| doc_map.get(&page))
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|&i| {
                        let c = self.comments.get(i)?;
                        if c.is_pdf() { Some(c) } else { None }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all PDF comments for a document
    pub fn get_doc_comments(&self, doc_id: &str) -> Vec<&Comment> {
        self.comments_by_location
            .get(doc_id)
            .map(|doc_map| {
                doc_map
                    .values()
                    .flat_map(|indices| {
                        indices.iter().filter_map(|&i| {
                            let c = self.comments.get(i)?;
                            if c.is_pdf() { Some(c) } else { None }
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_comment_by_id(&self, comment_id: &str) -> Option<&Comment> {
        self.comments_by_id
            .get(comment_id)
            .and_then(|&idx| self.comments.get(idx))
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
            let cwd = std::env::current_dir().context("Could not determine current directory")?;
            match crate::library::resolve_library_paths(&cwd) {
                Ok(paths) => paths.comments_dir,
                Err(_) => cwd.join(".bookokrat_comments"),
            }
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
        // Skip saving if file_path is empty (test mode)
        if self.file_path.as_os_str().is_empty() {
            return Ok(());
        }

        let yaml = serde_yaml::to_string(&self.comments).context("Failed to serialize comments")?;

        fs::write(&self.file_path, yaml).context("Failed to write comments file")?;

        Ok(())
    }

    fn find_comment_index(&self, chapter_href: &str, target: &CommentTarget) -> Option<usize> {
        self.comments
            .iter()
            .position(|c| c.matches_location(chapter_href, target))
    }

    fn find_comment_index_by_id(&self, comment_id: &str) -> Option<usize> {
        self.comments_by_id.get(comment_id).copied()
    }

    fn add_to_indices(&mut self, comment: &Comment) {
        let idx = self.comments.len();
        self.comments_by_location
            .entry(comment.chapter_href.clone())
            .or_default()
            .entry(comment.index_key())
            .or_default()
            .push(idx);
        self.comments_by_id.insert(comment.id.clone(), idx);
    }

    fn rebuild_indices(&mut self) {
        self.comments_by_location.clear();
        self.comments_by_id.clear();
        for (idx, comment) in self.comments.iter().enumerate() {
            self.comments_by_location
                .entry(comment.chapter_href.clone())
                .or_default()
                .entry(comment.index_key())
                .or_default()
                .push(idx);
            self.comments_by_id.insert(comment.id.clone(), idx);
        }
    }

    fn sort_comments(&mut self) {
        self.comments.sort_by(|a, b| {
            a.chapter_href
                .cmp(&b.chapter_href)
                .then(a.index_key().cmp(&b.index_key()))
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
        Comment::new(
            chapter.to_string(),
            CommentTarget::paragraph(node, None),
            content.to_string(),
            Utc::now(),
        )
    }

    fn create_code_comment(
        chapter: &str,
        node: usize,
        line_range: (usize, usize),
        content: &str,
    ) -> Comment {
        Comment::new(
            chapter.to_string(),
            CommentTarget::code_block(node, line_range),
            content.to_string(),
            Utc::now(),
        )
    }

    #[test]
    fn test_add_and_get_comments() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment = create_paragraph_comment("chapter1.xhtml", 3, "Nice paragraph");
        book_comments.add_comment(comment.clone()).unwrap();

        let comments = book_comments.get_node_comments("chapter1.xhtml", 3);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, comment.content);
    }

    #[test]
    fn test_update_comment() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment = create_paragraph_comment("chapter1.xhtml", 1, "Old text");
        book_comments.add_comment(comment.clone()).unwrap();

        let new_content = "Updated text".to_string();
        book_comments
            .update_comment_by_id(&comment.id, new_content.clone())
            .unwrap();

        let comments = book_comments.get_node_comments("chapter1.xhtml", 1);
        assert_eq!(comments[0].content, new_content);
    }

    #[test]
    fn test_delete_comment() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment = create_paragraph_comment("chapter1.xhtml", 2, "Delete me");
        book_comments.add_comment(comment.clone()).unwrap();

        book_comments.delete_comment_by_id(&comment.id).unwrap();

        let comments = book_comments.get_node_comments("chapter1.xhtml", 2);
        assert!(comments.is_empty());
    }

    #[test]
    fn test_code_block_comments() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment = create_code_comment("chapter2.xhtml", 5, (1, 3), "Highlight lines");
        book_comments.add_comment(comment.clone()).unwrap();

        let comments = book_comments.get_node_comments("chapter2.xhtml", 5);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].target.line_range(), Some((1, 3)));
    }

    #[test]
    fn test_multiple_code_comments_same_line_range() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

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
        assert_eq!(parsed[0].id, comment.id);
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
        assert_eq!(parsed[0].target.node_index(), Some(5));
        assert!(matches!(
            &parsed[0].target,
            CommentTarget::Text {
                subtarget: BlockSubtarget::Paragraph { .. },
                ..
            }
        ));
    }

    #[test]
    fn test_sorting_respects_targets() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment_a = create_paragraph_comment("chapter.xhtml", 1, "A");
        let comment_b = create_code_comment("chapter.xhtml", 1, (2, 4), "B");
        let comment_c = create_paragraph_comment("chapter.xhtml", 0, "C");

        book_comments.add_comment(comment_a).unwrap();
        book_comments.add_comment(comment_b).unwrap();
        book_comments.add_comment(comment_c).unwrap();

        let all = book_comments.get_all_comments();
        assert_eq!(all[0].node_index(), Some(0));
        assert_eq!(all[1].node_index(), Some(1));
        assert!(all[1].is_paragraph_comment());
        assert!(all[2].target.is_code_block());
    }

    #[test]
    fn test_multiple_paragraph_comments_same_node() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment_a = create_paragraph_comment("chapter.xhtml", 1, "First note");
        let comment_b = create_paragraph_comment("chapter.xhtml", 1, "Second note");
        let comment_c = create_paragraph_comment("chapter.xhtml", 1, "Third note");

        book_comments.add_comment(comment_a).unwrap();
        book_comments.add_comment(comment_b).unwrap();
        book_comments.add_comment(comment_c).unwrap();

        let comments = book_comments.get_node_comments("chapter.xhtml", 1);
        assert_eq!(comments.len(), 3);
        assert_eq!(comments[0].content, "First note");
        assert_eq!(comments[1].content, "Second note");
        assert_eq!(comments[2].content, "Third note");
    }
}
