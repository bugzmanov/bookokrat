# pdf_render_stress.tape
about Render-engine stress on an 80-page PDF (Kitty): stressful zoom/pan/enhance/dual/scroll-mode/cache chains that also validate the basics. Each shot doubles as a feature check. Guards the dual->zoom->enhance and dual->zoom->enhance->far-jump failure modes.

pdf tests/testdata/vhs_many.pdf

# Zen for max render area, start at page 1 top.
wait 1000
key space
key z
wait 400
key g
key g
wait 500
desc Page 1, fit-page, zen. Baseline: giant "PAGE 01 / 80", TOP/BOT + L/R-edge markers.
screenshot baseline

# --- S1: single-page zoom / anchor / pan / enhance / reset / floor ------------
key Z
wait 500
desc Fit-width (Z). Expect: page fills width, taller than viewport.
screenshot fitwidth

ctrl d
wait 300
key =
key =
wait 600
desc Scrolled to mid-page then zoomed in x2. Expect: view stays on the same content (anchor held), not jumped to TOP.
screenshot zoom_anchor

# Isolated pan check: fit-width top (both top edge markers at frame edges),
# zoom in so the page overflows width, then pan right to the clamp.
key z
wait 300
key Z
wait 400
key g
key g
wait 300
key =
key =
wait 500
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
desc Fit-width top, zoomed x2, panned right to the clamp (L x10). Expect: top R-edge marker now in view, top L-edge pushed off the left.
screenshot pan_right

key z
wait 500
desc Zoom reset (z) to fit-page. Expect: whole page centered again.
screenshot reset

key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
key -
wait 600
desc Zoom out x15 to the floor. Expect: tiny page clamped at min scale, NOT blank, no crash.
screenshot zoom_floor

key z
wait 500

# --- S2: dual -> zoom -> enhance -> far jump (cache evict) -> back ------------
key space
key D
wait 500
key g
key g
wait 500
desc Dual layout at top. Expect: pages 01 + 02 side by side, correctly paired.
screenshot dual_baseline

key =
key =
key =
wait 600
key e
wait 900
desc Dual -> zoom -> enhance. THE reported failure chain. Expect: page re-renders crisp with no ghost/overlap/blank/corruption (zoom pushes page 2 off-screen right; pairing is checked in dual_baseline and dual_farjump).
screenshot dual_enhance

# Reset to dual fit so the far-jump shot shows the page-number headers.
key z
wait 400
key space
key g
key p
key 5
key 5
return
wait 900
desc From the dual+enhance state, reset zoom then jump to PDF page 55 (+50, far beyond the 15+10 cache). Expect: pages 55 + 56 (headers visible), no stale tiles from pages 1-2, no blank.
screenshot dual_farjump

key space
key g
key p
key 1
return
wait 900
desc Jump back to page 1. Expect: pages 01 + 02 re-render correctly (no stale enhance frame, no blank).
screenshot dual_jumpback

key space
key D
wait 600
desc Toggle back to single page. Expect: single page 01, zoom retained, NO leftover right-page tile.
screenshot dual_to_single

# --- Switch to dual mid-book (not from page 1) -------------------------------
key space
key g
key p
key 4
key 1
return
wait 700
key space
key D
wait 700
desc Toggled dual while at page 41 (mid-book). Expect: pages 41 + 42 side by side, correctly paired, neither page missing.
screenshot dual_midbook

key space
key D
wait 400

# --- S3: pan + enhance in dual, and enhance carried single<->dual ------------
key z
wait 300
key space
key D
wait 500
key =
key =
key =
key =
key =
key =
wait 500
key L
key L
key L
wait 400
key e
wait 800
desc Dual -> zoom ~1.8x -> pan right -> enhance. Expect: pan offset preserved/re-clamped after enhance, both pages consistent and crisp.
screenshot dual_pan_enhance

key space
key D
wait 500
key e
wait 700
desc Toggle back to single, then press e. Expect: no-op with the "Already enhanced at this zoom level" HUD, since the single view is at native scale.
screenshot single_enhance_from_dual

key z
wait 400

# --- S4: scroll render-mode stress (KITTY ONLY: scroll mode requires the
#     Kitty graphics protocol; on other terminals Space+S shows an error HUD) --
@kitty key space
@kitty key S
@kitty wait 800
@kitty key g
@kitty key g
@kitty wait 600
@kitty desc Scroll render-mode at top. Expect: continuous scroll layout, page 01 with page 02 following contiguously.
@kitty screenshot scroll_mode

@kitty key =
@kitty key =
@kitty key =
@kitty key =
@kitty key =
@kitty wait 500
@kitty key e
@kitty wait 800
@kitty desc Scroll-mode -> zoom ~1.6x -> enhance. Expect: stable, crisp, contiguous, no gaps.
@kitty screenshot scroll_enhance

@kitty ctrl d
@kitty ctrl d
@kitty ctrl d
@kitty ctrl d
@kitty ctrl d
@kitty ctrl d
@kitty ctrl d
@kitty ctrl d
@kitty wait 800
@kitty desc Scrolled far down in scroll-mode (8x half-page). Expect: later pages stream in with no blank gaps or torn tiles.
@kitty screenshot scroll_far

@kitty key space
@kitty key D
@kitty wait 800
@kitty desc Scroll-mode + dual (dual-scroll path). Expect: two-up continuous layout, no corruption.
@kitty screenshot dual_scroll

# back to page mode, single, fit-page for the race section
@kitty key space
@kitty key D
@kitty wait 400
@kitty key space
@kitty key S
@kitty wait 600
key z
wait 400
key space
key g
key p
key 1
return
wait 700

# --- S5: generation / stale-frame races (capture settled state) --------------
key =
key -
key =
key -
key =
key -
wait 800
desc Rapid zoom in/out x3. Expect: settled render matches a clean single state, no torn/ghost frame left behind.
screenshot rapid_zoom

key space
key g
key p
key 6
key 0
return
key =
key =
wait 900
desc Jump to page 60 then immediately zoom (generation race during far-jump render). Expect: settled on page 60, zoomed, correct, no stale page-1 tiles.
screenshot farjump_zoom_race

key e
key space
key g
key p
key 2
key 0
return
key =
wait 900
desc Enhance -> far jump to page 20 -> zoom (compound stale). Expect: settled on page 20, correct, no leftover enhance frame from page 60.
screenshot enhance_jump_zoom_race

# --- S6: dual last-page fallback (odd total) ---------------------------------
key z
wait 300
key space
key g
key p
key 8
key 0
return
wait 800
key space
key D
wait 700
desc Toggle dual after jumping to page 80 (an odd index). Expect: anchor snaps to the even-aligned pair -> pages 79 + 80 spread (guards the even-align-on-toggle fix), no solo page.
screenshot dual_lastpage

# --- Enhance illustration (kept LAST: the high-zoom enhance re-render leaves
#     zoom/pan state that would otherwise poison the dual/layout tests above) ---
key space
key D
wait 400
key space
key g
key p
key 1
return
wait 700
key Z
wait 300
key g
key g
wait 300
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
key =
wait 300
ctrl d
ctrl d
ctrl d
ctrl d
ctrl d
wait 600
desc Zoomed in ~2.7x into the dense body text. Expect on KITTY: display-upscaled = clearly BLURRY (enhance not applied yet). On WezTerm/iTerm2 zoom re-renders at scale, so text is already crisp and identical to the enhance shot (by design).
screenshot zoom_blurry

key e
wait 900
desc Enhance (e). KITTY: re-renders at the true zoom = CRISP text vs zoom_blurry (also settles framing to the native-rendered zoom). WezTerm/iTerm2: no-op, matches zoom_blurry.
screenshot enhance

key q
