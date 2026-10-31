# Bookokrat

Terminal EPUB/PDF/DJVU reader focused on speed, smooth navigation, and Vim-style workflows.

https://github.com/user-attachments/assets/0ebe61c6-4629-4bde-8bd4-50feb9a424a3

## Highlights

- EPUB, PDF, and DJVU support in one TUI app
- Split layout: library/TOC on the left, reader on the right
- Fast PDF/DJVU pipeline with Kitty SHM image transfer in supported terminals
- Search, bookmarks, jump list history, reading stats
- Inline comments/annotations with persistent storage and Markdown export; note bodies can be hidden and peeked on demand
- Image rendering, link handling, and external viewer handoff
- Theme selection, adjustable margins, zen mode
- Vim-style keybindings and normal mode

## Install

### Homebrew (macOS)

```bash
brew install bookokrat
```

### Arch Linux (AUR)

Install from the [AUR](https://aur.archlinux.org/packages/bookokrat-bin):

```bash
yay -S bookokrat-bin
# or
paru -S bookokrat-bin
```

### Nix / NixOS (flakes)

```bash
nix run github:bugzmanov/bookokrat
# or install into profile
nix profile install github:bugzmanov/bookokrat
```

### Prebuilt Linux binaries

Download from [GitHub Releases](https://github.com/bugzmanov/bookokrat/releases).

### Cargo (all platforms)

Build from source. Requires [Rust](https://rustup.rs) and a C compiler/linker.

<details>
<summary>Prerequisites (click to expand)</summary>

**Linux (Ubuntu/Debian):**
```bash
sudo apt update
sudo apt install build-essential

# For PDF / DJVU support:
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
Install [MSYS2](https://www.msys2.org/) and run the following in its **UCRT64**
shell (with Rust available on `PATH`):

```bash
pacman -S --needed base-devel mingw-w64-ucrt-x86_64-toolchain mingw-w64-ucrt-x86_64-clang
rustup override set stable-x86_64-pc-windows-gnu
export AR="$(cygpath -m /ucrt64/bin/ar.exe)"
export RANLIB="$(cygpath -m /ucrt64/bin/ranlib.exe)"
export LIBCLANG_PATH="$(cygpath -w /ucrt64/bin)"
cargo build --locked --release
```

To bundle `target/release/bookokrat.exe` with its runtime DLLs into a ZIP
(requires `7z` on `PATH`):

```bash
bash tools/package_windows.sh target/release/bookokrat.exe bookokrat-windows.zip
```

> **Windows notes:**
> - PDF/DjVu need a terminal with image support (e.g. Windows Terminal or WezTerm).
> - For full Kitty graphics protocol support, consider using WSL with [Ghostty](https://ghostty.org/) or [Kitty](https://sw.kovidgoyal.net/kitty/).
> - `--synctex-forward` (editor → viewer) is not supported on native Windows.
> - For EPUB-only, build with `--no-default-features`.

</details>

```bash
cargo install bookokrat
```

Build without PDF / DJVU support:

```bash
cargo install bookokrat --no-default-features
```

## Quick Start

```bash
bookokrat
```

Optional direct open:

```bash
bookokrat path/to/book.epub
bookokrat path/to/book.pdf
bookokrat path/to/book.djvu
bookokrat path/to/book.epub --zen-mode
bookokrat path/to/book.epub --no-images   # text only; PDF and DJVU can't be opened
```

Press `?` inside the app to open the built-in help.

## SyncTeX (LaTeX ↔ PDF)

Bookokrat supports bidirectional SyncTeX navigation between LaTeX sources and PDF output. Compile your LaTeX document with `pdflatex --synctex=1` (or equivalent) to generate a `.synctex.gz` sidecar file. When a PDF is opened and its sidecar is found, SyncTeX activates automatically.

**Inverse search** (PDF → source): Ctrl+click, right-click, or `gd` in normal mode jumps to the corresponding LaTeX source line. `gd` also works on selected text (the mouse selection anchor is used). Configure which editor to open in `~/.bookokrat_settings.yaml`:

```yaml
# Neovim via remote socket (recommended for VimTeX \lv workflow)
synctex_editor: "nvim --server /tmp/nvim.sock --remote-send '<C-\\><C-n>:e {file}<CR>:{line}<CR>'"

# Open in a new terminal nvim
synctex_editor: "nvim +{line} {file}"
```

Placeholders: `{file}`, `{line}`, `{column}`.

**Forward search** (source → PDF): From your editor, send a forward search command to the running instance via Unix socket. VimTeX setup:

```vim
let g:vimtex_view_method = 'general'
let g:vimtex_view_general_cmd = 'bookokrat --synctex-forward @line:@col:@tex @pdf'
```

Then press `\lv` in Neovim to jump from source to PDF. Any editor that can shell out works — the generic form is:

```bash
bookokrat --synctex-forward LINE:COLUMN:FILE path/to/document.pdf
```

The `synctex_editor` setting can also be configured in the Settings popup (`Space+s`), under the Integrations tab.

## Documentation

- Full usage and keyboard reference: [`readme.txt`](readme.txt)

## Terminal Notes

PDF and DJVU viewing require a graphics-capable terminal.

- Best experience: Kitty, Ghostty
- Good: iTerm2, WezTerm, Warp, Konsole (with some limitations)
- Terminals without graphics protocol support - EPUB support without images. PDFs and DJVUs are not supported

For protocol details and troubleshooting, see the in-app help (`?`) and [`readme.txt`](readme.txt).

## Data Storage

Bookokrat stores state in XDG-compliant locations and keeps project directories clean.

| Data | Location |
|------|----------|
| Bookmarks | `<data_dir>/bookokrat/libraries/<library>/bookmarks.json` |
| Comments | `<data_dir>/bookokrat/libraries/<library>/comments/` |
| Image cache | `<cache_dir>/bookokrat/libraries/<library>/temp_images/` |
| Log file | `<state_dir>/bookokrat/bookokrat.log` |
| Settings | `~/.bookokrat_settings.yaml` |

Typical `<data_dir>` paths:

- macOS: `~/Library/Application Support`
- Linux: `~/.local/share`

## Vendored Dependency Notice

`tui-textarea` is vendored in `vendor/tui-textarea` because upstream is currently unmaintained and behind current `ratatui` compatibility. The vendored base comes from PR #118:
https://github.com/rhysd/tui-textarea/pull/118/changes

## Attribution

Bookokrat is based on [bookrat](https://github.com/dmitrysobolev/bookrat) by Dmitry Sobolev (MIT).

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later).
See [`LICENSE`](LICENSE).
