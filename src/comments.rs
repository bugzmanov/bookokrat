use crate::annotations::HighlightColor;
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

/// Address of an annotatable block in the parsed chapter AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockAddress {
    pub node_index: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_path: Vec<usize>,
}

impl BlockAddress {
    pub fn root(node_index: usize) -> Self {
        Self {
            node_index,
            child_path: Vec::new(),
        }
    }

    pub fn child(&self, child_index: usize) -> Self {
        let mut child_path = self.child_path.clone();
        child_path.push(child_index);
        Self {
            node_index: self.node_index,
            child_path,
        }
    }
}

/// One contiguous slice of EPUB text inside a single block.
/// A multi-paragraph highlight is a `CommentTarget::Text` whose `slices`
/// contains one of these per touched block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSlice {
    #[serde(flatten)]
    pub block: BlockAddress,
    #[serde(flatten)]
    pub subtarget: BlockSubtarget,
}

impl TextSlice {
    pub fn new(node_index: usize, subtarget: BlockSubtarget) -> Self {
        Self::new_at(BlockAddress::root(node_index), subtarget)
    }

    pub fn new_at(block: BlockAddress, subtarget: BlockSubtarget) -> Self {
        Self { block, subtarget }
    }
}

/// Identifies the location of a comment within the document.
/// Supports both EPUB text-based targeting and PDF pixel-based targeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentTarget {
    /// EPUB text-based targeting. A comment may span multiple blocks
    /// (e.g. a highlight across two paragraphs); each touched block
    /// contributes one `TextSlice`. The vec is non-empty by construction —
    /// callers can rely on `slices.first()` always being `Some` for `Text`.
    Text { slices: Vec<TextSlice> },
    /// PDF pixel-based targeting
    Pdf {
        page: usize,
        rects: Vec<PdfSelectionRect>,
    },
}

impl CommentTarget {
    /// Build a Text target from one or more slices. Caller's responsibility
    /// to ensure `slices` is non-empty.
    pub fn from_slices(slices: Vec<TextSlice>) -> Self {
        debug_assert!(
            !slices.is_empty(),
            "Text target must have at least one slice"
        );
        Self::Text { slices }
    }

    fn from_single(node_index: usize, subtarget: BlockSubtarget) -> Self {
        Self::from_single_at(BlockAddress::root(node_index), subtarget)
    }

    fn from_single_at(block: BlockAddress, subtarget: BlockSubtarget) -> Self {
        Self::Text {
            slices: vec![TextSlice { block, subtarget }],
        }
    }

    /// Create a Text target for EPUB paragraph
    pub fn paragraph(node_index: usize, word_range: Option<(usize, usize)>) -> Self {
        Self::from_single(node_index, BlockSubtarget::Paragraph { word_range })
    }

    /// Create a Text target for EPUB list item
    pub fn list_item(
        node_index: usize,
        item_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        Self::from_single(
            node_index,
            BlockSubtarget::ListItem {
                item_index,
                list_path: Vec::new(),
                word_range,
            },
        )
    }

    /// Create a Text target for EPUB list item with path
    pub fn list_item_with_path(
        node_index: usize,
        list_path: Vec<usize>,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        let item_index = list_path.last().copied().unwrap_or(0);
        Self::from_single(
            node_index,
            BlockSubtarget::ListItem {
                item_index,
                list_path,
                word_range,
            },
        )
    }

    /// Create a Text target for EPUB quote paragraph
    pub fn quote_paragraph(
        node_index: usize,
        paragraph_index: usize,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        Self::from_single(
            node_index,
            BlockSubtarget::QuoteParagraph {
                paragraph_index,
                word_range,
            },
        )
    }

    /// Create a Text target for EPUB definition item
    pub fn definition_item(
        node_index: usize,
        item_index: usize,
        is_term: bool,
        word_range: Option<(usize, usize)>,
    ) -> Self {
        Self::from_single(
            node_index,
            BlockSubtarget::DefinitionItem {
                item_index,
                is_term,
                word_range,
            },
        )
    }

    /// Create a Text target for EPUB code block
    pub fn code_block(node_index: usize, line_range: (usize, usize)) -> Self {
        Self::from_single(node_index, BlockSubtarget::CodeLines { line_range })
    }

    /// Create a Text target for EPUB code block at a nested block address
    pub fn code_block_at(block: BlockAddress, line_range: (usize, usize)) -> Self {
        Self::from_single_at(block, BlockSubtarget::CodeLines { line_range })
    }

    /// Create a Pdf target for PDF selection
    pub fn pdf(page: usize, rects: Vec<PdfSelectionRect>) -> Self {
        Self::Pdf { page, rects }
    }

    /// All slices for a Text target. Empty slice for Pdf.
    pub fn slices(&self) -> &[TextSlice] {
        match self {
            Self::Text { slices } => slices.as_slice(),
            Self::Pdf { .. } => &[],
        }
    }

    /// Number of EPUB blocks this comment spans (`>1` for multi-segment
    /// highlights). Always `0` for PDF.
    pub fn slice_count(&self) -> usize {
        self.slices().len()
    }

    /// Convenience: the first slice for a Text target.
    pub fn first_slice(&self) -> Option<&TextSlice> {
        self.slices().first()
    }

    /// Returns the node index of the **first** slice for Text targets, or None
    /// for Pdf targets. Callers that need to walk every block this comment
    /// touches must use `slices()` instead.
    pub fn node_index(&self) -> Option<usize> {
        self.first_slice().map(|s| s.block.node_index)
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

    /// Word range of the **first** slice. Multi-slice callers should
    /// iterate `slices()` directly to get each block's range.
    pub fn word_range(&self) -> Option<(usize, usize)> {
        self.first_slice().and_then(|s| s.subtarget.word_range())
    }

    pub fn list_item_index(&self) -> Option<usize> {
        self.first_slice()
            .and_then(|s| s.subtarget.list_item_index())
    }

    pub fn definition_item_index(&self) -> Option<usize> {
        self.first_slice()
            .and_then(|s| s.subtarget.definition_item_index())
    }

    pub fn quote_paragraph_index(&self) -> Option<usize> {
        self.first_slice()
            .and_then(|s| s.subtarget.quote_paragraph_index())
    }

    pub fn line_range(&self) -> Option<(usize, usize)> {
        self.first_slice().and_then(|s| s.subtarget.line_range())
    }

    pub fn kind_order(&self) -> u8 {
        match self {
            Self::Text { .. } => self
                .first_slice()
                .map(|s| s.subtarget.kind_order())
                .unwrap_or(0),
            Self::Pdf { .. } => 10, // PDF comments sort after text comments
        }
    }

    pub fn secondary_sort_key(&self) -> (usize, usize) {
        match self {
            Self::Text { .. } => self
                .first_slice()
                .map(|s| s.subtarget.secondary_sort_key())
                .unwrap_or((0, 0)),
            Self::Pdf { page, rects } => {
                let y = rects.first().map(|r| r.topleft_y as usize).unwrap_or(0);
                (*page, y)
            }
        }
    }

    /// True when every slice is a code-block target. Code highlights never
    /// cross blocks today, but we check all slices to be defensive — a future
    /// mixed selection would otherwise silently render as a normal highlight.
    pub fn is_code_block(&self) -> bool {
        match self {
            Self::Text { slices } => {
                !slices.is_empty() && slices.iter().all(|s| s.subtarget.is_code())
            }
            Self::Pdf { .. } => false,
        }
    }

    /// Subtarget of the **first** slice. Multi-slice callers should iterate.
    pub fn subtarget(&self) -> Option<&BlockSubtarget> {
        self.first_slice().map(|s| &s.subtarget)
    }

    /// Returns the list path for Text list item targets (first slice), or None
    pub fn list_path(&self) -> Option<&[usize]> {
        self.first_slice().and_then(|s| s.subtarget.list_path())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AnnotationBody {
    #[default]
    Comment,
    Highlight {
        color: HighlightColor,
    },
}

impl AnnotationBody {
    fn serde_type(&self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Highlight { .. } => "highlight",
        }
    }

    fn highlight_color(&self) -> Option<HighlightColor> {
        match self {
            Self::Comment => None,
            Self::Highlight { color } => Some(*color),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub chapter_href: String,
    pub target: CommentTarget,
    pub content: String,
    pub body: AnnotationBody,
    pub updated_at: DateTime<Utc>,
    /// The quoted/selected text that this comment refers to (primarily for PDF)
    pub quoted_text: Option<String>,
}

/// Serde representation for Text CommentTarget (EPUB format with node_index + subtarget)
#[derive(Serialize, Deserialize)]
struct CommentTextTargetSerde {
    #[serde(flatten)]
    block: BlockAddress,
    #[serde(flatten)]
    subtarget: BlockSubtarget,
}

/// Modern format for EPUB text comments
#[derive(Serialize, Deserialize)]
struct CommentTextSerde {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub chapter_href: String,
    #[serde(default, skip_serializing_if = "is_comment_annotation_type")]
    pub annotation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<HighlightColor>,
    pub target_type: String, // "text"
    #[serde(flatten)]
    pub target: CommentTextTargetSerde,
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
    #[serde(default, skip_serializing_if = "is_comment_annotation_type")]
    pub annotation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<HighlightColor>,
    pub target_type: String, // "pdf"
    pub page: usize,
    pub rects: Vec<PdfSelectionRect>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quoted_text: Option<String>,
}

/// New multi-slice format for EPUB text comments. Distinguished from the
/// legacy single-slice format by the presence of a `slices` array (and the
/// absence of a top-level `node_index`). Both shapes share the same
/// untagged enum, with this one ordered before the single-slice variant so
/// it wins when both fields are absent (e.g. an empty multi-slice — which
/// shouldn't happen but defensive).
#[derive(Serialize, Deserialize)]
struct CommentTextMultiSerde {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub chapter_href: String,
    #[serde(default, skip_serializing_if = "is_comment_annotation_type")]
    pub annotation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<HighlightColor>,
    pub target_type: String, // "text"
    pub slices: Vec<TextSlice>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
    // TextMulti is tried first: it owns the `slices` field that no legacy
    // variant has, so it matches only when the new format is present.
    TextMulti(CommentTextMultiSerde),
    Text(CommentTextSerde),
    Pdf(CommentPdfSerde),
    ModernLegacy(CommentModernLegacySerde),
    LegacyCodeBlock(CommentLegacyCodeBlockSerde),
    LegacyParagraph(CommentLegacyParagraphSerde),
}

impl From<CommentTextMultiSerde> for Comment {
    fn from(s: CommentTextMultiSerde) -> Self {
        Comment {
            id: s.id.unwrap_or_else(generate_comment_id),
            chapter_href: s.chapter_href,
            target: CommentTarget::Text { slices: s.slices },
            content: s.content,
            body: body_from_serde(&s.annotation_type, s.color),
            updated_at: s.updated_at,
            quoted_text: s.quoted_text,
        }
    }
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
            target: CommentTarget::from_single(legacy.paragraph_index, subtarget),
            content: legacy.content,
            body: AnnotationBody::Comment,
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
            body: AnnotationBody::Comment,
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
            target: CommentTarget::from_single_at(modern.target.block, modern.target.subtarget),
            content: modern.content,
            body: AnnotationBody::Comment,
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
            target: CommentTarget::from_single_at(text.target.block, text.target.subtarget),
            content: text.content,
            body: body_from_serde(&text.annotation_type, text.color),
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
            body: body_from_serde(&pdf.annotation_type, pdf.color),
            updated_at: pdf.updated_at,
            quoted_text: pdf.quoted_text,
        }
    }
}

fn body_from_serde(annotation_type: &str, color: Option<HighlightColor>) -> AnnotationBody {
    if annotation_type.eq_ignore_ascii_case("highlight") {
        AnnotationBody::Highlight {
            color: color.unwrap_or(HighlightColor::Yellow),
        }
    } else {
        AnnotationBody::Comment
    }
}

fn is_comment_annotation_type(annotation_type: &str) -> bool {
    annotation_type.is_empty() || annotation_type.eq_ignore_ascii_case("comment")
}

impl Serialize for Comment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.target {
            CommentTarget::Text { slices } => {
                // Smart-emit: single-slice highlights keep the legacy
                // top-level `node_index + subtarget` shape so older app
                // versions can still read them. Only multi-slice comments
                // (new in this refactor) emit the `slices:` array, which
                // older apps can't parse and will preserve verbatim via the
                // fault-tolerant loader.
                if slices.len() == 1 {
                    let first = &slices[0];
                    let serde = CommentTextSerde {
                        id: Some(self.id.clone()),
                        chapter_href: self.chapter_href.clone(),
                        annotation_type: self.body.serde_type().to_string(),
                        color: self.body.highlight_color(),
                        target_type: "text".to_string(),
                        target: CommentTextTargetSerde {
                            block: first.block.clone(),
                            subtarget: first.subtarget.clone(),
                        },
                        content: self.content.clone(),
                        updated_at: self.updated_at,
                        quoted_text: self.quoted_text.clone(),
                    };
                    serde.serialize(serializer)
                } else {
                    let serde = CommentTextMultiSerde {
                        id: Some(self.id.clone()),
                        chapter_href: self.chapter_href.clone(),
                        annotation_type: self.body.serde_type().to_string(),
                        color: self.body.highlight_color(),
                        target_type: "text".to_string(),
                        slices: slices.clone(),
                        content: self.content.clone(),
                        updated_at: self.updated_at,
                        quoted_text: self.quoted_text.clone(),
                    };
                    serde.serialize(serializer)
                }
            }
            CommentTarget::Pdf { page, rects } => {
                let serde = CommentPdfSerde {
                    id: Some(self.id.clone()),
                    chapter_href: self.chapter_href.clone(),
                    annotation_type: self.body.serde_type().to_string(),
                    color: self.body.highlight_color(),
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
            CommentSerde::TextMulti(multi) => Ok(Comment::from(multi)),
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
            body: AnnotationBody::Comment,
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
            body: AnnotationBody::Comment,
            updated_at,
            quoted_text,
        }
    }

    pub fn new_highlight(
        chapter_href: String,
        target: CommentTarget,
        color: HighlightColor,
        updated_at: DateTime<Utc>,
        quoted_text: Option<String>,
    ) -> Self {
        Self {
            id: generate_comment_id(),
            chapter_href,
            target,
            content: String::new(),
            body: AnnotationBody::Highlight { color },
            updated_at,
            quoted_text,
        }
    }

    pub fn is_comment(&self) -> bool {
        matches!(self.body, AnnotationBody::Comment)
    }

    pub fn is_highlight(&self) -> bool {
        matches!(self.body, AnnotationBody::Highlight { .. })
    }

    pub fn highlight_color(&self) -> Option<HighlightColor> {
        self.body.highlight_color()
    }

    fn body_order(&self) -> u8 {
        match self.body {
            AnnotationBody::Comment => 0,
            AnnotationBody::Highlight { .. } => 1,
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

    /// Returns the primary index key used for storage lookup (first slice's
    /// node_index for Text, page for Pdf). Multi-slice indexing happens
    /// separately in `BookComments::add_to_indices` which inserts under
    /// every slice.
    pub fn index_key(&self) -> usize {
        self.target
            .node_index()
            .or_else(|| self.target.page())
            .unwrap_or(0)
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

    pub fn overlaps_target(&self, chapter_href: &str, target: &CommentTarget) -> bool {
        self.chapter_href == chapter_href && self.target.overlaps(target)
    }
}

impl CommentTarget {
    pub fn overlaps(&self, other: &CommentTarget) -> bool {
        match (self, other) {
            (
                CommentTarget::Text { slices: a_slices },
                CommentTarget::Text { slices: b_slices },
            ) => {
                // Multi-slice ↔ multi-slice: any pair of slices that targets
                // the same (node_index, subtarget scope) and whose
                // word/line ranges intersect makes the whole comments
                // overlap. One overlapping slice is enough — we shouldn't
                // let users hide a real conflict behind other paragraphs.
                a_slices
                    .iter()
                    .any(|a| b_slices.iter().any(|b| slices_overlap(a, b)))
            }
            (CommentTarget::Pdf { rects: a, .. }, CommentTarget::Pdf { rects: b, .. }) => {
                a.iter().any(|left| {
                    b.iter()
                        .any(|right| left.page == right.page && pdf_rects_overlap(left, right))
                })
            }
            _ => false,
        }
    }
}

fn slices_overlap(a: &TextSlice, b: &TextSlice) -> bool {
    if a.block != b.block || !same_subtarget_scope(&a.subtarget, &b.subtarget) {
        return false;
    }
    match (a.subtarget.line_range(), b.subtarget.line_range()) {
        (Some(la), Some(lb)) => inclusive_ranges_overlap(la, lb),
        _ => ranges_overlap(
            a.subtarget.word_range().unwrap_or((0, usize::MAX)),
            b.subtarget.word_range().unwrap_or((0, usize::MAX)),
        ),
    }
}

fn same_subtarget_scope(a: &BlockSubtarget, b: &BlockSubtarget) -> bool {
    match (a, b) {
        (BlockSubtarget::Paragraph { .. }, BlockSubtarget::Paragraph { .. }) => true,
        (
            BlockSubtarget::ListItem {
                item_index: a_idx,
                list_path: a_path,
                ..
            },
            BlockSubtarget::ListItem {
                item_index: b_idx,
                list_path: b_path,
                ..
            },
        ) => a_idx == b_idx && a_path == b_path,
        (
            BlockSubtarget::QuoteParagraph {
                paragraph_index: a_idx,
                ..
            },
            BlockSubtarget::QuoteParagraph {
                paragraph_index: b_idx,
                ..
            },
        ) => a_idx == b_idx,
        (
            BlockSubtarget::DefinitionItem {
                item_index: a_idx,
                is_term: a_term,
                ..
            },
            BlockSubtarget::DefinitionItem {
                item_index: b_idx,
                is_term: b_term,
                ..
            },
        ) => a_idx == b_idx && a_term == b_term,
        (BlockSubtarget::CodeLines { .. }, BlockSubtarget::CodeLines { .. }) => true,
        _ => false,
    }
}

fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

fn inclusive_ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}

/// Eager migration from the legacy "group" model: N single-slice highlights
/// sharing a `group_id` become **one** multi-slice highlight whose `slices`
/// concatenate the original members in their stored order. The first member
/// keeps the merged Comment's id; the rest are discarded. Input pairs each
/// Comment with the `group_id` we read out of its YAML before deserialising
/// — that field doesn't exist on `Comment` anymore.
fn merge_legacy_groups(items: Vec<(Comment, Option<String>)>) -> Vec<Comment> {
    use std::collections::HashMap;

    let mut groups: HashMap<String, usize> = HashMap::new(); // group_id -> output idx
    let mut out: Vec<Comment> = Vec::with_capacity(items.len());

    for (c, gid) in items {
        let Some(gid) = gid else {
            out.push(c);
            continue;
        };
        // Only the first member's slices are kept verbatim; later members
        // append their slices to it. For non-Text targets (PDF) the group
        // semantics are undefined — pass them through unchanged.
        match groups.get(&gid).copied() {
            Some(idx) => {
                let merged_into_existing = matches!(
                    (&out[idx].target, &c.target),
                    (CommentTarget::Text { .. }, CommentTarget::Text { .. })
                );
                if merged_into_existing {
                    if let (
                        CommentTarget::Text { slices: existing },
                        CommentTarget::Text { slices: more },
                    ) = (&mut out[idx].target, c.target)
                    {
                        existing.extend(more);
                    }
                } else {
                    // Mixed-target group — shouldn't happen in practice.
                    // Keep the orphan as its own Comment so we don't silently drop it.
                    out.push(c);
                }
            }
            None => {
                groups.insert(gid, out.len());
                out.push(c);
            }
        }
    }

    out
}

fn pdf_rects_overlap(a: &PdfSelectionRect, b: &PdfSelectionRect) -> bool {
    a.topleft_x < b.bottomright_x
        && b.topleft_x < a.bottomright_x
        && a.topleft_y < b.bottomright_y
        && b.topleft_y < a.bottomright_y
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
    /// YAML entries we couldn't deserialize at load time — preserved verbatim
    /// so a save round-trip doesn't destroy comments written by a newer app
    /// version. Appended after the parseable entries on save.
    unparseable_entries: Vec<serde_yaml::Value>,
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
            unparseable_entries: Vec::new(),
        }
    }

    fn new_with_path(file_path: PathBuf) -> Result<Self> {
        let (comments, unparseable_entries) = if file_path.exists() {
            Self::load_from_file(&file_path)?
        } else {
            (Vec::new(), Vec::new())
        };

        let mut book_comments = Self {
            file_path,
            comments: Vec::new(),
            comments_by_location: HashMap::new(),
            comments_by_id: HashMap::new(),
            unparseable_entries,
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

    /// Change the color of an existing highlight in place, keeping its target.
    /// Multi-slice highlights are now a single Comment, so no fan-out is
    /// needed — one Comment ↔ one logical highlight.
    pub fn set_highlight_color_by_id(
        &mut self,
        comment_id: &str,
        color: HighlightColor,
    ) -> Result<()> {
        let idx = self
            .find_comment_index_by_id(comment_id)
            .context("Comment not found")?;

        self.comments[idx].body = AnnotationBody::Highlight { color };
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

    /// Delete a comment by id. Returns `Ok` when the id is absent — kept
    /// idempotent so legacy call sites that iterated over former group
    /// members (when one logical highlight was N storage comments) don't
    /// spuriously fail after the migration.
    pub fn delete_comment_by_id(&mut self, comment_id: &str) -> Result<()> {
        let Some(idx) = self.find_comment_index_by_id(comment_id) else {
            return Ok(());
        };
        self.comments.remove(idx);
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

    /// Get all PDF comments for a document. Order is unspecified — callers that need
    /// a stable ordering must sort the result themselves.
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
        // A multi-slice comment is registered under every slice's node, so
        // flat-mapping the buckets returns the same Comment N times. Dedupe
        // by storage index so callers see each Comment exactly once.
        let Some(chapter_map) = self.comments_by_location.get(chapter_href) else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for indices in chapter_map.values() {
            for &i in indices {
                if seen.insert(i) {
                    out.push(&self.comments[i]);
                }
            }
        }
        out
    }

    pub fn get_all_comments(&self) -> &[Comment] {
        &self.comments
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn testing_set_all_updated_at(&mut self, updated_at: DateTime<Utc>) {
        for comment in &mut self.comments {
            comment.updated_at = updated_at;
        }
    }

    pub fn has_overlapping_annotation(&self, chapter_href: &str, target: &CommentTarget) -> bool {
        // Comments and highlights share the same visual annotation layer, so
        // new annotations are rejected when they would overlap an existing one.
        self.comments
            .iter()
            .any(|comment| comment.overlaps_target(chapter_href, target))
    }

    /// Find the first highlight whose target overlaps the given target.
    /// Uses the same scope/overlap rules as `has_overlapping_annotation`, so
    /// the highlight palette never disagrees with the add-highlight overlap
    /// check (i.e. if `add_highlight` would reject the selection as
    /// overlapping, this will return the offending highlight).
    pub fn find_overlapping_highlight(
        &self,
        chapter_href: &str,
        target: &CommentTarget,
    ) -> Option<&Comment> {
        self.comments
            .iter()
            .find(|comment| comment.is_highlight() && comment.overlaps_target(chapter_href, target))
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

    /// Returns `(parseable_comments, unparseable_raw_yaml_entries)`.
    /// Per-element fault tolerance: a single corrupt or future-format entry
    /// must not abort loading every annotation in the file. Unparseable
    /// entries are kept in raw form so `save_to_disk` can write them back —
    /// otherwise an older app version round-tripping a newer file would
    /// silently destroy comments it didn't recognise.
    fn load_from_file(file_path: &Path) -> Result<(Vec<Comment>, Vec<serde_yaml::Value>)> {
        let content = fs::read_to_string(file_path).context("Failed to read comments file")?;

        if content.trim().is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let raw: Vec<serde_yaml::Value> =
            serde_yaml::from_str(&content).context("Failed to parse comments YAML as a list")?;

        let mut items: Vec<(Comment, Option<String>)> = Vec::with_capacity(raw.len());
        let mut unparseable: Vec<serde_yaml::Value> = Vec::new();
        for (idx, value) in raw.into_iter().enumerate() {
            // `group_id` only exists in legacy single-slice YAML. Pull it
            // out of the raw Value here so we can feed it to the merge pass
            // — `Comment` itself no longer carries the field, and serde
            // happily ignores unknown YAML keys on deserialise.
            // `group_id` only appears in legacy single-slice YAML. Pull it
            // out of the raw Value here so we can feed it to the merge pass
            // — `Comment` itself no longer carries the field, and serde
            // happily ignores unknown YAML keys on deserialise.
            let group_id = value.as_mapping().and_then(|m| {
                m.get(serde_yaml::Value::String("group_id".to_string()))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            });
            match serde_yaml::from_value::<Comment>(value.clone()) {
                Ok(comment) => items.push((comment, group_id)),
                Err(e) => {
                    log::warn!(
                        "Preserving unparseable comment at index {idx} in {} across save round-trip: {e}",
                        file_path.display()
                    );
                    unparseable.push(value);
                }
            }
        }
        // One-way upgrade: legacy YAML stored N single-slice Comments tied
        // by a shared `group_id`. The new model represents them as one
        // multi-slice Comment, so we eagerly merge them here.
        let comments = merge_legacy_groups(items);
        Ok((comments, unparseable))
    }

    fn save_to_disk(&self) -> Result<()> {
        // Skip saving if file_path is empty (test mode)
        if self.file_path.as_os_str().is_empty() {
            return Ok(());
        }

        // Preserve unparseable entries (written by newer app versions or
        // hand-edited oddities) by re-emitting them after the parseable ones.
        // Without this an older app reading a newer file would silently
        // destroy any comment it couldn't recognise on the next save.
        let mut all: Vec<serde_yaml::Value> =
            Vec::with_capacity(self.comments.len() + self.unparseable_entries.len());
        for comment in &self.comments {
            all.push(serde_yaml::to_value(comment).context("Failed to serialize a comment")?);
        }
        all.extend(self.unparseable_entries.iter().cloned());

        let yaml = serde_yaml::to_string(&all).context("Failed to serialize comments")?;

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

    /// Insert this comment into the location index under every key it
    /// touches. A multi-slice text comment lives in one bucket per
    /// `node_index`, so `get_node_comments(chapter, n)` finds it regardless
    /// of which of its blocks `n` refers to. PDF comments index under page
    /// (matching their existing single-key behaviour).
    fn add_to_indices(&mut self, comment: &Comment) {
        let idx = self.comments.len();
        let chapter_map = self
            .comments_by_location
            .entry(comment.chapter_href.clone())
            .or_default();
        for key in Self::index_keys_for(comment) {
            let bucket = chapter_map.entry(key).or_default();
            // Defensive: avoid duplicate entries if a multi-slice comment
            // happens to have two slices on the same block.
            if !bucket.contains(&idx) {
                bucket.push(idx);
            }
        }
        self.comments_by_id.insert(comment.id.clone(), idx);
    }

    fn rebuild_indices(&mut self) {
        self.comments_by_location.clear();
        self.comments_by_id.clear();
        for idx in 0..self.comments.len() {
            // Mirror add_to_indices but skipping the assumed-fresh-idx
            // semantics. We rebuild after every mutation that could shift
            // positions, so duplicate-key guards still apply.
            let (chapter_href, keys) = {
                let comment = &self.comments[idx];
                (comment.chapter_href.clone(), Self::index_keys_for(comment))
            };
            let chapter_map = self.comments_by_location.entry(chapter_href).or_default();
            for key in keys {
                let bucket = chapter_map.entry(key).or_default();
                if !bucket.contains(&idx) {
                    bucket.push(idx);
                }
            }
            self.comments_by_id
                .insert(self.comments[idx].id.clone(), idx);
        }
    }

    fn index_keys_for(comment: &Comment) -> Vec<usize> {
        match &comment.target {
            CommentTarget::Text { slices } => {
                let mut keys: Vec<usize> = slices.iter().map(|s| s.block.node_index).collect();
                keys.sort_unstable();
                keys.dedup();
                keys
            }
            CommentTarget::Pdf { page, .. } => vec![*page],
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
                .then(a.body_order().cmp(&b.body_order()))
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
    fn test_comment_serialization_keeps_comment_type_implicit() {
        let comment = create_paragraph_comment("chapter.xhtml", 3, "plain note");
        let yaml = serde_yaml::to_string(&vec![comment]).unwrap();

        assert!(!yaml.contains("annotation_type"));
        assert!(!yaml.contains("color:"));
    }

    #[test]
    fn test_multi_slice_highlight_roundtrip() {
        let highlight = Comment {
            id: "multi-1".to_string(),
            chapter_href: "ch.xhtml".to_string(),
            target: CommentTarget::Text {
                slices: vec![
                    TextSlice::new(
                        2,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 10)),
                        },
                    ),
                    TextSlice::new(
                        4,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 8)),
                        },
                    ),
                ],
            },
            content: String::new(),
            body: AnnotationBody::Highlight {
                color: HighlightColor::Green,
            },
            updated_at: Utc::now(),
            quoted_text: Some("first ... last".to_string()),
        };

        let yaml = serde_yaml::to_string(&vec![highlight.clone()]).unwrap();
        assert!(
            yaml.contains("slices:"),
            "multi-slice highlight must serialize with slices array, got:\n{yaml}"
        );

        // Roundtrip equality covers the "shape preserved" claim better than
        // raw-text inspection.
        let parsed: Vec<Comment> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].target.slices().len(), 2);
        assert_eq!(parsed[0].target.slices()[0].block.node_index, 2);
        assert_eq!(parsed[0].target.slices()[1].block.node_index, 4);
        assert_eq!(parsed[0].highlight_color(), Some(HighlightColor::Green));
    }

    #[test]
    fn test_single_slice_still_uses_legacy_shape() {
        // Back-compat with older apps: single-slice highlights must emit
        // the top-level node_index format so old versions of bookokrat keep
        // reading them. Only multi-slice flips to the new shape.
        let highlight = Comment::new_highlight(
            "ch.xhtml".to_string(),
            CommentTarget::paragraph(3, Some((2, 7))),
            HighlightColor::Yellow,
            Utc::now(),
            None,
        );
        let yaml = serde_yaml::to_string(&vec![highlight]).unwrap();
        assert!(
            yaml.contains("node_index: 3"),
            "single-slice highlight must keep legacy node_index shape, got:\n{yaml}"
        );
        assert!(
            !yaml.contains("slices:"),
            "single-slice must not emit a slices array, got:\n{yaml}"
        );
    }

    #[test]
    fn test_highlight_serialization_roundtrip() {
        let highlight = Comment::new_highlight(
            "chapter.xhtml".to_string(),
            CommentTarget::paragraph(3, Some((2, 7))),
            HighlightColor::Blue,
            Utc::now(),
            Some("quoted".to_string()),
        );
        let yaml = serde_yaml::to_string(&vec![highlight.clone()]).unwrap();

        assert!(yaml.contains("annotation_type: highlight"));
        assert!(yaml.contains("color: blue"));
        assert!(!yaml.contains("content:"));

        let parsed: Vec<Comment> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].is_highlight());
        assert_eq!(parsed[0].highlight_color(), Some(HighlightColor::Blue));
        assert_eq!(parsed[0].target, highlight.target);
        assert_eq!(parsed[0].content, "");
    }

    #[test]
    fn test_overlap_detection_uses_half_open_word_ranges() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment = Comment::new(
            "chapter.xhtml".to_string(),
            CommentTarget::paragraph(1, Some((2, 5))),
            "note".to_string(),
            Utc::now(),
        );
        book_comments.add_comment(comment).unwrap();

        assert!(book_comments.has_overlapping_annotation(
            "chapter.xhtml",
            &CommentTarget::paragraph(1, Some((4, 7)))
        ));
        assert!(!book_comments.has_overlapping_annotation(
            "chapter.xhtml",
            &CommentTarget::paragraph(1, Some((5, 8)))
        ));
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
        let first = parsed[0]
            .target
            .first_slice()
            .expect("Text target has slices");
        assert!(matches!(first.subtarget, BlockSubtarget::Paragraph { .. }));
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
    fn test_sorting_orders_target_before_annotation_body() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let comment = Comment::new(
            "chapter.xhtml".to_string(),
            CommentTarget::paragraph(1, Some((10, 12))),
            "later note".to_string(),
            Utc::now(),
        );
        let highlight = Comment::new_highlight(
            "chapter.xhtml".to_string(),
            CommentTarget::paragraph(1, Some((2, 4))),
            HighlightColor::Yellow,
            Utc::now(),
            None,
        );

        book_comments.add_comment(comment).unwrap();
        book_comments.add_comment(highlight).unwrap();

        let all = book_comments.get_all_comments();
        assert!(all[0].is_highlight());
        assert_eq!(all[0].target.word_range(), Some((2, 4)));
        assert!(all[1].is_comment());
        assert_eq!(all[1].target.word_range(), Some((10, 12)));
    }

    #[test]
    fn test_get_doc_comments_returns_all_pages() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let rect = |page| {
            vec![PdfSelectionRect {
                page,
                topleft_x: 10,
                topleft_y: 20,
                bottomright_x: 40,
                bottomright_y: 50,
            }]
        };

        book_comments
            .add_comment(Comment::with_quoted_text(
                "doc.pdf".to_string(),
                CommentTarget::pdf(2, rect(2)),
                "page 3".to_string(),
                Utc::now(),
                None,
            ))
            .unwrap();
        book_comments
            .add_comment(Comment::with_quoted_text(
                "doc.pdf".to_string(),
                CommentTarget::pdf(0, rect(0)),
                "page 1".to_string(),
                Utc::now(),
                None,
            ))
            .unwrap();
        book_comments
            .add_comment(Comment::with_quoted_text(
                "doc.pdf".to_string(),
                CommentTarget::pdf(1, rect(1)),
                "page 2".to_string(),
                Utc::now(),
                None,
            ))
            .unwrap();

        let mut pages = book_comments
            .get_doc_comments("doc.pdf")
            .into_iter()
            .filter_map(|comment| comment.page())
            .collect::<Vec<_>>();
        pages.sort_unstable();
        assert_eq!(pages, vec![0, 1, 2]);
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

    // Pre-Option-B "group_id" tests have been removed: multi-slice
    // highlights are one Comment now, so the previously-tested fan-out
    // semantics (delete one → all gone, recolor one → all recoloured) are
    // structurally guaranteed by the data model rather than by helpers.
    // The legacy-group migration path is covered by
    // `test_legacy_group_id_yaml_migrates_to_multi_slice_comment`.

    #[test]
    fn test_loader_drops_unparseable_entries_keeps_good_ones() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let book_hash = BookComments::compute_book_hash(&book_path);
        let file_path = comments_dir.join(format!("book_{book_hash}.yaml"));

        // Mixed YAML: one valid modern highlight, one structurally-correct
        // but unrecognized "future format", one valid legacy paragraph note.
        // The future-format entry must be dropped, the surrounding two must
        // both load.
        let yaml = r#"
- id: keep-1
  chapter_href: ch.xhtml
  annotation_type: highlight
  color: yellow
  target_type: text
  node_index: 1
  subtarget_kind: paragraph
  updated_at: "2024-01-01T12:00:00Z"
- id: drop-me
  chapter_href: ch.xhtml
  target_type: future_unknown_shape
  payload:
    something: 42
  updated_at: "2024-01-01T12:00:00Z"
- chapter_href: ch.xhtml
  paragraph_index: 2
  content: legacy survived
  updated_at: "2024-01-01T12:00:00Z"
"#;
        fs::write(&file_path, yaml).unwrap();

        let book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();
        let loaded: Vec<_> = book_comments.get_all_comments().to_vec();
        assert_eq!(
            loaded.len(),
            2,
            "expected 2 surviving comments, got {}: {loaded:?}",
            loaded.len()
        );
        assert!(loaded.iter().any(|c| c.id == "keep-1"));
        assert!(loaded.iter().any(|c| c.content == "legacy survived"));
    }

    #[test]
    fn test_legacy_group_id_yaml_migrates_to_multi_slice_comment() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let book_hash = BookComments::compute_book_hash(&book_path);
        let file_path = comments_dir.join(format!("book_{book_hash}.yaml"));

        // Pre-refactor YAML: three highlights sharing a group_id, plus an
        // unrelated standalone highlight. The standalone must survive
        // independently; the three group members must merge into one
        // multi-slice Comment with three slices.
        let yaml = r#"
- id: g1-a
  chapter_href: ch.xhtml
  annotation_type: highlight
  color: yellow
  target_type: text
  node_index: 1
  subtarget_kind: paragraph
  group_id: hg-legacy
  updated_at: "2024-01-01T12:00:00Z"
- id: g1-b
  chapter_href: ch.xhtml
  annotation_type: highlight
  color: yellow
  target_type: text
  node_index: 3
  subtarget_kind: paragraph
  group_id: hg-legacy
  updated_at: "2024-01-01T12:00:00Z"
- id: g1-c
  chapter_href: ch.xhtml
  annotation_type: highlight
  color: yellow
  target_type: text
  node_index: 5
  subtarget_kind: paragraph
  group_id: hg-legacy
  updated_at: "2024-01-01T12:00:00Z"
- id: standalone
  chapter_href: ch.xhtml
  annotation_type: highlight
  color: blue
  target_type: text
  node_index: 99
  subtarget_kind: paragraph
  updated_at: "2024-01-01T12:00:00Z"
"#;
        fs::write(&file_path, yaml).unwrap();

        let book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();
        let all = book_comments.get_all_comments();
        assert_eq!(
            all.len(),
            2,
            "3 group members collapse to 1 + 1 standalone = 2, got {}",
            all.len()
        );

        let merged = all
            .iter()
            .find(|c| c.id == "g1-a")
            .expect("group head kept");
        let slices = merged.target.slices();
        let mut nodes: Vec<_> = slices.iter().map(|s| s.block.node_index).collect();
        nodes.sort_unstable();
        assert_eq!(
            nodes,
            vec![1, 3, 5],
            "merged slices must cover all group members"
        );
        // group_id is no longer a field on Comment — the merge consumed it
        // from the raw YAML and produced a multi-slice target instead.
        assert_eq!(
            slices.len(),
            3,
            "merged target must have one slice per legacy member"
        );

        let standalone = all
            .iter()
            .find(|c| c.id == "standalone")
            .expect("standalone preserved");
        assert_eq!(standalone.target.slices().len(), 1);
    }

    #[test]
    fn test_get_chapter_comments_dedupes_multi_slice_comments() {
        // A multi-slice comment is registered under every slice's node. The
        // previous flat-map over location buckets returned it once per
        // bucket — which then caused the text reader to render its
        // `Note // …` block twice for a two-paragraph comment. Dedupe by
        // storage index so each Comment surfaces exactly once.
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let multi = Comment::new(
            "ch.xhtml".to_string(),
            CommentTarget::Text {
                slices: vec![
                    TextSlice::new(
                        1,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 5)),
                        },
                    ),
                    TextSlice::new(
                        3,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 4)),
                        },
                    ),
                ],
            },
            "shared note".to_string(),
            Utc::now(),
        );
        let id = multi.id.clone();
        book_comments.add_comment(multi).unwrap();

        let listed = book_comments.get_chapter_comments("ch.xhtml");
        assert_eq!(
            listed.len(),
            1,
            "multi-slice comment must surface exactly once, got {}: ids={:?}",
            listed.len(),
            listed.iter().map(|c| &c.id).collect::<Vec<_>>()
        );
        assert_eq!(listed[0].id, id);
    }

    #[test]
    fn test_multi_slice_comment_is_findable_via_every_node_index() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let highlight = Comment {
            id: "multi-idx".to_string(),
            chapter_href: "ch.xhtml".to_string(),
            target: CommentTarget::Text {
                slices: vec![
                    TextSlice::new(
                        3,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 5)),
                        },
                    ),
                    TextSlice::new(
                        7,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 8)),
                        },
                    ),
                ],
            },
            content: String::new(),
            body: AnnotationBody::Highlight {
                color: HighlightColor::Blue,
            },
            updated_at: Utc::now(),
            quoted_text: None,
        };
        book_comments.add_comment(highlight).unwrap();

        let from_first = book_comments.get_node_comments("ch.xhtml", 3);
        let from_second = book_comments.get_node_comments("ch.xhtml", 7);
        assert_eq!(
            from_first.len(),
            1,
            "node 3 should find the multi-slice comment"
        );
        assert_eq!(from_second.len(), 1, "node 7 should find the same comment");
        assert_eq!(from_first[0].id, "multi-idx");
        assert_eq!(from_second[0].id, "multi-idx");
        // Nodes the comment doesn't touch must NOT see it.
        assert!(book_comments.get_node_comments("ch.xhtml", 4).is_empty());
    }

    #[test]
    fn test_unparseable_entries_survive_save_roundtrip() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let book_hash = BookComments::compute_book_hash(&book_path);
        let file_path = comments_dir.join(format!("book_{book_hash}.yaml"));

        // One parseable + one future-format entry. After loading, adding a
        // new comment (which triggers a save), and re-reading from disk, the
        // future-format entry must still be there: otherwise an older app
        // reading a newer file silently destroys data on the first save.
        let yaml = r#"
- id: keep-1
  chapter_href: ch.xhtml
  annotation_type: highlight
  color: yellow
  target_type: text
  node_index: 1
  subtarget_kind: paragraph
  updated_at: "2024-01-01T12:00:00Z"
- id: future-1
  chapter_href: ch.xhtml
  target_type: future_unknown_shape
  payload:
    something: 42
  updated_at: "2024-01-01T12:00:00Z"
"#;
        fs::write(&file_path, yaml).unwrap();

        {
            let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();
            // Add a new comment to force a save_to_disk pass.
            book_comments
                .add_comment(create_paragraph_comment(
                    "ch.xhtml",
                    5,
                    "added by current version",
                ))
                .unwrap();
        }

        let on_disk = fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("future-1"),
            "unparseable future entry must round-trip through save, got:\n{on_disk}"
        );
        assert!(
            on_disk.contains("future_unknown_shape"),
            "unparseable target_type must round-trip, got:\n{on_disk}"
        );
        assert!(
            on_disk.contains("added by current version"),
            "newly-added comment must also be present, got:\n{on_disk}"
        );
    }

    #[test]
    fn test_loader_returns_error_when_root_is_not_a_sequence() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let book_hash = BookComments::compute_book_hash(&book_path);
        let file_path = comments_dir.join(format!("book_{book_hash}.yaml"));
        // Top-level structural corruption is genuinely bad data — we should
        // surface it as an error, not silently treat the whole file as empty.
        fs::write(&file_path, "not a list, just a scalar string").unwrap();

        let result = BookComments::new(&book_path, Some(&comments_dir));
        assert!(
            result.is_err(),
            "loader must surface structural corruption, got Ok"
        );
    }

    #[test]
    fn test_recolor_by_id_changes_color_in_place() {
        let (_temp_dir, book_path, comments_dir) = create_test_env();
        let mut book_comments = BookComments::new(&book_path, Some(&comments_dir)).unwrap();

        let highlight = Comment::new_highlight(
            "ch.xhtml".to_string(),
            CommentTarget::Text {
                slices: vec![
                    TextSlice::new(
                        1,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 10)),
                        },
                    ),
                    TextSlice::new(
                        2,
                        BlockSubtarget::Paragraph {
                            word_range: Some((0, 8)),
                        },
                    ),
                ],
            },
            HighlightColor::Yellow,
            Utc::now(),
            None,
        );
        let id = highlight.id.clone();
        book_comments.add_comment(highlight).unwrap();

        book_comments
            .set_highlight_color_by_id(&id, HighlightColor::Red)
            .unwrap();

        let updated = book_comments
            .get_comment_by_id(&id)
            .expect("highlight still present");
        assert_eq!(updated.highlight_color(), Some(HighlightColor::Red));
        // Multi-slice → one Comment, so the color change applies to the
        // whole logical highlight by virtue of being on a single struct.
        assert_eq!(updated.target.slices().len(), 2);
    }
}
