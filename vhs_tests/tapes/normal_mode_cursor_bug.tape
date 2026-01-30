# normal_mode_cursor_bug.tape
# Test case for normal mode, visual selection, and yank in PDF viewer
#
# Tests:
# 1. Normal mode cursor visibility after page jump
# 2. Visual mode line selection
# 3. Yank functionality

pdf tests/testdata/vhs_test.pdf

# Wait for initial load (1000ms needed for app startup + PDF rendering)
wait 1000

# Go to page 17 using Space+g<page>Enter
key space
wait 50
key g
wait 100
key 1
key 7
return
wait 300
screenshot page_17_before_normal

# Enter normal mode
key n
wait 300

# Move cursor down to verify normal mode is active
key j
wait 300
screenshot page_17_normal_mode_cursor

# Enter visual line mode
key V
wait 300

# Select two more lines down
key j
wait 200
key j
wait 300
screenshot page_17_visual_selection

# Yank the selection
key y
wait 300
screenshot page_17_after_yank

# Exit normal mode back to view mode
escape
wait 300
screenshot page_17_view_mode

# Quit
key q
