# pdf_mouse_gestures.tape
about Mouse gestures + normal-mode motions on a two-column PDF (fit-width): drag selection + column/page routing (guards a7ac8dd), double-click word, triple-click line, normal-mode click-to-move-cursor, scroll-wheel, and a vim motion chain (w/e/f/;/$/0, cross-line wrap, v+iw, yank, V-line, Space+c page extract). Guards the pending-motion-vs-keymap dispatch order (f target char must not dispatch as a motion).

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

# --- Normal-mode motion chain ------------------------------------------------
# Every shot is the endpoint of a motion subchain; a wrong intermediate motion
# lands the cursor on a visibly wrong labeled word/line.
key n
wait 300
key g
key g
key j
wait 300
key w
key w
key e
wait 300
desc Normal mode, gg then j (skip title line) to L01 start, then w w e. Expect: block cursor on the LAST char of "left" ("P1 L01 left ...").
screenshot nm_words

key 0
key f
key l
key ;
key ;
wait 300
desc 0, then f l ; ; (find 'l', repeat twice). Expect: cursor on the 'l' of "line" (3rd lowercase 'l' of L01).
screenshot nm_find

key $
key l
wait 300
desc $ (line end) then l. Expect: cursor WRAPPED to the first char of the next line (L02).
screenshot nm_wrap

# NOTE: {/} paragraph motions are intentionally unimplemented in PDF normal
# mode (navigation.rs dispatch no-op) — not covered here.

# Text object: visual + inner word
key v
key i
key w
wait 300
desc v then i w (inner word). Expect: exactly the word under the cursor highlighted.
screenshot nm_visual_iw

key e
key e
wait 300
desc Extend visual with e e. Expect: selection grown to the end of the second word to the right.
screenshot nm_visual_extend

key y
wait 400
desc y yanks the visual selection. Expect: highlight cleared, cursor at selection start, still in normal mode. (Copy toast is NOT visible over the kitty image - known issue.)
screenshot nm_yank

key V
key j
wait 300
desc V then j (visual line). Expect: two ENTIRE lines highlighted edge to edge.
screenshot nm_visual_line

# Leave visual, leave normal mode, extract page text via Space+c.
escape
wait 200
escape
wait 200
key space
key c
wait 400
desc Space+c extracts the page text. Expect: normal mode exited (no NORMAL badge, no block cursor); copy happens silently (toast not visible over kitty image - known issue).
screenshot page_text_copied

# --- Quick jump + go-to-page prompt, in DUAL mode ----------------------------
key space
key D
wait 600
# Let the 2s "Page layout: Dual" HUD expire so it isn't baked into the golden.
wait 500
wait 500
wait 500
key 3
key g
key g
wait 500
desc Dual mode, typed 3 g g (quick page jump). Expect: spread showing PAGE 3 (left) + PAGE 4 (right), indicator [3/6], no HUD.
screenshot dual_quickjump_p3

key space
key g
wait 300
desc Space+g opens the go-to-page prompt over the dual spread. Expect: page-input prompt, empty input.
screenshot dual_goto_prompt

# Out of range: document has 6 pages (prompt opens in Content mode).
key 9
key 9
return
wait 400
desc Entered 99 (doc has 6 pages). Expect: red error "Cannot map content page 99 ..." in the prompt; prompt stays open.
screenshot dual_goto_error

# Esc cancels (no snapshot), reopen and jump to a valid page.
escape
wait 200
key space
key g
wait 200
key 5
return
wait 500
desc Reopen Space+g, type 5 + Enter. Expect: prompt closed, dual spread showing PAGE 5 + PAGE 6.
screenshot dual_goto_valid

key q
