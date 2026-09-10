use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub enum JumpLocation {
    Epub {
        path: String,
        chapter: usize,
        node: usize,
    },
    #[cfg(feature = "pdf")]
    Pdf {
        path: String,
        page: usize,
        scroll_offset: u32,
    },
}

impl JumpLocation {
    pub fn epub(path: String, chapter: usize, node: usize) -> Self {
        Self::Epub {
            path,
            chapter,
            node,
        }
    }

    #[cfg(feature = "pdf")]
    pub fn pdf(path: String, page: usize, scroll_offset: u32) -> Self {
        Self::Pdf {
            path,
            page,
            scroll_offset,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Self::Epub { path, .. } => path,
            #[cfg(feature = "pdf")]
            Self::Pdf { path, .. } => path,
        }
    }
}

/// Jump list for navigation history (like vim's jump list)
pub struct JumpList {
    /// The actual list of jump locations
    entries: VecDeque<JumpLocation>,
    /// Current position in the jump list (-1 means at the newest entry)
    current_position: Option<usize>,
    /// Maximum number of entries to keep
    max_size: usize,
}

impl JumpList {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_size),
            current_position: None,
            max_size,
        }
    }

    pub fn push(&mut self, location: JumpLocation) {
        if let Some(pos) = self.current_position {
            self.entries.truncate(pos + 1);
        }
        // A push means we navigated, so we are at the head of the list now. Reset
        // before the dedup check below — otherwise a deduped push (e.g. clicking a
        // link from the page we just jumped back to) would leave current_position
        // pointing into history, breaking the next jump_back.
        self.current_position = None;

        if let Some(last) = self.entries.back() {
            if last == &location {
                return;
            }
        }
        self.entries.push_back(location);
        while self.entries.len() > self.max_size {
            self.entries.pop_front();
        }
    }

    /// Jump back in history. If `current_location` is provided and we're at the head,
    /// it will be pushed first so we can Ctrl+I back to it.
    pub fn jump_back(&mut self, current_location: Option<JumpLocation>) -> Option<JumpLocation> {
        // If at head and current location provided, save it first
        if self.current_position.is_none() {
            if let Some(ref loc) = current_location {
                // Only push if different from last entry
                if self.entries.back() != Some(loc) {
                    self.entries.push_back(loc.clone());
                    while self.entries.len() > self.max_size {
                        self.entries.pop_front();
                    }
                }
            }
        }

        match self.current_position {
            None => {
                // Jump to second-to-last entry (last entry is current position)
                if self.entries.len() >= 2 {
                    let new_pos = self.entries.len() - 2;
                    self.current_position = Some(new_pos);
                    self.entries.get(new_pos).cloned()
                } else {
                    // Only one entry or no entries - can't go back
                    None
                }
            }
            Some(pos) if pos > 0 => {
                self.current_position = Some(pos - 1);
                self.entries.get(pos - 1).cloned()
            }
            _ => None, // Already at the beginning
        }
    }

    pub fn jump_forward(&mut self) -> Option<JumpLocation> {
        match self.current_position {
            Some(pos) if pos < self.entries.len().saturating_sub(1) => {
                self.current_position = Some(pos + 1);
                self.entries.get(pos + 1).cloned()
            }
            Some(_) => {
                // At last entry - reset to "at newest" and return the latest
                self.current_position = None;
                self.entries.back().cloned()
            }
            _ => None, // Already at the newest (current_position is None)
        }
    }

    /// Clear the jump list
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_position = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jump_list_basic() {
        let mut list = JumpList::new(5);

        let loc1 = JumpLocation::epub("book1.epub".to_string(), 0, 0);
        let loc2 = JumpLocation::epub("book1.epub".to_string(), 1, 0);

        list.push(loc1.clone());

        // With only one entry and no current location, can't jump back
        assert_eq!(list.jump_back(None), None);

        list.clear();
        list.push(loc1.clone());
        list.push(loc2.clone());

        // First jump back (with current location) adds current to list: [loc1, loc2, current]
        // Then jumps to second-to-last which is loc2
        let current = JumpLocation::epub("book1.epub".to_string(), 2, 0);
        assert_eq!(list.jump_back(Some(current.clone())), Some(loc2.clone()));

        // Second jump back goes to loc1
        assert_eq!(list.jump_back(None), Some(loc1.clone()));

        // Can't go back further
        assert_eq!(list.jump_back(None), None);

        // Jump forward should go to loc2
        assert_eq!(list.jump_forward(), Some(loc2.clone()));

        // Jump forward again should go to current
        assert_eq!(list.jump_forward(), Some(current));
    }

    #[test]
    fn back_then_link_from_same_page_then_back() {
        // Regression: jump back to a page, then navigate (push) from that same
        // page. The pushed location equals the entry we landed on, so push()
        // dedups it — but must still reset current_position so the next
        // jump_back works instead of getting stuck.
        let mut list = JumpList::new(10);
        let a = JumpLocation::epub("b.epub".to_string(), 10, 0);
        let b = JumpLocation::epub("b.epub".to_string(), 20, 0);
        let c = JumpLocation::epub("b.epub".to_string(), 30, 0);

        // On page A, click link to B: push the from-page A.
        list.push(a.clone());
        // Ctrl+o from B: saves B, lands on A.
        assert_eq!(list.jump_back(Some(b.clone())), Some(a.clone()));
        // On A, click link to C: push from-page A (equals tail → deduped).
        list.push(a.clone());
        // Ctrl+o from C must still work and return A (previously returned None).
        assert_eq!(list.jump_back(Some(c.clone())), Some(a.clone()));
    }

    #[test]
    fn test_circular_buffer() {
        let mut list = JumpList::new(3);

        for i in 0..5 {
            list.push(JumpLocation::epub(format!("book{i}.epub"), i, 0));
        }

        // Should only have 3 entries (2, 3, 4)
        assert_eq!(list.entries.len(), 3);
    }
}
