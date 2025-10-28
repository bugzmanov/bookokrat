# Bookokrat

Bookokrat is a Rust terminal EPUB reader with a split-view library and reader, full MathML and image rendering, automatic bookmarks, and inline annotations.

## What You Can Do
- Browse every EPUB in the current directory, drill into the table of contents, and resume exactly where you left off.
- Search inside the current chapter or across the whole book, jump through a per-book history, and inspect reading statistics.
- Highlight text, attach comments, copy snippets or entire chapters, and toggle the raw HTML source for debugging.
- Open images in-place, follow internal anchors, launch external links in your browser, and hand off the book to your system viewer.

## Keyboard Reference

Bookokrat follows Vim-style keybindings throughout the interface for consistent, efficient navigation.

### Global Commands
- `q` - Quit application
- `Tab` - Switch focus between library/TOC and content panels
- `Esc` - Clear selection/search or dismiss popups

### Navigation (Vim-style)
- `j/k` - Move down/up (works in all lists and reader)
- `h/l` - Collapse/expand in TOC; previous/next chapter in reader
- `Ctrl+d` / `Ctrl+u` - Scroll half-page down/up
- `gg` - Jump to top
- `G` - Jump to bottom
- `Ctrl+o` / `Ctrl+i` - Jump backward/forward in history

### Search
- `/` - Start search (filter in library/TOC; search in reader)
- `n` / `N` - Jump to next/previous match
- `Space+f` - Reopen last book-wide search
- `Space+F` - Start fresh book-wide search

### Library & TOC Panel
- `Enter` - Open highlighted book or heading
- `h` / `l` - Collapse/expand entry
- `H` / `L` - Collapse/expand all

### Reader Panel
- `h` / `l` - Previous/next chapter
- `Space+s` - Toggle raw HTML view
- `Space+c` - Copy entire chapter
- `Space+z` - Copy debug transcript
- `c` or `Ctrl+C` - Copy selection
- `p` - Toggle profiler overlay

### Comments & Annotations
- `a` - Create or edit comment on selection
- `d` - Delete comment under cursor

### Popups & External Actions
- `Space+h` - Toggle reading history popup
- `Space+d` - Show book statistics popup
- `Space+o` - Open current book in OS viewer
- `Enter` - Open image popup (when on image) or activate popup selection

### Popup Navigation
All popups (search results, reading history, book stats) support:
- `j/k` - Move up/down
- `Ctrl+d` / `Ctrl+u` - Half-page scroll
- `gg` / `G` - Jump to top/bottom
- `Enter` - Activate selection
- `Esc` - Close popup

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

## Attribution

This project is based on [bookrat](https://github.com/dmitrysobolev/bookrat) by Dmitry Sobolev, licensed under the MIT License.
