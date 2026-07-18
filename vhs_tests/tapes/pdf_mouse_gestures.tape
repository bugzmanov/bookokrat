# pdf_mouse_gestures.tape
about Mouse gestures on a two-column PDF (fit-width): drag selection + column/page routing (guards a7ac8dd), double-click word, triple-click line, normal-mode click-to-move-cursor, and scroll-wheel.

pdf tests/testdata/vhs_twocol.pdf

# Render, maximize (zen), fit width, go to top
wait 1000
key space
key z
wait 300
key Z
wait 400
key g
key g
wait 500
desc Two-column page 1, single-page, fit-width, before any gesture. Left "P1 L##", right "P1 R##".
screenshot initial

# --- Drag selection + column routing -----------------------------------------
# Left column rows 03-06 (x 344-1060)
dragpx 400 720 1020 1190
wait 400
desc Drag inside the LEFT column. Expect: only P1 L03..L06 highlighted, no right-column lines.
screenshot drag_left_col

# Right column rows 03-06 (x 1648-2412) — a7ac8dd guard
dragpx 1700 720 2380 1190
wait 400
desc Drag inside the RIGHT column. Expect: only P1 R03..R06 highlighted, NOT the left column.
screenshot drag_right_col

# --- Double / triple click ---------------------------------------------------
# Double-click the word "column" in line "P1 L04 left column line 04" (x 753, y 884)
doubleclickpx 753 884
wait 400
desc Double-click the word "column" on line L04. Expect: just the word "column" selected.
screenshot double_click_word

# Triple-click the same line -> whole line
tripleclickpx 753 884
wait 400
desc Triple-click line L04. Expect: the entire line "P1 L04 left column line 04" selected.
screenshot triple_click_line

# --- Normal-mode click moves the cursor --------------------------------------
escape
wait 200
key n
wait 300
desc Entered normal mode. Expect: a block cursor visible at its default position.
screenshot normal_initial

# Click on the word "line" in row L06 (x 925, y 1181)
clickpx 925 1181
wait 300
desc Clicked at "line" on row L06 while in normal mode. Expect: the block cursor moved to that click position.
screenshot normal_click_moved_cursor
escape
wait 200

# --- Dual-page routing -------------------------------------------------------
key space
key D
wait 600
key Z
wait 400
key g
key g
wait 500
desc Dual-page, fit-width: page 1 (left) + page 2 (right), all four columns visible.
screenshot dual_initial

# Page 1 right column (x 828-1212, rows y 383-605)
dragpx 870 365 1200 615
wait 400
desc Dual: drag in RIGHT column of LEFT page. Expect: only P1 R03..R06 highlighted.
screenshot dual_p1_right

# Page 2 left column (x 1688-2048)
dragpx 1720 365 2040 615
wait 400
desc Dual: drag in LEFT column of RIGHT page. Expect: only P2 L03..L06 highlighted (routed to page 2).
screenshot dual_p2_left

# Page 2 right column (x 2340-2724) — far/rightmost column
dragpx 2380 365 2700 615
wait 400
desc Dual: drag in RIGHT column of RIGHT page. Expect: only P2 R03..R06 highlighted (correct page AND column).
screenshot dual_p2_right

# --- Scroll wheel ------------------------------------------------------------
# Clear selection, toggle dual off (Space+D), back to top of page 1.
escape
wait 200
key space
key D
wait 400
key g
key g
wait 500
desc Single page again, back at top of page 1 (baseline for scroll).
screenshot scroll_baseline

# Wheel down over the page.
scrollpx down 1400 900 5
wait 500
desc Scroll wheel DOWN 5 notches over the page. Expect: content scrolled down from the top.
screenshot scroll_down

# Wheel back up.
scrollpx up 1400 900 5
wait 500
desc Scroll wheel UP 5 notches. Expect: back near the top of page 1.
screenshot scroll_up

key q
