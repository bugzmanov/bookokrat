# pdf_halfblocks_blocked.tape
about PDF must be BLOCKED when the graphics protocol cannot display raster pages: with BOOKOKRAT_PROTOCOL=halfblocks forced, opening a PDF must not enter the (broken) half-configured pipeline - expect the blocked warning instead of a rendered page. Regression for the derive_pdf_capabilities protocol gate.

terminal kitty
appenv BOOKOKRAT_PROTOCOL=halfblocks
pdf tests/testdata/vhs_many.pdf

wait 1000
desc Launched with a PDF while the protocol is forced to halfblocks. Expect: NO rendered PDF page - the "PDF requires a graphics protocol" warning and/or the book list instead.
screenshot blocked

key q
