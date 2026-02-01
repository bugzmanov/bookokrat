use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug)]
pub struct KeySeq {
    keys: Vec<KeyEvent>,
    last_key_time: Instant,
    timeout: Duration,
}

impl Default for KeySeq {
    fn default() -> Self {
        Self::new()
    }
}

impl KeySeq {
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(1))
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            keys: Vec::new(),
            last_key_time: Instant::now(),
            timeout,
        }
    }

    /// Push a KeyEvent to the sequence
    pub fn push(&mut self, key: KeyEvent) {
        self.check_timeout();
        self.keys.push(key);
        self.last_key_time = Instant::now();
    }

    /// Check if the sequence matches a pattern exactly
    pub fn matches(&self, pattern: &[KeyCode]) -> bool {
        if self.is_expired() || self.keys.len() != pattern.len() {
            return false;
        }

        self.keys
            .iter()
            .zip(pattern.iter())
            .all(|(k, p)| k.code == *p)
    }

    /// Check if current sequence is a prefix of the given pattern
    pub fn is_prefix_of(&self, pattern: &[KeyCode]) -> bool {
        if self.is_expired() || self.keys.len() > pattern.len() {
            return false;
        }

        self.keys
            .iter()
            .zip(pattern.iter())
            .all(|(k, p)| k.code == *p)
    }

    /// Check if current sequence starts with the given pattern
    pub fn starts_with(&self, pattern: &[KeyCode]) -> bool {
        if self.is_expired() || self.keys.len() < pattern.len() {
            return false;
        }

        self.keys
            .iter()
            .zip(pattern.iter())
            .all(|(k, p)| k.code == *p)
    }

    /// Get the current sequence as KeyCodes
    pub fn codes(&self) -> Vec<KeyCode> {
        if self.is_expired() {
            Vec::new()
        } else {
            self.keys.iter().map(|k| k.code).collect()
        }
    }

    pub fn len(&self) -> usize {
        if self.is_expired() {
            0
        } else {
            self.keys.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_expired(&self) -> bool {
        self.keys.is_empty() || self.last_key_time.elapsed() > self.timeout
    }

    pub fn clear(&mut self) {
        self.keys.clear();
    }

    pub fn last(&self) -> Option<&KeyEvent> {
        if self.is_expired() {
            None
        } else {
            self.keys.last()
        }
    }

    pub fn keys(&self) -> &[KeyEvent] {
        if self.is_expired() { &[] } else { &self.keys }
    }

    fn check_timeout(&mut self) {
        if !self.keys.is_empty() && self.last_key_time.elapsed() > self.timeout {
            self.keys.clear();
        }
    }

    pub fn last_has_modifiers(&self, modifiers: KeyModifiers) -> bool {
        self.last().is_some_and(|k| k.modifiers.contains(modifiers))
    }

    // === Backward compatible char-based API ===

    /// Handle a character key press and return the current sequence as a string.
    /// This maintains backward compatibility with the old char-based API.
    pub fn handle_key(&mut self, key_char: char) -> String {
        self.check_timeout();

        // Limit sequence length to 2 for backward compat
        if self.keys.len() == 2 {
            self.keys.remove(0);
        }

        let key_event = KeyEvent::new(KeyCode::Char(key_char), KeyModifiers::NONE);
        self.keys.push(key_event);
        self.last_key_time = Instant::now();

        self.current_sequence()
    }

    /// Get the current sequence as a string (backward compatible)
    pub fn current_sequence(&self) -> String {
        if self.is_expired() {
            String::new()
        } else {
            self.keys
                .iter()
                .filter_map(|k| match k.code {
                    KeyCode::Char(c) => Some(c),
                    _ => None,
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn matches_exact_sequence() {
        let mut seq = KeySeq::new();

        seq.push(key(KeyCode::Char('g')));
        seq.push(key(KeyCode::Char('g')));

        assert!(seq.matches(&[KeyCode::Char('g'), KeyCode::Char('g')]));
        assert!(!seq.matches(&[KeyCode::Char('g')]));
        assert!(!seq.matches(&[KeyCode::Char('d'), KeyCode::Char('d')]));
    }

    #[test]
    fn is_prefix_works() {
        let mut seq = KeySeq::new();

        seq.push(key(KeyCode::Char('g')));

        assert!(seq.is_prefix_of(&[KeyCode::Char('g'), KeyCode::Char('g')]));
        assert!(!seq.is_prefix_of(&[KeyCode::Char('d')]));

        seq.push(key(KeyCode::Char('g')));
        assert!(!seq.is_prefix_of(&[KeyCode::Char('g')]));
    }

    #[test]
    fn clear_works() {
        let mut seq = KeySeq::new();

        seq.push(key(KeyCode::Char('g')));
        assert_eq!(seq.len(), 1);

        seq.clear();
        assert!(seq.is_empty());
    }

    #[test]
    fn timeout_clears_sequence() {
        let mut seq = KeySeq::with_timeout(Duration::from_millis(10));

        seq.push(key(KeyCode::Char('g')));
        assert_eq!(seq.len(), 1);

        std::thread::sleep(Duration::from_millis(20));

        assert!(seq.is_expired());
        assert_eq!(seq.len(), 0);
    }

    #[test]
    fn backward_compat_handle_key() {
        let mut seq = KeySeq::new();

        assert_eq!(seq.handle_key('g'), "g");
        assert_eq!(seq.handle_key('g'), "gg");
        assert_eq!(seq.current_sequence(), "gg");
    }

    #[test]
    fn backward_compat_clear() {
        let mut seq = KeySeq::new();

        seq.handle_key('g');
        seq.handle_key('g');
        assert_eq!(seq.current_sequence(), "gg");

        seq.clear();
        assert_eq!(seq.current_sequence(), "");
    }

    #[test]
    fn backward_compat_space_prefix() {
        let mut seq = KeySeq::new();

        seq.handle_key(' ');
        assert_eq!(seq.current_sequence(), " ");

        seq.handle_key('h');
        assert_eq!(seq.current_sequence(), " h");
    }
}
