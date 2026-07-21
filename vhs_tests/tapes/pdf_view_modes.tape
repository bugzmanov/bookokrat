# pdf_view_modes.tape
about PDF display-mode toggles on a WHITE-background bitmap-image page: image inversion (i), themed vs original rendering (I), dual layout (Space+D), and hiding the zen border (Space+b, single + dual), plus their combinations. Verifies each toggle and that it survives a re-render (zoom). White-authored fixture makes themed-vs-original unambiguous (vhs_test.pdf is dark-authored and cannot show the difference).

pdf tests/testdata/vhs_viewmodes.pdf

# Zen for a clean canvas; page 1 has the gradient bitmap.
wait 1000
key space
key z
wait 400
desc White-authored bitmap page in zen. Baseline: themed rendering ON + image inversion ON (defaults) - dark paper, original-color gradient image.
screenshot vm_baseline

# --- M1: invert / theme walk -------------------------------------------------
key i
wait 400
desc Toggled image inversion OFF (i). Expect: the bitmap image's light/dark flips vs baseline; page tint unchanged.
screenshot vm_invert_off

key I
wait 400
desc Toggled themed rendering OFF (I). Expect: original rendering - classic WHITE page background, black text, blue title (HUD: "rendering: original").
screenshot vm_theme_off

key =
key =
wait 500
desc Zoomed in x2 while theming is OFF. Expect: still the white original page after re-render (toggle survives zoom).
screenshot vm_original_zoom

key z
wait 300
key I
wait 300
key i
wait 400
desc Restored: themed ON + inverted ON, fit reset. Expect: back to the baseline look.
screenshot vm_restored

# --- M2: dual + display combos ----------------------------------------------
key space
key D
wait 500
desc Dual layout (Space+D), themed+inverted. Expect: two pages side by side, both themed.
screenshot vm_dual

key i
wait 400
desc Dual + invert OFF. Expect: both pages' images flip consistently.
screenshot vm_dual_invert

key I
wait 400
desc Dual + theme OFF. Expect: both pages white with original colors.
screenshot vm_dual_theme_off

# --- M3: app border disabled (Space+b hides the zen frame) -------------------
# Default zen shows the border (test mode never loads persisted settings), so
# one Space+b = border hidden.
key I
wait 300
key i
wait 300
key space
key D
wait 500
key space
key b
wait 500
desc Single page, themed+inverted, zen border HIDDEN (Space+b). Expect: no frame and no page indicator around the content, page bleeds to the window edges.
screenshot vm_border_off_single

key space
key D
wait 500
wait 500
desc Dual layout with the border still hidden. Expect: two pages side by side, no zen frame around them.
screenshot vm_border_off_dual

key q
