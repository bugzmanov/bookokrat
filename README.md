# Bookokrat

Bookokrat is a Rust terminal EPUB reader with a split-view library and reader, full MathML and image rendering, automatic bookmarks, and inline annotations.

## What You Can Do
- Browse every EPUB in the current directory, drill into the table of contents, and resume exactly where you left off.
- Search inside the current chapter or across the whole book, jump through a per-book history, and inspect reading statistics.
- Highlight text, attach comments, copy snippets or entire chapters, and toggle the raw HTML source for debugging.
- Open images in-place, follow internal anchors, launch external links in your browser, and hand off the book to your system viewer.

## Keyboard Reference
- **Global:** `q` quit; `Tab` switch library/content focus; `Esc` clear selection/search or dismiss popups; `Space+h` toggle reading history; `Space+d` book stats popup; `Space+o` open current book in the OS viewer.
- **Library & TOC panel:** `j/k` move; `Ctrl+d` / `Ctrl+u` half-page; `gg` top; `G` bottom; `/` start filter; `n` / `N` cycle matches; `h` collapse entry; `l` expand; `H` collapse all; `L` expand all; `Enter` open the highlighted book or heading.
- **Reader panel:** `j/k` scroll; `Ctrl+d` / `Ctrl+u` half-screen; `gg` top; `G` bottom; `h` / `l` previous or next chapter; `Ctrl+o` jump back; `Ctrl+i` jump forward; `/` search within chapter; `n` / `N` step through matches; `Space+f` reopen last book search; `Space+F` start a fresh book search; `Space+s` toggle raw HTML; `Space+c` copy entire chapter; `Space+z` copy a debug transcript; `c` or `Ctrl+C` copy selection; `a` create or edit a comment on the current selection; `d` delete the comment under the cursor; `p` toggle the profiler overlay.
- **Book search popup (Space+f / Space+F):** Type to search the whole book; `Enter` run the search or open the highlighted result; `j/k` or arrows move; `g` / `G` jump to top/bottom; `Space` return to the input field; `Esc` close.
- **Reading history popup (Space+h):** `j/k` move; `Ctrl+d` / `Ctrl+u` page; `gg` / `G` jump; `Enter` reopen the selection; `Esc` close.
- **Book stats popup (Space+d):** `j/k` move; `Ctrl+d` / `Ctrl+u` page; `gg` / `G` jump; `Enter` jump to the chapter; `Esc` close.
- **Comments:** Select text (keyboard or mouse), press `a` to add or edit, type your note, `Esc` saves; press `d` on a commented passage to remove it.
- **Images & links:** Click an image or highlight it and press `Enter` to open the zoomed popup (any key dismisses); following a link records your place so `Ctrl+o` / `Ctrl+i` step backward and forward.

## Mouse Support
- Scroll with the wheel over either pane; Bookokrat batches rapid wheel events for smooth scrolling.
- Single-click focuses a pane; double-click in the library opens the selection; double-click in the reader selects a word; triple-click selects the paragraph.
- Click-and-drag to highlight text; release on a hyperlink to open it; drag past the viewport edges to auto-scroll.
- Click images to open the zoom popup; click again or press any key to close; clicking history or stats entries activates them immediately.

## Quick Start
- Install Rust via https://rustup.rs if needed.
- From the repository, run:

```bash
cargo run
```

- Place EPUB files alongside the binary (or run within your library directory) and navigate with the shortcuts above.
