# pdf_dual_wezterm.tape
about REAL non-kitty PDF path (WezTerm auto-selects the iTerm2 protocol, no override): dual layout, spread switch (l), dual+zoom, dual pan and dual scroll. The only pixel coverage of the non-kitty state machine (build_non_kitty_dual_layout slicing, non_kitty zoom/pan/scroll) on a protocol real users run. Run with --terminal wezterm.

terminal wezterm
pdf tests/testdata/vhs_many.pdf

wait 1000
key space
key z
wait 400
key 1
key g
key g
wait 500
wait 500
wait 500
wait 500
desc WezTerm/iTerm2 protocol, zen, page 1. Expect: PAGE 01 / 80 rendered; [1/80]; no HUD.
screenshot wz_baseline

key space
key D
wait 500
wait 500
wait 500
wait 500
desc Dual layout (Space+D). Expect: PAGE 01 + PAGE 02 side by side; no HUD.
screenshot wz_dual

key l
wait 500
wait 500
wait 500
desc Next spread (l). Expect: PAGE 03 + PAGE 04 fully rendered - NOT the LOADING placeholder (non-kitty dual live-lock repro).
screenshot wz_dual_next

key =
key =
wait 500
wait 500
wait 500
wait 500
desc Dual + zoomed x2 (non-kitty dual slicing at zoom). Expect: both pages larger, clean side-by-side slices; no HUD.
screenshot wz_dual_zoom

key L
key L
key L
key L
key L
key L
key L
key L
key L
key L
wait 500
desc Dual + zoom + panned right (L x10). Expect: the visible slice shifted right (R-edge markers closer, left edge cropped further).
screenshot wz_dual_pan

ctrl d
wait 500
desc Dual + zoom + scrolled half a screen (Ctrl+d). Expect: content advanced vertically, same pan.
screenshot wz_dual_scroll

key q
