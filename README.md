# Bookokrat

Bookokrat is a terminal-based EPUB reader with a split-view library and reader, full MathML and image rendering, automatic bookmarks, and inline annotations.

## Demo

![CleanShot 2025-10-28 at 16 28 21](https://github.com/user-attachments/assets/a45d2e6a-4d2b-4f70-a77f-ed2f7cabc8d8)


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

## Installation

### Prerequisites
Bookokrat requires a C compiler/linker to be installed on your system for building dependencies.

**Linux (Ubuntu/Debian):**
```bash
sudo apt update
sudo apt install build-essential
```

**Linux (Fedora/RHEL):**
```bash
sudo dnf install gcc make
```

**macOS:**
```bash
xcode-select --install
```

**Windows:**
Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) with the "Desktop development with C++" workload.

### Install Bookokrat

1. Install Rust via https://rustup.rs if needed.
2. Install bookokrat using Cargo:

```bash
cargo install bookokrat
```

3. Place EPUB files alongside the binary (or run within your library directory) and navigate with the shortcuts above.

### Troubleshooting

**Error: "linker 'cc' not found"**

This means you don't have a C compiler installed. Install the build tools for your platform (see Prerequisites above), then try again.

## Attribution

This project is based on [bookrat](https://github.com/dmitrysobolev/bookrat) by Dmitry Sobolev, licensed under the MIT License.
