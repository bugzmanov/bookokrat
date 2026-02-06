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

# ── Help popup ─────────────────────────────────────────────
key ?
wait 10000

# Close help
escape
wait 500

# ── Search for book and open ───────────────────────────────
key /
wait 500
type AI Eng
wait 500

# Confirm search, then open the book
return
wait 200
return
wait 1200

# ── Navigate to Chapter 3 via TOC ─────────────────────────
# Tab to TOC panel
tab
wait 200

# Navigate down to Ch3 (← Books List → Preface → Ch1 → Ch2 → Ch3)
repeat_key j 4 0.15
wait 200

# Expand Chapter 3 subtree
key l
wait 300

# Open Chapter 3
return
wait 800

# Quick scroll with j (40x, 60ms between each in single osascript)
repeat_key j 40 0.06
wait 300

# ── Scroll down: 19x ctrl+d (single osascript, 100ms between each) ──
repeat_ctrl d 19 0.2
wait 800

# ── Change theme to Catppuccin Mocha ──────────────────────
key space
wait 150
key t
wait 400

# Catppuccin Mocha is 1 down from Oceanic Next
key j
wait 200
return
wait 200
escape
wait 600

# ── Zen mode + margins ────────────────────────────────────
ctrl z
wait 500

# Increase margins: 10x minus (50ms between each)
repeat_key - 10 0.05
wait 100

# Scroll a bit in zen mode (3x ctrl+d, single osascript)
repeat_ctrl d 3 0.2
wait 800

# Reduce margins back: 10x = (50ms between each)
repeat_key = 10 0.05
wait 100

# Exit zen mode
ctrl z
wait 600

# ── Search for "LLM" in reader ────────────────────────────
key /
wait 200
type LLM
wait 400
return
wait 400

# Jump through matches
key n
wait 400
key n
wait 400
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
type THIS IS A COMMENT
wait 800

# Save comment (Esc saves textarea)
escape
wait 2000

# Quit
key q
