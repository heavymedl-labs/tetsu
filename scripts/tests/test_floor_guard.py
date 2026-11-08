"""The floor guard must fail each floor violation and pass a clean control."""

import contextlib
import io
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock

from scripts import floor_guard
from scripts.tests.support import ROOT, run, write

GUARD = ROOT / "scripts" / "floor_guard.py"
CONSTRAINTS = """# Constraints

## Floor

- No secrets in source.
- No stubs.

## Checks by language

| Language | Coverage |
| --- | --- |
| Rust | cargo-mutants |

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
| --- | --- | --- | --- | --- | --- |
"""


class GuardCase(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.repo = Path(tmp.name)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "test@example.com")
        self.git("config", "user.name", "Test")
        write(self.repo / "CONSTRAINTS.md", CONSTRAINTS)
        write(self.repo / "src" / "lib.rs", "fn main() {}\n")
        write(self.repo / "tests" / "test_a.py", "def test_a():\n    assert 1 == 1\n")
        self.commit("base")
        self.git("tag", "base")

    def git(self, *args):
        result = run(["git", *args], self.repo)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message)

    def guard(self, base="base"):
        """Runs the guard in this process, which skips starting Python for every case, with what a separate
        process would get: no GIT_* variables, the test repository as the working directory, and its own
        arguments and output."""
        out, err, cwd = io.StringIO(), io.StringIO(), Path.cwd()
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        argv = ["floor_guard.py", "--base", base]
        try:
            os.chdir(self.repo)
            with (
                mock.patch.dict(os.environ, env, clear=True),
                mock.patch.object(sys, "argv", argv),
                contextlib.redirect_stdout(out),
                contextlib.redirect_stderr(err),
            ):
                code = floor_guard.main()
        finally:
            os.chdir(cwd)
        return types.SimpleNamespace(returncode=code, stdout=out.getvalue(), stderr=err.getvalue())

    def assertFlags(self, rule):
        result = self.guard()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(f"[{rule}]", result.stderr)


class CleanControl(GuardCase):
    def test_no_changes_pass(self):
        self.assertEqual(self.guard().returncode, 0)

    def test_ordinary_code_passes(self):
        write(self.repo / "src" / "lib.rs", 'fn main() { println!("hi"); }\n')
        self.assertEqual(self.guard().returncode, 0)

    def test_docs_may_mention_the_patterns(self):
        write(self.repo / "docs.md", "Never use `#[allow]`, `.skip`, or TODO.\n")
        self.assertEqual(self.guard().returncode, 0)

    def test_docs_with_spaces_in_the_name_may_mention_the_patterns(self):
        write(self.repo / "my doc.md", "Old\n")
        self.commit("add doc")
        self.git("tag", "-f", "base")
        write(self.repo / "my doc.md", "Old\nNo TODO here.\n")
        self.assertEqual(self.guard().returncode, 0)

    def test_tightening_with_a_new_rule_passes(self):
        path = self.repo / "CONSTRAINTS.md"
        path.write_text(path.read_text().replace("- No stubs.\n", "- No stubs.\n- No globals.\n"))
        self.assertEqual(self.guard().returncode, 0)

    def test_removing_an_exception_passes(self):
        path = self.repo / "CONSTRAINTS.md"
        path.write_text(path.read_text() + "| E1 | Tests | a.py | One-off | Maintainer | 2027-01-01 |\n")
        self.commit("add exception\n\nFloor-reason: test setup")
        self.git("tag", "-f", "base")
        path.write_text(
            path.read_text().replace("| E1 | Tests | a.py | One-off | Maintainer | 2027-01-01 |\n", "")
        )
        self.assertEqual(self.guard().returncode, 0)


class Suppressions(GuardCase):
    def test_flags_each_suppression_form(self):
        for line in (
            "#[allow(dead_code)]",
            "#![allow(unused)]",
            "x = 1  # noqa",
            "# shellcheck disable=SC2086",
            "// eslint-disable-next-line",
            "# flake8: noqa",
            "x = 1  # NOQA",
            "x = 1  # nosec",
            "x = 1  # pyright: ignore",
            "// @ts-expect-error",
            "-- squawk-ignore require-concurrent-index-creation",
            "SELECT 1 -- noqa: LT01",
            "#[cfg_attr(test, allow(dead_code))]",
            "    continue-on-error: true",
            "#[allow(dead_code)] // no reason",
            'exclude_re = ["replace foo"]',
            'exclude_globs = ["crates/**"]',
            'examine_globs = ["src/one.rs"]',
            "#[mutants::skip]",
            "#[cfg(not(coverage))]",
            "#[coverage(off)]",
            "// #lizard forgives",
            "// #lizard forgive",
            "// jscpd:ignore-start",
            "#[rustfmt::skip]",
            "#![rustfmt::skip]",
        ):
            with self.subTest(line=line):
                write(self.repo / "src" / "a.rs", line + "\n")
                self.assertFlags("silenced-checker")

    def test_no_verify_cannot_be_excused_by_a_reason(self):
        write(self.repo / "src" / "a.sh", "git push --no-verify  # reason: hurry\n")
        self.assertFlags("silenced-checker")

    def test_markdown_suppressions_are_flagged(self):
        write(self.repo / "doc.md", "<!-- markdownlint-disable MD041 -->\n")
        self.assertFlags("silenced-checker")

    def test_a_reason_on_the_line_passes(self):
        write(self.repo / "src" / "a.rs", '#[allow(dead_code, reason = "used by the macro")]\n')
        self.assertEqual(self.guard().returncode, 0)


class Stubs(GuardCase):
    def test_flags_each_stub_form(self):
        for line in (
            "todo!()",
            "unimplemented!()",
            "raise NotImplementedError",
            "// TODO finish this",
            "catch (e) {}",
            "except Exception: pass",
            "Err(_) => {}",
            "if let Err(_) = x {}",
            "Err(_) => (),",
            "Err(_error) => ()",
            "Err(..) => {}",
            "Err(..) => (),",
        ):
            with self.subTest(line=line):
                write(self.repo / "src" / "a.rs", line + "\n")
                self.assertFlags("unfinished-work")


class Handlers(GuardCase):
    def test_flags_except_then_pass_on_two_lines(self):
        write(self.repo / "src" / "a.py", "try:\n    x()\nexcept Exception:\n    pass\n")
        self.assertFlags("unfinished-work")

    def test_pass_elsewhere_is_fine(self):
        write(self.repo / "src" / "a.py", "class A:\n    pass\n")
        self.assertEqual(self.guard().returncode, 0)


class Tests(GuardCase):
    def test_flags_added_skips(self):
        for line in (
            "#[ignore]",
            "it.skip('x')",
            "@unittest.skip('later')",
            "@pytest.mark.skip",
            "self.skipTest('later')",
            "@skipIf(True, 'x')",
            "@unittest.expectedFailure",
            "pytest.skip('x')",
            "it.only('x')",
            "#[cfg_attr(not(ci), ignore)]",
            '#[cfg_attr(windows, ignore = "flaky")]',
            "#[cfg(any())]",
            "#![cfg(any())]",
            "#[cfg(any( ))]",
        ):
            with self.subTest(line=line):
                write(self.repo / "tests" / "test_b.py", line + "\n")
                self.assertFlags("test-made-easier")

    def test_flags_a_deleted_test_file(self):
        (self.repo / "tests" / "test_a.py").unlink()
        self.assertFlags("test-deleted")

    def test_flags_a_removed_assertion(self):
        write(self.repo / "tests" / "test_a.py", "def test_a():\n    pass\n")
        self.assertFlags("assertion-removed")

    def test_flags_a_test_moved_out_of_the_test_folder(self):
        self.git("mv", "tests/test_a.py", "src/a.py")
        self.assertFlags("test-deleted")

    def test_flags_removed_rust_inline_tests(self):
        write(self.repo / "src" / "lib.rs", "#[test]\nfn a() { assert_eq!(1, 1); }\n")
        self.commit("add inline test")
        self.git("tag", "-f", "base")
        write(self.repo / "src" / "lib.rs", "\n")
        self.assertFlags("assertion-removed")

    def test_removing_lines_from_other_files_passes(self):
        write(self.repo / "src" / "lib.rs", "\n")
        self.assertEqual(self.guard().returncode, 0)


class Constraints(GuardCase):
    def edit(self, old, new):
        path = self.repo / "CONSTRAINTS.md"
        text = path.read_text()
        self.assertIn(old, text)
        path.write_text(text.replace(old, new))

    def test_flags_a_removed_rule(self):
        self.edit("- No stubs.\n", "")
        self.assertFlags("rule-removed")

    def test_flags_a_weakened_row(self):
        self.edit("| Rust | cargo-mutants |", "| Rust | Gap |")
        self.assertFlags("rule-removed")

    def test_flags_removed_prose(self):
        self.edit("## Floor\n", "")
        self.assertFlags("rule-removed")

    def test_flags_a_renamed_constraints_file(self):
        self.git("mv", "CONSTRAINTS.md", "old.txt")
        self.assertFlags("rule-removed")

    def test_flags_an_exception_with_any_id(self):
        self.edit(
            "| --- | --- | --- | --- | --- | --- |\n",
            "| --- | --- | --- | --- | --- | --- |\n| ex-1 | Tests | a.py | Why | Me | 2027-01-01 |\n",
        )
        self.assertFlags("new-exception")

    def test_flags_a_new_exception(self):
        self.edit(
            "| --- | --- | --- | --- | --- | --- |\n",
            "| --- | --- | --- | --- | --- | --- |\n| E1 | Tests | a.py | Why | Me | 2027-01-01 |\n",
        )
        self.assertFlags("new-exception")


class GuardProtectsItself(GuardCase):
    def test_flags_removed_lines_in_the_check_files(self):
        for path in (
            "scripts/check.sh",
            "scripts/floor_guard.py",
            "scripts/check_coverage.py",
            ".githooks/pre-push",
            ".githooks/pre-commit",
            ".github/workflows/ci.yml",
            "clippy.toml",
            "ruff.toml",
            ".squawk.toml",
            ".sqlfluff",
            ".editorconfig",
            "Cargo.toml",
            ".cargo/mutants.toml",
            "scripts/check_doc_sizes.py",
            "scripts/check-adrs.sh",
            ".markdownlint-cli2.yaml",
        ):
            with self.subTest(path=path):
                write(self.repo / path, "one\ntwo\n")
                self.commit("add " + path)
                self.git("tag", "-f", "base")
                write(self.repo / path, "one\n")
                self.assertFlags("guard-weakened")
                write(self.repo / path, "one\ntwo\n")


class LintConfig(GuardCase):
    def test_flags_a_new_lint_config_file(self):
        for path in (
            "crates/a/clippy.toml",
            "crates/a/.clippy.toml",
            ".ruff.toml",
            "scripts/ruff.toml",
            "pyproject.toml",
            "rustfmt.toml",
            "crates/a/.markdownlint.yaml",
            ".markdownlint-cli2.jsonc",
            "crates/a/.sqlfluff",
            "crates/a/.cargo/mutants.toml",
            ".cargo/config.toml",
            "crates/a/.cargo/config",
            ".shellcheckrc",
            ".jscpd.json",
            "whitelizard.txt",
            ".sqlfluffignore",
            "setup.cfg",
            "tox.ini",
        ):
            with self.subTest(path=path):
                write(self.repo / path, "x = 1\n")
                self.assertFlags("config-added")
                (self.repo / path).unlink()

    def test_flags_a_loosening_line_added_to_lint_config(self):
        for path, line in (
            (".markdownlint-cli2.yaml", "config: {default: false}"),
            ("ruff.toml", 'ignore = ["E501"]'),
            (".cargo/mutants.toml", 'exclude_re = ["x"]'),
            ("Cargo.toml", 'unwrap_used = "allow"'),
            ("crates/a/Cargo.toml", 'panic = { level = "allow", priority = 1 }'),
            ("Cargo.toml", "missing_errors_doc = 'allow'"),
        ):
            with self.subTest(path=path):
                write(self.repo / path, "one\n")
                self.commit("add " + path)
                self.git("tag", "-f", "base")
                write(self.repo / path, "one\n" + line + "\n")
                self.assertFlags("guard-weakened")

    def test_tightening_lint_config_passes(self):
        write(self.repo / "clippy.toml", "one\n")
        self.commit("add clippy.toml")
        self.git("tag", "-f", "base")
        write(self.repo / "clippy.toml", 'one\ndisallowed-names = ["foo"]\n')
        self.assertEqual(self.guard().returncode, 0)


class CrateTier(GuardCase):
    def setUp(self):
        super().setUp()
        self.manifest = self.repo / "crates" / "a" / "Cargo.toml"
        write(self.manifest, '[package.metadata.tetsu]\nkind = "platform"\ntier = 1\n')
        self.commit("add crate")
        self.git("tag", "-f", "base")

    def test_flags_a_changed_tier(self):
        self.manifest.write_text(self.manifest.read_text().replace("tier = 1", "tier = 3"))
        self.assertFlags("tier-changed")

    def test_flags_a_changed_kind(self):
        self.manifest.write_text(self.manifest.read_text().replace('"platform"', '"root"'))
        self.assertFlags("tier-changed")

    def test_flags_dropped_workspace_lints(self):
        self.manifest.write_text(self.manifest.read_text() + "\n[lints]\nworkspace = true\n")
        self.commit("add lints")
        self.git("tag", "-f", "base")
        for dropped in ("workspace = true\n", "[lints]\nworkspace = true\n"):
            with self.subTest(dropped=dropped):
                self.manifest.write_text(self.manifest.read_text().replace(dropped, ""))
                self.assertFlags("lints-dropped")
                self.git("checkout", "--", ".")

    def test_flags_dropped_dotted_workspace_lints(self):
        self.manifest.write_text(self.manifest.read_text() + "lints.workspace = true\n")
        self.commit("add lints")
        self.git("tag", "-f", "base")
        self.manifest.write_text(self.manifest.read_text().replace("lints.workspace = true\n", ""))
        self.assertFlags("lints-dropped")

    def test_flags_a_new_crate_without_workspace_lints(self):
        write(
            self.repo / "crates" / "b" / "Cargo.toml",
            '[package.metadata.tetsu]\nkind = "platform"\ntier = 2\n',
        )
        self.assertFlags("lints-dropped")

    def test_a_new_crate_with_workspace_lints_passes(self):
        for lints in (
            "[lints]\nworkspace = true\n",
            "lints.workspace = true\n",
            "[lints]\nworkspace=true\n",
            "[lints]\n# shared with the workspace\n\nworkspace = true\n",
            "lints = { workspace = true }\n",
        ):
            with self.subTest(lints=lints):
                write(self.repo / "crates" / "b" / "Cargo.toml", '[package]\nname = "b"\n\n' + lints)
                self.assertEqual(self.guard().returncode, 0)

    def test_other_manifest_edits_pass(self):
        self.manifest.write_text(self.manifest.read_text() + "\n[dependencies]\n")
        self.assertEqual(self.guard().returncode, 0)


class Reasons(GuardCase):
    def test_a_trailer_only_covers_files_its_commit_touched(self):
        write(self.repo / "src" / "a.rs", "#[allow(unused)]\n")
        self.commit("allow\n\nFloor-reason: approved")
        (self.repo / "tests" / "test_a.py").unlink()
        result = self.guard()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[test-deleted]", result.stderr)

    def test_an_empty_trailer_does_not_count(self):
        (self.repo / "tests" / "test_a.py").unlink()
        self.commit("drop\n\nFloor-reason:\n\nOther: x")
        self.assertEqual(self.guard().returncode, 1)

    def test_a_commit_trailer_turns_findings_into_warnings(self):
        (self.repo / "tests" / "test_a.py").unlink()
        self.commit("drop test\n\nFloor-reason: the feature it covered is gone")
        result = self.guard()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("[test-deleted]", result.stderr)
        self.assertIn("the feature it covered is gone", result.stderr)

    def test_a_trailer_outside_the_range_does_not_count(self):
        self.git("commit", "-q", "--allow-empty", "-m", "old\n\nFloor-reason: stale")
        self.git("tag", "-f", "base")
        (self.repo / "tests" / "test_a.py").unlink()
        self.assertFlags("test-deleted")


class CouldNotRun(GuardCase):
    def test_an_unknown_base_exits_2(self):
        self.assertEqual(self.guard(base="no-such-ref").returncode, 2)

    def test_outside_a_repository_exits_2(self):
        with tempfile.TemporaryDirectory() as empty:
            result = run(["python3", GUARD, "--base", "main"], empty)
        self.assertEqual(result.returncode, 2)


class Untracked(GuardCase):
    def test_flags_untracked_files(self):
        write(self.repo / "new.rs", "#[allow(unused)]\n")
        self.assertFlags("silenced-checker")


if __name__ == "__main__":
    unittest.main()
