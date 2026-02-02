# pdf_comments.tape
# Test case for PDF comments/annotations functionality
#
# Tests:
# 1. Enter zen mode (required for comments in Kitty)
# 2. Visual selection on text
# 3. Add two comments
# 4. Switch to ToC mode to verify underline preserves
# 5. Test comments overview window with navigation
# 6. Test scaling doesn't break underline
# 7. Delete comments

pdf tests/testdata/vhs_test.pdf

# Wait for initial load
wait 500

# Go to page 17 (text content page)
key space
wait 50
key g
wait 100
key 1
key 7
return
wait 200

# Enter zen mode (required for comments in Kitty)
ctrl z
wait 200
screenshot zen_before_comment

# Enter normal mode
key n
wait 200

# Move cursor down a bit
key j
key j
key j
wait 200

# Enter visual line mode
key V
wait 200

# Select a few lines
key j
key j
wait 200
screenshot visual_selection

# Add comment with 'a'
key a
wait 200
screenshot comment_textarea

# Type a test comment (lowercase)
key t
key e
key s
key t
wait 100

# Save comment with Escape (as shown in UI: "Esc to save")
escape
wait 200
screenshot comment_1_saved

# Add a second comment on the same page
# Move down to select different text
key j
key j
key j
wait 200

# Enter visual line mode for second selection
key V
wait 200

# Select one more line
key j
wait 200

# Add second comment
key a
wait 300

# Type second comment
key t
key w
key o
wait 100

# Save second comment
escape
wait 500
screenshot comment_2_saved

# Test 1: Switch to ToC mode to verify underline preserves
ctrl z
wait 300
screenshot toc_mode_underline_preserved

# Back to zen mode
ctrl z
wait 300

# Test 2: Comments overview - first go to page 1
key space
wait 50
key g
wait 100
key 1
return
wait 300
screenshot page_1_before_overview

# Open comments overview with Space+a
key space
wait 50
key a
wait 500
screenshot comments_overview

# Navigate in comments overview and jump back to comment
return
wait 300
screenshot jumped_to_comment_page

# Test 3: Simplified scaling test in zen mode
# Scale down with '-' twice
key -
wait 300
key -
wait 300
screenshot zoom_out_underline

# Scale back up with '=' twice
key =
wait 300
key =
wait 300
screenshot zoom_restored

# Test deleting comments with Shift+Tab and 'd'
shift_tab
wait 300
screenshot switch_comment

# Delete the selected comment
key d
wait 300
screenshot after_delete_1

# Delete the remaining comment
key d
wait 300
screenshot after_delete_all

# Quit
key q
