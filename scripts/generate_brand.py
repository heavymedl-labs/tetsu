#!/usr/bin/env python3
"""Generates the Tetsu brand SVGs in docs/brand, and the slots the pages draw in
web/templates/brand.html: booked time slots on an open schedule grid.

usage: scripts/generate_brand.py [output-dir]

PNG exports for uploading are rendered from the SVGs; see docs/brand/README.md.
"""

import sys
from pathlib import Path
from typing import NamedTuple

GLYPHS = {
    "t": [".X.", "XXX", ".X.", ".X.", ".XX"],
    "e": ["XXX", "X.X", "XXX", "X..", "XXX"],
    "s": ["XXX", "X..", "XXX", "..X", "XXX"],
    "u": ["X.X", "X.X", "X.X", "X.X", "XXX"],
    "4": ["X.X", "X.X", "XXX", "..X", "..X"],
    "0": ["XXX", "X.X", "X.X", "X.X", "XXX"],
}
GLYPH_WIDTH, GLYPH_HEIGHT, GLYPH_PITCH = 3, 5, 4
WORD_COLUMNS = 19  # five glyphs of 3 columns, with a gap after each but the last
TEMPLATE = Path(__file__).resolve().parent.parent / "web" / "templates" / "brand.html"
PITCH, SIZE = 30, 24  # the pages' grid: a slot every 30 pixels, 24 across
HOT_ROW, HOT_COLUMN = 1, 2  # the booked slot: the top right of the first letter
SOCIAL_WIDTH, SOCIAL_HEIGHT = 1280, 640
GHOST_STROKE = 1.5


class Style(NamedTuple):
    """How to draw a slot: its colors, the ghost outline's opacity, and its corner radius."""

    on: str
    hot: str
    ghost: str
    ghost_opacity: float
    rx: int


THEMES = {"light": ("#3A4250", "#E5622B", "#3A4250"), "dark": ("#D9DEE5", "#F07A45", "#D9DEE5")}


def cells(word="tetsu"):
    """Yields (col, row, state) over the word's grid; state is on, hot, or empty."""
    for col in range(len(word) * GLYPH_PITCH - 1):
        letter, column = divmod(col, GLYPH_PITCH)
        for row in range(GLYPH_HEIGHT):
            on = column < GLYPH_WIDTH and GLYPHS[word[letter]][row][column] == "X"
            hot = letter == 0 and row == HOT_ROW and column == HOT_COLUMN
            yield col, row, ("hot" if hot else "on") if on else "empty"


def slot(x, y, size, state, style):
    if state == "empty":
        w = GHOST_STROKE
        return (
            f'<rect x="{x + w / 2:g}" y="{y + w / 2:g}" width="{size - w:g}" height="{size - w:g}" '
            f'rx="{style.rx}" fill="none" stroke="{style.ghost}" stroke-opacity="{style.ghost_opacity}" '
            f'stroke-width="{w}"/>'
        )
    fill = style.hot if state == "hot" else style.on
    return f'<rect x="{x:g}" y="{y:g}" width="{size:g}" height="{size:g}" rx="{style.rx}" fill="{fill}"/>'


def svg(w, h, body, title):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" '
        f'aria-labelledby="t"><title id="t">{title}</title>\n{body}\n</svg>\n'
    )


def wordmark(theme):
    style = Style(*THEMES[theme], ghost_opacity=0.22, rx=3)
    size, pitch = 24, 30
    body = "\n".join(slot(c * pitch, r * pitch, size, s, style) for c, r, s in cells())
    return svg(WORD_COLUMNS * pitch - 6, GLYPH_HEIGHT * pitch - 6, body, "tetsu")


def avatar():
    t = [[1, 1, 1], [0, 1, 0], [0, 1, 0]]
    style = Style("#F4F2EE", "#E5622B", "#F4F2EE", ghost_opacity=0.28, rx=15)
    parts = ['<rect width="500" height="500" fill="#3A4250"/>']
    for r in range(3):
        for c in range(3):
            state = ("hot" if (r, c) == (0, 2) else "on") if t[r][c] else "empty"
            parts.append(slot(80 + c * 120, 80 + r * 120, 100, state, style))
    return svg(500, 500, "\n".join(parts), "Tetsu")


def social():
    size, pitch = 32, 40
    x0 = (SOCIAL_WIDTH - (WORD_COLUMNS * pitch - 8)) // 2
    y0 = (SOCIAL_HEIGHT - (GLYPH_HEIGHT * pitch - 8)) // 2
    word = {(c, r): s for c, r, s in cells()}
    near_style = Style(*THEMES["dark"], ghost_opacity=0.2, rx=4)
    far_style = near_style._replace(ghost_opacity=0.08)
    parts = [f'<rect width="{SOCIAL_WIDTH}" height="{SOCIAL_HEIGHT}" fill="#16181C"/>']
    for r in range(-7, 12):
        for c in range(-8, 27):
            x, y = x0 + c * pitch, y0 + r * pitch
            if x + size < 0 or y + size < 0 or x > SOCIAL_WIDTH or y > SOCIAL_HEIGHT:
                continue
            near = 0 <= c < WORD_COLUMNS and 0 <= r < GLYPH_HEIGHT
            parts.append(slot(x, y, size, word.get((c, r), "empty"), near_style if near else far_style))
    return svg(SOCIAL_WIDTH, SOCIAL_HEIGHT, "\n".join(parts), "Tetsu")


def framed(word, frame):
    """A word's cells with `frame` open columns on each side."""
    width = len(word) * GLYPH_PITCH - 1
    for c in range(frame):
        for r in range(GLYPH_HEIGHT):
            yield c, r, "empty"
            yield c + frame + width, r, "empty"
    for c, r, state in cells(word):
        yield c + frame, r, state


def template_slots(word, frame=0):
    """A word's booked slots for the pages, booking in from left to right, 45ms a column and 15ms a row."""
    lines = []
    for c, r, state in framed(word, frame):
        if state == "empty":
            continue
        color = "tetsu-hot fill-forge" if state == "hot" else "fill-current"
        lines.append(
            f'<rect x="{c * PITCH}" y="{r * PITCH}" width="{SIZE}" height="{SIZE}" rx="3" '
            f'class="tetsu-slot {color} [animation-delay:{c * 45 + r * 15}ms]"/>'
        )
    return "\n".join(lines)


def template_ghosts(word, frame=0):
    """A word's open slots for the pages: outlines in the text color, inset by half their stroke."""
    w = GHOST_STROKE
    return "\n".join(
        f'<rect x="{c * PITCH + w / 2:g}" y="{r * PITCH + w / 2:g}" '
        f'width="{SIZE - w:g}" height="{SIZE - w:g}" rx="3" fill="none" stroke="currentColor" '
        f'stroke-width="{w:g}" class="tetsu-ghost"/>'
        for c, r, state in sorted(framed(word, frame))
        if state == "empty"
    )


HEADER = """{#- Generated by scripts/generate_brand.py; do not edit. The brand's slots for the pages, in
    the text color and one running hot in forge orange: the wordmark's (slots, ghosts), which the hero
    board also books, and the not-found page's 404 (lost_slots, lost_ghosts). Booked slots book in from
    left to right (motion.css). A test in serve.rs checks the wordmark's against the brand file. -#}
"""


def write_template(path=TEMPLATE):
    macros = [("slots", template_slots("tetsu")), ("ghosts", template_ghosts("tetsu"))]
    # The 404 sits in a column of open slots on each side, a little of the schedule around it.
    macros += [("lost_slots", template_slots("404", 1)), ("lost_ghosts", template_ghosts("404", 1))]
    body = "".join(f"{{% macro {name}() %}}\n{text}\n{{% endmacro %}}\n" for name, text in macros)
    path.write_text(HEADER + body)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/brand")
    files = {
        "tetsu-wordmark-light.svg": wordmark("light"),
        "tetsu-wordmark-dark.svg": wordmark("dark"),
        "tetsu-avatar.svg": avatar(),
        "tetsu-social-preview.svg": social(),
    }
    for name, text in files.items():
        (out / name).write_text(text)
        print(name, len(text))
    write_template()
    print(TEMPLATE.name)


if __name__ == "__main__":
    main()
