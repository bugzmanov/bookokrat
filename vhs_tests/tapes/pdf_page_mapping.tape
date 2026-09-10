# pdf_page_mapping.tape
about Printed-page mapping + TOC on an offset fixture (4 roman-numbered front-matter pages; printed 1 = physical 5): offset detection feeds the go-to-page [Content] mode, [PDF] mode addresses physical pages, {count}gg is physical, out-of-range printed numbers error. Every page shows PHYS NN (+ PRINTED NN on body pages) so the applied interpretation is provable from pixels.

pdf tests/testdata/vhs_offset.pdf

wait 1000
# Fit the WHOLE page in the viewport so the printed footer number (the thing
# the mapping is about) is visible in every shot.
key z
wait 500
# Let the 2s zoom HUD expire so it isn't baked into the golden.
wait 500
wait 500
wait 500
desc Opened at the cover, fit-page. Expect: PHYS 01, FRONT MATTER (i), roman "- i -" footer at page bottom; TOC panel lists Cover/Preface/Chapter 1-3; no HUD.
screenshot opened

# --- TOC navigation ----------------------------------------------------------
tab
wait 300
# Anchor selection at the top (gg lands on the "<- Books List" back-entry),
# then 4x j: Cover, Preface, Chapter 1, Chapter 2.
key g
key g
wait 200
key j
key j
key j
key j
wait 200
return
wait 600
desc TOC: moved to "Chapter 2" and pressed Enter. Expect: PHYS 09 / PRINTED 05 (Chapter 2), footer "- 5 -".
screenshot toc_chapter2

# Zen for clean go-to-page shots.
key space
key z
wait 400

# --- Content mode: printed numbers, offset applied ---------------------------
key space
key g
wait 300
desc Go-to-page prompt. Expect: [Content] mode selected, printed range (1-12) - proof the offset (4 front pages) was detected.
screenshot goto_prompt_range

key 1
return
wait 600
desc Content page 1. Expect: PHYS 05 / PRINTED 01, footer "- 1 -" - printed 1 mapped over the front matter to physical 5.
screenshot content_page1

# --- PDF mode: physical numbers ----------------------------------------------
key space
key g
wait 200
key p
wait 200
key 1
return
wait 600
desc Switched prompt to PDF mode (p), entered 1. Expect: back on the cover - PHYS 01, FRONT MATTER (i), roman "- i -" footer.
screenshot pdf_page1

# --- Content mode out-of-range -----------------------------------------------
key space
key g
wait 200
key c
wait 200
key 1
key 3
return
wait 400
desc Content page 13 (printed range is 1-12). Expect: red "Cannot map content page 13 ..." error; prompt stays open.
screenshot content_out_of_range
escape
wait 300

# --- Quick jump is PHYSICAL --------------------------------------------------
key 1
key 2
key g
key g
wait 600
desc 12 g g quick jump. Expect: PHYS 12 / PRINTED 08, footer "- 8 -" - quick jump addresses physical pages, not printed.
screenshot quickjump_phys12

key q
