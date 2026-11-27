"""Templates use only classes the built stylesheet defines, and never inline styles or built class names."""

import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "check_templates.py"
CSS = (
    r".flex{display:flex}.sm\:py-32{}.text-sm\/6{}.w-144\.5{}:is(.\*\:pl-6>*){}.left-\[calc\(50\%-11rem\)\]{}"
)


class TemplateCheckTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: run(["rm", "-rf", self.root], ROOT))
        write(self.root / "built.css", CSS)

    def check(self, html):
        write(self.root / "templates" / "page.html", html)
        return run(["python3", SCRIPT, self.root / "built.css", self.root / "templates" / "page.html"], ROOT)

    def assert_fails(self, html, *words):
        result = self.check(html)
        self.assertEqual(result.returncode, 1, result.stderr)
        for word in words:
            self.assertIn(word, result.stderr)

    def test_classes_the_stylesheet_defines_pass_however_tailwind_escapes_them(self):
        html = '<div class="flex sm:py-32 text-sm/6 w-144.5 *:pl-6 left-[calc(50%-11rem)]">x</div>'
        result = self.check(html)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_class_the_stylesheet_lacks_fails_with_its_line(self):
        self.assert_fails('<p>\n<div class="flex text-acent">x</div>', "page.html:2", "text-acent")

    def test_marker_classes_need_no_rule(self):
        self.assertEqual(self.check('<div class="group peer flex">x</div>').returncode, 0)

    def test_an_inline_style_fails(self):
        self.assert_fails('<div style="color: red">x</div>', "style=")

    def test_a_style_element_fails(self):
        self.assert_fails("<head>\n<style>p { color: red }</style>\n</head>", "page.html:2", "<style>")

    def test_a_single_quoted_class_is_read(self):
        self.assert_fails("<div class='flex nope'>x</div>", "nope")

    def test_an_expression_inside_a_class_fails(self):
        self.assert_fails('<div class="flex {{ color }}">x</div>', "{{")

    def test_tags_inside_a_class_are_read_for_both_branches(self):
        html = '<div class="{% if demo %}flex{% else %}sm:py-32{% endif %}">x</div>'
        self.assertEqual(self.check(html).returncode, 0)
        self.assert_fails('<div class="{% if demo %}flex{% else %}nope{% endif %}">x</div>', "nope")

    def test_a_transform_utility_inside_starting_fails(self):
        # WebKit ignores Tailwind's registered transform variables in @starting-style, so nothing slides.
        for name in [
            "starting:open:-translate-x-full",
            "md:starting:scale-95",
            "starting:rotate-3",
            "starting:[&:popover-open]:-translate-x-full",
            "popovers:[&:not(:popover-open)]:starting:scale-95",
            "group-hover/menu:starting:scale-95",
            "starting:open:!-translate-x-full",
            "starting:[@media(x)]:rotate-3",
        ]:
            self.assert_fails(f'<div class="{name}">', name, "animates nothing in WebKit")

    def test_a_plain_transform_inside_starting_passes_the_rule(self):
        for name in [
            "starting:open:[translate:-100%_0]",
            "starting:open:opacity-0",
            "open:-translate-x-full",
        ]:
            result = self.check(f'<div class="{name}">')
            self.assertNotIn("animates nothing", result.stderr, name)

    def test_a_closing_transition_needs_a_scripted_close(self):
        # Only Chromium plays a popover's closing transition; WebKit hides it at once, unless by script.
        self.assert_fails('<div class="motion-safe:transition-discrete">', "only Chromium plays")
        result = self.check(
            '<div class="motion-safe:transition-discrete [&[data-closing]]:[translate:-100%_0]">'
        )
        self.assertNotIn("only Chromium plays", result.stderr)

    def test_a_class_split_over_lines_is_read(self):
        self.assert_fails('<div class="flex\n  nope">x</div>', "nope")


if __name__ == "__main__":
    unittest.main()
