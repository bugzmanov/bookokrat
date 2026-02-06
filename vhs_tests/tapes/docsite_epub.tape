# docsite_epub.tape
# All EPUB screenshots for the docs slideshow (4 screenshots, 1 session)

file AI Engineering Building Applications.epub

# Let the book render
wait 1200

# ── Screenshot 1: Help popup ──────────────────────────────
key ?
wait 300
screenshot help
escape
wait 300

# ── Screenshot 2: Chapter 2 with expanded TOC ─────────────
# Tab to reader (help popup returns focus to nav panel)
tab
wait 200

# Navigate to Chapter 2: 5x next chapter
key l
wait 300
key l
wait 300
key l
wait 300
key l
wait 300
key l
wait 500

# Focus TOC to expand Ch2 subtree
tab
wait 300

# Navigate to Ch2 in TOC (starts at "← Books List", index 0)
# Index 1: Preface, Index 2: Ch1, Index 3: Ch2
key j
key j
key j

# Expand Ch2 and all its subsections
key l
key j
key l
key j
key j
key j
key l
key j
key l
key j
key j
key j
key j
key l
key j
key j
key l
key j
key j
key j
key l
wait 200

# Back to reader
tab
wait 300

# Scroll to target position: 22x Ctrl+d
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 500

# Fine-tune: 10x j
key j
key j
key j
key j
key j
key j
key j
key j
key j
key j
wait 200

screenshot epub_ch2

# ── Screenshot 3: Theme selector on Ch3 ──────────────────
# Next chapter (Ch2 → Ch3)
key l
wait 500

# Scroll: 13x Ctrl+d
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 500

# Fine-tune: 8x j
key j
key j
key j
key j
key j
key j
key j
key j
wait 200

# Open theme selector
key space
wait 200
key t
wait 500

# Move to Catppuccin Mocha (1 down from Oceanic Next)
key j
wait 300

screenshot theme_selection

# ── Screenshot 4: Zen mode with Catppuccin Mocha ─────────
# Apply Catppuccin Mocha and close settings
return
wait 300
escape
wait 500

# Go back to Ch2
key h
wait 500

# Scroll: 13x Ctrl+d + 5x j
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 120
ctrl d
wait 500
key j
key j
key j
key j
key j
wait 200

# Enter zen mode
ctrl z
wait 300

# Increase margins to max: 20x minus
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
wait 300

screenshot zen_mode_epub

# Quit
key q
