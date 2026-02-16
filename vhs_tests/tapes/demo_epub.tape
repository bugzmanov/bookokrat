# demo_epub.tape
# EPUB demo for website video/GIF recording
# Showcases: book list, help, search, reading, themes, zen mode, comments

nofile
window 70

# App loads showing book list
wait 1500

# ── Sort by type ───────────────────────────────────────────
key S
wait 800

# ── Search for book (don't open yet) ───────────────────────
key /
wait 500
type AI Eng
wait 500
return
wait 200

# ── Help popup ─────────────────────────────────────────────
key ?
wait 10000

# Close help
escape
wait 500

# Open the book
return
wait 1200

# ── Navigate to "Exact Evaluation" -> "Introduction to Embeddings" ──
# Tab to TOC panel
tab
wait 50

# Navigate down to "Exact Evaluation" (← Books List → Preface → Ch1 → Ch2 → Ch3)
repeat_key j 4 0.15
wait 50

# Expand "Exact Evaluation" subtree
key l
wait 50

# Move within Chapter 3 subtree: 3j, l, 3j, enter
repeat_key j 3 0.15
wait 50
key l
wait 50
repeat_key j 3 0.15
wait 200
return
wait 800

# Quick scroll with j (60x, 60ms between each in single osascript)
repeat_key j 60 0.06
wait 800

# ── Scroll down: 19x ctrl+d (single osascript, 100ms between each) ──
repeat_ctrl d 19 0.2
wait 800

# ── Change theme to Catppuccin Mocha ──────────────────────
key space
wait 80
key t
wait 150

# Catppuccin Mocha is 1 down from Oceanic Next
key j
wait 120
return
wait 100
escape
wait 250

# ── Zen mode + margins ────────────────────────────────────
ctrl z
wait 500

# Increase margins: 10x minus (50ms between each)
repeat_key - 10 0.05
wait 100

# Scroll a bit in zen mode (5x ctrl+d, single osascript)
repeat_ctrl d 5 0.2
wait 800

# Exit zen mode
ctrl z
wait 600

# Reduce margins back: 10x = (50ms between each)
repeat_key = 10 0.05
wait 100

# ── Search for "LLM" in reader ────────────────────────────
key /
wait 200
type LLM
wait 400
return
wait 400

# Jump through matches
key n
wait 800

# Exit search
escape
wait 300

# ── Normal mode: select & comment ─────────────────────────
# Enter normal mode
key n
wait 300

# Character-wise visual selection (stays within one paragraph node)
key v
wait 200

# Select several words (8x word-end motion, 50ms between each)
repeat_key e 8 0.05
wait 300

# Add comment
key a
wait 400
type Demo: this is a comment!
wait 800

# Save comment (Esc saves textarea)
escape
wait 2000

# Quit
key q
