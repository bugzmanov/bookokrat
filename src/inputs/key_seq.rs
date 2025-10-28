use std::time::Instant;

pub struct KeySeq {
    key_sequence: Vec<char>,
    last_key_time: Option<Instant>,
}

impl Default for KeySeq {
    fn default() -> Self {
        Self::new()
    }
}

impl KeySeq {
    pub fn new() -> Self {
        Self {
            key_sequence: Vec::new(),
            last_key_time: None,
        }
    }

    fn should_reset_key_sequence(&self) -> bool {
        const KEY_SEQUENCE_TIMEOUT_MS: u64 = 1000; // 1 second timeout

        if let Some(last_time) = self.last_key_time {
            Instant::now().duration_since(last_time).as_millis() > KEY_SEQUENCE_TIMEOUT_MS as u128
        } else {
            false
        }
    }

    pub fn handle_key(&mut self, key_char: char) -> String {
        if self.should_reset_key_sequence() {
            self.key_sequence.clear();
        }

        if self.key_sequence.len() == 2 {
            self.key_sequence.remove(0);
        }

        self.key_sequence.push(key_char);
        self.last_key_time = Some(Instant::now());

        self.key_sequence.iter().collect()
    }

    pub fn clear(&mut self) {
        self.key_sequence.clear();
        self.last_key_time = None;
    }

    pub fn current_sequence(&self) -> String {
        self.key_sequence.iter().collect()
    }
}
