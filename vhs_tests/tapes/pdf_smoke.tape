# pdf_smoke.tape
# PDF rendering smoke test
#
# Tests: initial render, page navigation, vector graphics, bitmap images, zen mode, help popup
# Uses the 20-page test PDF with TOC, 7 chapters, vector and bitmap content

pdf tests/testdata/vhs_test.pdf

# Capture initial render (title page)
# Initial load needs more time for app startup + PDF rendering
wait 1000
screenshot initial

# Navigate forward 2 pages (page 3 is still Introduction content)
key l
wait 200
key l
wait 200
screenshot page_3

# Navigate via Table of Contents to Bitmap Images
# Tab switches focus to TOC (starts at top), 5x j to reach Chapter 4, Enter selects
tab
wait 100
key j
key j
key j
key j
key j
wait 100
return
wait 500
screenshot bitmap_via_toc

# Toggle zen mode to see graphics without sidebar
ctrl z
wait 200
screenshot zen_mode

# Open help popup in zen
key ?
wait 100
screenshot zen_help_popup
escape
wait 50

# Exit zen mode
ctrl z
wait 200

# Open help popup with sidebar visible
key ?
wait 100
screenshot help_popup

# Close help and quit
escape
wait 200
key q
