#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

"$SCRIPT_DIR/run.sh" --terminal ghostty --tape docsite_epub --update
"$SCRIPT_DIR/run.sh" --terminal ghostty --tape docsite_pdf --update

EPUB_DIR="$SCRIPT_DIR/output/screenshots/ghostty/docsite_epub"
PDF_DIR="$SCRIPT_DIR/output/screenshots/ghostty/docsite_pdf"

for pair in \
    "$EPUB_DIR/help.png|$PROJECT_ROOT/docs/help.png" \
    "$EPUB_DIR/epub_ch2.png|$PROJECT_ROOT/docs/epub.png" \
    "$EPUB_DIR/theme_selection.png|$PROJECT_ROOT/docs/theme_selection.png" \
    "$EPUB_DIR/zen_mode_epub.png|$PROJECT_ROOT/docs/zen_mode_epub.png" \
    "$PDF_DIR/pdf_view.png|$PROJECT_ROOT/docs/pdf_view.png"; do
    src="${pair%%|*}"
    dest="${pair##*|}"
    if [ ! -f "$src" ]; then
        echo "Expected screenshot not found: $src" >&2
        exit 1
    fi
    cp "$src" "$dest"
    # Keep docsite images lightweight but crisp.
    # Target 1600px width, preserve aspect ratio.
    sips -Z 1600 "$dest" >/dev/null
    printf "Updated %s\n" "$dest"
done
