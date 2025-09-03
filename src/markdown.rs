pub type Range<Idx> = std::ops::Range<Idx>;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub blocks: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub block: Block,
    pub source_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: HeadingLevel,
        content: Text,
    },
    Paragraph {
        content: Text,
    },
    CodeBlock {
        language: Option<String>,
        content: String,
    },
    Quote {
        content: Vec<Node>,
    },
    List {
        kind: ListKind,
        items: Vec<ListItem>,
    },
    Table {
        header: Option<TableRow>,
        rows: Vec<TableRow>,
        alignment: Vec<TableAlignment>,
    },
    DefinitionList {
        items: Vec<DefinitionListItem>,
    },
    EpubBlock {
        epub_type: String,
        element_name: String,
        content: Vec<Node>,
    },
    ThematicBreak,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Style {
    Code,
    Emphasis,
    Strong,
    Strikethrough,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextNode {
    pub content: String,
    pub style: Option<Style>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Text(Vec<TextOrInline>);

#[derive(Debug, Clone, PartialEq)]
pub enum TextOrInline {
    Text(TextNode),
    Inline(Inline),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Link {
        text: Text,
        url: String,
        title: Option<String>,
    },
    Image {
        alt_text: String,
        url: String,
        title: Option<String>,
    },
    LineBreak,
    SoftBreak,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadingLevel {
    H1 = 1,
    H2 = 2,
    H3 = 3,
    H4 = 4,
    H5 = 5,
    H6 = 6,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListKind {
    Ordered { start: u32 },
    Unordered,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Checked,
    Unchecked,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub content: Vec<Node>,
    pub task_status: Option<TaskStatus>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub content: Text,
    pub is_header: bool,
    pub rowspan: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefinitionListItem {
    pub term: Text,
    pub definitions: Vec<Text>,
}

impl HeadingLevel {
    pub fn from_u8(level: u8) -> Option<Self> {
        match level {
            1 => Some(HeadingLevel::H1),
            2 => Some(HeadingLevel::H2),
            3 => Some(HeadingLevel::H3),
            4 => Some(HeadingLevel::H4),
            5 => Some(HeadingLevel::H5),
            6 => Some(HeadingLevel::H6),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl Node {
    pub fn new(block: Block, source_range: Range<usize>) -> Self {
        Self {
            block,
            source_range,
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Document { blocks: Vec::new() }
    }
}

impl TextNode {
    pub fn new(content: String, style: Option<Style>) -> Self {
        Self { content, style }
    }
}

impl ListItem {
    pub fn new(content: Vec<Node>) -> Self {
        ListItem {
            content,
            task_status: None,
        }
    }

    pub fn new_task(content: Vec<Node>, status: TaskStatus) -> Self {
        ListItem {
            content,
            task_status: Some(status),
        }
    }
}

impl TableRow {
    pub fn new(cells: Vec<TableCell>) -> Self {
        TableRow { cells }
    }
}

impl DefinitionListItem {
    pub fn new(term: Text, definitions: Vec<Text>) -> Self {
        DefinitionListItem { term, definitions }
    }

    pub fn new_single(term: Text, definition: Text) -> Self {
        DefinitionListItem {
            term,
            definitions: vec![definition],
        }
    }
}

impl TableCell {
    pub fn new(content: Text) -> Self {
        TableCell {
            content,
            is_header: false,
            rowspan: 1,
        }
    }

    pub fn new_header(content: Text) -> Self {
        TableCell {
            content,
            is_header: true,
            rowspan: 1,
        }
    }

    pub fn new_with_rowspan(content: Text, rowspan: u32) -> Self {
        TableCell {
            content,
            is_header: false,
            rowspan,
        }
    }

    pub fn new_header_with_rowspan(content: Text, rowspan: u32) -> Self {
        TableCell {
            content,
            is_header: true,
            rowspan,
        }
    }
}

// From implementations for Text
impl From<&str> for Text {
    fn from(value: &str) -> Self {
        TextNode::from(value).into()
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        TextNode::from(value).into()
    }
}

impl From<TextNode> for Text {
    fn from(value: TextNode) -> Self {
        Self(vec![TextOrInline::Text(value)])
    }
}

impl From<Vec<TextNode>> for Text {
    fn from(value: Vec<TextNode>) -> Self {
        Self(value.into_iter().map(TextOrInline::Text).collect())
    }
}

impl From<TextOrInline> for Text {
    fn from(value: TextOrInline) -> Self {
        Self(vec![value])
    }
}

impl From<Vec<TextOrInline>> for Text {
    fn from(value: Vec<TextOrInline>) -> Self {
        Self(value)
    }
}

// From implementations for TextNode
impl From<&str> for TextNode {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

impl From<String> for TextNode {
    fn from(value: String) -> Self {
        Self {
            content: value,
            ..Default::default()
        }
    }
}

// Text methods
impl Text {
    pub fn push(&mut self, item: TextOrInline) {
        self.0.push(item);
    }

    pub fn push_text(&mut self, node: TextNode) {
        self.0.push(TextOrInline::Text(node));
    }

    pub fn push_inline(&mut self, inline: Inline) {
        self.0.push(TextOrInline::Inline(inline));
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> std::slice::Iter<TextOrInline> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<TextOrInline> {
        self.0.iter_mut()
    }
}

// IntoIterator for Text
impl IntoIterator for Text {
    type Item = TextOrInline;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
