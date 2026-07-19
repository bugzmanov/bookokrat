# pdf_marks_highlights.tape
about Marks, colored highlights and annotation-nav on the two-column PDF (Kitty, zen): m<ch> set / '<ch> jump with cross-page jumps and the 1.5s flash on the anchor line; visual -> H palette -> yellow/red highlights; highlight survives zoom and render-mode switch; dd in normal mode deletes the highlight under the cursor; Shift+Tab nav cycles TEXT comments (j/k, d deletes); remaining highlight deleted via the Space+a overview (dd). Tape cleans up all annotations it creates.

pdf tests/testdata/vhs_twocol.pdf

# Zen (enables annotations), fit width, top; let the zoom HUD expire.
wait 1000
key space
key z
wait 300
key Z
wait 400
key g
key g
wait 500
wait 500
wait 500
wait 500

# --- Marks -------------------------------------------------------------------
key m
key a
wait 300
key 3
key g
key g
wait 500
key m
key b
wait 300

key '
key a
wait 400
desc 'a after jumping away to page 3. Expect: back at page 1 top ([1/6]) with the 1.5s flash highlight on the anchor line.
screenshot mark_a_flash

key '
key b
wait 400
desc 'b cross-page mark. Expect: page 3 ([3/6]) with the flash on its anchor line.
screenshot mark_b_cross

# --- Highlights (zen => annotations enabled) ---------------------------------
key n
wait 300
key g
key g
key j
wait 300
key v
key e
key e
wait 300
key H
wait 300
desc H over a visual selection (first words of P3 L01). Expect: highlight color palette modal (r/g/b/y/p).
screenshot hl_palette

key y
wait 400
desc Picked Yellow (y). Expect: the selected words on P3 L01 highlighted yellow; visual selection cleared.
screenshot hl_yellow

# Zoom survival ('=' is a standard-mode binding, exit normal first).
escape
wait 200
key =
key =
wait 500
desc Zoomed in x2 (standard mode = =) with a highlight present. Expect: yellow highlight still glued to the same words after the re-render.
screenshot hl_zoom_survives

key z
wait 400
# Render-mode switch survival; let the "Render mode" HUD expire before the shot.
key space
key S
wait 500
wait 500
wait 500
wait 500
desc Switched to continuous-scroll render mode (Space+S). Expect: yellow highlight still visible in the scroll layout; no HUD.
screenshot hl_scroll_mode
key space
key S
wait 600

# --- Second highlight in another color ---------------------------------------
key n
wait 200
key g
key g
key j
key j
key j
wait 200
key v
key e
key e
wait 200
key H
wait 300
key r
wait 400
desc Red highlight (r) on P3 L03. Expect: yellow (L01) and red (L03) highlights visible together.
screenshot hl_two_colors

# --- dd deletes the highlight under the cursor (normal mode, no overview) ----
# Robustness: leave the page entirely (jump to page 1 and back), re-enter
# normal mode fresh, and land the cursor MID-highlight (not at its creation
# anchor) before pressing dd.
escape
wait 200
key 1
key g
key g
wait 500
key 3
key g
key g
wait 500
key n
wait 300
key g
key g
key j
key j
key j
wait 200
key w
key l
key l
wait 200
key d
key d
wait 400
desc Jumped to page 1 and back, re-entered normal mode, cursor placed mid-highlight (w l l -> inside "L03") then dd. Expect: red highlight (still present after the round-trip) deleted in place; yellow (L01) remains.
screenshot hl_dd_deleted

# --- Text comments feed Shift+Tab annotation nav -----------------------------
# (nav cycles TEXT comments only; pure highlights are managed via Space+a)
key j
key j
wait 200
key V
wait 200
key a
wait 300
key n
key a
key v
wait 100
escape
wait 400
key j
key j
key j
wait 200
key V
wait 200
key a
wait 300
key t
key w
key o
wait 100
escape
wait 400
desc Two text comments created (V select, a, type, Esc saves) on L05 and L08. Expect: comment underlines on both lines; highlights untouched.
screenshot comments_created

shift_tab
wait 300
desc Shift+Tab enters annotation nav. Expect: first comment's lines emphasized.
screenshot nav_focus_first

key j
wait 300
desc j cycles to the second comment. Expect: emphasis moved to the L08 comment.
screenshot nav_focus_second

key d
wait 400
key d
wait 400
desc d twice deletes both comments from nav (cleanup). Expect: comment underlines gone; the yellow highlight remains.
screenshot nav_deleted

# --- Highlight cleanup via the annotations overview (Space+a) ----------------
escape
wait 200
key space
key a
wait 400
desc Space+a annotations overview. Expect: only the remaining yellow highlight listed with quoted text.
screenshot overview_highlights

key d
key d
wait 400
escape
wait 400
desc Deleted the yellow highlight in the overview (dd), closed it. Expect: page clean - no highlights, no underlines (tape cleanup).
screenshot page_clean

key q
