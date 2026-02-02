//! Render service - manages worker pool and cache

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use flume::{Receiver, Sender};
use mupdf::Document;

use super::TocEntry;
use super::cache::{CacheKey, PageCache};
use super::parsing::page_numbers;
use super::request::{PageSelectionBounds, RenderRequest, RenderResponse, RequestId};
use super::state::{Command, Effect, RenderState};
use super::types::PageData;
use super::worker::render_worker;
use super::{CellSize, DEFAULT_CACHE_SIZE, DEFAULT_PREFETCH_RADIUS, DEFAULT_WORKERS};

#[derive(Debug)]
enum PendingRequest {
    Page(usize),
    Prefetch(usize),
    ExtractText,
}

/// Manages PDF rendering with worker threads and caching
pub struct RenderService {
    state: RenderState,
    request_tx: Sender<RenderRequest>,
    response_rx: Receiver<RenderResponse>,
    next_request_id: u64,
    pending_requests: HashMap<RequestId, PendingRequest>,
    cache: Arc<Mutex<PageCache>>,
    num_workers: usize,
    prefetch_radius: usize,
    prefetch_in_flight: std::collections::HashSet<usize>,
    doc_info: Option<DocumentInfo>,
}

/// Document metadata
#[derive(Clone, Debug)]
pub struct DocumentInfo {
    pub page_count: usize,
    pub title: Option<String>,
    pub toc: Vec<TocEntry>,
    pub page_number_samples: Vec<(usize, i32)>,
}

impl RenderService {
    /// Create a new render service with default configuration
    #[must_use]
    pub fn new(doc_path: PathBuf, cell_size: CellSize, black: i32, white: i32) -> Self {
        Self::with_config(
            doc_path,
            cell_size,
            black,
            white,
            DEFAULT_WORKERS,
            DEFAULT_CACHE_SIZE,
            DEFAULT_PREFETCH_RADIUS,
        )
    }

    /// Create a new render service with custom configuration
    #[must_use]
    pub fn with_config(
        doc_path: PathBuf,
        cell_size: CellSize,
        black: i32,
        white: i32,
        num_workers: usize,
        cache_size: usize,
        prefetch_radius: usize,
    ) -> Self {
        let cache = Arc::new(Mutex::new(PageCache::new(cache_size)));

        // We use flume for MPMC (multi-producer, multi-consumer) channels.
        // std::sync::mpsc and tokio::sync::mpsc are MPSC only - their Receiver
        // cannot be cloned. We need multiple workers to pull from a shared
        // request queue (fan-out), which requires MPMC. Flume is recommended
        // by Tokio for this pattern: https://github.com/tokio-rs/tokio/discussions/3891
        let (request_tx, request_rx) = flume::unbounded();
        let (response_tx, response_rx) = flume::unbounded();

        // Spawn worker threads - each clones request_rx to pull from shared queue
        for _ in 0..num_workers.max(1) {
            let path = doc_path.clone();
            let rx = request_rx.clone();
            let tx = response_tx.clone();
            let cache_clone = cache.clone();

            std::thread::spawn(move || {
                render_worker(&path, rx, tx, cache_clone);
            });
        }

        // Load document metadata
        let doc_info = Self::load_document_info(&doc_path);

        let mut state = RenderState::new(doc_path, cell_size, black, white);
        if let Some(ref info) = doc_info {
            state.page_count = info.page_count;
        }

        Self {
            state,
            request_tx,
            response_rx,
            next_request_id: 1,
            pending_requests: HashMap::new(),
            cache,
            num_workers: num_workers.max(1),
            prefetch_radius,
            prefetch_in_flight: std::collections::HashSet::new(),
            doc_info,
        }
    }

    fn load_document_info(doc_path: &Path) -> Option<DocumentInfo> {
        let doc = Document::open(doc_path.to_string_lossy().as_ref()).ok()?;
        let page_count = doc.page_count().ok()? as usize;

        if page_count == 0 {
            return None;
        }

        let title = doc
            .metadata(mupdf::MetadataName::Title)
            .ok()
            .filter(|t| !t.is_empty());

        let toc = super::parsing::toc::extract_toc(&doc, page_count);
        let page_number_samples = page_numbers::collect_page_number_samples(&doc, page_count);

        Some(DocumentInfo {
            page_count,
            title,
            toc,
            page_number_samples,
        })
    }

    /// Get document metadata
    #[must_use]
    pub fn document_info(&self) -> Option<&DocumentInfo> {
        self.doc_info.as_ref()
    }

    /// Get current render state
    #[must_use]
    pub fn state(&self) -> &RenderState {
        &self.state
    }

    /// Set the current page without triggering any render effects.
    /// Use this to sync the initial page before the first render.
    pub fn set_current_page_no_render(&mut self, page: usize) {
        self.state.current_page = page.min(self.state.page_count.saturating_sub(1));
    }

    /// Apply a command to the render state
    pub fn apply_command(&mut self, cmd: Command) {
        let effects = self.state.apply(cmd);
        self.execute_effects(effects);
    }

    fn execute_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::InvalidateCache => {
                    self.cache
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .invalidate_all();
                    self.prefetch_in_flight.clear();
                }

                Effect::InvalidatePage(page) => {
                    self.cache
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .invalidate_page(page);
                    self.prefetch_in_flight.remove(&page);
                }

                Effect::RenderCurrentPage => {
                    self.request_page(self.state.current_page);
                }

                Effect::RenderPage(page) => {
                    self.request_page(page);
                }

                Effect::ReloadDocument => {
                    self.doc_info = Self::load_document_info(&self.state.doc_path);
                    if let Some(ref info) = self.doc_info {
                        self.state.page_count = info.page_count;
                    }
                }

                Effect::UpdatePrefetch => {
                    self.schedule_prefetch();
                }
            }
        }
    }

    /// Request a page to be rendered
    pub fn request_page(&mut self, page: usize) -> RequestId {
        let id = self.next_id();
        let params = self.state.render_params();

        let _ = self
            .request_tx
            .send(RenderRequest::Page { id, page, params });
        self.pending_requests.insert(id, PendingRequest::Page(page));
        self.prefetch_in_flight.remove(&page);

        id
    }

    /// Request a page only if it is not cached or already in flight.
    pub fn request_page_if_needed(&mut self, page: usize) -> Option<RequestId> {
        if self.is_page_cached(page) || self.is_page_in_flight(page) {
            return None;
        }

        Some(self.request_page(page))
    }

    /// Extract text from selection bounds
    pub fn extract_text(&mut self, bounds: Vec<PageSelectionBounds>) -> RequestId {
        let id = self.next_id();
        let params = self.state.render_params();

        let _ = self
            .request_tx
            .send(RenderRequest::ExtractText { id, bounds, params });
        self.pending_requests
            .insert(id, PendingRequest::ExtractText);

        id
    }

    fn prefetch_page(&mut self, page: usize) -> RequestId {
        let id = self.next_id();
        let params = self.state.render_params();

        let _ = self
            .request_tx
            .send(RenderRequest::Prefetch { id, page, params });
        self.pending_requests
            .insert(id, PendingRequest::Prefetch(page));
        self.prefetch_in_flight.insert(page);

        id
    }

    fn schedule_prefetch(&mut self) {
        let current = self.state.current_page;
        let page_count = self.state.page_count;

        if page_count == 0 {
            return;
        }

        let key = CacheKey::from_params(current, &self.state.render_params());
        let current_cached = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&key);

        if !current_cached && !self.prefetch_in_flight.contains(&current) {
            self.request_page(current);
        }

        for offset in 1..=self.prefetch_radius {
            if current + offset < page_count {
                self.maybe_prefetch(current + offset);
            }
            if current >= offset {
                self.maybe_prefetch(current - offset);
            }
        }
    }

    fn maybe_prefetch(&mut self, page: usize) {
        if self.prefetch_in_flight.contains(&page) {
            return;
        }

        let key = CacheKey::from_params(page, &self.state.render_params());
        let cached = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&key);

        if !cached {
            self.prefetch_page(page);
        }
    }

    fn is_page_in_flight(&self, page: usize) -> bool {
        if self.prefetch_in_flight.contains(&page) {
            return true;
        }

        self.pending_requests.values().any(|request| match request {
            PendingRequest::Page(p) | PendingRequest::Prefetch(p) => *p == page,
            PendingRequest::ExtractText => false,
        })
    }

    /// Poll for completed render responses
    pub fn poll_responses(&mut self) -> Vec<RenderResponse> {
        let mut responses = vec![];

        while let Ok(response) = self.response_rx.try_recv() {
            match &response {
                RenderResponse::Page { id, page, .. } => {
                    self.pending_requests.remove(id);
                    self.prefetch_in_flight.remove(page);
                }
                RenderResponse::Cancelled(id) | RenderResponse::Error { id, .. } => {
                    if let Some(PendingRequest::Page(page) | PendingRequest::Prefetch(page)) =
                        self.pending_requests.remove(id)
                    {
                        self.prefetch_in_flight.remove(&page);
                    }
                }
                RenderResponse::ExtractedText { id, .. } => {
                    self.pending_requests.remove(id);
                }
                _ => {}
            }

            responses.push(response);
        }

        responses
    }

    /// Get the response receiver for async usage
    #[must_use]
    pub fn response_receiver(&self) -> &Receiver<RenderResponse> {
        &self.response_rx
    }

    /// Check if a page is cached
    #[must_use]
    pub fn is_page_cached(&self, page: usize) -> bool {
        let key = CacheKey::from_params(page, &self.state.render_params());
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&key)
    }

    /// Get a cached page if available
    #[must_use]
    pub fn get_cached_page(&self, page: usize) -> Option<Arc<PageData>> {
        let key = CacheKey::from_params(page, &self.state.render_params());
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
    }

    /// Shutdown all workers
    pub fn shutdown(&self) {
        for _ in 0..self.num_workers {
            let _ = self.request_tx.send(RenderRequest::Shutdown);
        }
    }

    fn next_id(&mut self) -> RequestId {
        let id = RequestId::new(self.next_request_id);
        self.next_request_id += 1;
        id
    }
}

impl Drop for RenderService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
