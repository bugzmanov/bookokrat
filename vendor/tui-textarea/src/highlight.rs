use crate::ratatui::style::Style;
use crate::ratatui::text::Span;
use crate::util::{num_digits, spaces};
#[cfg(feature = "ratatui")]
use ratatui_core::text::Line;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::iter;
#[cfg(feature = "tuirs")]
use tui::text::Spans as Line;
use unicode_width::UnicodeWidthChar as _;

enum Boundary {
    Cursor(Style),
    Select(Style),
    #[cfg(feature = "search")]
    Search(Style),
    End,
}

impl Boundary {
    fn cmp(&self, other: &Boundary) -> Ordering {
        fn rank(b: &Boundary) -> u8 {
            match b {
                Boundary::Cursor(_) => 3,
                #[cfg(feature = "search")]
                Boundary::Search(_) => 2,
                Boundary::Select(_) => 1,
                Boundary::End => 0,
            }
        }
        rank(self).cmp(&rank(other))
    }

    fn style(&self) -> Option<Style> {
        match self {
            Boundary::Cursor(s) => Some(*s),
            Boundary::Select(s) => Some(*s),
            #[cfg(feature = "search")]
            Boundary::Search(s) => Some(*s),
            Boundary::End => None,
        }
    }
}

struct DisplayTextBuilder {
    tab_len: u8,
    width: usize,
    mask: Option<char>,
}

impl DisplayTextBuilder {
    fn new(tab_len: u8, mask: Option<char>) -> Self {
        Self {
            tab_len,
            width: 0,
            mask,
        }
    }

    fn build<'s>(&mut self, s: &'s str) -> Cow<'s, str> {
        if let Some(ch) = self.mask {
            // Note: We don't need to track width on masking text since width of tab character is fixed
            let masked = iter::repeat(ch).take(s.chars().count()).collect();
            return Cow::Owned(masked);
        }

        let tab = spaces(self.tab_len);
        let mut buf = String::new();
        for (i, c) in s.char_indices() {
            if c == '\t' {
                if buf.is_empty() {
                    buf.reserve(s.len());
                    buf.push_str(&s[..i]);
                }
                if self.tab_len > 0 {
                    let len = self.tab_len as usize - (self.width % self.tab_len as usize);
                    buf.push_str(&tab[..len]);
                    self.width += len;
                }
            } else {
                if !buf.is_empty() {
                    buf.push(c);
                }
                self.width += c.width().unwrap_or(0);
            }
        }

        if !buf.is_empty() {
            Cow::Owned(buf)
        } else {
            Cow::Borrowed(s)
        }
    }
}

pub struct LineHighlighter<'a> {
    line: &'a str,
    spans: Vec<Span<'a>>,
    boundaries: Vec<(Boundary, usize)>, // TODO: Consider smallvec
    style_begin: Style,
    cursor_at_end: bool,
    cursor_style: Style,
    tab_len: u8,
    mask: Option<char>,
    select_at_end: bool,
    select_style: Style,
}

impl<'a> LineHighlighter<'a> {
    pub fn new(
        line: &'a str,
        cursor_style: Style,
        tab_len: u8,
        mask: Option<char>,
        select_style: Style,
    ) -> Self {
        Self {
            line,
            spans: vec![],
            boundaries: vec![],
            style_begin: Style::default(),
            cursor_at_end: false,
            cursor_style,
            tab_len,
            mask,
            select_at_end: false,
            select_style,
        }
    }

    pub fn line_number(&mut self, row: usize, lnum_len: u8, style: Style) {
        let pad = spaces(lnum_len - num_digits(row + 1) + 1);
        self.spans
            .push(Span::styled(format!("{}{} ", pad, row + 1), style));
    }

    pub fn cursor_line(&mut self, cursor_col: usize, style: Style) {
        if let Some((start, c)) = self.line.char_indices().nth(cursor_col) {
            self.boundaries
                .push((Boundary::Cursor(self.cursor_style), start));
            self.boundaries.push((Boundary::End, start + c.len_utf8()));
        } else {
            self.cursor_at_end = true;
        }
        self.style_begin = style;
    }

    #[cfg(feature = "search")]
    pub fn search(&mut self, matches: impl Iterator<Item = (usize, usize)>, style: Style) {
        for (start, end) in matches {
            if start != end {
                self.boundaries.push((Boundary::Search(style), start));
                self.boundaries.push((Boundary::End, end));
            }
        }
    }

    pub fn selection(
        &mut self,
        current_row: usize,
        start_row: usize,
        start_off: usize,
        end_row: usize,
        end_off: usize,
    ) {
        let (start, end) = if current_row == start_row {
            if start_row == end_row {
                (start_off, end_off)
            } else {
                self.select_at_end = true;
                (start_off, self.line.len())
            }
        } else if current_row == end_row {
            (0, end_off)
        } else if start_row < current_row && current_row < end_row {
            self.select_at_end = true;
            (0, self.line.len())
        } else {
            return;
        };
        if start != end {
            self.boundaries
                .push((Boundary::Select(self.select_style), start));
            self.boundaries.push((Boundary::End, end));
        }
    }

    pub fn into_spans(self) -> Line<'a> {
        let Self {
            line,
            mut spans,
            mut boundaries,
            tab_len,
            style_begin,
            cursor_style,
            cursor_at_end,
            mask,
            select_at_end,
            select_style,
        } = self;
        let mut builder = DisplayTextBuilder::new(tab_len, mask);

        if boundaries.is_empty() {
            let built = builder.build(line);
            if !built.is_empty() {
                spans.push(Span::styled(built, style_begin));
            }
            if cursor_at_end {
                spans.push(Span::styled(" ", cursor_style));
            } else if select_at_end {
                spans.push(Span::styled(" ", select_style));
            }
            return Line::from(spans);
        }

        boundaries.sort_unstable_by(|(l, i), (r, j)| match i.cmp(j) {
            Ordering::Equal => l.cmp(r),
            o => o,
        });

        let mut style = style_begin;
        let mut start = 0;
        let mut stack = vec![];

        for (next_boundary, end) in boundaries {
            if start < end {
                spans.push(Span::styled(builder.build(&line[start..end]), style));
            }

            style = if let Some(s) = next_boundary.style() {
                stack.push(style);
                s
            } else {
                stack.pop().unwrap_or(style_begin)
            };
            start = end;
        }

        if start != line.len() {
            spans.push(Span::styled(builder.build(&line[start..]), style));
        }

        if cursor_at_end {
            spans.push(Span::styled(" ", cursor_style));
        } else if select_at_end {
            spans.push(Span::styled(" ", select_style));
        }

        Line::from(spans)
    }
}

// Tests for spans don't work with tui-rs
#[cfg(all(test, feature = "ratatui"))]
mod tests {
    use super::*;
    use crate::ratatui::style::Color;
    use std::fmt::Debug;
    use unicode_width::UnicodeWidthStr as _;

    fn build(text: &'static str, tab: u8, mask: Option<char>) -> Cow<'static, str> {
        DisplayTextBuilder::new(tab, mask).build(text)
    }

    #[track_caller]
    fn build_with_offset(offset: usize, text: &'static str, tab: u8) -> Cow<'static, str> {
        let mut b = DisplayTextBuilder::new(tab, None);
        b.width = offset;
        let built = b.build(text);
        let want = offset + built.as_ref().width();
        assert_eq!(b.width, want, "in={:?}, out={:?}", text, built); // Check post condition
        built
    }

    #[test]
    #[rustfmt::skip]
    fn line_display_text() {
        assert_eq!(&build(      "",  0,      None),                  "");
        assert_eq!(&build(      "",  4,      None),                  "");
        assert_eq!(&build(      "",  8,      None),                  "");
        assert_eq!(&build(      "",  0, Some('x')),                  "");
        assert_eq!(&build(      "",  4, Some('x')),                  "");
        assert_eq!(&build(      "",  8, Some('x')),                  "");
        assert_eq!(&build(     "a",  0,      None),                 "a");
        assert_eq!(&build(     "a",  4,      None),                 "a");
        assert_eq!(&build(     "a",  8,      None),                 "a");
        assert_eq!(&build(     "a",  0, Some('x')),                 "x");
        assert_eq!(&build(     "a",  4, Some('x')),                 "x");
        assert_eq!(&build(     "a",  8, Some('x')),                 "x");
        assert_eq!(&build(   "a\t",  0,      None),                 "a");
        assert_eq!(&build(   "a\t",  4,      None),              "a   ");
        assert_eq!(&build(   "a\t",  8,      None),          "a       ");
        assert_eq!(&build(   "a\t",  0, Some('x')),                "xx");
        assert_eq!(&build(   "a\t",  4, Some('x')),                "xx");
        assert_eq!(&build(   "a\t",  8, Some('x')),                "xx");
        assert_eq!(&build(    "\t",  0,      None),                "\t");
        assert_eq!(&build(    "\t",  4,      None),              "    ");
        assert_eq!(&build(    "\t",  8,      None),          "        ");
        assert_eq!(&build(    "\t",  0, Some('x')),                 "x");
        assert_eq!(&build(    "\t",  4, Some('x')),                 "x");
        assert_eq!(&build(    "\t",  8, Some('x')),                 "x");
        assert_eq!(&build(  "a\tb",  0,      None),                "ab");
        assert_eq!(&build(  "a\tb",  4,      None),             "a   b");
        assert_eq!(&build(  "a\tb",  8,      None),         "a       b");
        assert_eq!(&build(  "a\tb",  0, Some('x')),               "xxx");
        assert_eq!(&build(  "a\tb",  4, Some('x')),               "xxx");
        assert_eq!(&build(  "a\tb",  8, Some('x')),               "xxx");
        assert_eq!(&build("a\t\tb",  0,      None),                "ab");
        assert_eq!(&build("a\t\tb",  4,      None),         "a       b");
        assert_eq!(&build("a\t\tb",  8,      None), "a               b");
        assert_eq!(&build("a\t\tb",  0, Some('x')),              "xxxx");
        assert_eq!(&build("a\t\tb",  4, Some('x')),              "xxxx");
        assert_eq!(&build("a\t\tb",  8, Some('x')),              "xxxx");
        assert_eq!(&build("a\tb\tc", 0,      None),               "abc");
        assert_eq!(&build("a\tb\tc", 4,      None),         "a   b   c");
        assert_eq!(&build("a\tb\tc", 8,      None), "a       b       c");
        assert_eq!(&build("a\tb\tc", 0, Some('x')),             "xxxxx");
        assert_eq!(&build("a\tb\tc", 4, Some('x')),             "xxxxx");
        assert_eq!(&build("a\tb\tc", 8, Some('x')),             "xxxxx");
        assert_eq!(&build("ab\t\t",  0,      None),                "ab");
        assert_eq!(&build("ab\t\t",  4,      None),          "ab      ");
        assert_eq!(&build("ab\t\t",  8,      None),  "ab              ");
        assert_eq!(&build("abcd\t",  4,      None),          "abcd    ");
        assert_eq!(&build(  "あ\t",  0,      None),                "あ");
        assert_eq!(&build(  "あ\t",  4,      None),              "あ  ");
        assert_eq!(&build(  "🐶\t",  4,      None),              "🐶  ");
        assert_eq!(&build(  "あ\t",  4, Some('x')),                "xx");

        // When the start position of the text is not start of the line (#43)
        assert_eq!(&build_with_offset(1,         "", 0),           "");
        assert_eq!(&build_with_offset(1,        "a", 0),          "a");
        assert_eq!(&build_with_offset(1,       "あ", 0),         "あ");
        assert_eq!(&build_with_offset(1,       "\t", 4),        "   ");
        assert_eq!(&build_with_offset(1,      "a\t", 4),        "a  ");
        assert_eq!(&build_with_offset(1,     "あ\t", 4),        "あ ");
        assert_eq!(&build_with_offset(2,       "\t", 4),         "  ");
        assert_eq!(&build_with_offset(2,      "a\t", 4),         "a ");
        assert_eq!(&build_with_offset(2,     "あ\t", 4),     "あ    ");
        assert_eq!(&build_with_offset(3,      "a\t", 4),      "a    ");
        assert_eq!(&build_with_offset(4,       "\t", 4),       "    ");
        assert_eq!(&build_with_offset(4,      "a\t", 4),       "a   ");
        assert_eq!(&build_with_offset(4,     "あ\t", 4),       "あ  ");
        assert_eq!(&build_with_offset(5,       "\t", 4),        "   ");
        assert_eq!(&build_with_offset(5,      "a\t", 4),        "a  ");
        assert_eq!(&build_with_offset(5,     "あ\t", 4),        "あ ");
        assert_eq!(&build_with_offset(2,     "\t\t", 4),     "      ");
        assert_eq!(&build_with_offset(2,   "a\ta\t", 4),     "a a   ");
        assert_eq!(&build_with_offset(1, "あ\tあ\t", 4),    "あ あ  ");
        assert_eq!(&build_with_offset(2, "あ\tあ\t", 4), "あ    あ  ");
    }

    fn assert_spans<T: Debug>(lh: LineHighlighter, want: &[(&str, Style)], context: T) {
        let line = lh.into_spans();
        let have = line
            .spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style))
            .collect::<Vec<_>>();
        assert_eq!(&have, want, "Test case: {context:?}");
    }

    const DEFAULT: Style = Style::new();
    const CUR: Style = Style::new().bg(Color::Red); // Cursor
    #[allow(unused)]
    const SEARCH: Style = Style::new().bg(Color::Green);
    const SEL: Style = Style::new().bg(Color::Blue);
    const LINE: Style = Style::new().bg(Color::Gray);
    const LNUM: Style = Style::new().bg(Color::Yellow);

    #[test]
    fn into_spans_normal_line() {
        let tests = [
            ("", &[][..]),
            ("abc", &[("abc", DEFAULT)][..]),
            ("a\tb\tc", &[("a   b   c", DEFAULT)][..]),
        ];
        for test in tests {
            let (line, want) = test;
            let lh = LineHighlighter::new(line, CUR, 4, None, SEL);
            assert_spans(lh, want, test);
        }
    }

    #[test]
    fn into_spans_cursor_line() {
        let tests = [
            ("", 0, &[(" ", CUR)][..]),
            ("a", 0, &[("a", CUR)][..]),
            ("a", 1, &[("a", LINE), (" ", CUR)][..]),
            ("あいう", 0, &[("あ", CUR), ("いう", LINE)][..]),
            ("あいう", 1, &[("あ", LINE), ("い", CUR), ("う", LINE)][..]),
            ("あいう", 2, &[("あい", LINE), ("う", CUR)][..]),
            ("a\tb", 1, &[("a", LINE), ("   ", CUR), ("b", LINE)][..]),
        ];

        for test in tests {
            let (line, col, want) = test;
            let mut lh = LineHighlighter::new(line, CUR, 4, None, SEL);
            lh.cursor_line(col, LINE);
            assert_spans(lh, want, test);
        }
    }

    #[test]
    fn into_spans_line_number() {
        let tests = [
            (0, 1, &[(" 1 ", LNUM)][..]),
            (123, 3, &[(" 124 ", LNUM)][..]),
            (123, 5, &[("   124 ", LNUM)][..]),
        ];
        for test in tests {
            let (row, len, want) = test;
            let mut lh = LineHighlighter::new("", CUR, 4, None, SEL);
            lh.line_number(row, len, LNUM);
            assert_spans(lh, want, test);
        }
    }

    #[cfg(feature = "search")]
    #[test]
    fn into_spans_search() {
        let tests = [
            ("abcde", &[(0, 5)][..], &[("abcde", SEARCH)][..]),
            (
                "abcde",
                &[(0, 1), (2, 3), (4, 5)][..],
                &[
                    ("a", SEARCH),
                    ("b", DEFAULT),
                    ("c", SEARCH),
                    ("d", DEFAULT),
                    ("e", SEARCH),
                ][..],
            ),
            (
                "abcde",
                &[(1, 2), (3, 4)][..],
                &[
                    ("a", DEFAULT),
                    ("b", SEARCH),
                    ("c", DEFAULT),
                    ("d", SEARCH),
                    ("e", DEFAULT),
                ][..],
            ),
            (
                "abcde",
                &[(0, 2), (2, 4), (4, 5)][..],
                &[("ab", SEARCH), ("cd", SEARCH), ("e", SEARCH)][..],
            ),
            ("abcde", &[(1, 1)][..], &[("abcde", DEFAULT)][..]),
            (
                "あいうえお",
                &[(0, 3), (6, 9), (12, 15)][..],
                &[
                    ("あ", SEARCH),
                    ("い", DEFAULT),
                    ("う", SEARCH),
                    ("え", DEFAULT),
                    ("お", SEARCH),
                ][..],
            ),
            (
                "\ta\tb\t",
                &[(0, 1), (2, 3), (3, 4)][..],
                &[
                    ("    ", SEARCH),
                    ("a", DEFAULT),
                    ("   ", SEARCH),
                    ("b", SEARCH),
                    ("   ", DEFAULT),
                ][..],
            ),
        ];

        for test in tests {
            let (line, matches, want) = test;
            let mut lh = LineHighlighter::new(line, CUR, 4, None, SEL);
            lh.search(matches.iter().copied(), SEARCH);
            assert_spans(lh, want, test);
        }
    }

    #[test]
    fn into_spans_selection() {
        let tests = [
            // (line, (row, start_row, start_off, end_row, end_off), want)
            ("abc", (0, 1, 0, 2, 0), &[("abc", DEFAULT)][..]),
            ("abc", (1, 1, 0, 1, 1), &[("a", SEL), ("bc", DEFAULT)][..]),
            ("abc", (1, 1, 2, 1, 3), &[("ab", DEFAULT), ("c", SEL)][..]),
            ("abc", (1, 1, 0, 1, 3), &[("abc", SEL)][..]),
            ("abc", (1, 1, 0, 2, 0), &[("abc", SEL), (" ", SEL)][..]),
            (
                "abc",
                (1, 1, 2, 2, 0),
                &[("ab", DEFAULT), ("c", SEL), (" ", SEL)][..],
            ),
            ("abc", (1, 1, 3, 2, 0), &[("abc", DEFAULT), (" ", SEL)][..]),
            ("abc", (2, 1, 0, 3, 0), &[("abc", SEL), (" ", SEL)][..]),
            ("abc", (2, 1, 0, 2, 0), &[("abc", DEFAULT)][..]),
            ("abc", (2, 1, 0, 2, 2), &[("ab", SEL), ("c", DEFAULT)][..]),
            ("abc", (2, 1, 0, 2, 3), &[("abc", SEL)][..]),
            (
                "ab\t",
                (1, 1, 2, 2, 0),
                &[("ab", DEFAULT), ("  ", SEL), (" ", SEL)][..],
            ),
            ("a\tb", (2, 1, 0, 3, 0), &[("a   b", SEL), (" ", SEL)][..]),
            (
                "a\tb",
                (2, 1, 0, 2, 2),
                &[("a   ", SEL), ("b", DEFAULT)][..],
            ),
        ];

        for test in tests {
            let (line, (row, start_row, start_off, end_row, end_off), want) = test;
            let mut lh = LineHighlighter::new(line, CUR, 4, None, SEL);
            lh.selection(row, start_row, start_off, end_row, end_off);
            assert_spans(lh, want, test);
        }
    }

    #[test]
    fn into_spans_mixed_highlights() {
        let tests = [
            (
                "cursor on selection",
                {
                    let mut lh = LineHighlighter::new("abcde", CUR, 4, None, SEL);
                    lh.cursor_line(2, LINE);
                    lh.selection(0, 0, 1, 0, 4);
                    lh
                },
                &[("a", LINE), ("b", SEL), ("c", CUR), ("d", SEL), ("e", LINE)][..],
            ),
            #[cfg(feature = "search")]
            (
                "cursor + selection + search",
                {
                    let mut lh = LineHighlighter::new("abcdefg", CUR, 4, None, SEL);
                    lh.cursor_line(3, LINE);
                    lh.selection(0, 0, 2, 0, 5);
                    lh.search([(1, 2), (5, 6)].into_iter(), SEARCH);
                    lh
                },
                &[
                    ("a", LINE),
                    ("b", SEARCH),
                    ("c", SEL),
                    ("d", CUR),
                    ("e", SEL),
                    ("f", SEARCH),
                    ("g", LINE),
                ][..],
            ),
            (
                "selection + cursor at end",
                {
                    let mut lh = LineHighlighter::new("ab", CUR, 4, None, SEL);
                    lh.cursor_line(2, LINE);
                    lh.selection(0, 0, 1, 2, 0);
                    lh
                },
                &[("a", LINE), ("b", SEL), (" ", CUR)][..],
            ),
            (
                "cursor at start of selection",
                {
                    let mut lh = LineHighlighter::new("abcd", CUR, 4, None, SEL);
                    lh.cursor_line(1, LINE);
                    lh.selection(0, 0, 1, 0, 3);
                    lh
                },
                &[("a", LINE), ("b", CUR), ("c", SEL), ("d", LINE)][..],
            ),
            (
                "cursor at end of selection",
                {
                    let mut lh = LineHighlighter::new("abcd", CUR, 4, None, SEL);
                    lh.cursor_line(2, LINE);
                    lh.selection(0, 0, 1, 0, 3);
                    lh
                },
                &[("a", LINE), ("b", SEL), ("c", CUR), ("d", LINE)][..],
            ),
            (
                "cursor covers selection",
                {
                    let mut lh = LineHighlighter::new("abc", CUR, 4, None, SEL);
                    lh.cursor_line(1, LINE);
                    lh.selection(0, 0, 1, 0, 2);
                    lh
                },
                &[("a", LINE), ("b", CUR), ("c", LINE)][..],
            ),
        ];

        for (what, lh, want) in tests {
            assert_spans(lh, want, what);
        }
    }
}
