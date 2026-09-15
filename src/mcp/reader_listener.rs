use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};

use crate::mcp::protocol::{
    McpResponse, ReaderSnapshot, build_tool_result, decode_request_line, encode_response_line,
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
                    // The listener is non-blocking so accept() returns
                    // WouldBlock when idle; accepted streams inherit that.
                    // We handle each connection synchronously, so switch this
                    // stream back to blocking — otherwise write_all returns
                    // WouldBlock mid-write once the send buffer fills and
                    // truncates a large response (a full chapter is the large
                    // payload), surfacing to the client as "bookokrat is not
                    // running".
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));
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
            Err(_) => {
                return McpResponse::Error {
                    error: "reader state unavailable".into(),
                };
            }
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
pub fn send_request(
    socket_path: &Path,
    req: &crate::mcp::protocol::McpRequest,
) -> Result<McpResponse> {
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

    fn snapshot_with_chapter(page: &str, chapter: &str) -> ReaderSnapshot {
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
            page_text: page.into(),
            chapter_text: chapter.into(),
        }
    }

    #[test]
    fn listener_returns_page_text_when_book_loaded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sock = tmp.path().join("reader.sock");
        let snap = Arc::new(RwLock::new(Some(sample_snapshot("hello page"))));
        let listener = McpReaderListener::start(sock.clone(), snap).unwrap();

        let resp = send_request(
            &sock,
            &McpRequest {
                tool: McpTool::GetCurrentPage,
            },
        )
        .unwrap();
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

        let resp = send_request(
            &sock,
            &McpRequest {
                tool: McpTool::GetCurrentChapter,
            },
        )
        .unwrap();
        match resp {
            McpResponse::Error { error } => {
                assert!(error.to_lowercase().contains("no book"), "got: {error}");
            }
            McpResponse::Ok { .. } => panic!("expected error for no book"),
        }
        drop(listener);
    }

    #[test]
    fn listener_returns_chapter_text_when_book_loaded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sock = tmp.path().join("reader.sock");
        let snap = Arc::new(RwLock::new(Some(snapshot_with_chapter(
            "hello page",
            "the whole chapter",
        ))));
        let listener = McpReaderListener::start(sock.clone(), snap).unwrap();

        let resp = send_request(
            &sock,
            &McpRequest {
                tool: McpTool::GetCurrentChapter,
            },
        )
        .unwrap();
        match resp {
            McpResponse::Ok { result } => assert_eq!(result.text, "the whole chapter"),
            McpResponse::Error { error } => panic!("expected Ok, got error: {error}"),
        }
        drop(listener);
    }

    #[test]
    fn listener_returns_chapter_text_large_payload() {
        // A real EPUB chapter is hundreds of KB; the visible page is ~2KB.
        // get_current_page works, get_current_chapter must too — i.e. the
        // full-chapter payload must round-trip over the socket at size.
        let big = "line of chapter text.\n".repeat(50_000); // ~1.15 MB
        let tmp = tempfile::TempDir::new().unwrap();
        let sock = tmp.path().join("reader.sock");
        let snap = Arc::new(RwLock::new(Some(snapshot_with_chapter("hello page", &big))));
        let listener = McpReaderListener::start(sock.clone(), snap).unwrap();

        let resp = send_request(
            &sock,
            &McpRequest {
                tool: McpTool::GetCurrentChapter,
            },
        )
        .expect("chapter request must not error");
        match resp {
            McpResponse::Ok { result } => assert_eq!(result.text, big),
            McpResponse::Error { error } => panic!("expected Ok, got error: {error}"),
        }
        drop(listener);
    }
}
