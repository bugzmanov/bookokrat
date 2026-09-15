# MCP Server Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Let an external AI agent call a `bookokrat mcp` stdio MCP server to fetch the text of the page/chapter the user is currently reading in a running bookokrat instance.

**Architecture:** Two processes, one Unix domain socket (reusing the proven `SyncTexListener` thread pattern at `src/pdf/synctex.rs:545`). The live TUI maintains a cheap `Arc<RwLock<Option<ReaderSnapshot>>>` rebuilt only when the on-screen page/chapter changes; a background listener thread serves it over the socket. `bookokrat mcp` is a separate stdio JSON-RPC server the agent spawns; on each tool call it connects to the socket, pulls the snapshot, returns the text. Unix-only for v1, no `pdf` feature gate on the server (it just ferries JSON).

**Tech Stack:** Rust 2024, `serde_json` (already a dep), `std::os::unix::net` (same as SyncTeX), `clap` (already wired), `tempfile` (dev dep, for tests).

**Design doc:** `docs/plans/2026-09-03-mcp-server-design.md` — read it first.

---

## Design correction discovered during planning

The design doc assumed PDF page text is pre-cached in `PageData`. **It is not** — `PageData` (`src/pdf/types.rs:118`) carries only `line_bounds` (positions), not text. Two existing extraction paths exist:

1. **`Space+c` (CopyChapterText)** at `src/main_app.rs:5047` — calls `service.extract_text(...)` which is **async** (returns a `RequestId`, result arrives later via the conversion channel). Too much plumbing for a synchronous snapshot rebuild.
2. **Search** at `src/main_app.rs:7190` — opens the `Document` directly via `Document::open(doc_path)` + `page.to_text_page(TextPageFlags::empty())` **synchronously**, one-shot. **This is the pattern we reuse.**

So the PDF snapshot rebuild opens the Document and extracts the current page ± 2 pages of text synchronously. This blocks the render loop briefly **on page change only** (not per frame) — MuPDF text extraction is cheap (rasterizing is the expensive part, and we don't rasterize). The search path already extracts *all* pages synchronously in one go without issue, so 3 pages on page-change is fine.

```rust
// ponytail: synchronous mupdf open on page-change blocks the loop briefly.
// Upgrade path: route through the existing service.extract_text() async channel
// (like Space+c does) if this shows up in profiling.
```

EPUB extraction is genuinely free: `RenderedLine.raw_text` (`src/widget/text_reader/types.rs:62`) already holds each line's plain text (used for text selection), and the full rendered chapter lives in `text_reader.rendered_content.lines`. The viewport height is `text_reader.get_visible_height()` (`navigation.rs:631`); the top line is `text_reader.scroll_offset`.

---

## File map

| File | Kind | Purpose |
|------|------|---------|
| `src/mcp/mod.rs` | new | Module root, `pub fn run_mcp_server()`, socket-path helper |
| `src/mcp/protocol.rs` | new | `ReaderSnapshot`, `McpTool`, `McpRequest`/`McpResponse`, JSON encode/decode |
| `src/mcp/extract.rs` | new | Pure extraction fns: EPUB page/chapter text, PDF neighbor window |
| `src/mcp/reader_listener.rs` | new | Background Unix-socket listener thread (clone of `SyncTexListener`) |
| `src/main_app.rs` | modify | 2 new `App` fields, spawn listener in init, `update_reader_snapshot()` in loop |
| `src/cli.rs` | modify | Add `Command::Mcp` variant |
| `src/main.rs` | modify | Dispatch `Command::Mcp` before TUI init; `mod` declarations |
| `src/lib.rs` | modify | `pub mod mcp;` |

All socket code is `#[cfg(unix)]`. The stdio server entry `run_mcp_server()` is also `#[cfg(unix)]`; on non-unix `Command::Mcp` prints an error and exits nonzero (handled in `main.rs` dispatch).

The snapshot's PDF text extraction (mupdf) is `#[cfg(feature = "pdf")]`. Without the `pdf` feature, EPUB still works and the PDF branch is absent — unreachable anyway since no PDF can be opened.

---

## Task 1: Protocol types + JSON round-trip

**Files:**
- Create: `src/mcp/mod.rs`, `src/mcp/protocol.rs`
- Modify: `src/lib.rs` (add `pub mod mcp;`)

**Step 1: Write the failing test** in `src/mcp/protocol.rs` (bottom of file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_snapshot() -> ReaderSnapshot {
        ReaderSnapshot {
            format: SnapshotFormat::Epub,
            book_path: PathBuf::from("/abs/book.epub"),
            book_title: "Book".into(),
            chapter_title: "Ch 1".into(),
            chapter_index: 0,
            page_number: None,
            screen_index: Some(2),
            total_chapters: 5,
            total_pages: None,
            page_text: "screen text".into(),
            chapter_text: "whole chapter".into(),
        }
    }

    #[test]
    fn request_round_trip() {
        let req = McpRequest { tool: McpTool::GetCurrentPage };
        let line = encode_request_line(&req);
        let back = decode_request_line(&line).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn response_ok_round_trip() {
        let snap = sample_snapshot();
        let result = build_tool_result(&snap, &McpTool::GetCurrentChapter);
        let resp = McpResponse::Ok { result };
        let line = encode_response_line(&resp);
        let back: McpResponse = serde_json::from_str(&line).unwrap();
        match back {
            McpResponse::Ok { result } => assert_eq!(result.text, "whole chapter"),
            McpResponse::Error { .. } => panic!("expected Ok"),
        }
    }

    #[test]
    fn tool_result_carries_chosen_text_and_metadata() {
        let snap = sample_snapshot();
        let page = build_tool_result(&snap, &McpTool::GetCurrentPage);
        assert_eq!(page.text, "screen text");
        assert_eq!(page.book_title, "Book");
        assert_eq!(page.screen_index, Some(2));
        // page_number is None for epub -> omitted from JSON
        let json = serde_json::to_string(&page).unwrap();
        assert!(!json.contains("page_number"));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test --lib mcp::protocol
```
Expected: FAIL — module/types don't exist.

**Step 3: Write minimal implementation** in `src/mcp/protocol.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotFormat {
    Epub,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReaderSnapshot {
    pub format: SnapshotFormat,
    pub book_path: PathBuf,
    pub book_title: String,
    pub chapter_title: String,
    pub chapter_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_index: Option<usize>,
    pub total_chapters: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<usize>,
    pub page_text: String,
    pub chapter_text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTool {
    GetCurrentPage,
    GetCurrentChapter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpRequest {
    pub tool: McpTool,
}

/// Per-tool response: metadata + exactly one `text` (the chosen page/chapter).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub format: SnapshotFormat,
    pub book_path: PathBuf,
    pub book_title: String,
    pub chapter_title: String,
    pub chapter_index: usize,
    pub total_chapters: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum McpResponse {
    Ok { result: ToolResult },
    Error { error: String },
}

pub fn build_tool_result(snap: &ReaderSnapshot, tool: McpTool) -> ToolResult {
    let text = match tool {
        McpTool::GetCurrentPage => snap.page_text.clone(),
        McpTool::GetCurrentChapter => snap.chapter_text.clone(),
    };
    ToolResult {
        format: snap.format.clone(),
        book_path: snap.book_path.clone(),
        book_title: snap.book_title.clone(),
        chapter_title: snap.chapter_title.clone(),
        chapter_index: snap.chapter_index,
        total_chapters: snap.total_chapters,
        page_number: snap.page_number,
        screen_index: snap.screen_index,
        total_pages: snap.total_pages,
        text,
    }
}

pub fn encode_request_line(req: &McpRequest) -> String {
    serde_json::to_string(req).expect("request serializes")
}

pub fn decode_request_line(line: &str) -> Option<McpRequest> {
    serde_json::from_str(line.trim()).ok()
}

pub fn encode_response_line(resp: &McpResponse) -> String {
    serde_json::to_string(resp).expect("response serializes")
}
```

In `src/mcp/mod.rs`:

```rust
pub mod protocol;
```

In `src/lib.rs` add with the other `pub mod` lines:

```rust
pub mod mcp;
```

**Step 4: Run test to verify it passes**

```bash
cargo test --lib mcp::protocol
```
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add src/mcp/ src/lib.rs
git commit -m "feat(mcp): add protocol types and JSON round-trip"
```

---

## Task 2: EPUB extraction functions

**Files:**
- Create: `src/mcp/extract.rs`
- Modify: `src/mcp/mod.rs` (add `pub mod extract;`)

Pure functions over `&[RenderedLine]`. The quantized page chunk: `chunk_start = (scroll_offset / height) * height`; return lines `[chunk_start .. chunk_start + height]` joined by `\n` via `raw_text`. Chapter text: all lines joined.

**Step 1: Write the failing test** in `src/mcp/extract.rs`:

```rust
use crate::widget::text_reader::types::{RenderedLine, LineType};

fn line(text: &str) -> RenderedLine {
    let mut l = RenderedLine::empty();
    l.raw_text = text.into();
    l.line_type = LineType::Text;
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_text_joins_all_lines() {
        let lines = vec![line("a"), line("b"), line("c")];
        assert_eq!(epub_chapter_text(&lines), "a\nb\nc");
    }

    #[test]
    fn page_text_quantizes_to_screen_top() {
        // 10 lines, viewport height 4, scrolled to offset 5.
        // quantized chunk_start = (5/4)*4 = 4 -> lines 4,5,6,7
        let lines: Vec<_> = (0..10).map(|i| line(&format!("L{i}"))).collect();
        let text = epub_page_text(&lines, 5, 4);
        assert_eq!(text, "L4\nL5\nL6\nL7");
    }

    #[test]
    fn page_text_same_chunk_for_nearby_scroll() {
        let lines: Vec<_> = (0..10).map(|i| line(&format!("L{i}"))).collect();
        // offsets 4,5,6,7 all quantize to chunk_start 4
        for off in [4, 5, 6, 7] {
            assert_eq!(epub_page_text(&lines, off, 4), "L4\nL5\nL6\nL7", "off={off}");
        }
    }

    #[test]
    fn page_text_clamps_at_end_of_chapter() {
        let lines: Vec<_> = (0..6).map(|i| line(&format!("L{i}"))).collect();
        // 6 lines, height 4, offset 4 -> chunk_start 4 -> only L4,L5 remain
        assert_eq!(epub_page_text(&lines, 4, 4), "L4\nL5");
    }

    #[test]
    fn page_text_empty_chapter() {
        assert_eq!(epub_page_text(&[], 0, 4), "");
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test --lib mcp::extract
```
Expected: FAIL — functions undefined.

**Step 3: Write minimal implementation** in `src/mcp/extract.rs` (above the test module):

```rust
use crate::widget::text_reader::types::RenderedLine;

/// Whole current chapter: all rendered lines joined by newline.
pub fn epub_chapter_text(lines: &[RenderedLine]) -> String {
    lines
        .iter()
        .map(|l| l.raw_text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Current quantized viewport screenful.
///
/// `chunk_start = (scroll_offset / height) * height` so minor scrolling
/// within the same screen returns identical text.
pub fn epub_page_text(lines: &[RenderedLine], scroll_offset: usize, viewport_height: usize) -> String {
    if lines.is_empty() || viewport_height == 0 {
        return String::new();
    }
    let chunk_start = (scroll_offset / viewport_height) * viewport_height;
    let end = (chunk_start + viewport_height).min(lines.len());
    lines[chunk_start..end]
        .iter()
        .map(|l| l.raw_text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
```

Add `pub mod extract;` to `src/mcp/mod.rs`.

**Step 4: Run test to verify it passes**

```bash
cargo test --lib mcp::extract
```
Expected: PASS (5 tests).

**Step 5: Commit**

```bash
git add src/mcp/extract.rs src/mcp/mod.rs
git commit -m "feat(mcp): add EPUB page/chapter text extraction"
```

---

## Task 3: PDF neighbor-page window helper

**Files:**
- Modify: `src/mcp/extract.rs`

Pure windowing (testable without mupdf): given current page, total pages, window size, return the inclusive page range `[start, end]` clamped to `[0, total)`. The actual mupdf text extraction is added in Task 5 (it needs the doc path + `#[cfg(feature = "pdf")]`).

**Step 1: Write the failing test** (append to `src/mcp/extract.rs` test module):

```rust
    #[test]
    fn pdf_window_mid_book() {
        assert_eq!(pdf_neighbor_window(5, 10, 2), 3..=7);
    }

    #[test]
    fn pdf_window_clamps_at_start() {
        assert_eq!(pdf_neighbor_window(0, 10, 2), 0..=2);
        assert_eq!(pdf_neighbor_window(1, 10, 2), 0..=3);
    }

    #[test]
    fn pdf_window_clamps_at_end() {
        assert_eq!(pdf_neighbor_window(9, 10, 2), 7..=9);
        assert_eq!(pdf_neighbor_window(8, 10, 2), 6..=9);
    }

    #[test]
    fn pdf_window_when_window_exceeds_total() {
        assert_eq!(pdf_neighbor_window(1, 3, 5), 0..=2);
    }
```

**Step 2: Run test to verify it fails**

```bash
cargo test --lib mcp::extract
```
Expected: FAIL — `pdf_neighbor_window` undefined.

**Step 3: Write minimal implementation**:

```rust
use std::ops::RangeInclusive;

/// Inclusive page range `[start, end]` for current page ± `window`, clamped to `[0, total)`.
pub fn pdf_neighbor_window(current: usize, total: usize, window: usize) -> RangeInclusive<usize> {
    if total == 0 {
        return 0..=0;
    }
    let last = total - 1;
    let start = current.saturating_sub(window).min(last);
    let end = (current + window).min(last);
    start..=end
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test --lib mcp::extract
```
Expected: PASS (9 tests total in extract).

**Step 5: Commit**

```bash
git add src/mcp/extract.rs
git commit -m "feat(mcp): add PDF neighbor-page window helper"
```

---

## Task 4: Reader socket listener (the bridge)

**Files:**
- Create: `src/mcp/reader_listener.rs`
- Modify: `src/mcp/mod.rs` (add `#[cfg(unix)] pub mod reader_listener;`)

Clone of `SyncTexListener` (`src/pdf/synctex.rs:545` — read it first). Differences: instead of forwarding commands over flume, it reads the snapshot from an `Arc<RwLock<Option<ReaderSnapshot>>>` directly and writes the JSON response. One request → one response → close, newline-delimited JSON.

**Step 1: Write the failing test** in `src/mcp/reader_listener.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::{McpRequest, McpResponse, McpTool, ReaderSnapshot, SnapshotFormat};
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};

    fn sample_snapshot(text: &str) -> ReaderSnapshot {
        ReaderSnapshot {
            format: SnapshotFormat::Epub,
            book_path: PathBuf::from("/abs/book.epub"),
            book_title: "Book".into(),
            chapter_title: "Ch 1".into(),
            chapter_index: 0,
            page_number: None,
            screen_index: Some(2),
            total_chapters: 5,
            total_pages: None,
            page_text: text.into(),
            chapter_text: format!("{text} full chapter"),
        }
    }

    #[test]
    fn listener_returns_page_text_when_book_loaded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sock = tmp.path().join("reader.sock");
        let snap = Arc::new(RwLock::new(Some(sample_snapshot("hello page"))));
        let listener = McpReaderListener::start(sock.clone(), snap).unwrap();

        let resp = send_request(&sock, &McpRequest { tool: McpTool::GetCurrentPage }).unwrap();
        match resp {
            McpResponse::Ok { result } => assert_eq!(result.text, "hello page"),
            McpResponse::Error { error } => panic!("expected Ok, got error: {error}"),
        }
        drop(listener);
        assert!(!sock.exists(), "socket should be cleaned up on drop");
    }

    #[test]
    fn listener_returns_error_when_no_book_loaded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sock = tmp.path().join("reader.sock");
        let snap: Arc<RwLock<Option<ReaderSnapshot>>> = Arc::new(RwLock::new(None));
        let listener = McpReaderListener::start(sock.clone(), snap).unwrap();

        let resp = send_request(&sock, &McpRequest { tool: McpTool::GetCurrentChapter }).unwrap();
        match resp {
            McpResponse::Error { error } => {
                assert!(error.to_lowercase().contains("no book"), "got: {error}");
            }
            McpResponse::Ok { .. } => panic!("expected error for no book"),
        }
        drop(listener);
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test --lib mcp::reader_listener
```
Expected: FAIL — types don't exist.

**Step 3: Write minimal implementation** in `src/mcp/reader_listener.rs`:

```rust
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};

use crate::mcp::protocol::{
    McpResponse, McpTool, ReaderSnapshot, build_tool_result, decode_request_line,
    encode_response_line,
};

type SnapshotSlot = Arc<RwLock<Option<ReaderSnapshot>>>;

pub struct McpReaderListener {
    socket_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl McpReaderListener {
    /// Bind `socket_path` and serve snapshot reads until dropped.
    /// `bind` failure (e.g. another instance already bound) returns Err;
    /// callers log and continue without the MCP bridge.
    pub fn start(socket_path: PathBuf, snapshot: SnapshotSlot) -> Result<Self> {
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)
            .with_context(|| format!("failed to bind mcp socket: {}", socket_path.display()))?;
        listener.set_nonblocking(true)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        // Rendezvous: don't return until the accept loop is about to run
        // (prevents a connect-before-accept race on macOS — same fix as SyncTeX).
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(0);

        let join_handle = std::thread::Builder::new()
            .name("mcp-reader-listener".into())
            .spawn(move || {
                let _ = ready_tx.send(());
                Self::loop_(listener, snapshot, shutdown_clone);
            })
            .context("failed to spawn mcp listener thread")?;

        let _ = ready_rx.recv();
        log::info!("MCP reader listener started on {}", socket_path.display());

        Ok(Self {
            socket_path,
            shutdown,
            join_handle: Some(join_handle),
        })
    }

    fn loop_(
        listener: std::os::unix::net::UnixListener,
        snapshot: SnapshotSlot,
        shutdown: Arc<AtomicBool>,
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    let reader = match stream.try_clone() {
                        Ok(r) => r,
                        Err(e) => {
                            log::warn!("mcp: failed to clone stream: {e}");
                            continue;
                        }
                    };
                    let mut buf_reader = std::io::BufReader::new(reader);
                    let mut line = String::new();
                    if buf_reader.read_line(&mut line).is_err() {
                        continue;
                    }
                    let resp = match decode_request_line(&line) {
                        Some(req) => Self::handle(req, &snapshot),
                        None => McpResponse::Error {
                            error: "invalid request".into(),
                        },
                    };
                    let _ = writeln!(stream, "{}", encode_response_line(&resp));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    log::error!("mcp listener accept error: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    }

    fn handle(req: crate::mcp::protocol::McpRequest, snapshot: &SnapshotSlot) -> McpResponse {
        let guard = match snapshot.read() {
            Ok(g) => g,
            Err(_) => return McpResponse::Error { error: "reader state unavailable".into() },
        };
        match guard.as_ref() {
            Some(snap) => McpResponse::Ok {
                result: build_tool_result(snap, req.tool),
            },
            None => McpResponse::Error {
                error: "No book currently open in bookokrat.".into(),
            },
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for McpReaderListener {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // Unblock a parked accept()
        let _ = std::os::unix::net::UnixStream::connect(&self.socket_path);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Client: connect, send one request line, read one response line. Used by
/// the `bookokrat mcp` stdio server (Task 6) and by tests.
pub fn send_request(socket_path: &Path, req: &crate::mcp::protocol::McpRequest) -> Result<McpResponse> {
    use crate::mcp::protocol::encode_request_line;
    let mut stream = std::os::unix::net::UnixStream::connect(socket_path)
        .with_context(|| format!("failed to connect to mcp socket: {}", socket_path.display()))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    writeln!(stream, "{}", encode_request_line(req))?;
    stream.flush()?;
    let mut resp_line = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(&stream), &mut resp_line)?;
    Ok(serde_json::from_str(resp_line.trim())?)
}

// keep McpTool import used even when only send_request references it via type
#[allow(unused_imports)]
use McpTool as _UnusedMcpTool;
```

> Note: the trailing `_UnusedMcpTool` import is a guard if the compiler flags `McpTool` as unused (it's used in `handle` via `req.tool`). Run `cargo fmt` after — if fmt/clippy flags the import, remove it. Prefer letting `cargo fmt` + `cargo clippy` decide; do not hand-format.

Add to `src/mcp/mod.rs`:

```rust
#[cfg(unix)]
pub mod reader_listener;
```

**Step 4: Run test to verify it passes**

```bash
cargo test --lib mcp::reader_listener
```
Expected: PASS (2 tests). Both the text round-trip and the no-book error, plus socket cleanup on drop.

**Step 5: Commit**

```bash
git add src/mcp/reader_listener.rs src/mcp/mod.rs
git commit -m "feat(mcp): add reader socket listener with snapshot bridge"
```

---

## Task 5: Snapshot build + App wiring

**Files:**
- Modify: `src/main_app.rs`

This is the integration glue. The testable core (extraction fns, key) is already covered; here we add the `App` fields, spawn the listener, and rebuild the snapshot on change. Two new `#[cfg(unix)]` `App` fields mirroring the SyncTeX fields at `src/main_app.rs:435`.

**Step 1: Write the failing test** for the pure state-key (append to a test module in `src/mcp/extract.rs` or a new `src/mcp/snapshot.rs` — keep it in `extract.rs` to avoid a new file):

```rust
    #[test]
    fn snapshot_key_differs_on_chapter_change() {
        let k0 = snapshot_key(SnapshotFormat::Epub, 1, None, Some(2));
        let k1 = snapshot_key(SnapshotFormat::Epub, 2, None, Some(2));
        assert_ne!(k0, k1);
    }

    #[test]
    fn snapshot_key_same_within_screen_chunk() {
        // same chapter, same quantized screen index -> same key
        let k = snapshot_key(SnapshotFormat::Epub, 1, None, Some(2));
        assert_eq!(k, snapshot_key(SnapshotFormat::Epub, 1, None, Some(2)));
    }
```

(Add `use crate::mcp::protocol::SnapshotFormat;` to the test module imports.)

**Step 2: Run test to verify it fails**

```bash
cargo test --lib mcp::extract
```
Expected: FAIL — `snapshot_key` undefined.

**Step 3: Write the key helper** in `src/mcp/extract.rs`:

```rust
use crate::mcp::protocol::SnapshotFormat;

/// Cheap state key. The snapshot is rebuilt only when this changes.
/// (chapter_index, page_number, screen_index) — whichever the format uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotKey {
    format: SnapshotFormat,
    chapter_index: usize,
    page_number: Option<usize>,
    screen_index: Option<usize>,
}

pub fn snapshot_key(
    format: SnapshotFormat,
    chapter_index: usize,
    page_number: Option<usize>,
    screen_index: Option<usize>,
) -> SnapshotKey {
    SnapshotKey {
        format,
        chapter_index,
        page_number,
        screen_index,
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test --lib mcp::extract
```
Expected: PASS.

**Step 5: Wire the snapshot onto `App`** (no unit test — integration; verified by `cargo build` + not breaking existing tests). In `src/main_app.rs`:

1. Add imports near the top (with the other `use crate::` lines):
   ```rust
   #[cfg(unix)]
   use crate::mcp::reader_listener::McpReaderListener;
   #[cfg(unix)]
   use crate::mcp::protocol::{ReaderSnapshot, SnapshotFormat};
   #[cfg(unix)]
   use crate::mcp::extract::{epub_chapter_text, epub_page_text, pdf_neighbor_window, snapshot_key};
   use std::sync::{Arc, RwLock};
   ```

2. Add two fields on `App` (next to the `synctex_listener` field, ~line 435):
   ```rust
   #[cfg(unix)]
   reader_snapshot: Arc<RwLock<Option<ReaderSnapshot>>>,
   #[cfg(unix)]
   mcp_listener: Option<McpReaderListener>,
   ```

3. Init both in `App::new` (in the same struct-literal block where `synctex_listener: None` etc. are set, ~line 845):
   ```rust
   #[cfg(unix)]
   reader_snapshot: Arc::new(RwLock::new(None)),
   #[cfg(unix)]
   mcp_listener: None,
   ```

4. Add a spawn method and call it once after the app is constructed. Following the SyncTeX pattern (`src/main_app.rs:7465`), add:
   ```rust
   #[cfg(unix)]
   pub fn start_mcp_listener(&mut self) {
       let path = crate::mcp::mcp_socket_path();
       match McpReaderListener::start(path.clone(), self.reader_snapshot.clone()) {
           Ok(listener) => {
               self.mcp_listener = Some(listener);
               log::info!("MCP socket: {}", path.display());
           }
           Err(e) => log::warn!("MCP listener not started (another instance?): {e}"),
       }
   }
   ```
   Call it in `src/main.rs` right after `load_custom_themes();` (~line 359): `app.start_mcp_listener();` (the `app` is constructed a few lines below — call it after `App::new(...)` returns, before `run_app_with_event_source`).

5. Add the `mcp_socket_path()` helper to `src/mcp/mod.rs`:
   ```rust
   #[cfg(unix)]
   pub fn mcp_socket_path() -> std::path::PathBuf {
       // Stable singleton path under the user's config dir; fall back to temp.
       match crate::settings::preferred_config_dir() {
           Some(dir) => dir.join("reader.sock"),
           None => std::env::temp_dir().join("bookokrat-reader.sock"),
       }
   }
   ```

6. Add `update_reader_snapshot()` on `App` and call it in the loop. The method:
   ```rust
   #[cfg(unix)]
   fn update_reader_snapshot(&mut self) {
       // Determine current state + key. Bail (snapshot = None) when no book.
       let (key, snap) = self.build_reader_snapshot();
       let prev = self.reader_snapshot.read().map(|g| g.as_ref().map(|s|
           snapshot_key(s.format.clone(), s.chapter_index, s.page_number, s.screen_index)
       )).ok().flatten();
       if prev.as_ref() == Some(&key) {
           return; // nothing moved
       }
       if let Ok(mut g) = self.reader_snapshot.write() {
           *g = snap;
       }
   }
   ```
   `build_reader_snapshot()` returns `(SnapshotKey, Option<ReaderSnapshot>)`:
   - If no book open → `(dummy key, None)`.
   - **EPUB/Html:** pull `&self.text_reader.rendered_content.lines`, `scroll_offset`, `get_visible_height()`. `screen_index = Some(scroll_offset / height)`. `page_text = epub_page_text(...)`, `chapter_text = epub_chapter_text(...)`. chapter_title/index/total from existing reader fields (mirror how the status bar reads them). `format = SnapshotFormat::Epub`.
   - **PDF** (`#[cfg(feature = "pdf")]`): `page = self.pdf_reader.as_ref().unwrap().page`; `total_pages` from the document; `page_number = Some(page + 1)`. Extract text via `crate::pdf` mupdf: `Document::open(book_path)`, for `p` in `pdf_neighbor_window(page, total, 2)` `load_page(p).to_text_page(...)` and join. `page_text` = current page's text; `chapter_text` = all window pages joined. `format = SnapshotFormat::Pdf`. (Reuse the exact extraction loop from `src/main_app.rs:7190-7230`.)
   - Wrap PDF extraction in the `ponytail:` comment noted in the design-correction section above.

7. Call it in the loop: in `run_app_with_event_source`, near the `if app.poll_synctex_commands()` block (`src/main_app.rs:8679`), add inside the same tick branch:
   ```rust
   #[cfg(unix)]
   if needs_redraw {
       app.update_reader_snapshot();
   }
   ```

**Step 6: Build + run lib tests (no regressions)**

```bash
cargo build
cargo test --lib
```
Expected: build OK; 561+ tests still pass (plus the new mcp tests).

**Step 7: Commit**

```bash
git add src/main_app.rs src/main.rs src/mcp/mod.rs src/mcp/extract.rs
git commit -m "feat(mcp): maintain reader snapshot and spawn listener in App"
```

---

## Task 6: stdio MCP server + CLI wiring

**Files:**
- Modify: `src/mcp/mod.rs` (add `run_mcp_server`)
- Modify: `src/cli.rs` (add `Command::Mcp`)
- Modify: `src/main.rs` (dispatch + `mod`/`use`)

Minimal JSON-RPC 2.0 server: handle `initialize`, `notifications/initialized` (no response), `tools/list`, `tools/call`. For `tools/call`, read the tool name, connect to the socket via `reader_listener::send_request`, wrap the result as MCP `text` content.

**Step 1: Write the failing test** for the pure JSON-RPC response builders. Add to a new test module in `src/mcp/mod.rs`:

```rust
#[cfg(test)]
mod server_tests {
    use super::*;

    #[test]
    fn tools_list_advertises_both_tools() {
        let resp = tools_list_result(1);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("get_current_page"));
        assert!(json.contains("get_current_chapter"));
    }

    #[test]
    fn tool_call_success_wraps_text_content() {
        let resp = tool_call_ok(1, "hello world");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("hello world"));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test --lib mcp::server_tests
```
Expected: FAIL.

**Step 3: Write minimal implementation** in `src/mcp/mod.rs`:

```rust
use serde_json::{json, Value};

pub mod protocol;
pub mod extract;
#[cfg(unix)]
pub mod reader_listener;

#[cfg(unix)]
pub fn mcp_socket_path() -> std::path::PathBuf {
    match crate::settings::preferred_config_dir() {
        Some(dir) => dir.join("reader.sock"),
        None => std::env::temp_dir().join("bookokrat-reader.sock"),
    }
}

fn tools_list_result(id: i64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "get_current_page",
                    "description": "Get the text of the page/slide currently displayed in the running bookokrat reader.",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "get_current_chapter",
                    "description": "Get the text of the current chapter (broader context) from the running bookokrat reader.",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]
        }
    })
}

fn tool_call_ok(id: i64, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [ { "type": "text", "text": text } ]
        }
    })
}

fn error_result(id: i64, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(unix)]
pub fn run_mcp_server() -> anyhow::Result<()> {
    use std::io::{BufRead, Write};
    use crate::mcp::protocol::{McpRequest, McpTool};
    use crate::mcp::reader_listener::send_request;

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let sock = mcp_socket_path();
    let mut id_counter: i64 = 0;

    for line in stdin.lock().lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        if line.trim().is_empty() { continue; }
        let req: Value = match serde_json::from_str(&line) { Ok(v) => v, Err(_) => continue };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        id_counter += 1;

        let resp: Value = match method {
            "initialize" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "bookokrat", "version": env!("CARGO_PKG_VERSION") }
                }
            }),
            "notifications/initialized" => continue, // notification -> no response
            "tools/list" => tools_list_result(id_counter),
            "tools/call" => {
                let tool_name = req.pointer("/params/name")
                    .and_then(|v| v.as_str()).unwrap_or("");
                let tool = match tool_name {
                    "get_current_page" => McpTool::GetCurrentPage,
                    "get_current_chapter" => McpTool::GetCurrentChapter,
                    _ => {
                        let _ = writeln!(stdout, "{}", error_result(id_counter, -32602, "unknown tool"));
                        stdout.flush()?; continue;
                    }
                };
                match send_request(&sock, &McpRequest { tool }) {
                    Ok(crate::mcp::protocol::McpResponse::Ok { result }) => {
                        let payload = serde_json::to_string(&result)?;
                        tool_call_ok(id_counter, &payload)
                    }
                    Ok(crate::mcp::protocol::McpResponse::Error { error }) => {
                        error_result(id_counter, -32000, &error)
                    }
                    Err(_) => error_result(id_counter, -32000,
                        "bookokrat is not running. Open a book first."),
                }
            }
            _ => error_result(id_counter, -32601, "method not found"),
        };
        writeln!(stdout, "{}", resp)?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn run_mcp_server() -> anyhow::Result<()> {
    eprintln!("bookokrat mcp is not supported on this platform (requires a Unix domain socket).");
    std::process::exit(1);
}
```

Add `Command::Mcp` to `src/cli.rs` (in the `Command` enum):

```rust
    /// Run as a stdio MCP server bridging to a running bookokrat instance
    Mcp,
```

Dispatch in `src/main.rs` — add to the `match command` block (~line 158), before the closing brace:

```rust
            cli::Command::Mcp => {
                return bookokrat::mcp::run_mcp_server();
            }
```

**Step 4: Run test to verify it passes**

```bash
cargo test --lib mcp::server_tests
```
Expected: PASS (2 tests).

**Step 5: Build (both feature sets) + verify CLI**

```bash
cargo build
cargo build --features pdf
cargo run -- mcp --help
```
Expected: builds clean both ways; `mcp --help` lists the `mcp` subcommand.

**Step 6: Commit**

```bash
git add src/mcp/mod.rs src/cli.rs src/main.rs
git commit -m "feat(mcp): add stdio MCP server and `bookokrat mcp` subcommand"
```

---

## Task 7: End-to-end manual smoke + final formatting

**Step 1: Manual smoke test** (two terminals):

Terminal 1 — run the reader on any book:
```bash
cargo run -- /path/to/book.epub
```
Terminal 2 — drive the MCP server by hand (JSON-RPC over stdio):
```bash
cargo run -- mcp <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"initialize"}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_current_page"}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_current_chapter"}}
EOF
```
Expected: each request gets one JSON response line; `tools/list` shows both tools; the two `tools/call` responses contain the current page text and chapter text respectively. Quit the reader in terminal 1, repeat a `tools/call` → expect the `"bookokrat is not running"` error.

**Step 2: Run the full lib test suite (no pdf) and with pdf**

```bash
cargo test --lib
cargo test --lib --features pdf
```
Expected: all pass.

**Step 3: Format + lint**

```bash
cargo fmt
cargo clippy
cargo clippy --features pdf
```
Fix any clippy warnings introduced by the new code (do not touch unrelated warnings).

**Step 4: Commit formatting if any**

```bash
git add -A
git commit -m "style: cargo fmt"  # only if fmt changed anything
```

---

## Verification checklist

- [ ] `cargo test --lib` and `cargo test --lib --features pdf` both pass (561 + new tests, 0 failures).
- [ ] `cargo build` and `cargo build --features pdf` both clean.
- [ ] `cargo clippy` / `cargo clippy --features pdf` introduce no new warnings.
- [ ] Manual smoke: `get_current_page` and `get_current_chapter` return live text from a running reader.
- [ ] Reader-not-running case returns the clear MCP error, not a hang/crash.
- [ ] Socket file is cleaned up on quit (check `ls ~/.config/bookokrat/reader.sock` after exit).
- [ ] Two reader instances: second logs a warning and continues without the bridge (no crash).
- [ ] No golden snapshots or VHS tapes touched (this feature adds no UI rendering).
- [ ] No changes to `~/.config/bookokrat/config.yaml` format (no settings migration needed).

## Notes for the implementer

- **Read `src/pdf/synctex.rs:545-670` first** — the listener is a direct clone of that shape.
- **`cargo fmt` after every task** — never hand-format (CLAUDE.md rule 5/6).
- **No `eprintln` in lib/TUI code** — use `log::` (CLAUDE.md). The `#[cfg(not(unix))]` `run_mcp_server` `eprintln` is fine: it's a CLI-only error path before the TUI starts, and `bookokrat mcp` owns stdout (JSON-RPC) so errors must go to stderr.
- **Tests are sandbox-safe**: all use `tempfile::TempDir` for sockets, never touch the real config dir or `dirs::cache_dir()`. The prod `mcp_socket_path()` uses `preferred_config_dir()`; tests bypass it by passing explicit paths.
- **PDF text extraction reuses the search path** (`src/main_app.rs:7190`), not the async `service.extract_text()` path — keeps the snapshot rebuild synchronous and self-contained.
- **The `pdf` feature gate**: extraction of PDF text is `#[cfg(feature = "pdf")]`; without it the PDF branch is absent (unreachable, since no PDF can open). The MCP module itself and `bookokrat mcp` are never feature-gated.
