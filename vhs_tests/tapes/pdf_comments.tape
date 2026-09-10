# pdf_comments.tape
about PDF comments lifecycle: create two comments in zen mode, verify underline survives mode-switch and zoom, browse the overview, then delete both.

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
key space
key z
wait 200
desc Page 17 in zen mode (required for PDF comments on Kitty). Expect: clean page, no sidebar.
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
desc Visual-line selection of a few lines. Expect: ~3 lines highlighted.
screenshot visual_selection

# Add comment with 'a'
key a
wait 200
desc Pressed a to add a comment. Expect: comment text-area editor open over the page.
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
desc Typed "test" and saved with Esc. Expect: first comment saved, selected text underlined.
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
desc Added and saved a second comment ("two"). Expect: two underlined regions on the page.
screenshot comment_2_saved

# Test 1: Switch to ToC mode to verify underline preserves
key space
key z
wait 300
desc Toggled out of zen (TOC/sidebar mode). Expect: underlines still aligned on the text.
screenshot toc_mode_underline_preserved

# Back to zen mode
key space
key z
wait 300

# Test 2: Comments overview - first go to page 1
key space
wait 50
key g
wait 100
key 1
return
wait 300
desc Jumped to page 1 before opening the overview. Expect: title page, no comments here.
screenshot page_1_before_overview

# Open comments overview with Space+a
key space
wait 50
key a
wait 500
desc Opened comments overview (Space+a). Expect: list popup showing both comments.
screenshot comments_overview

# Navigate in comments overview and jump back to comment
return
wait 300
desc Pressed Enter on a comment. Expect: jumped back to page 17 at that comment.
screenshot jumped_to_comment_page

# Test 3: Simplified scaling test in zen mode
# Scale down with '-' twice
key -
wait 300
key -
wait 300
desc Zoomed out twice. Expect: page smaller, underlines still aligned to their text.
screenshot zoom_out_underline

# Scale back up with '=' twice
key =
wait 300
key =
wait 300
desc Zoomed back in twice. Expect: original size, underlines still correct.
screenshot zoom_restored

# Test deleting comments with Shift+Tab and 'd'
shift_tab
wait 300
desc Entered comment-nav (Shift+Tab). Expect: one comment highlighted as selected.
screenshot switch_comment

# Delete the selected comment
key d
wait 300
desc Deleted the selected comment (d). Expect: one underline remains.
screenshot after_delete_1

# Delete the remaining comment
key d
wait 300
desc Deleted the remaining comment (d). Expect: no underlines left on the page.
screenshot after_delete_all

# Quit
key q
