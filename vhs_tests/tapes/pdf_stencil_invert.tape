# pdf_stencil_invert.tape
about Image inversion toggle (i) on scanned-book style pages whose images are STENCIL masks (ImageMask true: only the ink is painted, the paper is unpainted). vhs_stencil.pdf page 1 = stencil checker next to an opaque gray ramp, page 2 = full-page bitonal mask. With the transparent background ON, "Images: original" must composite the mask onto WHITE paper exactly like the opaque render. Before the fix the ink landed on the terminal background and the page still looked themed (#206 follow-up: "some images change, others don't"). Kitty only (transparent + scroll mode).

terminal kitty
pdf tests/testdata/vhs_stencil.pdf

wait 1000
key space
key z
wait 400
desc Opaque, themed, images inverted (default). Expect: dark paper, checker and gray ramp tinted to theme colours.
screenshot st_opaque_inverted

key i
wait 500
desc Opaque, Images: original (i). Expect: checker black-on-white box, gray ramp original. Reference look for the transparent case.
screenshot st_opaque_original

key i
wait 300
key space
key t
wait 400
key G
wait 200
return
wait 300
escape
wait 600
desc Transparent ON, themed, images inverted. Expect: no paper (terminal shows through); checker and ramp tinted.
screenshot st_transparent_inverted

key i
wait 500
desc Transparent ON, Images: original (i). Expect: checker black-on-WHITE box and original ramp, same as the opaque shot. Bug: checker black on the terminal background.
screenshot st_transparent_original

key l
wait 600
desc Page 2 (full-page bitonal mask), transparent, Images: original. Expect: opaque WHITE page with black ink. Bug: dark page with black ink.
screenshot st_transparent_page2_original

key i
wait 500
desc Page 2, Images: inverted. Expect: dark page, theme-coloured ink.
screenshot st_transparent_page2_inverted

key space
key S
wait 800
key i
wait 800
desc Scroll render mode (Space+S), page 2, Images: original. Expect: white page with black ink in the continuous layout.
screenshot st_scroll_page2_original

key q
