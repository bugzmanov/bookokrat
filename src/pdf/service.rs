//! Render service - manages worker pool and cache

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use flume::{Receiver, Sender};
use mupdf::Document;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::TocEntry;
use super::cache::{CacheKey, PageCache};
use super::parsing::page_numbers;
use super::parsing::toc::TocTarget;
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
    latest_page_request: HashMap<usize, RequestId>,
    cache: Arc<Mutex<PageCache>>,
    num_workers: usize,
    prefetch_radius: usize,
    prefetch_in_flight: std::collections::HashSet<usize>,
    doc_info: Option<DocumentInfo>,
    reload_generation: Arc<AtomicU64>,
    last_applied_reload_generation: u64,
    _watcher: Option<RecommendedWatcher>,
}

/// Document metadata
#[derive(Clone, Debug)]
pub struct DocumentInfo {
    pub page_count: usize,
    pub title: Option<String>,
    pub author: Option<String>,
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
        let reload_generation = Arc::new(AtomicU64::new(0));
        let num_workers = num_workers.max(1);

        // Spawn worker threads - each clones request_rx to pull from shared queue
        for _ in 0..num_workers {
            let path = doc_path.clone();
            let rx = request_rx.clone();
            let tx = response_tx.clone();
            let cache_clone = cache.clone();
            let generation = reload_generation.clone();

            std::thread::spawn(move || {
                render_worker(&path, rx, tx, cache_clone, generation);
            });
        }

        // Load document metadata
        let doc_info = Self::load_document_info(&doc_path);

        let mut state = RenderState::new(doc_path.clone(), cell_size, black, white);
        if let Some(ref info) = doc_info {
            state.page_count = info.page_count;
        }

        Self {
            state,
            request_tx,
            response_rx,
            next_request_id: 1,
            pending_requests: HashMap::new(),
            latest_page_request: HashMap::new(),
            cache,
            num_workers,
            prefetch_radius,
            prefetch_in_flight: std::collections::HashSet::new(),
            doc_info,
            reload_generation,
            last_applied_reload_generation: 0,
            _watcher: None,
        }
    }

    fn start_watcher(
        doc_path: &Path,
        reload_generation: &Arc<AtomicU64>,
        request_tx: &Sender<RenderRequest>,
    ) -> Option<RecommendedWatcher> {
        if super::worker::is_djvu_path(doc_path) {
            log::info!("File watching not supported for DjVu files");
            return None;
        }

        let parent = doc_path.parent()?;
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        let target_name = doc_path.file_name()?.to_owned();
        let last_reload = Arc::new(Mutex::new(Instant::now()));
        let generation = reload_generation.clone();
        let tx = request_tx.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
            let Ok(event) = res else { return };

            let dominated = matches!(
                event.kind,
                notify::EventKind::Access(_) | notify::EventKind::Remove(_)
            );
            if dominated {
                if matches!(event.kind, notify::EventKind::Remove(_)) {
                    log::warn!("Watched PDF file was removed");
                }
                return;
            }

            let hits_target = event
                .paths
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == target_name));
            if !hits_target {
                return;
            }

            let mut last = last_reload.lock().unwrap_or_else(|e| e.into_inner());
            if last.elapsed().as_millis() < 50 {
                return;
            }
            *last = Instant::now();

            generation.fetch_add(1, std::sync::atomic::Ordering::Release);
            // Wake a worker so it detects the generation change.
            // The worker reloads, sends Reloaded, service reschedules
            // prefetch which wakes remaining workers.
            let _ = tx.send(RenderRequest::Cancel(RequestId::new(0)));
        })
        .ok()?;

        if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
            log::warn!("Failed to watch PDF parent directory: {e}");
            return None;
        }

        log::info!("Watching for changes: {}", doc_path.display());
        Some(watcher)
    }

    fn load_document_info(doc_path: &Path) -> Option<DocumentInfo> {
        if super::worker::is_djvu_path(doc_path) {
            return Self::load_djvu_document_info(doc_path);
        }

        let doc = Document::open(doc_path.to_string_lossy().as_ref()).ok()?;
        let page_count = doc.page_count().ok()? as usize;

        if page_count == 0 {
            return None;
        }

        let title = doc
            .metadata(mupdf::MetadataName::Title)
            .ok()
            .filter(|t| !t.is_empty());

        let author = doc
            .metadata(mupdf::MetadataName::Author)
            .ok()
            .filter(|a| !a.is_empty());

        let toc = super::parsing::toc::extract_toc(&doc, page_count);
        let page_number_samples = page_numbers::collect_page_number_samples(&doc, page_count);

        Some(DocumentInfo {
            page_count,
            title,
            author,
            toc,
            page_number_samples,
        })
    }

    fn load_djvu_document_info(doc_path: &Path) -> Option<DocumentInfo> {
        let doc = rdjvu::Document::open(doc_path).ok()?;
        let page_count = doc.page_count();

        if page_count == 0 {
            return None;
        }

        let toc = doc
            .bookmarks()
            .map(|bookmarks| flatten_djvu_bookmarks(&bookmarks, 0, page_count))
            .unwrap_or_default();

        Some(DocumentInfo {
            page_count,
            title: None,
            author: None,
            toc,
            page_number_samples: Vec::new(),
        })
    }

    /// Enable file watching (auto-reload on disk change)
    pub fn enable_watching(&mut self) {
        if self._watcher.is_some() {
            return;
        }
        self._watcher = Self::start_watcher(
            &self.state.doc_path,
            &self.reload_generation,
            &self.request_tx,
        );
    }

    /// Disable file watching
    pub fn disable_watching(&mut self) {
        self._watcher = None;
    }

    /// Whether file watching is currently active
    #[must_use]
    pub fn is_watching(&self) -> bool {
        self._watcher.is_some()
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
        self.latest_page_request.insert(page, id);
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
        self.latest_page_request.insert(page, id);
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

        if !current_cached && !self.is_page_in_flight(current) {
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
        let mut saw_reload = false;

        while let Ok(response) = self.response_rx.try_recv() {
            let mut skip_response = false;
            match &response {
                RenderResponse::Page { id, page, .. } => {
                    self.pending_requests.remove(id);
                    self.prefetch_in_flight.remove(page);
                    let is_latest = self.latest_page_request.get(page) == Some(id);
                    if !is_latest {
                        skip_response = true;
                        log::debug!(
                            "Dropping stale render response page={} id={} latest={:?}",
                            page,
                            id.0,
                            self.latest_page_request.get(page).map(|r| r.0)
                        );
                    }
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
                RenderResponse::Reloaded { generation } => {
                    if saw_reload || *generation <= self.last_applied_reload_generation {
                        skip_response = true;
                    } else {
                        saw_reload = true;
                        self.last_applied_reload_generation = *generation;
                        self.doc_info = Self::load_document_info(&self.state.doc_path);
                        if let Some(ref info) = self.doc_info {
                            self.state.page_count = info.page_count;
                            if info.page_count > 0 {
                                self.state.current_page =
                                    self.state.current_page.min(info.page_count - 1);
                            }
                        }
                        self.cache
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .invalidate_all();
                        self.prefetch_in_flight.clear();
                        self.pending_requests.clear();
                        self.latest_page_request.clear();
                        log::info!("Document reloaded from disk");
                    }
                }
                _ => {}
            }

            if !skip_response {
                responses.push(response);
            }
        }

        if saw_reload {
            self.schedule_prefetch();
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

fn flatten_djvu_bookmarks(
    bookmarks: &[rdjvu::Bookmark],
    level: usize,
    page_count: usize,
) -> Vec<TocEntry> {
    let mut entries = Vec::new();

    for bookmark in bookmarks {
        let title = bookmark.title.trim();
        if title.is_empty() {
            continue;
        }

        entries.push(TocEntry {
            title: title.to_string(),
            level,
            target: djvu_bookmark_target(&bookmark.url, page_count),
        });

        entries.extend(flatten_djvu_bookmarks(
            &bookmark.children,
            level + 1,
            page_count,
        ));
    }

    entries
}

fn djvu_bookmark_target(url: &str, page_count: usize) -> TocTarget {
    let trimmed = url.trim();

    let parse_page = |value: &str| {
        value
            .parse::<usize>()
            .ok()
            .and_then(|page| page.checked_sub(1))
            .filter(|&page| page < page_count)
    };

    if let Some(page) = trimmed.strip_prefix('#').and_then(parse_page) {
        return TocTarget::InternalPage(page);
    }

    if let Some(page) = parse_page(trimmed) {
        return TocTarget::InternalPage(page);
    }

    TocTarget::External(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_cell_size() -> CellSize {
        CellSize::new(8, 16)
    }

    fn wait_for_page(service: &mut RenderService, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let responses = service.poll_responses();
            for r in &responses {
                if matches!(r, RenderResponse::Page { .. }) {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn wait_for_reload(service: &mut RenderService, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let responses = service.poll_responses();
            for r in &responses {
                if matches!(r, RenderResponse::Reloaded { .. }) {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn test_page_data(page: usize) -> Arc<PageData> {
        Arc::new(PageData {
            img_data: crate::pdf::types::ImageData {
                pixels: vec![0, 0, 0],
                width_px: 1,
                height_px: 1,
                width_cell: 1,
                height_cell: 1,
            },
            page_num: page,
            scale_factor: 1.0,
            requested_scale: 1.0,
            render_area_width_cells: 1,
            render_area_height_cells: 1,
            line_bounds: Vec::new(),
            link_rects: Vec::new(),
            page_height_px: 1.0,
        })
    }

    fn test_service_with_response_tx(doc_path: PathBuf) -> (RenderService, Sender<RenderResponse>) {
        let cache = Arc::new(Mutex::new(PageCache::new(5)));
        let (request_tx, _request_rx) = flume::unbounded();
        let (response_tx, response_rx) = flume::unbounded();
        let doc_info = RenderService::load_document_info(&doc_path);
        let mut state = RenderState::new(doc_path, test_cell_size(), 0, 0x00FF_FFFF);
        if let Some(ref info) = doc_info {
            state.page_count = info.page_count;
        }

        (
            RenderService {
                state,
                request_tx,
                response_rx,
                next_request_id: 1,
                pending_requests: HashMap::new(),
                latest_page_request: HashMap::new(),
                cache,
                num_workers: 0,
                prefetch_radius: 0,
                prefetch_in_flight: std::collections::HashSet::new(),
                doc_info,
                reload_generation: Arc::new(AtomicU64::new(0)),
                last_applied_reload_generation: 0,
                _watcher: None,
            },
            response_tx,
        )
    }

    #[test]
    fn reload_picks_up_new_pdf_content() {
        let pdf_a = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/vhs_test.pdf");
        let pdf_b =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/di_book_toc_test.pdf");

        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("test.pdf");
        std::fs::copy(&pdf_a, &target).unwrap();

        let mut service =
            RenderService::with_config(target.clone(), test_cell_size(), 0, 0x00FF_FFFF, 2, 5, 0);

        let page_count_a = service.document_info().unwrap().page_count;
        assert_eq!(page_count_a, 20);

        // Render page 0 from the original PDF
        service.request_page(0);
        assert!(
            wait_for_page(&mut service, Duration::from_secs(5)),
            "timed out waiting for initial page render"
        );

        // Replace the file with a different PDF
        std::fs::copy(&pdf_b, &target).unwrap();

        // Bump the generation counter (same as watcher would)
        service
            .reload_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);

        // Request a page — workers will detect the generation change before rendering
        service.request_page(0);

        // Workers reload before processing the page request, so we should see
        // both Reloaded and Page responses
        assert!(
            wait_for_reload(&mut service, Duration::from_secs(5)),
            "timed out waiting for reload response"
        );

        // After reload, doc_info should reflect the new PDF
        let page_count_b = service.document_info().unwrap().page_count;
        assert_eq!(page_count_b, 538);

        // Render page 0 from the reloaded PDF — should succeed
        assert!(
            wait_for_page(&mut service, Duration::from_secs(5)),
            "timed out waiting for re-render after reload"
        );

        // Verify by requesting a page that only exists in the new PDF.
        service.request_page(100);
        assert!(
            wait_for_page(&mut service, Duration::from_secs(5)),
            "timed out rendering page 100 (only in new PDF)"
        );
    }

    #[test]
    fn duplicate_reload_generation_is_ignored_across_polls() {
        let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/vhs_test.pdf");
        let (mut service, response_tx) = test_service_with_response_tx(pdf);

        response_tx
            .send(RenderResponse::Reloaded { generation: 1 })
            .unwrap();
        let responses = service.poll_responses();
        assert!(matches!(
            responses.as_slice(),
            [RenderResponse::Reloaded { generation: 1 }]
        ));

        service.pending_requests.clear();
        service.latest_page_request.clear();

        let id = RequestId::new(42);
        service.pending_requests.insert(id, PendingRequest::Page(0));
        service.latest_page_request.insert(0, id);

        response_tx
            .send(RenderResponse::Reloaded { generation: 1 })
            .unwrap();
        response_tx
            .send(RenderResponse::Page {
                id,
                page: 0,
                data: test_page_data(0),
            })
            .unwrap();

        let responses = service.poll_responses();
        assert!(matches!(
            responses.as_slice(),
            [RenderResponse::Page { id: got_id, page: 0, .. }] if *got_id == id
        ));
        assert!(service.pending_requests.is_empty());
    }
}
