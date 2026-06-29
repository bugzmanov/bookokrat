# pdf_sel_twocol.tape
about Two-column PDF selection routing (guards a7ac8dd/fcc1d3d): a drag in a given column/page must highlight ONLY that column's text, in both single-page and dual-page mode.

pdf tests/testdata/vhs_twocol.pdf

# Wait for app + PDF render, then maximize the page (zen)
wait 1000
key space
key z
wait 400
desc Two-column page 1, single-page mode, before any selection. Expect: clean page; left lines "P1 L##", right lines "P1 R##".
screenshot twocol_initial

# --- Single-page mode ---------------------------------------------------------
# Left column, rows 03-06 (band x 692-1192, rows y 515-827)
dragpx 720 500 1150 840
wait 400
desc Single-page: dragged inside the LEFT column. Expect: only P1 L03..L06 highlighted, NO right-column (R) lines.
screenshot twocol_single_left

# Right column, rows 03-06 (band x 1608-2140) — the a7ac8dd bug grabbed the LEFT column here
dragpx 1640 500 2110 840
wait 400
desc Single-page: dragged inside the RIGHT column. Expect: only P1 R03..R06 highlighted, NOT the left column (this is the a7ac8dd regression).
screenshot twocol_single_right

# --- Dual-page mode ----------------------------------------------------------
key space
key D
wait 600
desc Dual-page mode: page 1 (left) and page 2 (right), each with two columns.
screenshot twocol_dual_initial

# Page 1's RIGHT column, rows 03-06 (band x 1156-1692)
dragpx 1190 500 1660 840
wait 400
desc Dual-page: dragged in the RIGHT column of the LEFT page. Expect: only P1 R03..R06 highlighted (not P1 L, not page 2).
screenshot twocol_dual_p1right

# Page 2's LEFT column, rows 03-06 (band x 2368-2864)
dragpx 2400 500 2840 840
wait 400
desc Dual-page: dragged in the LEFT column of the RIGHT page. Expect: only P2 L03..L06 highlighted (selection routed to page 2, not page 1).
screenshot twocol_dual_p2left

key q
