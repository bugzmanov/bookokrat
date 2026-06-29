# mouse_drag.tape
about Mouse drag-selection in the PDF reader: a left-button drag across body text should highlight it.

pdf tests/testdata/vhs_test.pdf

# Wait for app + PDF render
wait 1000

# Go to a text-heavy page (17)
key space
wait 50
key g
wait 100
key 1
key 7
return
wait 400

# Maximize the PDF area (zen) so text fills the width
key space
key z
wait 400
desc Page 17 in zen mode, before any selection. Expect: clean page, no highlight.
screenshot before_drag

# Drag-select across the bottom paragraph (pixel coords, sub-cell precision)
dragpx 730 1410 1520 1620
wait 500
desc After dragging across the bottom paragraph. Expect: the three "Testing Methodology" lines highlighted in orange.
screenshot after_drag

key q
