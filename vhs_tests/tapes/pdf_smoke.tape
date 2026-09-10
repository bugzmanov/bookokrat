# pdf_smoke.tape
about PDF rendering smoke test: initial render, page nav, TOC jump to bitmap page, zen mode, help popup.

pdf tests/testdata/vhs_test.pdf

# Capture initial render (title page)
# Initial load needs more time for app startup + PDF rendering
wait 1000
desc App just opened the PDF. Expect: title page rendered with sidebar visible.
screenshot initial

# Navigate forward 2 pages (page 3 is still Introduction content)
key l
wait 200
key l
wait 200
desc Pressed l twice (next page x2). Expect: page 3, Introduction content.
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
desc Jumped via TOC to the Bitmap Images chapter. Expect: page with a bitmap image rendered.
screenshot bitmap_via_toc

# Toggle zen mode to see graphics without sidebar
key space
key z
wait 200
desc Toggled zen mode. Expect: same page, sidebar hidden, image fills more width.
screenshot zen_mode

# Open help popup in zen
key ?
wait 100
desc Opened help popup while in zen. Expect: ANSI-art help overlay on top of the page.
screenshot zen_help_popup
escape
wait 50

# Exit zen mode
key space
key z
wait 200

# Open help popup with sidebar visible
key ?
wait 100
desc Opened help popup with sidebar visible. Expect: help overlay with nav panel behind it.
screenshot help_popup

# Close help and quit
escape
wait 200
key q
