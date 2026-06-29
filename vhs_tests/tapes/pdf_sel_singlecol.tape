# pdf_sel_singlecol.tape
about Single-column PDF drag selection in single-page and dual-page mode: a multi-line drag highlights contiguous text; in dual mode a drag in the right page selects the right page, not the left.

pdf tests/testdata/vhs_test.pdf

# Go to a text page (17) and maximize it (zen)
wait 1000
key space
wait 50
key g
wait 100
key 1
key 7
return
wait 400
key space
key z
wait 400
desc Page 17, single-page mode, before selection. Expect: clean page.
screenshot single_initial

# Single-page: multi-line drag over the bottom "Testing Methodology" paragraph
dragpx 730 1410 1520 1620
wait 400
desc Single-page: multi-line drag over the bottom paragraph. Expect: the 3 "Testing Methodology" body lines highlighted.
screenshot single_multiline

# Dual-page mode: page 17 (left) + page 18 (right)
key space
key D
wait 700
desc Dual-page mode. Expect: page 17 left, page 18 ("Navigation Testing") right.
screenshot single_dual_initial

# Drag in the RIGHT page (page 18) list region
dragpx 2410 850 2900 1370
wait 400
desc Dual-page: drag in the RIGHT page (page 18). Expect: page-18 text highlighted (the navigation list), nothing on page 17.
screenshot single_dual_right

key q
