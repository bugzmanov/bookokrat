use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A persisted reference to a position in some document.
///
/// Both variants are always present in the type so a marks file written by a
/// PDF-enabled build still loads correctly in a build without the `pdf`
/// feature (the PDF mark is just unreachable then).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarkLocation {
    Epub {
        path: String,
        chapter: usize,
        node: usize,
        /// Canonical character offset within the paragraph (`node`) at which
        /// the mark was set. Robust to terminal resize / margin / justify
        /// changes (which re-wrap the paragraph into different visual lines).
        /// `None` for marks set before this field existed — fall back to the
        /// start of the paragraph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_offset: Option<usize>,
        /// Text excerpt captured at mark-set time. Used for popup labels so
        /// global marks pointing to other books can show context without
        /// loading those books.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snippet: Option<String>,
        /// Chapter title captured at mark-set time. Avoids surfacing the raw
        /// EPUB spine index (which counts cover/TOC/copyright pages) in the
        /// popup.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chapter_title: Option<String>,
    },
    Pdf {
        path: String,
        page: usize,
        scroll_offset: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snippet: Option<String>,
        /// Index into `line_bounds` of the line that was bookmarked. Used to
        /// flash a brief highlight after `` `<x> `` jumps. In normal mode this
        /// is the cursor line; in scroll mode this is the first visible text
        /// line when line geometry is available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_idx: Option<usize>,
    },
}

impl MarkLocation {
    pub fn path(&self) -> &str {
        match self {
            Self::Epub { path, .. } => path,
            Self::Pdf { path, .. } => path,
        }
    }

    pub fn is_pdf(&self) -> bool {
        matches!(self, Self::Pdf { .. })
    }

    pub fn snippet(&self) -> Option<&str> {
        match self {
            Self::Epub { snippet, .. } => snippet.as_deref(),
            Self::Pdf { snippet, .. } => snippet.as_deref(),
        }
    }

    pub fn chapter_title(&self) -> Option<&str> {
        match self {
            Self::Epub { chapter_title, .. } => chapter_title.as_deref(),
            Self::Pdf { .. } => None,
        }
    }

    pub fn retarget_path(self, new_path: String) -> Self {
        match self {
            Self::Epub {
                chapter,
                node,
                node_offset,
                snippet,
                chapter_title,
                ..
            } => Self::Epub {
                path: new_path,
                chapter,
                node,
                node_offset,
                snippet,
                chapter_title,
            },
            Self::Pdf {
                page,
                scroll_offset,
                snippet,
                line_idx,
                ..
            } => Self::Pdf {
                path: new_path,
                page,
                scroll_offset,
                snippet,
                line_idx,
            },
        }
    }

    /// Returns the corresponding `JumpLocation` if this build supports it.
    /// PDF marks return `None` in builds without the `pdf` feature.
    pub fn try_into_jump_location(self) -> Option<crate::jump_list::JumpLocation> {
        match self {
            Self::Epub {
                path,
                chapter,
                node,
                ..
            } => Some(crate::jump_list::JumpLocation::Epub {
                path,
                chapter,
                node,
            }),
            #[cfg(feature = "pdf")]
            Self::Pdf {
                path,
                page,
                scroll_offset,
                ..
            } => Some(crate::jump_list::JumpLocation::Pdf {
                path,
                page,
                scroll_offset,
            }),
            #[cfg(not(feature = "pdf"))]
            Self::Pdf { .. } => None,
        }
    }
}

pub fn validate_mark_char(ch: char) -> Option<MarkScope> {
    if ch.is_ascii_lowercase() {
        Some(MarkScope::Local(ch))
    } else if ch.is_ascii_uppercase() {
        Some(MarkScope::Global(ch))
    } else {
        None
    }
}

/// Where a mark lives.
///
/// `Local` is per-book (lowercase a–z). `Global` is cross-library (uppercase
/// A–Z) — accessible from any book in any library.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkScope {
    Local(char),
    Global(char),
}

/// A-Z marks shared across every library. Lives at
/// `data_dir/bookokrat/marks_global.json`.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct GlobalMarks {
    #[serde(default)]
    marks: HashMap<char, MarkLocation>,

    #[serde(skip)]
    file_path: Option<PathBuf>,
}

impl GlobalMarks {
    pub fn ephemeral() -> Self {
        Self::default()
    }

    pub fn load(file_path: PathBuf) -> Self {
        if file_path.exists() {
            match fs::read_to_string(&file_path)
                .map_err(anyhow::Error::from)
                .and_then(|s| serde_json::from_str::<Self>(&s).map_err(anyhow::Error::from))
            {
                Ok(mut m) => {
                    m.file_path = Some(file_path);
                    return m;
                }
                Err(e) => {
                    log::error!("Failed to load global marks from {file_path:?}: {e}");
                }
            }
        }
        Self {
            marks: HashMap::new(),
            file_path: Some(file_path),
        }
    }

    pub fn get(&self, ch: char) -> Option<&MarkLocation> {
        self.marks.get(&ch)
    }

    pub fn set(&mut self, ch: char, loc: MarkLocation) {
        self.marks.insert(ch, loc);
        if let Err(e) = self.save() {
            log::error!("Failed to save global marks: {e}");
        }
    }

    pub fn remove(&mut self, ch: char) -> bool {
        if self.marks.remove(&ch).is_none() {
            return false;
        }
        if let Err(e) = self.save() {
            log::error!("Failed to save global marks: {e}");
        }
        true
    }

    fn save(&self) -> anyhow::Result<()> {
        let Some(path) = &self.file_path else {
            return Ok(());
        };
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

/// Build a short, single-line snippet from an iterator of text lines, for use
/// as a mark label.
///
/// Joins non-blank trimmed lines with a single space. Stops once the buffer
/// reaches `max_chars` characters, or once it has at least `max_chars / 2`
/// characters and ends in a sentence terminator (`.`, `!`, `?`). Returns
/// `None` if no non-blank lines were seen. The result is char-truncated to
/// `max_chars` with a trailing `…` when the buffer overruns.
///
/// Callers are responsible for filtering out lines they don't want (e.g. EPUB
/// headings/horizontal rules at the start of the viewport) — this helper
/// concatenates whatever it's given.
pub fn build_text_snippet<S, I>(lines: I, max_chars: usize) -> Option<String>
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
{
    let mut buf = String::new();
    let mut count: usize = 0;
    let half = max_chars / 2;
    for line in lines {
        let trimmed = line.as_ref().trim();
        if trimmed.is_empty() {
            continue;
        }
        if !buf.is_empty() {
            buf.push(' ');
            count += 1;
        }
        buf.push_str(trimmed);
        count += trimmed.chars().count();
        if count >= max_chars {
            break;
        }
        if count >= half
            && trimmed
                .chars()
                .last()
                .is_some_and(|c| matches!(c, '.' | '!' | '?'))
        {
            break;
        }
    }
    if buf.is_empty() {
        return None;
    }
    if count > max_chars {
        let mut s: String = buf.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        Some(s)
    } else {
        Some(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_lowercase_is_local() {
        assert_eq!(validate_mark_char('a'), Some(MarkScope::Local('a')));
        assert_eq!(validate_mark_char('z'), Some(MarkScope::Local('z')));
    }

    #[test]
    fn validate_uppercase_is_global() {
        assert_eq!(validate_mark_char('A'), Some(MarkScope::Global('A')));
        assert_eq!(validate_mark_char('Z'), Some(MarkScope::Global('Z')));
    }

    #[test]
    fn validate_other_chars_rejected() {
        assert_eq!(validate_mark_char('1'), None);
        assert_eq!(validate_mark_char(' '), None);
        assert_eq!(validate_mark_char('!'), None);
    }

    #[test]
    fn mark_location_serde_roundtrip_epub() {
        let loc = MarkLocation::Epub {
            path: "book.epub".into(),
            chapter: 3,
            node: 42,
            snippet: None,
            node_offset: None,
            chapter_title: None,
        };
        let json = serde_json::to_string(&loc).unwrap();
        let parsed: MarkLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, loc);
    }

    #[test]
    fn mark_location_serde_roundtrip_pdf() {
        let loc = MarkLocation::Pdf {
            path: "book.pdf".into(),
            page: 17,
            scroll_offset: 256,
            snippet: None,
            line_idx: None,
        };
        let json = serde_json::to_string(&loc).unwrap();
        let parsed: MarkLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, loc);
    }

    #[test]
    fn pdf_mark_loads_in_non_pdf_build() {
        // The Pdf variant is always present in the type definition, so a
        // marks file with PDF entries decodes regardless of feature flags.
        let json = r#"{"kind":"pdf","path":"x.pdf","page":1,"scroll_offset":0}"#;
        let parsed: MarkLocation = serde_json::from_str(json).unwrap();
        assert!(parsed.is_pdf());
    }

    #[test]
    fn global_marks_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("global_marks.json");

        let mut marks = GlobalMarks::load(path.clone());
        marks.set(
            'A',
            MarkLocation::Epub {
                path: "/abs/book.epub".into(),
                chapter: 1,
                node: 0,
                snippet: None,
                node_offset: None,
                chapter_title: None,
            },
        );
        marks.set(
            'B',
            MarkLocation::Pdf {
                path: "/abs/book.pdf".into(),
                page: 5,
                scroll_offset: 10,
                snippet: None,
                line_idx: None,
            },
        );

        let reloaded = GlobalMarks::load(path);
        assert!(matches!(
            reloaded.get('A'),
            Some(MarkLocation::Epub { chapter: 1, .. })
        ));
        assert!(matches!(
            reloaded.get('B'),
            Some(MarkLocation::Pdf { page: 5, .. })
        ));
        assert!(reloaded.get('C').is_none());
    }

    #[test]
    fn build_text_snippet_empty_input_returns_none() {
        let lines: Vec<&str> = vec![];
        assert_eq!(build_text_snippet(lines, 50), None);
        assert_eq!(build_text_snippet(vec!["", "   ", "\t"], 50), None);
    }

    #[test]
    fn build_text_snippet_joins_with_space_and_trims() {
        let lines = vec!["  hello  ", "world  "];
        assert_eq!(
            build_text_snippet(lines, 50),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn build_text_snippet_stops_at_sentence_after_half() {
        // max=20, half=10. "Hello world." reaches len 12 >= 10 and ends in '.',
        // so the loop stops before consuming "Tail".
        let lines = vec!["Hello world.", "Tail"];
        assert_eq!(
            build_text_snippet(lines, 20),
            Some("Hello world.".to_string())
        );
    }

    #[test]
    fn build_text_snippet_does_not_stop_at_sentence_below_half() {
        // max=40, half=20. "Hi." is 3 chars (< 20), so we keep going.
        let lines = vec!["Hi.", "More content here"];
        assert_eq!(
            build_text_snippet(lines, 40),
            Some("Hi. More content here".to_string())
        );
    }

    #[test]
    fn build_text_snippet_truncates_with_ellipsis() {
        let lines = vec!["abcdefghijklmnop"]; // 16 chars
        // max=10 → take 9 + '…'
        assert_eq!(
            build_text_snippet(lines, 10),
            Some("abcdefghi…".to_string())
        );
    }

    #[test]
    fn build_text_snippet_handles_multibyte_chars() {
        // Each '€' is 3 bytes but 1 char. max=4 → take 3 + '…'.
        let lines = vec!["€€€€€€"];
        assert_eq!(build_text_snippet(lines, 4), Some("€€€…".to_string()));
    }

    #[test]
    fn global_marks_overwrite() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("g.json");
        let mut marks = GlobalMarks::load(path);

        marks.set(
            'A',
            MarkLocation::Epub {
                path: "old.epub".into(),
                chapter: 0,
                node: 0,
                snippet: None,
                node_offset: None,
                chapter_title: None,
            },
        );
        marks.set(
            'A',
            MarkLocation::Epub {
                path: "new.epub".into(),
                chapter: 5,
                node: 7,
                snippet: None,
                node_offset: None,
                chapter_title: None,
            },
        );

        match marks.get('A') {
            Some(MarkLocation::Epub { path, chapter, .. }) => {
                assert_eq!(path, "new.epub");
                assert_eq!(*chapter, 5);
            }
            _ => panic!("expected epub mark"),
        }
    }
}
