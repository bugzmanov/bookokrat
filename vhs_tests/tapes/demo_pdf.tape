# demo_pdf.tape
# PDF demo for website video/GIF recording
# Showcases: book list, search, invert colors, page jump, scroll mode, themes, zen mode, zoom, comments

nofile
window 70

# App loads showing book list
wait 1500

# ── Search for PDF book and open ─────────────────────────
key /
wait 500
type Build a Large
wait 500

# Confirm search, then open the book
return
wait 200
return
wait 2000

# ── Invert colors ────────────────────────────────────────
key i
wait 500

# ── Jump to PDF page 1 ──────────────────────────────────
key space
wait 150
key g
wait 300
# Switch to PDF page mode (default is content pages)
key p
wait 200
type 1
wait 200
return
wait 10000

# ── Enable scroll mode via settings ──────────────────────
ctrl s
wait 400
# Navigate to Scroll option (index 3, 3 down from default)
repeat_key j 3 0.15
wait 200
return
wait 200
escape
wait 600

# ── Quick scroll with j (30x) ───────────────────────────
repeat_key j 30 0.06
wait 1000

# ── Jump to page 30 ─────────────────────────────────────
key space
wait 150
key g
wait 300
key p
wait 200
type 30
wait 200
return
wait 1000

# ── Change theme to Rosepine Moon ─ ────────────────────────
key space
wait 150
key t
wait 400

# Rosepine Moon is 4 down from Oceanic Next
repeat_key j 4 0.15
wait 200
return
wait 200
escape
wait 1000

# ── Zen mode ─────────────────────────────────────────────
ctrl z
wait 500

# ── Zoom out 2 times ─────────────────────────────────────
key -
wait 300
key -
wait 800

# ── Scroll down: 10x ctrl+d ─────────────────────────────
repeat_ctrl d 10 0.2
wait 800

# ── Normal mode: select & comment ────────────────────────
# Enter normal mode
key n
wait 300

# Character-wise visual selection
key v
wait 200

# Select several words
repeat_key e 8 0.05
wait 300

# Add comment
key a
wait 400
type this is comment
wait 800

# Save comment (Esc saves textarea)
escape
wait 500
# Exit normal mode
escape
wait 2000

# ── Global search ────────────────────────────────────────
key /
wait 500
type Fine tunning for classification
wait 500
return
wait 800

# Navigate to result (7j + enter)
repeat_key j 7 0.15
wait 200
return
wait 1000

# ── Exit zen mode ────────────────────────────────────────
ctrl z
wait 500

# ── Scroll down with j (20x) ────────────────────────────
repeat_key j 20 0.06
wait 5000

# Quit
key q
