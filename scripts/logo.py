"""The Flashwave.tf logo: an original badge in TF2's colours (not Valve's logo).

The name, drawn: a cream lightning bolt (the flash) over two waves, on a
dark disc in a ring of TF2's HUD tan. It replaced a badge with four
crosshair ticks that read as a sniper scope (Flashy, October 2026).

Writes ui/public/logo.svg and favicon.svg, and target/hl-logo-1024.png
(drawn with Pillow at three times the size and scaled down, for clean
edges). Then make every app icon:

    python scripts/logo.py
    npx tauri icon target/hl-logo-1024.png

and delete the android/, ios/ and Square*/StoreLogo files it adds.
"""
import os

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_PNG = os.path.join(ROOT, "target", "hl-logo-1024.png")
os.makedirs(os.path.dirname(OUT_PNG), exist_ok=True)

BG = "#2b2420"      # dark warm brown
TAN = "#cf7336"     # TF2's HUD tan
CREAM = "#f2e6d9"

# Geometry on a 512 canvas, centred at (256, 256).
DISC_R = 236
RING_W = 34
# The waves: each a pair of cubic curves (start, control, control, end).
WAVES = [
    ([(96, 330), (150, 290), (200, 370), (256, 330)], [(256, 330), (312, 290), (362, 370), (416, 330)], CREAM, 30),
    ([(120, 390), (170, 356), (214, 420), (262, 390)], [(262, 390), (310, 360), (352, 418), (396, 392)], TAN, 22),
]
BOLT = [(282, 86), (176, 250), (240, 250), (216, 346), (336, 190), (270, 190), (300, 86)]

# ---- SVG -------------------------------------------------------------------


def curve_path(a, b):
    (x0, y0), (c1x, c1y), (c2x, c2y), (x1, y1) = a
    (_, _), (d1x, d1y), (d2x, d2y), (x2, y2) = b
    return f"M{x0} {y0} C{c1x} {c1y} {c2x} {c2y} {x1} {y1} C{d1x} {d1y} {d2x} {d2y} {x2} {y2}"


svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">',
    f'<circle cx="256" cy="256" r="{DISC_R}" fill="{BG}"/>',
    f'<circle cx="256" cy="256" r="{DISC_R}" fill="none" stroke="{TAN}" stroke-width="{RING_W}"/>',
]
for a, b, colour, width in WAVES:
    svg.append(f'<path d="{curve_path(a, b)}" fill="none" stroke="{colour}" stroke-width="{width}" stroke-linecap="round"/>')
svg.append(f'<polygon points="{" ".join(f"{x},{y}" for x, y in BOLT)}" fill="{CREAM}"/>')
svg.append("</svg>")
text = "\n".join(svg) + "\n"
for rel in ("ui/public/logo.svg", "ui/public/favicon.svg"):
    open(os.path.join(ROOT, rel), "w", encoding="utf-8", newline="\n").write(text)

# ---- PNG: drawn large, then scaled down -----------------------------------

SIZE = 1024
K = SIZE * 3 / 512  # canvas units to pixels at the drawing size


def bezier(p, steps=64):
    (x0, y0), (x1, y1), (x2, y2), (x3, y3) = p
    out = []
    for i in range(steps + 1):
        t = i / steps
        u = 1 - t
        out.append((u**3 * x0 + 3 * u * u * t * x1 + 3 * u * t * t * x2 + t**3 * x3, u**3 * y0 + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t**3 * y3))
    return out


img = Image.new("RGBA", (SIZE * 3, SIZE * 3), (0, 0, 0, 0))
g = ImageDraw.Draw(img)


def disc(cx, cy, r, colour):
    g.ellipse([(cx - r) * K, (cy - r) * K, (cx + r) * K, (cy + r) * K], fill=colour)


# The ring is the tan disc with the dark one inside it.
disc(256, 256, DISC_R + RING_W / 2, TAN)
disc(256, 256, DISC_R - RING_W / 2, BG)
for a, b, colour, width in WAVES:
    # A run of overlapping discs: a thick polyline leaves slivers at its
    # joints, and discs give the round caps the SVG has for free.
    for x, y in bezier(a, 400) + bezier(b, 400):
        disc(x, y, width / 2, colour)
g.polygon([(x * K, y * K) for x, y in BOLT], fill=CREAM)

img.resize((SIZE, SIZE), Image.LANCZOS).save(OUT_PNG)
print(OUT_PNG, os.path.getsize(OUT_PNG))
