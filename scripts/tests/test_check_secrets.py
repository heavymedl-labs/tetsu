"""The secrets list names every workflow and holds no secret past its yearly rotation (ADR-0027)."""

import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "check_secrets.py"
TODAY = "2026-11-09"


def secrets_list(rows, workflows=("ci.yml",), permissions="`contents: read`"):
    secrets = "\n".join(f"| `{name}` | GitHub | repository | deploys | {rotated} |" for name, rotated in rows)
    tokens = "\n".join(f"| `{name}` | {permissions} |" for name in workflows)
    return f"""# Secrets and tokens

## Secrets

| Secret | Store | Scope | Use | Last rotated |
| --- | --- | --- | --- | --- |
{secrets}

## Workflow tokens

| Workflow | `GITHUB_TOKEN` permissions |
| --- | --- |
{tokens}
"""


class CheckSecretsTest(unittest.TestCase):
    def check(self, text, workflows=("ci.yml",), today=TODAY, workflow="on: push\n"):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            if text is not None:
                write(repo / "docs/operations/secrets.md", text)
            for name in workflows:
                write(repo / ".github/workflows" / name, workflow)
            return run(["python3", SCRIPT, "--today", today], repo)

    def test_a_current_list_passes(self):
        result = self.check(secrets_list([("GITHUB_TOKEN", "automatic"), ("DEPLOY_KEY", "2025-10-09")]))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_secret_over_thirteen_months_old_fails(self):
        result = self.check(secrets_list([("DEPLOY_KEY", "2025-10-08")]))
        self.assertEqual(result.returncode, 1)
        self.assertIn("DEPLOY_KEY was last rotated 2025-10-08, over 13 months ago", result.stderr)

    def test_thirteen_months_counts_calendar_months(self):
        # 13 months after 2025-01-31 is 2026-02-28, the last day of a shorter month.
        self.assertEqual(self.check(secrets_list([("K", "2025-01-31")]), today="2026-02-28").returncode, 0)
        self.assertEqual(self.check(secrets_list([("K", "2025-01-31")]), today="2026-03-01").returncode, 1)

    def test_a_date_that_is_not_one_fails(self):
        for rotated in ["2026-13-01", "11/09/2026", "", "soon", "2026-11-31"]:
            result = self.check(secrets_list([("DEPLOY_KEY", rotated)]))
            self.assertEqual(result.returncode, 1, rotated)
            self.assertIn("DEPLOY_KEY: last rotated must be YYYY-MM-DD or automatic", result.stderr, rotated)

    def test_a_date_in_the_future_fails(self):
        result = self.check(secrets_list([("DEPLOY_KEY", "2026-11-10")]))
        self.assertEqual(result.returncode, 1)
        self.assertIn("DEPLOY_KEY was last rotated 2026-11-10, after today", result.stderr)

    def test_every_workflow_must_list_its_token_permissions(self):
        result = self.check(secrets_list([("GITHUB_TOKEN", "automatic")]), workflows=("ci.yml", "deploy.yml"))
        self.assertEqual(result.returncode, 1)
        self.assertIn("deploy.yml has no row in the workflow tokens table", result.stderr)

    def test_a_row_for_a_workflow_that_is_gone_fails(self):
        result = self.check(secrets_list([("GITHUB_TOKEN", "automatic")], workflows=("ci.yml", "old.yml")))
        self.assertEqual(result.returncode, 1)
        self.assertIn("old.yml is listed but no such workflow exists", result.stderr)

    def test_every_permission_a_workflow_sets_is_listed(self):
        workflow = """on: push
permissions:
  contents: read
jobs:
  deploy:
    permissions:
      contents: read
      packages: write  # to push the image
    runs-on: ubuntu-latest
"""
        listed = secrets_list([("GITHUB_TOKEN", "automatic")], permissions="`contents: read`")
        result = self.check(listed, workflow=workflow)
        self.assertEqual(result.returncode, 1)
        self.assertIn("ci.yml sets packages: write, which its row does not list", result.stderr)
        both = secrets_list(
            [("GITHUB_TOKEN", "automatic")], permissions="`contents: read`, `packages: write`"
        )
        self.assertEqual(self.check(both, workflow=workflow).returncode, 0)

    def test_a_shorthand_permission_is_listed_as_written(self):
        workflow = "on: push\npermissions: read-all\n"
        result = self.check(secrets_list([("GITHUB_TOKEN", "automatic")]), workflow=workflow)
        self.assertIn("ci.yml sets permissions: read-all, which its row does not list", result.stderr)

    def test_every_secret_a_workflow_uses_has_a_row(self):
        workflow = "on: push\njobs:\n  a:\n    steps:\n      - env:\n"
        workflow += "          KEY: ${{ secrets.RENDER_API_KEY }}\n"
        result = self.check(secrets_list([("GITHUB_TOKEN", "automatic")]), workflow=workflow)
        self.assertEqual(result.returncode, 1)
        expected = "ci.yml uses secrets.RENDER_API_KEY, which has no row in the secrets table"
        self.assertIn(expected, result.stderr)
        listed = secrets_list([("GITHUB_TOKEN", "automatic"), ("RENDER_API_KEY", "2026-11-01")])
        self.assertEqual(self.check(listed, workflow=workflow).returncode, 0)

    def test_a_missing_list_or_table_fails(self):
        self.assertIn("docs/operations/secrets.md is missing", self.check(None).stderr)
        no_tokens = secrets_list([("GITHUB_TOKEN", "automatic")]).split("## Workflow tokens")[0]
        result = self.check(no_tokens)
        self.assertEqual(result.returncode, 1)
        self.assertIn("no Workflow tokens table", result.stderr)
        tokens_only = secrets_list([]).split("## Workflow tokens")[1]
        no_secrets = "# Secrets and tokens\n\n## Workflow tokens" + tokens_only
        result = self.check(no_secrets)
        self.assertEqual(result.returncode, 1)
        self.assertIn("no Secrets table", result.stderr)

    def test_unknown_arguments_are_a_usage_error(self):
        for args in [["--today"], ["--today", "nope"], ["--soon"]]:
            result = run(["python3", SCRIPT, *args], ROOT)
            self.assertEqual(result.returncode, 2, args)
            self.assertIn("usage:", result.stderr)

    def test_the_repository_list_passes_today(self):
        result = run(["python3", SCRIPT], ROOT)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
