//! LRU page cache for rendered PDF pages

use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;

use super::request::RenderParams;
use super::types::PageData;

/// Cache key for rendered pages
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Page number
    pub page: usize,
    /// Viewport width
    pub area_width: u16,
    /// Viewport height
    pub area_height: u16,
    /// Scale factor (stored as millionths for stable hashing)
    pub scale_millionths: u32,
    /// Whether images are inverted
    pub invert_images: bool,
}

impl CacheKey {
    /// Create a cache key from render parameters
    #[must_use]
    pub fn from_params(page: usize, params: &RenderParams) -> Self {
        Self {
            page,
            area_width: params.area.width,
            area_height: params.area.height,
            scale_millionths: (params.scale * 1_000_000.0) as u32,
            invert_images: params.invert_images,
        }
    }
}

/// LRU cache for rendered page data
pub struct PageCache {
    cache: LruCache<CacheKey, Arc<PageData>>,
}

impl PageCache {
    /// Create a new cache with the given capacity
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).expect("1 is non-zero")),
            ),
        }
    }

    /// Get a cached page, promoting it in the LRU order
    #[must_use]
    pub fn get(&mut self, key: &CacheKey) -> Option<Arc<PageData>> {
        self.cache.get(key).cloned()
    }

    /// Check if a key is in the cache without promoting it
    #[must_use]
    pub fn contains(&self, key: &CacheKey) -> bool {
        self.cache.contains(key)
    }

    /// Insert a page into the cache, returning an Arc to the data
    pub fn insert(&mut self, key: CacheKey, data: PageData) -> Arc<PageData> {
        let arc = Arc::new(data);
        self.cache.put(key, arc.clone());
        arc
    }

    /// Clear all cached pages
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }

    /// Invalidate all cached versions of a specific page
    pub fn invalidate_page(&mut self, page: usize) {
        let keys_to_remove: Vec<_> = self
            .cache
            .iter()
            .filter(|(k, _)| k.page == page)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            self.cache.pop(&key);
        }
    }

    /// Number of cached pages
    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Cache capacity
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.cache.cap().get()
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::super::CellSize;
    use super::*;

    fn test_params() -> RenderParams {
        RenderParams {
            area: Rect::new(0, 0, 100, 50),
            scale: 1.0,
            invert_images: false,
            cell_size: CellSize::new(10, 20),
            black: 0,
            white: 0xFFFFFF,
        }
    }

    fn test_page_data(page: usize) -> PageData {
        PageData {
            img_data: super::super::types::ImageData {
                pixels: vec![0; 300],
                width_px: 10,
                height_px: 10,
                width_cell: 10,
                height_cell: 5,
            },
            page_num: page,
            scale_factor: 1.0,
            line_bounds: vec![],
            link_rects: vec![],
            page_height_px: 100.0,
        }
    }

    #[test]
    fn cache_insert_and_get() {
        let mut cache = PageCache::new(10);
        let params = test_params();
        let key = CacheKey::from_params(0, &params);
        let data = test_page_data(0);

        cache.insert(key.clone(), data);

        assert!(cache.contains(&key));
        assert!(cache.get(&key).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cache_lru_eviction() {
        let mut cache = PageCache::new(2);
        let params = test_params();

        for i in 0..3 {
            let key = CacheKey::from_params(i, &params);
            cache.insert(key, test_page_data(i));
        }

        assert_eq!(cache.len(), 2);
        assert!(!cache.contains(&CacheKey::from_params(0, &params)));
        assert!(cache.contains(&CacheKey::from_params(1, &params)));
        assert!(cache.contains(&CacheKey::from_params(2, &params)));
    }

    #[test]
    fn cache_invalidate_all() {
        let mut cache = PageCache::new(10);
        let params = test_params();

        for i in 0..5 {
            let key = CacheKey::from_params(i, &params);
            cache.insert(key, test_page_data(i));
        }

        assert_eq!(cache.len(), 5);
        cache.invalidate_all();
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_invalidate_page() {
        let mut cache = PageCache::new(10);
        let params = test_params();

        // Insert two versions of page 0
        let key1 = CacheKey::from_params(0, &params);
        cache.insert(key1, test_page_data(0));

        let mut params2 = params.clone();
        params2.invert_images = true;
        let key2 = CacheKey::from_params(0, &params2);
        cache.insert(key2, test_page_data(0));

        // Insert page 1
        let key3 = CacheKey::from_params(1, &params);
        cache.insert(key3.clone(), test_page_data(1));

        assert_eq!(cache.len(), 3);

        // Invalidate page 0
        cache.invalidate_page(0);

        assert_eq!(cache.len(), 1);
        assert!(cache.contains(&key3));
    }
}
