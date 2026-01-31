use crate::ratatui::style::{Color, Style};
use regex::Regex;

#[derive(Clone, Debug)]
pub struct Search {
    pub pat: Option<Regex>,
    pub style: Style,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            pat: None,
            style: Style::default().bg(Color::Blue),
        }
    }
}

impl Search {
    pub fn matches<'a>(
        &'a self,
        line: &'a str,
    ) -> Option<impl Iterator<Item = (usize, usize)> + 'a> {
        let pat = self.pat.as_ref()?;
        let matches = pat.find_iter(line).map(|m| (m.start(), m.end()));
        Some(matches)
    }

    pub fn set_pattern(&mut self, query: &str) -> Result<(), regex::Error> {
        match &self.pat {
            Some(r) if r.as_str() == query => {}
            _ if query.is_empty() => self.pat = None,
            _ => self.pat = Some(Regex::new(query)?),
        }
        Ok(())
    }

    pub fn forward(
        &mut self,
        lines: &[String],
        cursor: (usize, usize),
        match_cursor: bool,
    ) -> Option<(usize, usize)> {
        let pat = if let Some(pat) = &self.pat {
            pat
        } else {
            return None;
        };
        let (row, col) = cursor;
        let current_line = &lines[row];

        // Search current line after cursor
        let start_col = if match_cursor { col } else { col + 1 };
        if let Some((i, _)) = current_line.char_indices().nth(start_col) {
            if let Some(m) = pat.find_at(current_line, i) {
                let col = start_col + current_line[i..m.start()].chars().count();
                return Some((row, col));
            }
        }

        // Search lines after cursor
        for (i, line) in lines[row + 1..].iter().enumerate() {
            if let Some(m) = pat.find(line) {
                let col = line[..m.start()].chars().count();
                return Some((row + 1 + i, col));
            }
        }

        // Search lines before cursor (wrap)
        for (i, line) in lines[..row].iter().enumerate() {
            if let Some(m) = pat.find(line) {
                let col = line[..m.start()].chars().count();
                return Some((i, col));
            }
        }

        // Search current line before cursor
        let col_idx = current_line
            .char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(current_line.len());
        if let Some(m) = pat.find(current_line) {
            let i = m.start();
            if i <= col_idx {
                let col = current_line[..i].chars().count();
                return Some((row, col));
            }
        }

        None
    }

    pub fn back(
        &mut self,
        lines: &[String],
        cursor: (usize, usize),
        match_cursor: bool,
    ) -> Option<(usize, usize)> {
        let pat = if let Some(pat) = &self.pat {
            pat
        } else {
            return None;
        };
        let (row, col) = cursor;
        let current_line = &lines[row];

        // Search current line before cursor
        if col > 0 || match_cursor {
            let start_col = if match_cursor { col } else { col - 1 };
            if let Some((i, _)) = current_line.char_indices().nth(start_col) {
                if let Some(m) = pat
                    .find_iter(current_line)
                    .take_while(|m| m.start() <= i)
                    .last()
                {
                    let col = current_line[..m.start()].chars().count();
                    return Some((row, col));
                }
            }
        }

        // Search lines before cursor
        for (i, line) in lines[..row].iter().enumerate().rev() {
            if let Some(m) = pat.find_iter(line).last() {
                let col = line[..m.start()].chars().count();
                return Some((i, col));
            }
        }

        // Search lines after cursor (wrap)
        for (i, line) in lines[row + 1..].iter().enumerate().rev() {
            if let Some(m) = pat.find_iter(line).last() {
                let col = line[..m.start()].chars().count();
                return Some((row + 1 + i, col));
            }
        }

        // Search current line after cursor
        if let Some((i, _)) = current_line.char_indices().nth(col) {
            if let Some(m) = pat
                .find_iter(current_line)
                .skip_while(|m| m.start() < i)
                .last()
            {
                let col = col + current_line[i..m.start()].chars().count();
                return Some((row, col));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches() {
        let mut s = Search::default();
        s.set_pattern("fo+").unwrap();

        let m: Vec<_> = s.matches("fo foo bar fooo").unwrap().collect();
        assert_eq!(m, [(0, 2), (3, 6), (11, 15)]);

        s.set_pattern("").unwrap();
        assert!(s.matches("fo foo bar fooo").is_none());
    }
}
