#!/usr/bin/env python3
"""
Generate fixture PDFs for mouse-interaction VHS tests.

Produces:
  tests/testdata/vhs_twocol.pdf  - two-column pages; every line is uniquely
      labelled "P<page> L<nn>" (left column) / "P<page> R<nn>" (right column) so
      a screenshot makes it unambiguous which column/page a selection landed in.
      Used for two-column selection + dual-page selection routing.
  tests/testdata/vhs_links.pdf   - pages with a large internal link, a large
      external (URI) link, and a small inline citation link, plus printed page
      numbers. Used for link-click (sub-cell precision) and content-page tests.

  tests/testdata/vhs_stencil.pdf - scanned-book style pages whose images are
      stencil image masks (ink only, paper unpainted) next to an opaque image.
      Used for the image-inversion toggle with a transparent background.

No copyrighted content. Re-run to regenerate.
"""

import os
from reportlab.lib.pagesizes import letter
from reportlab.pdfgen import canvas
from reportlab.lib.units import inch

OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "tests", "testdata")
PAGE_W, PAGE_H = letter


def two_column_pdf(path, pages=6, lines=14):
    c = canvas.Canvas(path, pagesize=letter)
    left_x = 0.9 * inch
    right_x = 4.6 * inch
    top_y = PAGE_H - 1.2 * inch
    leading = 0.42 * inch
    for p in range(1, pages + 1):
        c.setFont("Helvetica-Bold", 18)
        c.drawString(left_x, PAGE_H - 0.8 * inch, f"TWO-COLUMN TEST  -  PAGE {p}")
        c.setFont("Helvetica", 13)
        # Draw the ENTIRE left column first, then the entire right column, so the
        # PDF text stream is column-major (like real two-column PDFs). This is
        # what makes column-aware selection meaningful: an interleaved L/R stream
        # would make a right-column selection sweep up left-column lines in
        # reading order.
        for i in range(1, lines + 1):
            y = top_y - (i - 1) * leading
            c.drawString(left_x, y, f"P{p} L{i:02d} left column line {i:02d}")
        for i in range(1, lines + 1):
            y = top_y - (i - 1) * leading
            c.drawString(right_x, y, f"P{p} R{i:02d} right column line {i:02d}")
        c.setFont("Helvetica", 10)
        c.drawCentredString(PAGE_W / 2, 0.5 * inch, f"- {p} -")
        c.showPage()
    c.save()
    print("wrote", path)


def links_pdf(path, pages=4):
    c = canvas.Canvas(path, pagesize=letter)
    # Bookmark every page so internal links can target them.
    for p in range(1, pages + 1):
        c.bookmarkPage(f"pg{p}")
        c.setFont("Helvetica-Bold", 18)
        c.drawString(0.9 * inch, PAGE_H - 0.9 * inch, f"LINKS TEST  -  PAGE {p}")

        c.setFont("Helvetica", 14)
        # Large internal link -> page 3.
        if p == 1:
            txt = "Jump to page 3 (internal link)"
            x, y = 0.9 * inch, PAGE_H - 2.0 * inch
            c.setFillColorRGB(0, 0, 0.8)
            c.drawString(x, y, txt)
            w = c.stringWidth(txt, "Helvetica", 14)
            c.linkAbsolute("internal", "pg3", (x, y - 3, x + w, y + 14))

            # Large external URI link.
            txt2 = "Open https://example.com (external link)"
            y2 = PAGE_H - 2.8 * inch
            c.drawString(x, y2, txt2)
            w2 = c.stringWidth(txt2, "Helvetica", 14)
            c.linkURL("https://example.com", (x, y2 - 3, x + w2, y2 + 14), relative=0)
            c.setFillColorRGB(0, 0, 0)

            # Small inline citation link (~1 cell wide) -> page 4. The hard case
            # for sub-cell precision.
            base = "See the appendix for details "
            c.drawString(x, PAGE_H - 3.6 * inch, base)
            bx = x + c.stringWidth(base, "Helvetica", 14)
            by = PAGE_H - 3.6 * inch
            c.setFillColorRGB(0, 0, 0.8)
            cite = "[1]"
            c.drawString(bx, by, cite)
            cw = c.stringWidth(cite, "Helvetica", 14)
            c.linkAbsolute("cite", "pg4", (bx, by - 3, bx + cw, by + 14))
            c.setFillColorRGB(0, 0, 0)

        c.setFont("Helvetica", 10)
        c.drawCentredString(PAGE_W / 2, 0.5 * inch, f"- {p} -")
        c.showPage()
    c.save()
    print("wrote", path)


_LOREM = (
    "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod "
    "tempor incididunt ut labore et dolore magna aliqua enim ad minim veniam "
    "quis nostrud exercitation ullamco laboris nisi aliquip ex ea commodo "
    "duis aute irure reprehenderit voluptate velit esse cillum fugiat nulla"
).split()


def _body_line(p, i):
    """Deterministic pseudo-sentence, unique per (page, line)."""
    n = len(_LOREM)
    start = (p * 7 + i * 3) % n
    return " ".join(_LOREM[(start + k) % n] for k in range(6))


def many_pages_pdf(path, pages=80):
    """Many DENSE, uniquely-labelled pages for zoom/pan/enhance/cache stress.

    Sparse pages make it impossible to see whether enhance sharpened anything or
    whether a far-jump render is stale/torn. So each page is a realistic
    two-column wall of small (7pt) text - every line reads "P<pg> C<col> L<nn>
    ..." so stale tiles from another page are obvious (wrong "P" number) and
    enhance is visible (blurry small text -> crisp). Deterministic markers are
    kept for verification: giant "PAGE NN / total" header (far-jump id), L-edge
    /R-edge markers in the clear header/footer rows (pan), TOP/BOT (scroll
    anchor).
    """
    c = canvas.Canvas(path, pagesize=letter)
    left_x = 0.6 * inch
    gap = 0.4 * inch
    col_w = (PAGE_W - 1.2 * inch - gap) / 2
    right_x = left_x + col_w + gap
    body_top = PAGE_H - 1.5 * inch
    body_bottom = 1.1 * inch
    leading = 0.155 * inch
    lines_per_col = int((body_top - body_bottom) / leading)
    for p in range(1, pages + 1):
        # Header: giant page number (far-jump id) + TOP marker.
        c.setFont("Helvetica-Bold", 30)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 0.75 * inch, f"PAGE {p:02d} / {pages}")
        c.setFont("Helvetica-Bold", 11)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 0.35 * inch, f"TOP {p:02d}")
        # Edge markers live in the clear header/footer rows (never over body).
        c.setFont("Helvetica-Bold", 14)
        for edge_y in (PAGE_H - 1.15 * inch, 0.8 * inch):
            c.drawString(0.3 * inch, edge_y, f"L-edge {p:02d}")
            c.drawRightString(PAGE_W - 0.3 * inch, edge_y, f"R-edge {p:02d}")
        # Dense two-column body.
        c.setFont("Helvetica", 7)
        li = 1
        for cx, cn in ((left_x, 1), (right_x, 2)):
            y = body_top
            for _ in range(lines_per_col):
                c.drawString(cx, y, f"P{p:02d} C{cn} L{li:02d}  {_body_line(p, li)}")
                y -= leading
                li += 1
        # Footer: BOT marker + printed page number.
        c.setFont("Helvetica-Bold", 11)
        c.drawCentredString(PAGE_W / 2, 0.5 * inch, f"BOT {p:02d}")
        c.setFont("Helvetica", 9)
        c.drawCentredString(PAGE_W / 2, 0.3 * inch, f"- {p} -")
        c.showPage()
    c.save()
    print("wrote", path)


def view_modes_pdf(path, pages=2):
    """Classic WHITE-background pages with an embedded bitmap image, for the
    display-mode toggle tests (themed vs original rendering, image inversion).

    vhs_test.pdf is authored with a dark background, so on it "theming off"
    looks almost identical to themed rendering. These pages paint an explicit
    white background rect: themed ON -> dark paper, themed OFF -> classic white
    paper, unambiguous in a screenshot. The gradient bitmap gives the inversion
    toggle a raster region to act on.
    """
    from io import BytesIO

    from PIL import Image
    from reportlab.lib.utils import ImageReader

    w, h = 256, 128
    img = Image.new("RGB", (w, h))
    px = img.load()
    for x in range(w):
        for y in range(h):
            px[x, y] = (int(255 * x / (w - 1)), int(255 * y / (h - 1)), 160)
    buf = BytesIO()
    img.save(buf, format="PNG")
    buf.seek(0)
    gradient = ImageReader(buf)

    c = canvas.Canvas(path, pagesize=letter)
    for p in range(1, pages + 1):
        # Explicit white background: the page must be genuinely white-authored,
        # not just "unpainted" (unpainted areas go transparent in the app's
        # transparent mode).
        c.setFillColorRGB(1, 1, 1)
        c.rect(0, 0, PAGE_W, PAGE_H, stroke=0, fill=1)

        c.setFillColorRGB(0.1, 0.3, 0.8)
        c.setFont("Helvetica-Bold", 22)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 1.0 * inch, f"VIEW MODES  -  PAGE {p}")

        c.setFillColorRGB(0, 0, 0)
        c.setFont("Helvetica", 13)
        for i in range(1, 7):
            c.drawString(
                0.9 * inch,
                PAGE_H - (1.6 + 0.35 * i) * inch,
                f"P{p} body line {i:02d}: black text on a white page",
            )

        iw, ih = 4.5 * inch, 2.25 * inch
        ix, iy = (PAGE_W - iw) / 2, PAGE_H - 7.2 * inch
        c.drawImage(gradient, ix, iy, width=iw, height=ih)
        c.setFont("Helvetica", 10)
        c.drawCentredString(PAGE_W / 2, iy - 0.25 * inch, "Pattern: gradient (bitmap)")

        c.drawCentredString(PAGE_W / 2, 0.5 * inch, f"- {p} -")
        c.showPage()
    c.save()
    print("wrote", path)


def transparent_pdf(path, pages=2):
    """Ordinary UNPAINTED pages with an embedded bitmap, for the transparent
    background tests.

    Unlike view_modes_pdf there is deliberately no white background rect: real
    PDFs leave the paper unpainted, and only unpainted areas go transparent in
    the app's transparent mode. The gradient bitmap gives the inversion toggle
    a raster region to act on.
    """
    from io import BytesIO

    from PIL import Image
    from reportlab.lib.utils import ImageReader

    w, h = 256, 128
    img = Image.new("RGB", (w, h))
    px = img.load()
    for x in range(w):
        for y in range(h):
            px[x, y] = (int(255 * x / (w - 1)), int(255 * y / (h - 1)), 160)
    buf = BytesIO()
    img.save(buf, format="PNG")
    buf.seek(0)
    gradient = ImageReader(buf)

    c = canvas.Canvas(path, pagesize=letter)
    for p in range(1, pages + 1):
        c.setFillColorRGB(0.1, 0.3, 0.8)
        c.setFont("Helvetica-Bold", 22)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 1.0 * inch, f"TRANSPARENT  -  PAGE {p}")

        c.setFillColorRGB(0, 0, 0)
        c.setFont("Helvetica", 13)
        for i in range(1, 7):
            c.drawString(
                0.9 * inch,
                PAGE_H - (1.6 + 0.35 * i) * inch,
                f"P{p} body line {i:02d}: black text on an unpainted page",
            )

        iw, ih = 4.5 * inch, 2.25 * inch
        ix, iy = (PAGE_W - iw) / 2, PAGE_H - 7.2 * inch
        c.drawImage(gradient, ix, iy, width=iw, height=ih)
        c.setFont("Helvetica", 10)
        c.drawCentredString(PAGE_W / 2, iy - 0.25 * inch, "Pattern: gradient (bitmap)")

        c.drawCentredString(PAGE_W / 2, 0.5 * inch, f"- {p} -")
        c.showPage()
    c.save()
    print("wrote", path)


def stencil_mask_pdf(path):
    """Scanned-book style pages whose "images" are stencil image masks.

    Bitonal scans (JBIG2 / CCITT) are usually /ImageMask true: only the ink is
    painted and the paper stays unpainted, so whatever is behind the image shows
    through. Page 1 puts a stencil checker next to an opaque DeviceGray ramp so
    the two kinds of image can be compared in one shot; page 2 is a full-page
    bitonal mask like a scanned text page. Hand-written PDF (reportlab cannot
    emit a stencil mask); fully deterministic, no dates or ids.
    """
    page_w, page_h = int(PAGE_W), int(PAGE_H)

    def pack_rows(w, h, bit_at):
        out = bytearray()
        for y in range(h):
            row = 0
            for x in range(w):
                row = (row << 1) | bit_at(x, y)
                if x % 8 == 7:
                    out.append(row)
                    row = 0
            if w % 8:
                out.append(row << (8 - w % 8))
        return bytes(out)

    checker_w, checker_h = 64, 32
    checker = pack_rows(checker_w, checker_h, lambda x, y: 1 if ((x // 4 + y // 4) % 2 == 0) else 0)
    ramp = bytes((x * 255 // 63) for _y in range(32) for x in range(64))
    scan_w, scan_h = 400, 500
    # Rows of ink 6px tall every 16px, broken into "words" 30px wide every 40px.
    scan = pack_rows(scan_w, scan_h, lambda x, y: 1 if ((y % 16) < 6 and (x % 40) < 30) else 0)

    objs = []

    def add(body):
        objs.append(body)
        return len(objs)

    def stream(dictionary, data):
        return dictionary + b" /Length %d >>\nstream\n" % len(data) + data + b"\nendstream"

    font = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    # /Decode [1 0]: a 1 bit paints (the default paints 0 bits).
    mask_checker = add(stream(
        b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ImageMask true"
        b" /BitsPerComponent 1 /Decode [1 0]" % (checker_w, checker_h), checker))
    gray_ramp = add(stream(
        b"<< /Type /XObject /Subtype /Image /Width 64 /Height 32 /ColorSpace /DeviceGray"
        b" /BitsPerComponent 8", ramp))
    mask_scan = add(stream(
        b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ImageMask true"
        b" /BitsPerComponent 1 /Decode [1 0]" % (scan_w, scan_h), scan))

    page1 = (
        b"BT /F1 20 Tf 72 720 Td (STENCIL  -  PAGE 1) Tj ET\n"
        b"q 0 0 0 rg 240 0 0 120 72 520 cm /M1 Do Q\n"
        b"q 240 0 0 120 340 520 cm /G1 Do Q\n"
        b"BT /F1 12 Tf 72 500 Td (ImageMask: black ink, paper unpainted) Tj ET\n"
        b"BT /F1 12 Tf 340 500 Td (DeviceGray 8-bit: opaque) Tj ET\n"
        b"BT /F1 10 Tf 290 36 Td (- 1 -) Tj ET\n"
    )
    page2 = (
        b"BT /F1 20 Tf 72 740 Td (STENCIL  -  PAGE 2: full-page bitonal mask) Tj ET\n"
        b"q 0 0 0 rg 468 0 0 620 72 90 cm /M2 Do Q\n"
        b"BT /F1 10 Tf 290 36 Td (- 2 -) Tj ET\n"
    )
    contents1 = add(stream(b"<<", page1))
    contents2 = add(stream(b"<<", page2))
    pages_id = len(objs) + 3
    p1 = add(
        b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %d %d] /Resources << /Font << /F1 %d 0 R >>"
        b" /XObject << /M1 %d 0 R /G1 %d 0 R >> >> /Contents %d 0 R >>"
        % (pages_id, page_w, page_h, font, mask_checker, gray_ramp, contents1))
    p2 = add(
        b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %d %d] /Resources << /Font << /F1 %d 0 R >>"
        b" /XObject << /M2 %d 0 R >> >> /Contents %d 0 R >>"
        % (pages_id, page_w, page_h, font, mask_scan, contents2))
    pages = add(b"<< /Type /Pages /Kids [%d 0 R %d 0 R] /Count 2 >>" % (p1, p2))
    assert pages == pages_id
    catalog = add(b"<< /Type /Catalog /Pages %d 0 R >>" % pages)

    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for num, body in enumerate(objs, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % num + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
        len(objs) + 1, catalog, xref)
    with open(path, "wb") as f:
        f.write(out)
    print("wrote", path)


_ROMAN = ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii"]


def offset_pdf(path, front=4, body=12):
    """Printed-page-offset fixture: printed numbers are DECOUPLED from physical
    page indices, for testing printed->PDF page mapping (page_numbers.rs) and
    the go-to-page [Content] vs [PDF] modes.

    Physical pages 1..front are front matter with roman-numeral footers
    (i, ii, ...) - romans deliberately do NOT parse as page numbers, so they
    contribute no samples. Physical pages front+1.. are body pages with printed
    arabic footers starting at 1, giving a constant offset (printed 1 =
    physical front+1). Every page displays its PHYSICAL index in giant text
    ("PHYS NN" - contains letters, so it can't be mistaken for a printed page
    number by the edge-line detector), and body pages also show "PRINTED NN",
    so a screenshot proves which interpretation of "page N" was applied.

    A flat outline (Cover / Preface / Chapter 1..3) covers TOC navigation on
    the same fixture.
    """
    total = front + body
    c = canvas.Canvas(path, pagesize=letter)
    outline = {0: "Cover", 2: "Preface", front: "Chapter 1"}
    outline[front + body // 3] = "Chapter 2"
    outline[front + 2 * (body // 3)] = "Chapter 3"

    for idx in range(total):  # 0-based physical index
        phys = idx + 1
        if idx in outline:
            key = f"out{idx}"
            c.bookmarkPage(key)
            c.addOutlineEntry(outline[idx], key, level=0)

        is_front = idx < front
        c.setFont("Helvetica-Bold", 34)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 1.5 * inch, f"PHYS {phys:02d}")
        c.setFont("Helvetica-Bold", 20)
        if is_front:
            c.drawCentredString(
                PAGE_W / 2, PAGE_H - 2.3 * inch, f"FRONT MATTER ({_ROMAN[idx]})"
            )
        else:
            printed = idx - front + 1
            c.drawCentredString(PAGE_W / 2, PAGE_H - 2.3 * inch, f"PRINTED {printed:02d}")
            chapter = [t for i, t in sorted(outline.items()) if i <= idx][-1]
            c.setFont("Helvetica", 13)
            c.drawCentredString(PAGE_W / 2, PAGE_H - 3.0 * inch, f"({chapter})")

        # Footer: the printed page number the detector should (or should not) see.
        c.setFont("Helvetica", 11)
        if is_front:
            c.drawCentredString(PAGE_W / 2, 0.5 * inch, f"- {_ROMAN[idx]} -")
        else:
            c.drawCentredString(PAGE_W / 2, 0.5 * inch, f"- {idx - front + 1} -")
        c.showPage()
    c.save()
    print("wrote", path)


def reload_pdf(path, version, pages):
    """Reload-test fixture: versions of "the same" document for the file-watch
    auto-reload tests. VERSION A has 3 pages, VERSION B has 2 (thick border) -
    overwriting A with B while watching must swap the visible content AND
    re-clamp the current page (page 3 of A -> page 2 of B). VERSION C has 3
    pages like A - overwriting A with C must preserve page/zoom/scroll/pan.
    Corner markers (TL/TR/BL/BR + version + page) make a zoomed/panned crop
    self-identifying so position preservation is provable from pixels.
    vhs_reload.pdf is the scratch copy the tape opens and overwrites; the tape
    restores it from vhs_reload_a.pdf when done.
    """
    c = canvas.Canvas(path, pagesize=letter)
    for p in range(1, pages + 1):
        c.setFont("Helvetica-Bold", 16)
        c.drawString(0.45 * inch, PAGE_H - 0.6 * inch, f"TL {version}{p}")
        c.drawRightString(PAGE_W - 0.45 * inch, PAGE_H - 0.6 * inch, f"TR {version}{p}")
        c.drawString(0.45 * inch, 0.45 * inch, f"BL {version}{p}")
        c.drawRightString(PAGE_W - 0.45 * inch, 0.45 * inch, f"BR {version}{p}")
        # Mid-edge markers help identify a zoomed crop that misses the corners.
        c.drawCentredString(PAGE_W / 2, PAGE_H - 0.6 * inch, f"TOP {version}{p}")
        c.drawCentredString(PAGE_W / 2, 0.45 * inch, f"BOT {version}{p}")
        c.drawString(0.45 * inch, PAGE_H / 2, f"L {version}{p}")
        c.drawRightString(PAGE_W - 0.45 * inch, PAGE_H / 2, f"R {version}{p}")
        c.setFont("Helvetica-Bold", 44)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 2.0 * inch, "RELOAD TEST")
        c.setFont("Helvetica-Bold", 72)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 4.2 * inch, f"VERSION {version}")
        c.setFont("Helvetica-Bold", 30)
        c.drawCentredString(PAGE_W / 2, PAGE_H - 5.8 * inch, f"PAGE {p} / {pages}")
        # Dense label grid below the title block: any zoomed/panned crop of the
        # page contains version+page+row+column identifiers.
        c.setFont("Helvetica-Bold", 12)
        row = 1
        y = PAGE_H - 6.5 * inch
        while y > 0.8 * inch:
            for col, x in ((1, 1.2 * inch), (2, PAGE_W / 2), (3, PAGE_W - 1.2 * inch)):
                c.drawCentredString(x, y, f"{version}{p} r{row:02d} c{col}")
            y -= 0.5 * inch
            row += 1
        if version == "B":
            c.setLineWidth(6)
            c.rect(0.35 * inch, 0.35 * inch, PAGE_W - 0.7 * inch, PAGE_H - 0.7 * inch)
        c.showPage()
    c.save()
    print("wrote", path)


if __name__ == "__main__":
    two_column_pdf(os.path.join(OUT_DIR, "vhs_twocol.pdf"))
    links_pdf(os.path.join(OUT_DIR, "vhs_links.pdf"))
    many_pages_pdf(os.path.join(OUT_DIR, "vhs_many.pdf"))
    view_modes_pdf(os.path.join(OUT_DIR, "vhs_viewmodes.pdf"))
    transparent_pdf(os.path.join(OUT_DIR, "vhs_transparent.pdf"))
    stencil_mask_pdf(os.path.join(OUT_DIR, "vhs_stencil.pdf"))
    offset_pdf(os.path.join(OUT_DIR, "vhs_offset.pdf"))
    reload_pdf(os.path.join(OUT_DIR, "vhs_reload_a.pdf"), "A", 3)
    reload_pdf(os.path.join(OUT_DIR, "vhs_reload_b.pdf"), "B", 2)
    reload_pdf(os.path.join(OUT_DIR, "vhs_reload_c.pdf"), "C", 3)
    import shutil

    shutil.copyfile(
        os.path.join(OUT_DIR, "vhs_reload_a.pdf"),
        os.path.join(OUT_DIR, "vhs_reload.pdf"),
    )
    print("wrote", os.path.join(OUT_DIR, "vhs_reload.pdf"))
