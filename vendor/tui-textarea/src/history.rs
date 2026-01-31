use crate::util::Pos;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub enum EditKind {
    InsertChar(char),
    DeleteChar(char),
    InsertNewline,
    DeleteNewline,
    InsertStr(String),
    DeleteStr(String),
    InsertChunk(Vec<String>),
    DeleteChunk(Vec<String>),
}

impl EditKind {
    pub(crate) fn apply(&self, lines: &mut Vec<String>, before: &Pos, after: &Pos) {
        match self {
            EditKind::InsertChar(c) => {
                lines[before.row].insert(before.offset, *c);
            }
            EditKind::DeleteChar(_) => {
                lines[before.row].remove(after.offset);
            }
            EditKind::InsertNewline => {
                let line = &mut lines[before.row];
                let next_line = line[before.offset..].to_string();
                line.truncate(before.offset);
                lines.insert(before.row + 1, next_line);
            }
            EditKind::DeleteNewline => {
                debug_assert!(before.row > 0, "invalid pos: {:?}", before);
                let line = lines.remove(before.row);
                lines[before.row - 1].push_str(&line);
            }
            EditKind::InsertStr(s) => {
                lines[before.row].insert_str(before.offset, s.as_str());
            }
            EditKind::DeleteStr(s) => {
                lines[after.row].drain(after.offset..after.offset + s.len());
            }
            EditKind::InsertChunk(c) => {
                debug_assert!(c.len() > 1, "Chunk size must be > 1: {:?}", c);

                // Handle first line of chunk
                let first_line = &mut lines[before.row];
                let mut last_line = first_line.drain(before.offset..).as_str().to_string();
                first_line.push_str(&c[0]);

                // Handle last line of chunk
                let next_row = before.row + 1;
                last_line.insert_str(0, c.last().unwrap());
                lines.insert(next_row, last_line);

                // Handle middle lines of chunk
                lines.splice(next_row..next_row, c[1..c.len() - 1].iter().cloned());
            }
            EditKind::DeleteChunk(c) => {
                debug_assert!(c.len() > 1, "Chunk size must be > 1: {:?}", c);

                // Remove middle lines of chunk
                let mut last_line = lines
                    .drain(after.row + 1..after.row + c.len())
                    .next_back()
                    .unwrap();
                // Remove last line of chunk
                last_line.drain(..c[c.len() - 1].len());

                // Remove first line of chunk and concat remaining
                let first_line = &mut lines[after.row];
                first_line.truncate(after.offset);
                first_line.push_str(&last_line);
            }
        }
    }

    fn invert(&self) -> Self {
        use EditKind::*;
        match self.clone() {
            InsertChar(c) => DeleteChar(c),
            DeleteChar(c) => InsertChar(c),
            InsertNewline => DeleteNewline,
            DeleteNewline => InsertNewline,
            InsertStr(s) => DeleteStr(s),
            DeleteStr(s) => InsertStr(s),
            InsertChunk(c) => DeleteChunk(c),
            DeleteChunk(c) => InsertChunk(c),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Edit {
    kind: EditKind,
    before: Pos,
    after: Pos,
}

impl Edit {
    pub fn new(kind: EditKind, before: Pos, after: Pos) -> Self {
        Self {
            kind,
            before,
            after,
        }
    }

    pub fn redo(&self, lines: &mut Vec<String>) {
        self.kind.apply(lines, &self.before, &self.after);
    }

    pub fn undo(&self, lines: &mut Vec<String>) {
        self.kind.invert().apply(lines, &self.after, &self.before); // Undo is redo of inverted edit
    }

    pub fn cursor_before(&self) -> (usize, usize) {
        (self.before.row, self.before.col)
    }

    pub fn cursor_after(&self) -> (usize, usize) {
        (self.after.row, self.after.col)
    }
}

#[derive(Clone, Debug)]
pub struct History {
    index: usize,
    max_items: usize,
    edits: VecDeque<Edit>,
}

impl History {
    pub fn new(max_items: usize) -> Self {
        Self {
            index: 0,
            max_items,
            edits: VecDeque::new(),
        }
    }

    pub fn push(&mut self, edit: Edit) {
        if self.max_items == 0 {
            return;
        }

        if self.edits.len() == self.max_items {
            self.edits.pop_front();
            self.index = self.index.saturating_sub(1);
        }

        if self.index < self.edits.len() {
            self.edits.truncate(self.index);
        }

        self.index += 1;
        self.edits.push_back(edit);
    }

    pub fn redo(&mut self, lines: &mut Vec<String>) -> Option<(usize, usize)> {
        if self.index == self.edits.len() {
            return None;
        }
        let edit = &self.edits[self.index];
        edit.redo(lines);
        self.index += 1;
        Some(edit.cursor_after())
    }

    pub fn undo(&mut self, lines: &mut Vec<String>) -> Option<(usize, usize)> {
        self.index = self.index.checked_sub(1)?;
        let edit = &self.edits[self.index];
        edit.undo(lines);
        Some(edit.cursor_before())
    }

    pub fn max_items(&self) -> usize {
        self.max_items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_delete_chunk() {
        #[rustfmt::skip]
        let tests = [
            // Positions
            (
                // Text before edit
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                // (row, col) position before edit
                (0, 0),
                // Chunk to be inserted
                &[
                    "x", "y",
                ][..],
                // Text after edit
                &[
                    "x",
                    "yab",
                    "cd",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (0, 1),
                &[
                    "x", "y",
                ][..],
                &[
                    "ax",
                    "yb",
                    "cd",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (0, 2),
                &[
                    "x", "y",
                ][..],
                &[
                    "abx",
                    "y",
                    "cd",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (1, 0),
                &[
                    "x", "y",
                ][..],
                &[
                    "ab",
                    "x",
                    "ycd",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (1, 1),
                &[
                    "x", "y",
                ][..],
                &[
                    "ab",
                    "cx",
                    "yd",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (1, 2),
                &[
                    "x", "y",
                ][..],
                &[
                    "ab",
                    "cdx",
                    "y",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (2, 0),
                &[
                    "x", "y",
                ][..],
                &[
                    "ab",
                    "cd",
                    "x",
                    "yef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (2, 1),
                &[
                    "x", "y",
                ][..],
                &[
                    "ab",
                    "cd",
                    "ex",
                    "yf",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (2, 2),
                &[
                    "x", "y",
                ][..],
                &[
                    "ab",
                    "cd",
                    "efx",
                    "y",
                ][..],
            ),
            // More than 2 lines
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (1, 1),
                &[
                    "x", "y", "z", "w"
                ][..],
                &[
                    "ab",
                    "cx",
                    "y",
                    "z",
                    "wd",
                    "ef",
                ][..],
            ),
            // Empty lines
            (
                &[
                    "",
                    "",
                    "",
                ][..],
                (0, 0),
                &[
                    "x", "y", "z"
                ][..],
                &[
                    "x",
                    "y",
                    "z",
                    "",
                    "",
                ][..],
            ),
            (
                &[
                    "",
                    "",
                    "",
                ][..],
                (1, 0),
                &[
                    "x", "y", "z"
                ][..],
                &[
                    "",
                    "x",
                    "y",
                    "z",
                    "",
                ][..],
            ),
            (
                &[
                    "",
                    "",
                    "",
                ][..],
                (2, 0),
                &[
                    "x", "y", "z"
                ][..],
                &[
                    "",
                    "",
                    "x",
                    "y",
                    "z",
                ][..],
            ),
            // Empty buffer
            (
                &[
                    "",
                ][..],
                (0, 0),
                &[
                    "x", "y", "z"
                ][..],
                &[
                    "x",
                    "y",
                    "z",
                ][..],
            ),
            // Insert empty lines
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (0, 0),
                &[
                    "", "", "",
                ][..],
                &[
                    "",
                    "",
                    "ab",
                    "cd",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (1, 0),
                &[
                    "", "", "",
                ][..],
                &[
                    "ab",
                    "",
                    "",
                    "cd",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (1, 1),
                &[
                    "", "", "",
                ][..],
                &[
                    "ab",
                    "c",
                    "",
                    "d",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (1, 2),
                &[
                    "", "", "",
                ][..],
                &[
                    "ab",
                    "cd",
                    "",
                    "",
                    "ef",
                ][..],
            ),
            (
                &[
                    "ab",
                    "cd",
                    "ef",
                ][..],
                (2, 2),
                &[
                    "", "", "",
                ][..],
                &[
                    "ab",
                    "cd",
                    "ef",
                    "",
                    "",
                ][..],
            ),
            // Multi-byte characters
            (
                &[
                    "🐶🐱",
                    "🐮🐰",
                    "🐧🐭",
                ][..],
                (0, 0),
                &[
                    "🐷", "🐼", "🐴",
                ][..],
                &[
                    "🐷",
                    "🐼",
                    "🐴🐶🐱",
                    "🐮🐰",
                    "🐧🐭",
                ][..],
            ),
            (
                &[
                    "🐶🐱",
                    "🐮🐰",
                    "🐧🐭",
                ][..],
                (0, 2),
                &[
                    "🐷", "🐼", "🐴",
                ][..],
                &[
                    "🐶🐱🐷",
                    "🐼",
                    "🐴",
                    "🐮🐰",
                    "🐧🐭",
                ][..],
            ),
            (
                &[
                    "🐶🐱",
                    "🐮🐰",
                    "🐧🐭",
                ][..],
                (1, 0),
                &[
                    "🐷", "🐼", "🐴",
                ][..],
                &[
                    "🐶🐱",
                    "🐷",
                    "🐼",
                    "🐴🐮🐰",
                    "🐧🐭",
                ][..],
            ),
            (
                &[
                    "🐶🐱",
                    "🐮🐰",
                    "🐧🐭",
                ][..],
                (1, 1),
                &[
                    "🐷", "🐼", "🐴",
                ][..],
                &[
                    "🐶🐱",
                    "🐮🐷",
                    "🐼",
                    "🐴🐰",
                    "🐧🐭",
                ][..],
            ),
            (
                &[
                    "🐶🐱",
                    "🐮🐰",
                    "🐧🐭",
                ][..],
                (2, 2),
                &[
                    "🐷", "🐼", "🐴",
                ][..],
                &[
                    "🐶🐱",
                    "🐮🐰",
                    "🐧🐭🐷",
                    "🐼",
                    "🐴",
                ][..],
            ),
        ];

        for test in tests {
            let (before, pos, input, expected) = test;
            let (row, col) = pos;
            let before_pos = {
                let offset = before[row]
                    .char_indices()
                    .map(|(i, _)| i)
                    .nth(col)
                    .unwrap_or(before[row].len());
                Pos::new(row, col, offset)
            };
            let mut lines: Vec<_> = before.iter().map(|s| s.to_string()).collect();
            let chunk: Vec<_> = input.iter().map(|s| s.to_string()).collect();
            let after_pos = {
                let row = row + input.len() - 1;
                let last = input.last().unwrap();
                let col = last.chars().count();
                Pos::new(row, col, last.len())
            };

            let edit = EditKind::InsertChunk(chunk.clone());
            edit.apply(&mut lines, &before_pos, &after_pos);
            assert_eq!(&lines, expected, "{test:?}");

            let edit = EditKind::DeleteChunk(chunk);
            edit.apply(&mut lines, &after_pos, &before_pos);
            assert_eq!(&lines, &before, "{test:?}");
        }
    }
}
