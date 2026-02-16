# Bookokrat

Terminal EPUB/PDF reader focused on speed, smooth navigation, and Vim-style workflows.

https://github.com/user-attachments/assets/0ebe61c6-4629-4bde-8bd4-50feb9a424a3

## Highlights

- EPUB and PDF support in one TUI app
- Split layout: library/TOC on the left, reader on the right
- Fast PDF pipeline with Kitty SHM image transfer in supported terminals
- Search, bookmarks, jump list history, reading stats
- Inline comments/annotations with persistent storage and Markdown export
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

# For PDF support:
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
Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) with the \"Desktop development with C++\" workload.

</details>

```bash
cargo install bookokrat
```

Build without PDF support:

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
bookokrat path/to/book.epub --zen-mode
```

Press `?` inside the app to open the built-in help.

## Documentation

- Full usage and keyboard reference: [`readme.txt`](readme.txt)

## Terminal Notes

PDF viewing requires a graphics-capable terminal.

- Best experience: Kitty, Ghostty
- Good: iTerm2, WezTerm, Warp, Konsole (with some limitations)
- Terminals without graphics protocol support - EPUB support without images. PDFs are not supported

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
