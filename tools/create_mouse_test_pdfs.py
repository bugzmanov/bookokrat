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


if __name__ == "__main__":
    two_column_pdf(os.path.join(OUT_DIR, "vhs_twocol.pdf"))
    links_pdf(os.path.join(OUT_DIR, "vhs_links.pdf"))
    many_pages_pdf(os.path.join(OUT_DIR, "vhs_many.pdf"))
    view_modes_pdf(os.path.join(OUT_DIR, "vhs_viewmodes.pdf"))
