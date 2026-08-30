# pdf_synctex.tape
about SyncTeX forward + inverse on a real LaTeX build (20-page test_comprehensive.pdf with .synctex.gz sidecar): the app auto-starts a Unix-socket listener; `shell` pokes it like an editor would ("forward <line> <col> <file>") - the viewport must jump to the mapped page and flash the target line WITHOUT moving the cursor. Inverse = right-click on a line (resolves source file:line; without synctex_editor configured it only emits a toast).

pdf tests/testdata/synctex/test_comprehensive.pdf

wait 1000
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
desc LaTeX doc open in zen at page 1 of 20; synctex sidecar loaded (socket listener running); HUDs expired.
screenshot initial

# --- Forward search: editor -> PDF -------------------------------------------
shell S=$(ls "${TMPDIR%/}/"bookokrat-stx-test-comprehensi*.sock | head -1) && printf 'forward 48 1 test_comprehensive.tex\n' | nc -U "$S"
wait 500
desc Editor sent "forward 48 1" (intro text). Expect: jumped to page 3 ([3/20]) scrolled so the target line is in view. (No flash highlight - forward search scrolls only.)
screenshot forward_p3

shell S=$(ls "${TMPDIR%/}/"bookokrat-stx-test-comprehensi*.sock | head -1) && printf 'forward 305 1 test_comprehensive.tex\n' | nc -U "$S"
wait 500
desc Editor sent "forward 305 1". Expect: jumped to page 8 ([8/20]) scrolled to the mapped position.
screenshot forward_p8

# --- Inverse search: PDF -> editor (right-click) ------------------------------
rclick 110 24
wait 400
desc Right-click on a text line (inverse search). Expect: source resolved to test_comprehensive.tex:<line>; toast bottom-right (may be hidden by the kitty image - known issue); no navigation.
screenshot inverse_click

key q
