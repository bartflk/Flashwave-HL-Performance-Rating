"""The HL Rating logo: an original badge in TF2's colours (not Valve's logo).

Writes ui/public/logo.svg and favicon.svg, and target/hl-logo-1024.png
(pure Python, anti-aliased by signed distance). Then make every app icon:

    python scripts/logo.py
    npx tauri icon target/hl-logo-1024.png

and delete the android/, ios/ and Square*/StoreLogo files it adds.
"""
import math, os, struct, zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_PNG = os.path.join(ROOT, "target", "hl-logo-1024.png")
os.makedirs(os.path.dirname(OUT_PNG), exist_ok=True)

BG = (0x2B, 0x24, 0x20)      # dark warm brown
RING = (0xE0, 0x76, 0x3A)    # TF2 orange
CREAM = (0xF2, 0xE6, 0xD9)
# Geometry on a 512 canvas, centred at (256, 256).
R_OUT, R_RING_IN = 244, 212          # badge edge, inner edge of the orange ring
TICKS = [(0, -1), (0, 1), (-1, 0), (1, 0)]
TICK_FROM, TICK_TO, TICK_W = 150, 196, 22
BARS = [(-78, 70, CREAM), (-22, 112, CREAM), (34, 156, RING)]  # (left x, height, colour)
BAR_W, BAR_BASE = 44, 330            # bars stand on y = 330

svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">',
    f'<circle cx="256" cy="256" r="{R_OUT}" fill="#{RING[0]:02x}{RING[1]:02x}{RING[2]:02x}"/>',
    f'<circle cx="256" cy="256" r="{R_RING_IN}" fill="#{BG[0]:02x}{BG[1]:02x}{BG[2]:02x}"/>',
]
for dx, dy in TICKS:
    if dx == 0:
        y0 = 256 + dy * TICK_FROM if dy > 0 else 256 - TICK_TO
        svg.append(f'<rect x="{256 - TICK_W // 2}" y="{y0}" width="{TICK_W}" height="{TICK_TO - TICK_FROM}" rx="{TICK_W // 2}" fill="#f2e6d9"/>')
    else:
        x0 = 256 + TICK_FROM if dx > 0 else 256 - TICK_TO
        svg.append(f'<rect x="{x0}" y="{256 - TICK_W // 2}" width="{TICK_TO - TICK_FROM}" height="{TICK_W}" rx="{TICK_W // 2}" fill="#f2e6d9"/>')
for x, h, c in BARS:
    svg.append(f'<rect x="{256 + x}" y="{BAR_BASE - h}" width="{BAR_W}" height="{h}" rx="8" fill="#{c[0]:02x}{c[1]:02x}{c[2]:02x}"/>')
svg.append("</svg>")
text = "\n".join(svg) + "\n"
for rel in ("ui/public/logo.svg", "ui/public/favicon.svg"):
    open(os.path.join(ROOT, rel), "w", encoding="utf-8", newline="\n").write(text)


def rrect_sdf(px, py, x0, y0, w, h, r):
    cx, cy = x0 + w / 2, y0 + h / 2
    qx, qy = abs(px - cx) - (w / 2 - r), abs(py - cy) - (h / 2 - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


shapes = []  # (sdf function, colour), painted in order
shapes.append((lambda x, y: math.hypot(x - 256, y - 256) - R_OUT, RING))
shapes.append((lambda x, y: math.hypot(x - 256, y - 256) - R_RING_IN, BG))
for dx, dy in TICKS:
    if dx == 0:
        y0 = 256 + TICK_FROM if dy > 0 else 256 - TICK_TO
        shapes.append(((lambda y0: lambda x, y: rrect_sdf(x, y, 256 - TICK_W / 2, y0, TICK_W, TICK_TO - TICK_FROM, TICK_W / 2))(y0), CREAM))
    else:
        x0 = 256 + TICK_FROM if dx > 0 else 256 - TICK_TO
        shapes.append(((lambda x0: lambda x, y: rrect_sdf(x, y, x0, 256 - TICK_W / 2, TICK_TO - TICK_FROM, TICK_W, TICK_W / 2))(x0), CREAM))
for bx, h, c in BARS:
    shapes.append(((lambda bx, h: lambda x, y: rrect_sdf(x, y, 256 + bx, BAR_BASE - h, BAR_W, h, 8))(bx, h), c))

N = 1024
scale = 512 / N  # canvas units per pixel
rows = []
for j in range(N):
    row = bytearray([0])
    y = (j + 0.5) * scale
    for i in range(N):
        x = (i + 0.5) * scale
        r = g = b = a = 0.0
        for sdf, col in shapes:
            d = sdf(x, y) / scale  # in pixels
            cov = 0.5 - d
            if cov <= 0:
                continue
            cov = min(cov, 1.0)
            # Source-over, premultiplied.
            r = col[0] * cov + r * (1 - cov)
            g = col[1] * cov + g * (1 - cov)
            b = col[2] * cov + b * (1 - cov)
            a = cov + a * (1 - cov)
        if a > 0:
            row += bytes((round(r / a), round(g / a), round(b / a), round(a * 255)))
        else:
            row += b"\0\0\0\0"
    rows.append(bytes(row))


def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
open(OUT_PNG, "wb").write(png)
print(OUT_PNG, len(png))
