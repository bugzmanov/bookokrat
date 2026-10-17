# pdf_transparent_bg.tape
about Transparent UI background (Settings > Themes > Background > Transparent) on an ordinary UNPAINTED page with a bitmap image (vhs_transparent.pdf; vhs_viewmodes.pdf paints an explicit white rect and cannot show this). Kitty only. With theming ON the terminal shows through the paper; toggling theming OFF (I) must bring back an opaque white page. Regression for #206.

terminal kitty
pdf tests/testdata/vhs_transparent.pdf

wait 1000
key space
key z
wait 400
desc Default: opaque, themed, image inversion ON. Expect: dark paper, theme-coloured text, gradient in original colours.
screenshot tb_opaque_themed

key I
wait 500
desc Default: opaque, theming OFF (I). Expect: classic WHITE page, black text, original gradient.
screenshot tb_opaque_original

key I
wait 300
key space
key t
wait 400
key G
wait 200
desc Settings > Themes with the Background radio moved to the last entry. Expect: "Transparent" highlighted, "Theme color" still selected.
screenshot tb_settings_transparent_focused

return
wait 300
desc Transparent applied. Expect: "Transparent" selected.
screenshot tb_settings_transparent_applied

escape
wait 600
desc Transparent ON, themed. Expect: no paper, terminal shows through; theme-coloured text; gradient in original colours.
screenshot tb_transparent_themed

key i
wait 500
desc Transparent ON, themed, image inversion OFF (i). Expect: gradient tinted to theme colours; paper still transparent.
screenshot tb_transparent_themed_invert_off

key i
wait 300
key I
wait 600
desc Transparent ON, theming OFF (I). Expect: opaque WHITE page, black text, original gradient (alpha applies to themed rendering only). Bug #206 before the fix: no paper, black text invisible.
screenshot tb_transparent_original

key =
key =
wait 500
desc Zoomed x2, theming OFF. Expect: still a white page after the re-render.
screenshot tb_transparent_original_zoom

key z
wait 300
key I
wait 500
desc Theming back ON. Expect: terminal shows through again, theme-coloured text.
screenshot tb_transparent_themed_restored

key q
