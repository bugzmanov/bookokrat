# Bookokrat

Bookokrat is a terminal-based EPUB and PDF reader with a split-view library and reader, full MathML and image rendering, automatic bookmarks, inline annotations, and customizable themes.

## Demo

![CleanShot 2025-10-28 at 16 28 21](https://github.com/user-attachments/assets/a45d2e6a-4d2b-4f70-a77f-ed2f7cabc8d8)


## What You Can Do
- Browse every EPUB & PDF in the current directory or automatically detect and read from your Calibre library with proper metadata.
- Drill into the table of contents, and resume exactly where you left off.
- Search inside the current chapter or across the whole book, jump through a per-book history, and inspect reading statistics.
- Highlight text, attach comments, export annotations to Markdown, copy snippets or entire chapters, and toggle the raw HTML source for debugging.
- Read PDFs with a dedicated renderer (TOC navigation, page/scroll modes, bookmarks, and annotations) in graphics-capable terminals.
- Open images in-place, follow internal anchors, launch external links in your browser, and hand off the EPUB to your system viewer.
- Customize with multiple color themes, adjustable margins, and zen mode; settings persist across sessions.
- Enter a Vim-style normal mode in the reader for precise motions, visual selection, and yanking to clipboard.
- Load EPUB bundles (exploded `.epub` directories, including Apple Books exports) without repackaging.
- Read complex HTML tables and rich cell content with improved rendering and image support.

## Installation

### Homebrew (macOS)

```bash
brew install bookokrat
```

### Arch Linux

Install from the [AUR](https://aur.archlinux.org/packages/bookokrat-bin) using your preferred AUR helper:

```bash
yay -S bookokrat-bin
```

or

```bash
paru -S bookokrat-bin
```

### Nix/NixOS (via Flakes)

If you have [Nix](https://nixos.org/download.html) with Flakes enabled, you can run Bookokrat directly:

```bash
nix run github:bugzmanov/bookokrat
```

To install it into your Nix profile:

```bash
nix profile install github:bugzmanov/bookokrat
```

### Pre-built Binaries (Linux)

Download from [GitHub Releases](https://github.com/bugzmanov/bookokrat/releases):

```bash
# x86_64 (Intel/AMD)
curl -LO https://github.com/bugzmanov/bookokrat/releases/latest/download/bookokrat-v0.2.2-x86_64-unknown-linux-musl.tar.gz
tar -xzf bookokrat-v0.2.2-x86_64-unknown-linux-musl.tar.gz
sudo mv bookokrat /usr/local/bin/
```

### Cargo (all platforms)

Build from source. Requires [Rust](https://rustup.rs) and a C compiler/linker.

<details>
<summary>Prerequisites (click to expand)</summary>

**Linux (Ubuntu/Debian):**
```bash
sudo apt update 
sudo apt install build-essential

# For PDF to work:
sudo apt install pkg-config libfontconfig1-dev clang libclang-dev
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

</details>

```bash
cargo install bookokrat
```

Note: PDF support is enabled by default. If you want to build without PDF support:
```bash
cargo install bookokrat --no-default-features
```

If you need symbols for profiling or debugging, build with the debug release profile:
```bash
cargo build --profile release-debug
```

## Vendored Dependency Notice

**Important:** `tui-textarea` is vendored in `vendor/tui-textarea` because the upstream
repository (https://github.com/rhysd/tui-textarea) is currently unmaintained and out of
step with the latest `ratatui`. Our vendored copy is based on PR #118:
https://github.com/rhysd/tui-textarea/pull/118/changes

### Getting Started

Navigate to a directory with EPUB files and run `bookokrat`. Use `j/k` to navigate, `Enter` to open a book, and `?` for help.

You can also open a specific file or start in zen mode, but this is not the main flow:

```bash
bookokrat path/to/book.epub
bookokrat path/to/book.pdf
bookokrat path/to/book.epub --zen-mode
```

## Keyboard Reference

Bookokrat follows Vim-style keybindings throughout the interface for consistent, efficient navigation.

### Global Commands
- `q` - Quit application
- `Tab` - Switch focus between library/TOC and content panels
- `Esc` - Clear selection/search or dismiss popups
- `Ctrl+z` - Toggle zen mode (hide sidebar/status bar)
- `?` - Show help screen
- `Space+t` - Open theme selector
- `Space+s` / `Ctrl+s` - Open settings (PDF support + render mode)
- `+` / `-` - Increase/decrease content margins

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
- `ss` - Toggle raw HTML view
- `Space+c` - Copy entire chapter (EPUB) / extract current page text (PDF)
- `Space+z` - Copy debug transcript
- `c` or `Ctrl+C` - Copy selection
- `p` - Toggle profiler overlay
- `n` - Toggle normal mode (Vim motions, visual selection, yanking)
- `v` / `V` (normal mode) - Enter visual character/line selection; `y` to yank, `Esc` to exit
- `Enter` (normal mode) - Open link under cursor

### Comments & Annotations
- `a` - Create or edit comment on selection
- `d` - Delete comment under cursor
- `Space+e` - Export all annotations to Markdown (in comments viewer)

### Popups & External Actions
- `Space+h` - Toggle reading history popup
- `Space+d` - Show book statistics popup
- `Space+a` - Open comments/annotations viewer
- `Space+o` - Open current EPUB in OS viewer
- `Enter` - Open image popup (when on image) or activate popup selection

### Popup Navigation
All popups (search results, reading history, book stats) support:
- `j/k` - Move up/down
- `Ctrl+d` / `Ctrl+u` - Half-page scroll
- `gg` / `G` - Jump to top/bottom
- `Enter` - Activate selection
- `Esc` - Close popup

## Image Rendering

Bookokrat automatically selects the best image protocol for your terminal:

| Terminal | Protocol | Reason |
|----------|----------|--------|
| **Kitty** | Kitty | Native support |
| **Ghostty** | Kitty | Native support |
| **iTerm2** | Sixel | Native protocol causes flickering; Kitty buggy in recent betas |
| **WezTerm** | iTerm2 | Kitty is buggy ([#986](https://github.com/wezterm/wezterm/issues/986)); Sixel broken on Windows ([#5758](https://github.com/wezterm/wezterm/issues/5758)); some flickering expected |
| **Alacritty** | Halfblocks | No graphics protocol support ([#910](https://github.com/alacritty/alacritty/issues/910)) |
| **Others** | Auto-detected | Kitty > Sixel > iTerm2 > Halfblocks |

**Experience by terminal:**
- **Excellent:** Kitty, Ghostty, iTerm2
- **Good:** WezTerm (some flickering)
- **Basic:** Alacritty, Linux default terminals (Halfblocks fallback)
- **No images:** macOS Terminal.app (no graphics protocol support)

**PDF viewing requires a graphics-capable terminal.** For the best PDF experience, **Kitty or Ghostty are strongly recommended**—they support SHM-based image transfer for smooth 60fps rendering. WezTerm and iTerm2 work but with reduced performance and some feature limitations. Use the settings popup (`Space+s` or `Ctrl+s`) to disable PDF mode if your terminal does not support graphics.

If images look wrong, check `bookokrat.log` for the detected protocol. Experiencing issues not covered above? Just [open an issue](https://github.com/bugzmanov/bookokrat/issues) — happy to help!

## Mouse Support
- Scroll with the wheel over either pane; Bookokrat batches rapid wheel events for smooth scrolling.
- Single-click focuses a pane; double-click in the library opens the selection; double-click in the reader selects a word; triple-click selects the paragraph.
- Click-and-drag to highlight text; release on a hyperlink to open it; drag past the viewport edges to auto-scroll.
- Click images to open the zoom popup; click again or press any key to close; clicking history or stats entries activates them immediately.

## Attribution

This project is based on [bookrat](https://github.com/dmitrysobolev/bookrat) by Dmitry Sobolev, licensed under the MIT License.

## License

This project is licensed under the GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later). See [LICENSE](LICENSE) for details.
