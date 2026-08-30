# normal_mode_cursor_bug.tape
about PDF normal mode regression: cursor must stay visible after a page jump, then visual-line select and yank.

pdf tests/testdata/vhs_test.pdf

# Wait for initial load. Two waits: app startup + first page render must be
# fully settled before the Space+g prompt, or the jump is silently dropped
# (seen flaking on wezterm with a single 1000ms wait).
wait 1000
wait 500

# Go to page 17 using Space+g<page>Enter
key space
wait 200
key g
wait 300
key 1
key 7
return
wait 300
desc Jumped to page 17 (still in standard mode). Expect: page 17 text, no cursor yet.
screenshot page_17_before_normal

# Enter normal mode
key n
wait 300

# Move cursor down to verify normal mode is active
key j
wait 300
desc Entered normal mode and moved cursor down. Expect: visible cursor on the page (the bug: cursor was invisible after a jump).
screenshot page_17_normal_mode_cursor

# Enter visual line mode
key V
wait 300

# Select two more lines down
key j
wait 200
key j
wait 300
desc Visual-line mode, extended 2 lines down. Expect: three lines highlighted.
screenshot page_17_visual_selection

# Yank the selection
key y
wait 300
desc Yanked the selection. Expect: selection cleared, "copied" feedback, cursor back at selection start.
screenshot page_17_after_yank

# Exit normal mode back to view mode
escape
wait 300
desc Pressed Esc to leave normal mode. Expect: standard view, no cursor/selection.
screenshot page_17_view_mode

# Quit
key q
