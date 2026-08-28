# pdf_pan_resize.tape
about Kitty horizontal pan must survive viewport-width changes. Zoom past the viewport width, enhance, pan right, then change the viewport width two ways: growing the nav panel (>) which re-renders the pages at the narrower width, and shrinking the OS window which re-crops on the next input. The panned position must be preserved proportionally, never snapping back to the left edge.

terminal kitty
pdf tests/testdata/vhs_test.pdf

wait 1000

# Zoom in well past fit-width so the page overflows horizontally.
repeat_key = 8 0.08
wait 500

# Enhance re-renders at the new scale; the long wait lets the render pipeline
# and the zoom HUD fully settle so the goldens are pixel-stable.
key e
wait 4000

# Pan hard to the right (display-only on kitty, no re-render happens).
repeat_key L 30 0.03
wait 400
desc Zoomed (8 steps, enhanced) and panned fully right (L x30), nav panel visible. Expect: RIGHT side of the page ("...S Test PDF"), left edge cut off.
screenshot panned

# Grow the nav panel: the PDF viewport narrows and the pages RE-RENDER at the
# new width.
key >
wait 1000
desc Nav panel grown by 3 cols (>), pages re-rendered for the narrower viewport. Expect: view still panned to the right side, same relative position.
screenshot after_nav_grow

# Shrink the OS window and restore the nav panel width: on kitty the OS resize
# is only picked up on the next input, the nav shrink re-renders again.
resize -20 0
wait 400
key <
wait 1000
desc Window shrunk 20 cols, nav panel back to original width (<). Expect: view still panned to the right side of the page.
screenshot after_os_resize

# Restore the window size for subsequent tapes.
resize 20 0
wait 300

key q
