use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub struct JumpLocation {
    pub epub_path: String,
    pub chapter_index: usize,
    pub node_index: usize,
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

        if let Some(last) = self.entries.back() {
            if last == &location {
                return;
            }
        }
        self.entries.push_back(location);
        while self.entries.len() > self.max_size {
            self.entries.pop_front();
        }
        self.current_position = None;
    }

    pub fn jump_back(&mut self) -> Option<JumpLocation> {
        match self.current_position {
            None => {
                if !self.entries.is_empty() {
                    let new_pos = self.entries.len() - 1;
                    self.current_position = Some(new_pos);
                    self.entries.get(new_pos).cloned()
                } else {
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
            Some(pos) if pos < self.entries.len() - 1 => {
                self.current_position = Some(pos + 1);
                self.entries.get(pos + 1).cloned()
            }
            Some(pos) if pos == self.entries.len() - 1 => {
                self.current_position = None;
                None
            }
            _ => None, // Already at the newest
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

        let loc1 = JumpLocation {
            epub_path: "book1.epub".to_string(),
            chapter_index: 0,
            node_index: 0,
        };

        let loc2 = JumpLocation {
            epub_path: "book1.epub".to_string(),
            chapter_index: 1,
            node_index: 0,
        };

        list.push(loc1.clone());

        assert_eq!(list.jump_back(), Some(loc1.clone()));

        assert_eq!(list.jump_back(), None);

        list.clear();
        list.push(loc1.clone());
        list.push(loc2.clone());

        assert_eq!(list.jump_back(), Some(loc2.clone()));

        assert_eq!(list.jump_back(), Some(loc1.clone()));

        assert_eq!(list.jump_forward(), Some(loc2.clone()));
    }

    #[test]
    fn test_circular_buffer() {
        let mut list = JumpList::new(3);

        for i in 0..5 {
            list.push(JumpLocation {
                epub_path: format!("book{i}.epub"),
                chapter_index: i,
                node_index: 0,
            });
        }

        // Should only have 3 entries (2, 3, 4)
        assert_eq!(list.entries.len(), 3);
    }
}
