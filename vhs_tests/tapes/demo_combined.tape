# demo_combined.tape
# Combined PDF + EPUB demo (~60s), sharp/snappy pacing

nofile
window 70

# App loads showing book list
wait 1500

# ── PDF flow (first) ───────────────────────────────────────
# Search for PDF book and open
key /
wait 500
type Build a Large
wait 500
return
wait 200
return
wait 2000

# Invert colors
key i
wait 500

# Show help briefly (2s)
key ?
wait 2000
escape
wait 300

# Enable scroll mode via settings (same path as demo_pdf)
ctrl s
wait 400
repeat_key j 3 0.15
wait 200
return
wait 200
escape
wait 600

# Quick scroll
repeat_key j 30 0.06
wait 800

# Jump to page 30 (same as demo_pdf)
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

# Theme selection: Rosepine Moon (same as demo_pdf)
key space
wait 150
key t
wait 400
repeat_key j 4 0.15
wait 200
return
wait 200
escape
wait 1000

# Zen mode + zoom + scroll
ctrl z
wait 500
key -
wait 300
key -
wait 800
repeat_ctrl d 10 0.2
wait 800

# Comment while still in zen mode
key n
wait 300
key v
wait 200
repeat_key e 8 0.05
wait 300
key a
wait 400
type Demo: this is a comment
wait 800
escape
wait 500
escape
wait 400

# Global search
key /
wait 500
type Fine tunning for classification
wait 500
return
wait 800
repeat_key j 7 0.15
wait 200
return
wait 1000

# Exit zen mode
ctrl z
wait 500

# Return to book list (via TOC back item)
tab
wait 200
repeat_key k 20 0.03
wait 100
return
wait 1000

# ── EPUB flow (second) ─────────────────────────────────────
# Search for EPUB and open
key /
wait 300
# Clear previous query before typing new book search
repeat_ctrl h 30 0.03
wait 150
type AI Eng
wait 400
return
wait 200
return
wait 1400

# Navigate TOC to Chapter 3, expand it, and open a subchapter
tab
wait 200
repeat_key j 4 0.15
wait 200
key l
wait 300
repeat_key j 2 0.15
wait 200
key l
wait 250
key j
wait 200
return
wait 800

# Read/scroll with j and ctrl+d
repeat_key j 25 0.06
wait 300
repeat_ctrl d 8 0.2
wait 500

# Zen mode + margins + comment showcase
ctrl z
wait 500
repeat_key - 6 0.05
wait 200
key n
wait 300
key v
wait 200
repeat_key e 6 0.05
wait 200
key a
wait 300
type QUICK NOTE
wait 600
escape
wait 500
escape
wait 300
ctrl z
wait 500

# Quit
wait 1000
key q
