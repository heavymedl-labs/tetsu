"""The brand generator draws every slot grid from one set of glyphs, one geometry, and one
booking order, and the templates it writes are the ones committed."""

import importlib.util
import re
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT

SPEC = importlib.util.spec_from_file_location("generate_brand", ROOT / "scripts" / "generate_brand.py")
brand = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(brand)

RECT = re.compile(r'<rect x="(?P<x>[\d.]+)" y="(?P<y>[\d.]+)"[^>]*class="(?P<class>[^"]+)"/>')


def rects(text):
    return [(float(m["x"]), float(m["y"]), m["class"]) for m in RECT.finditer(text)]


class TemplateSlotsTest(unittest.TestCase):
    def test_a_booked_slot_books_in_45ms_a_column_and_15ms_a_row_after_the_first(self):
        for x, y, classes in rects(brand.template_slots("tetsu")):
            column, row = int(x) // 30, int(y) // 30
            self.assertIn(f"[animation-delay:{column * 45 + row * 15}ms]", classes)

    def test_one_slot_runs_hot_in_forge_orange_and_the_rest_take_the_text_color(self):
        for word, frame, booked in [("tetsu", 0, 50), ("404", 1, 30)]:
            classes = [c for _, _, c in rects(brand.template_slots(word, frame))]
            self.assertEqual(len(classes), booked, word)
            hot = [c for c in classes if "tetsu-hot" in c]
            self.assertEqual(len(hot), 1, word)
            self.assertIn("fill-forge", hot[0])
            self.assertTrue(all("fill-current" in c for c in classes if c not in hot), word)

    def test_open_cells_are_ghosts_inset_by_half_their_stroke_and_fill_the_rest_of_the_grid(self):
        for word, frame, columns in [("tetsu", 0, 19), ("404", 1, 13)]:
            ghosts = rects(brand.template_ghosts(word, frame))
            booked = rects(brand.template_slots(word, frame))
            self.assertTrue(all(c == "tetsu-ghost" for _, _, c in ghosts), word)
            self.assertTrue(all(x % 30 == 0.75 and y % 30 == 0.75 for x, y, _ in ghosts), word)
            cells = {(int(x) // 30, int(y) // 30) for x, y, _ in ghosts + booked}
            self.assertEqual(len(ghosts) + len(booked), columns * 5, word)
            self.assertEqual(cells, {(c, r) for c in range(columns) for r in range(5)}, word)

    def test_404_is_drawn_from_its_glyphs_one_open_column_in(self):
        booked = {(int(x) // 30, int(y) // 30) for x, y, _ in rects(brand.template_slots("404", 1))}
        rows = ["#.#.###.#.#", "#.#.#.#.#.#", "###.#.#.###", "..#.#.#...#", "..#.###...#"]
        drawn = {(c + 1, r) for r, row in enumerate(rows) for c, cell in enumerate(row) if cell == "#"}
        self.assertEqual(booked, drawn)

    def test_the_committed_template_is_what_the_generator_writes(self):
        with tempfile.TemporaryDirectory() as scratch:
            written = Path(scratch) / "brand.html"
            brand.write_template(written)
            committed = (ROOT / "web" / "templates" / "brand.html").read_text()
            self.assertEqual(written.read_text(), committed, "run scripts/generate_brand.py")


if __name__ == "__main__":
    unittest.main()
