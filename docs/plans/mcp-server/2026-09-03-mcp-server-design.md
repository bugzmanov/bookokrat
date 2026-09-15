# MCP Server — External Agent Access to Current Reading Slide

**Date:** 2026-09-03
**Status:** Design (not yet implemented)

## Goal

Let an external AI agent ask the bookokrat MCP server for the text of what
the user is currently reading, so the agent can explain the current page.

**Primary use case:** *As an active reader, I want to ask my AI agent to
explain the current page from the book I'm currently reading in bookokrat.*

## Non-goals (v1)

- Tool parameters (e.g. optional `lines` padding on `get_current_page`).
  Parameterless tools only; revisit in a later version.
- Auth / tokens. UDS `0600` is the access control on a single-user machine.
- Multi-instance coordination. Single live reader is the norm; a second
  instance simply runs without the MCP listener (see Lifecycle).
- Windows named-pipe transport. Unix-only for v1, matching the SyncTeX
  precedent. Windows support is a later, self-contained `#[cfg(windows)]` add.
- Persistent socket connections. One connection per request for v1.

## Topology (chosen: B)

Three topologies were considered:

- **A. Socket-transport MCP, in-process** — the TUI process speaks MCP
  directly over a Unix socket. Rejected: requires the agent harness to
  support non-stdio MCP transports (many don't).
- **B. Separate `bookokrat mcp` subcommand (stdio) bridging to the live
  reader over a Unix socket** — chosen. The agent spawns
  `bookokrat mcp` like any normal stdio MCP server; that process connects
  to a socket the live reader publishes and pulls the current page text on
  demand. Matches the agent's spawn-a-server model, keeps the TUI process's
  stdout untouched (which *is* the terminal canvas — it cannot double as a
  transport), and reuses the proven socket+thread bridge already shipped as
  `SyncTexListener` (`src/pdf/synctex.rs:545`).
- **C. Snapshot file** — the reader writes current-page text to a known
  file; a stateless stdio server reads it. Rejected: staleness and write
  amplification are real downsides for a "explain *this* page" use case
  where freshness matters.

Two processes, one Unix socket:

- **Live reader (the TUI)** binds a background thread to a fixed singleton
  socket, e.g. `~/.cache/bookokrat/reader.sock`. Spawns the listener at
  startup; unlinks the socket on quit (same `Drop` pattern as
  `SyncTexListener`).
- **`bookokrat mcp` subcommand** is a plain stdio MCP server (JSON-RPC over
  stdin/stdout). On each tool call it connects to the socket, sends a
  request, reads a response, closes, and returns the text to the agent.

The reader produces responses via a snapshot, not a main-loop round-trip
(see next section).

## Transport & state bridge

The real fork was how the live reader produces the response:

- **(a) Round-trip via flume** — listener forwards the request to the main
  loop (like `poll_synctex_commands`), main loop builds the answer from
  live state, returns it via a oneshot. Rejected: a request would wait up
  to one event-loop iteration (~250ms tick, longer if blocked on a PDF
  render), and MCP's synchronous request/response needs a brand-new
  response channel anyway — no real reuse win over SyncTeX's fire-and-forget.
- **(b) Snapshot via `Arc<RwLock<ReaderSnapshot>>`** — chosen. The App
  maintains a cheap snapshot behind an Arc; the listener thread holds an
  Arc clone and reads directly on request. Zero main-loop involvement on
  the hot path, no loop-cadence latency, no stall risk when PDF rendering
  is mid-flight.

## Tool surface

Two parameterless MCP tools:

| Tool | EPUB | PDF |
|------|------|-----|
| `get_current_page` | quantized-viewport screenful | current page text |
| `get_current_chapter` | whole current chapter | current page ± 2 neighbors |

Rationale for the split:

- **EPUB `get_current_page`** uses a *quantized* viewport slice —
  `chunk_start = (scroll_offset / viewport_height) * viewport_height`,
  return `lines[chunk_start .. chunk_start + viewport_height]`, stringified.
  Quantizing the scroll offset to the top of the screen you're "on" means
  minor scrolling returns the same text, with zero pre-chunked store to
  maintain. Stable within a given terminal size; a resize re-flows the
  chapter (inherent to reflow, unavoidable, not worth fighting).
- **EPUB `get_current_chapter`** returns the whole current chapter — already
  fully in `rendered_content.lines`, just stringify all of it.
- **PDF `get_current_page`** is trivial: `PageData` text for the current
  page number.
- **PDF `get_current_chapter`** uses current page ± 2 neighbors. PDF has no
  "chapter" — it has an outline and individual pages. The full current TOC
  section was rejected as unbounded (a big section could dump 40 pages of
  text into the agent's context); page ± N is bounded broader context.

Output is bounded for both tools, which matters because the agent feeds it
to an LLM.

### Response envelope

Both tools return the same metadata envelope plus a `text` field
(pre-stringified in the snapshot, so the server does zero work):

```json
{
  "format": "epub" | "pdf",
  "book_path": "/abs/path/to/book.epub",
  "book_title": "...",
  "chapter_title": "...",
  "chapter_index": 3,
  "total_chapters": 12,
  "page_number": 7,
  "screen_index": 2,
  "total_pages": 240,
  "text": "..."
}
```

`page_number` is pdf-only, `screen_index` is epub-only, `total_pages` is
pdf-only — these are `Option`s and omitted from JSON when `None`
(`skip_serializing_if`).

### Error cases

- No book open (snapshot is `None`) → MCP error result:
  `"No book currently open in bookokrat."`
- `bookokrat mcp` cannot connect to the socket → MCP error result:
  `"bookokrat is not running. Open a book first."`

### Socket protocol

Newline-delimited JSON, one connection per request. `bookokrat mcp`, on each
`tools/call`, opens the socket, sends `{"tool":"get_current_page"}`, reads
one line, closes, returns to the agent. The reader's accept loop handles one
request → one response → close. Per-request connect is the laziest shape
that works and avoids framing/state on either side.

## `ReaderSnapshot` & update cadence

A small struct behind `Arc<RwLock<Option<ReaderSnapshot>>>`, cloned into the
listener thread at startup:

```rust
struct ReaderSnapshot {
    format: BookFormat,          // Epub | Pdf
    book_path: PathBuf,          // absolute, so the agent can cite/open it
    book_title: String,
    chapter_title: String,       // EPUB chapter title; PDF: nearest TOC entry or "Page N"
    chapter_index: usize,        // EPUB chapter idx
    page_number: Option<usize>,  // PDF (1-based)
    screen_index: Option<usize>, // EPUB quantized screen idx
    total_chapters: usize,
    total_pages: Option<usize>,  // PDF
    page_text: String,           // tool A output, pre-stringified
    chapter_text: String,        // tool B output, pre-stringified
}
```

The listener just reads the Arc and returns whichever field the requested
tool maps to — no per-request work, no main-loop involvement.

**Update trigger** — the key insight that keeps it cheap: don't rebuild
every frame. The render path computes a cheap state key
`(format, chapter_index, page_number, screen_index)`; only when it changes
does it rebuild and `Arc::write` the snapshot:

- `page_text` rebuilds on chunk/page change (one screenful, cheap slice +
  stringify).
- `chapter_text` rebuilds on chapter switch (EPUB, rare) or page change
  (PDF ±2 neighbors, cheap — three pages of text).
- No book open → snapshot is `None`; the listener returns the clear
  "no book loaded" response.

Hook point: one `App::update_reader_snapshot()` called near the end of the
loop iteration in `run_app_with_event_source` (alongside the existing
`poll_synctex_commands` block at `main_app.rs:8679`), gated behind
`needs_redraw` so idle frames skip it. The key-compare makes it effectively
free when nothing moved.

Scrolling *within* the same screen chunk touches nothing; scrolling to a
new chunk does one cheap rebuild; switching chapters does a (still cheap)
chapter-text rebuild. No path through the hot loop is materially slower.

## Lifecycle & edge cases

**Startup.** During app init (right after `load_custom_themes()` at
`main.rs:359`), spawn the reader socket listener thread + create the
`Arc<RwLock<Option<ReaderSnapshot>>>`. Store both on `App` (two new fields,
mirroring how `synctex_listener` / `synctex_rx` live on `App`). The listener
gets an Arc clone of the snapshot; the main loop keeps the other clone.

**Stale socket.**

- Socket file exists from a crashed instance → `unlink()` before `bind()`.
  A leftover `.sock` must not block a fresh launch.
- Two live readers → second instance's `bind()` fails → log a warning,
  continue *without* the MCP listener. The TUI still runs; only the agent
  bridge is dark. Single-instance enforcement is unrequested; skip rather
  than fight.

**No book open.** Snapshot is `None`. Listener returns the
`"No book currently open"` MCP error. No special-casing in the App — the
snapshot reflects whatever is loaded, `None` included.

**Mid-session format switch (EPUB↔PDF).** The state key
`(format, …)` changes → next `update_reader_snapshot()` rebuilds everything.
Fields the other format doesn't use are `None` (that's why the snapshot
carries `Option`s, and the JSON envelope omits nulls). No reset logic; the
key-change handles it.

**`bookokrat mcp` without a running reader.** Socket connect fails → MCP
server returns `"bookokrat is not running. Open a book first."` as an MCP
error result. Agent surfaces it. Never crashes the server, never hangs.

**Shutdown.** `Drop` for the listener unlinks the socket (same pattern as
`SyncTexListener`). The `Arc<RwLock>` snapshot needs no cleanup — process
exit reclaims it.

**Cross-platform.** `#[cfg(unix)]` on the socket code in both the reader
listener and the `mcp` subcommand's client side. On non-unix,
`bookokrat mcp` prints a clear error and exits nonzero. Windows named-pipe
support is a later, self-contained addition behind `#[cfg(windows)]`.

## Testing

MCP is IPC plumbing, not UI, so it follows the `synctex.rs` precedent
(plain `#[test]` + `cargo test`), **not** the SVG snapshot tests — those
scope to UI rendering. All three testable layers are sandbox-safe (no home
dir, no network, no TTY):

- **Snapshot key-compare (unit, in `src/mcp/`):** build fake reader state,
  call `update_reader_snapshot()` twice with the same key → snapshot Arc
  untouched; change the key → rebuilt with new text. Uses `test_utils`
  fake books, no terminal.
- **JSON protocol (unit):** serialize a `{"tool":"get_current_page"}`
  request, deserialize a response, round-trip. Pure functions.
- **End-to-end bridge (integration, TempDir-bound socket):** spawn the
  listener thread bound to a `tempfile::TempDir` socket path (not home dir,
  no network — sandbox-clean), inject a `Some(snapshot)`, connect a client,
  assert the text; inject `None`, assert the error response. The real socket
  round-trip — the integration point that breaks if framing is wrong.

The end-to-end test is the smallest thing that fails if the whole bridge
breaks (satisfies the one-check rule).

## Implementation map

Minimal footprint, six touch points:

| File | Change |
|------|--------|
| `src/cli.rs` | Add `Command::Mcp` variant (no args) |
| `src/main.rs` | Dispatch `Command::Mcp` → run stdio server **before** terminal init |
| `src/mcp/mod.rs` (new) | Stdio JSON-RPC server + socket client + JSON types, `#[cfg(unix)]` |
| `src/mcp/reader_listener.rs` (new) | Background listener thread bound to the socket — clone of `SyncTexListener`'s shape |
| `src/main_app.rs` | Two new `App` fields (snapshot `Arc` + listener), spawn in init, `update_reader_snapshot()` in the loop behind `needs_redraw` |
| Extraction helpers | `page_text`/`chapter_text` from the EPUB text reader (quantized slice + stringify spans / all lines) and PDF (`PageData` text / page±2 neighbors) |

`bookokrat mcp` runs entirely outside the terminal-init path — it's a stdio
process, never touches raw mode, crossterm, or the alternate screen. Clean
separation, no TUI entanglement.
