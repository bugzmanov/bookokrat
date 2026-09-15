# MCP Server Bugfixes — Postmortem

Two bugs surfaced after the initial MCP server implementation
(`feature/mcp-server` branch). Both let the `bookokrat mcp` stdio server
return a broken page/chapter view even though all unit tests were green.
This file records the symptom, root cause, why the test suite missed it,
and the regression guard added for each.

## Bug 1 — `get_current_page` returned the book/chapter title but empty `text`

**Symptom.** With a book open in the running TUI, an agent calling
`get_current_page` received a populated `book_title`, `chapter_title`,
`chapter_index` — but `text` was `""`. `get_current_chapter` (before bug 2
was hit) behaved the same.

**Root cause.** `App::update_reader_snapshot()` ran at the _start_ of the
`if needs_redraw {` block, i.e. **before** `terminal.draw()`. But
`MarkdownTextReader::rendered_lines()` is only populated _inside_ `draw()`
(lazy render on a cache-generation bump). So on the first redraw after
opening a book or navigating, the snapshot read an empty (or
previous-chapter) line slice → `epub_page_text()` / `epub_chapter_text()`
returned `""`. `book_title` and `chapter_title` are sourced independently of
the rendered lines, which is exactly why they were present while `text` was
empty.

A second, related defect: the snapshot-key used to skip unchanged frames
was `(format, chapter_index, page_number, screen_index)`. It did **not**
include the render generation, so a comment edit, raw-HTML toggle, or image
settle at the _same_ scroll position left the agent's view stale until the
user navigated.

**Why tests missed it.** `src/mcp/extract.rs` tests `epub_page_text` /
`epub_chapter_text` as pure functions over hand-built `Vec<RenderedLine>`
that are _already populated_. They verify slicing/joining logic in
isolation but never exercise the run-loop ordering property "at the moment
`App::build_reader_snapshot` runs, `rendered_lines()` must be populated."
The mock line slices actively hid the lifecycle that broke.

**Fix (commit `…after draw so page_text is populated`).**

1. Moved the `update_reader_snapshot()` call to **after** `terminal.draw()`
   (after `EndSynchronizedUpdate`), so `rendered_lines()` reflects the frame
   just painted. This also fixes a sibling: on chapter navigation the
   pre-draw read returned the _old_ chapter's lines.
2. Added a `generation: u64` field to `SnapshotKey`; new
   `MarkdownTextReader::render_generation()` accessor (returns
   `cache_generation`). `update_reader_snapshot` now compares against a
   tracked `last_snapshot_key` on `App` (the protocol `ReaderSnapshot` does
   not carry the generation) and rebuilds whenever the position _or_ the
   render generation changes.

**Regression test.** `snapshot_key_differs_on_generation_change` in
`src/mcp/extract.rs` — locks in the generation-in-key behavior.

## Bug 2 — `get_current_chapter` returned "bookokrat is not running. Open a book first."

**Symptom.** `get_current_page` worked (returned the visible page text), but
`get_current_chapter` _always_ errored with `bookokrat is not running. Open a
book first.` — the exact string the stdio server emits from the
`send_request()` `Err(_)` arm.

**Root cause.** The reader socket **listener** is `set_nonblocking(true)`
so `accept()` returns `WouldBlock` when idle and the listener thread can
sleep. On Unix, **accepted streams inherit that non-blocking mode**. The
handler then does a synchronous `writeln!`→`write_all` of the JSON response
line — but `write_all` is built for _blocking_ streams: it loops on partial
`Ok(n)` writes and on `Interrupted`, but it does **not** retry on
`WouldBlock`; it returns the error immediately.

So once the response exceeds the kernel send buffer (~8 KiB on macOS):

- `get_current_page` → small payload (~2 KiB, one screen) → fits in a single
  write, no `WouldBlock` → **works**.
- `get_current_chapter` → large payload (whole chapter, 100s of KiB) → fills
  the send buffer → `write_all` returns `Err(WouldBlock)` after ~8 KiB → the
  stream drops, the connection closes mid-line → the client `read_line` hits
  `EOF while parsing a string at line 1 column 8192` → `send_request()`
  returns `Err` → the stdio server emits the "not running" message.

Instrumentation confirmed it directly:

```
[srv] resp_line.len()=1150178
[srv] write_all result=Err(Os { code: 35, kind: WouldBlock, message: "Resource temporarily unavailable" })
[cli] resp_line.len()=8192
```

**Why tests missed it.** The existing listener tests exercised
`GetCurrentPage` with a populated snapshot and `GetCurrentChapter` only with
a `None` snapshot. There was no test for `GetCurrentChapter` _with_ a
populated snapshot — and certainly none at a realistic payload size — so
the truncation never triggered.

**Fix (commit `…not truncated`).** Set each **accepted** stream back to
blocking mode (`stream.set_nonblocking(false)`) plus a
`set_write_timeout(5s)` so a dead client cannot hang the listener thread.
The listener itself stays non-blocking for idle `accept()`. One line of
real logic; the rest is timeouts.

**Regression tests.** In `src/mcp/reader_listener.rs`:

- `listener_returns_chapter_text_when_book_loaded` — baseline small chapter
  round-trip (the gap that let this slip through).
- `listener_returns_chapter_text_large_payload` — ~1.15 MB chapter; fails
  with `EOF … column 8192` before the fix, passes after.

## Lesson

Both bugs share one shape: a **boundary / lifecycle property** that pure
unit tests cannot see. Bug 1 is a run-loop ordering property ("render before
snapshot"); bug 2 is a socket-mode property ("accepted streams inherit
non-blocking, and `write_all` does not retry on `WouldBlock`"). Mocked
inputs at the function level keep the suite green while the integration
breaks. The regression guards therefore live at the integration layer
(listener round-trip at realistic payload size; snapshot-key semantics) —
the layer where the bug actually occurred.
