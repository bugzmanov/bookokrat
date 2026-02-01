use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct FrameRegistry {
    entries: HashMap<i64, u32>,
}

impl FrameRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn record(&mut self, page: i64, image_id: u32) {
        self.entries.insert(page, image_id);
    }

    pub fn lookup(&self, page: i64) -> Option<u32> {
        self.entries.get(&page).copied()
    }

    pub fn invalidate(&mut self, page: i64) {
        self.entries.remove(&page);
    }

    pub fn invalidate_range(&mut self, min: i64, max: i64) {
        let keys: Vec<i64> = self
            .entries
            .keys()
            .filter(|&&page| page >= min && page <= max)
            .copied()
            .collect();
        for key in keys {
            self.entries.remove(&key);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn frames_in_range(&self, min: i64, max: i64) -> Vec<(i64, u32)> {
        self.entries
            .iter()
            .filter_map(|(page, id)| {
                if *page >= min && *page <= max {
                    Some((*page, *id))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FrameRegistry;

    #[test]
    fn registry_record_lookup_invalidate() {
        let mut registry = FrameRegistry::new();
        registry.record(1, 10);
        registry.record(2, 20);

        assert_eq!(registry.lookup(1), Some(10));
        assert_eq!(registry.lookup(3), None);

        registry.invalidate(1);
        assert_eq!(registry.lookup(1), None);
        assert_eq!(registry.lookup(2), Some(20));
    }

    #[test]
    fn registry_invalidate_range() {
        let mut registry = FrameRegistry::new();
        registry.record(1, 10);
        registry.record(2, 20);
        registry.record(3, 30);
        registry.record(4, 40);

        registry.invalidate_range(2, 3);
        assert_eq!(registry.lookup(1), Some(10));
        assert_eq!(registry.lookup(2), None);
        assert_eq!(registry.lookup(3), None);
        assert_eq!(registry.lookup(4), Some(40));
    }
}
