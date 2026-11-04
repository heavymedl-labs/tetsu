#!/usr/bin/env python3
"""Generates the Tetsu brand SVGs in docs/brand: booked time slots on an open
schedule grid.

usage: scripts/generate-brand.py [output-dir]

PNG exports for uploading are rendered from the SVGs; see docs/brand/README.md.
"""
import sys
out = sys.argv[1] if len(sys.argv) > 1 else "docs/brand"
GLYPHS = {"t": [".X.", "XXX", ".X.", ".X.", ".XX"], "e": ["XXX", "X.X", "XXX", "X..", "XXX"],
          "s": ["XXX", "X..", "XXX", "..X", "XXX"], "u": ["X.X", "X.X", "X.X", "X.X", "XXX"]}
THEMES = {"light": ("#3A4250", "#E5622B", "#3A4250"), "dark": ("#D9DEE5", "#F07A45", "#D9DEE5")}

def cells(word="tetsu"):
    """Yields (col, row, state) over the word's grid; state is on, hot, or empty."""
    for col in range(len(word) * 4 - 1):
        li, lc = divmod(col, 4)
        for row in range(5):
            on = lc < 3 and GLYPHS[word[li]][row][lc] == "X"
            hot = li == 0 and row == 1 and lc == 2
            yield col, row, ("hot" if hot else "on") if on else "empty"

def slot(x, y, size, state, on, hot, ghost, ghost_opacity, rx):
    if state == "empty":
        w = 1.5
        return (f'<rect x="{x + w/2:g}" y="{y + w/2:g}" width="{size - w:g}" height="{size - w:g}" rx="{rx}" '
                f'fill="none" stroke="{ghost}" stroke-opacity="{ghost_opacity}" stroke-width="{w}"/>')
    return f'<rect x="{x:g}" y="{y:g}" width="{size:g}" height="{size:g}" rx="{rx}" fill="{hot if state == "hot" else on}"/>'

def svg(w, h, body, title):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" '
            f'aria-labelledby="t"><title id="t">{title}</title>\n{body}\n</svg>\n')

def wordmark(theme):
    on, hot, ghost = THEMES[theme]
    size, pitch = 24, 30
    body = "\n".join(slot(c * pitch, r * pitch, size, s, on, hot, ghost, 0.22, 3) for c, r, s in cells())
    return svg(19 * pitch - 6, 5 * pitch - 6, body, "tetsu")

def avatar():
    t = [[1, 1, 1], [0, 1, 0], [0, 1, 0]]
    parts = ['<rect width="500" height="500" fill="#3A4250"/>']
    for r in range(3):
        for c in range(3):
            state = ("hot" if (r, c) == (0, 2) else "on") if t[r][c] else "empty"
            parts.append(slot(80 + c * 120, 80 + r * 120, 100, state, "#F4F2EE", "#E5622B", "#F4F2EE", 0.28, 15))
    return svg(500, 500, "\n".join(parts), "Tetsu")

def social():
    on, hot, ghost = THEMES["dark"]
    size, pitch = 32, 40
    x0, y0 = (1280 - (19 * pitch - 8)) // 2, (640 - (5 * pitch - 8)) // 2
    word = {(c, r): s for c, r, s in cells()}
    parts = ['<rect width="1280" height="640" fill="#16181C"/>']
    for r in range(-7, 12):
        for c in range(-8, 27):
            x, y = x0 + c * pitch, y0 + r * pitch
            if x + size < 0 or y + size < 0 or x > 1280 or y > 640:
                continue
            state = word.get((c, r), "empty")
            near = 0 <= c < 19 and 0 <= r < 5
            parts.append(slot(x, y, size, state, on, hot, ghost, 0.2 if near else 0.08, 4))
    return svg(1280, 640, "\n".join(parts), "Tetsu")

files = {"tetsu-wordmark-light.svg": wordmark("light"), "tetsu-wordmark-dark.svg": wordmark("dark"),
         "tetsu-avatar.svg": avatar(), "tetsu-social-preview.svg": social()}
for name, text in files.items():
    open(f"{out}/{name}", "w").write(text)
    print(name, len(text))
