# pdf_links.tape
about Link-click on a PDF (fit-width): clicking an internal link jumps to its target page; a small inline citation link tests sub-cell hit precision (cell precision on WezTerm). External (URI) links are not exercised (they would launch a real browser). Runs on kitty and wezterm; click coordinates are per-terminal (@kitty/@wezterm) because window geometry differs.

pdf tests/testdata/vhs_links.pdf

# Render, maximize (zen), fit width, go to top.
wait 1000
key space
key z
wait 300
key Z
wait 400
key g
key g
wait 500
desc Links test page 1, fit-width. Internal link (-> p3), external link, and inline "[1]" citation (-> p4).
screenshot initial

# --- Internal link -> page 3 -------------------------------------------------
@kitty clickpx 770 725
@wezterm clickpx 786 836
wait 500
desc Clicked "Jump to page 3 (internal link)". Expect: now on "LINKS TEST - PAGE 3".
screenshot internal_jumped

# --- Jump list: Ctrl+o back to where the link was clicked --------------------
# (Ctrl+i / jump-forward can't be exercised here: the app does not enable the
#  kitty keyboard protocol, so the terminal delivers Ctrl+i as the Tab byte.)
ctrl o
wait 400
desc Ctrl+o (jump back). Expect: back on "LINKS TEST - PAGE 1" (the link-click was recorded as a jump).
screenshot jump_back

# --- Small inline citation link "[1]" -> page 4 (sub-cell precision) ---------
@kitty clickpx 1236 1289
@wezterm clickpx 1239 1395
wait 500
desc Clicked the small "[1]" citation link (~1 cell wide). Expect: now on "LINKS TEST - PAGE 4".
screenshot citation_jumped

key q
