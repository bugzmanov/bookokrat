# pdf_reload.tape
about File-watch auto-reload (Kitty): Space+w enables watching; overwriting the opened PDF (VERSION A, 3pp -> VERSION B, 2pp) auto-reloads and re-clamps the current page; a content update on a zoomed+panned+scrolled page 2 (A -> C, same page count) must PRESERVE page, zoom, pan and scroll position (corner/edge markers prove the crop is unchanged); with watching OFF a disk change does NOT reload. Uses `shell` to overwrite the file; restores VERSION A at the end.

pdf tests/testdata/vhs_reload.pdf

wait 1000
# Enable watching (default is off), then force-restore VERSION A through the
# watcher itself - this self-heals the scratch file even if a previous run died
# mid-tape leaving VERSION B on disk.
key space
key w
wait 300
shell cp tests/testdata/vhs_reload_a.pdf tests/testdata/vhs_reload.pdf
wait 500

# Zen, top; wait out the "Watching enabled" HUD (2s).
key space
key z
wait 300
key 1
key g
key g
wait 500
wait 500
wait 500
wait 500
desc VERSION A open, watching enabled, zen. Expect: RELOAD TEST / VERSION A / PAGE 1 of 3, indicator [1/3] with [watching] tag, no HUD.
screenshot version_a

key 3
key g
key g
wait 500
desc Jumped to the last page of A. Expect: VERSION A PAGE 3 / 3, indicator [3/3].
screenshot version_a_p3

# Overwrite the file on disk with VERSION B (2 pages, thick border).
# Extra waits: let the 2s "Document reloaded" HUD expire before the shot.
shell cp tests/testdata/vhs_reload_b.pdf tests/testdata/vhs_reload.pdf
wait 500
wait 500
wait 500
wait 500
wait 500
desc File overwritten on disk while watching. Expect: AUTO-RELOADED to VERSION B (thick border), page re-clamped from 3 to [2/2]; no HUD.
screenshot reloaded_b_clamped

key 1
key g
key g
wait 500
desc 1gg to the first page of the reloaded doc. Expect: VERSION B PAGE 1 / 2, indicator [1/2].
screenshot reloaded_b_p1

# --- Position preservation across a content update ---------------------------
# Back to VERSION A (3 pages), then set up an arbitrary viewport far from the
# origin: DUAL layout (pages 1+2 side by side), zoomed past fit, clamped to
# the bottom-right of the strip. Wait out the reload + layout + zoom HUDs.
shell cp tests/testdata/vhs_reload_a.pdf tests/testdata/vhs_reload.pdf
wait 500
wait 500
key space
key D
wait 500
wait 500
# Zoom deep enough that the spread overflows both axes, then clamp the
# viewport to the BOTTOM-RIGHT corner: pan and scroll counts deliberately
# overshoot so both hit their clamps - fully deterministic on any terminal.
key =
key =
key =
key =
key =
key =
key =
key =
wait 500
wait 500
repeat_key L 160 0.01
wait 400
repeat_ctrl d 8 0.1
wait 500
wait 500
wait 500
wait 500
desc VERSION A dual spread (pages 1+2), zoomed x8, viewport clamped to the BOTTOM-RIGHT of the strip. Expect: right columns of the A2 grid with the BR/BOT A2 corner markers - this exact viewport must survive the update.
screenshot position_before

# Update the content (A -> C, same page count). Let the reload HUD expire.
shell cp tests/testdata/vhs_reload_c.pdf tests/testdata/vhs_reload.pdf
wait 500
wait 500
wait 500
wait 500
wait 500
desc File updated to VERSION C while watching. Expect: SAME dual spread, SAME bottom-right crop - identical rNN/cN label positions and clipping, labels now reading C2 with BR/BOT C2 markers; no HUD.
screenshot position_preserved

# --- Watching OFF: no reload --------------------------------------------------
key space
key w
wait 300
shell cp tests/testdata/vhs_reload_a.pdf tests/testdata/vhs_reload.pdf
wait 500
wait 500
wait 500
wait 500
desc Watching disabled (Space+w), then the file was replaced with VERSION A on disk. Expect: STILL showing VERSION C - no auto-reload; no HUD.
screenshot no_reload_when_disabled

key q
