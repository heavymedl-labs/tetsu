"""scan_secrets.sh must stop each secret form ADR-0027 names, at commit, at push, and in history, and must
pass code that only reads a secret. Planted values are built here at run time, so this file holds none."""

import re
import secrets
import shutil
import string
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "scan_secrets.sh"
BETTERLEAKS = ROOT / ".tools" / "bin" / "betterleaks"
NEON_HOST = "ep-cool-river-a1b2c3d4.us-east-2.aws.neon.tech"
# Built here so this file does not itself hold the comment the scan must ignore.
ALLOW_COMMENT = "betterleaks:" + "allow"


def token(length, alphabet=string.ascii_letters + string.digits):
    return "".join(secrets.choice(alphabet) for _ in range(length))


def leaks():
    """One line for each form a real secret of ours takes."""
    return {
        "postgres url": f'url = "postgresql://owner:npg_{token(14)}@{NEON_HOST}/neondb"',
        "PGPASSWORD": f"PGPASSWORD=npg_{token(16)}",
        "keyword connection string": (
            f'dsn = "host={NEON_HOST} user=owner password=npg_{token(16)} dbname=neondb"'
        ),
        "password first": f"password=npg_{token(16)} host={NEON_HOST} dbname=neondb",
        "quoted keyword password": f"host={NEON_HOST} user=u password='npg_{token(16)}' dbname=d",
        "spaced keyword password": f"host = {NEON_HOST} user = u password = npg_{token(16)}",
        "quoted password": f'password = "{token(20)}"',
        "session secret": f"SESSION_SECRET={secrets.token_urlsafe(32)}",
        "base64 session key": f"TETSU_SESSION_KEY={secrets.token_urlsafe(48)}",
        "github token": f'token = "ghp_{token(36)}"',
        "render api key": f'key = "rnd_{token(32)}"',
        "bare render api key": f"render login --api-key rnd_{token(32)}",
        "long render api key": f"| key | rnd_{token(40)} |",
        "postgres uri query password": f"postgres://{NEON_HOST}/neondb?user=owner&password={token(16)}",
        "bare neon password": f"echo npg_{token(12)} | gh secret set PGPASSWORD",
        "neon api key": f'NEON_API_KEY = "napi_{token(64, string.ascii_lowercase + string.digits)}"',
        "password before another option": f"password=npg_{token(16)} channel_binding=require",
        "render deploy hook": f"HOOK=https://api.render.com/deploy/srv-{token(20).lower()}?key={token(11)}",
        "render deploy hook, key last": (
            f"https://api.render.com/deploy/srv-{token(20).lower()}?ref=main&key={token(11)}"
        ),
    }


# Code that names a secret without holding one, and fake credentials. None of these may fail a commit.
READS = [
    "let password = config.database_password;",
    'let password = env::var("PGPASSWORD").unwrap();',
    'password = os.environ["PGPASSWORD"]',
    'let password = read(Some("PGPASSWORD"));',
    "PGPASSWORD=$PGPASSWORD psql",
    'format!("host={} user={} password={} dbname={}", h, u, p, d)',
    "SESSION_SECRET=${{ secrets.SESSION_SECRET }}",
    'let secret = env::var("TETSU_SESSION_KEY")?;',
    "    password: SecretString,",
    "    password: Option<String>,",
    "    password: Zeroizing<String>,",
    "session_secret: SessionSecretConfigurationFromEnvironment",
    'url = "postgres://tetsu:fake-pw-1@127.0.0.1:1/tetsu"',
    'url = "postgres://u:fake-p%40ss@127.0.0.1/db"',
    'dsn = "host=h user=u password=fake-pw-123 dbname=d"',
]


class ScanSecrets(unittest.TestCase):
    def setUp(self):
        self.assertTrue(BETTERLEAKS.exists(), "Betterleaks is missing; run scripts/install-tools.sh")
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.tmp = Path(tmp.name)
        self.repo = self.tmp / "repo"
        self.remote = self.tmp / "remote.git"
        run(["git", "init", "-q", "--bare", self.remote], self.tmp, check=True)
        run(["git", "clone", "-q", self.remote, self.repo], self.tmp, check=True)
        self.configure(self.repo)
        (self.repo / "scripts").mkdir()
        shutil.copy(SCRIPT, self.repo / "scripts")
        shutil.copy(ROOT / ".betterleaks.toml", self.repo)
        write(self.repo / ".betterleaksignore", "# none\n")
        (self.repo / ".tools" / "bin").mkdir(parents=True)
        (self.repo / ".tools" / "bin" / "betterleaks").symlink_to(BETTERLEAKS)
        write(self.repo / ".gitignore", "/.tools\n")
        self.commit("base")
        self.git("push", "-q", "origin", "HEAD:main")

    def configure(self, repo):
        run(["git", "config", "user.email", "test@example.com"], repo, check=True)
        run(["git", "config", "user.name", "Test"], repo, check=True)

    def git(self, *args, repo=None):
        result = run(["git", *args], repo or self.repo)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def commit(self, message, *paths):
        self.git("add", *(paths or ["-A"]))
        self.git("commit", "-q", "-m", message)

    def scan(self, *args, stdin=None):
        return run([self.repo / "scripts" / "scan_secrets.sh", *args], self.repo, input=stdin)

    def push(self, ref="HEAD", remote_ref="refs/heads/main"):
        line = f"refs/heads/main {self.git('rev-parse', ref)} {remote_ref} {'0' * 40}\n"
        return self.scan("--push", "origin", stdin=line)

    def assertCaught(self, result):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def assertClean(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_each_secret_form_fails_at_commit_in_source_and_test_paths(self):
        for name, line in leaks().items():
            for folder in ("src", "tests"):
                with self.subTest(form=name, folder=folder):
                    write(self.repo / folder / "a.rs", line + "\n")
                    self.git("add", folder)
                    result = self.scan("--staged")
                    self.git("rm", "-q", "--cached", "-r", folder)
                    self.assertCaught(result)

    def test_each_secret_form_fails_at_push_and_in_history_without_showing_it(self):
        for name, line in leaks().items():
            with self.subTest(form=name):
                write(self.repo / "src" / "a.rs", line + "\n")
                self.commit("leak")
                values = [v for v in re.findall(r"[A-Za-z0-9_+/=-]{14,}", line) if v not in NEON_HOST]
                for result in (self.push(), self.scan("--history")):
                    self.assertCaught(result)
                    for value in values:
                        self.assertNotIn(value, result.stdout + result.stderr)
                self.git("reset", "-q", "--hard", "HEAD~1")

    def test_code_that_reads_a_secret_and_fake_credentials_pass(self):
        write(self.repo / "src" / "a.rs", "\n".join(READS) + "\n")
        self.git("add", "src")
        self.assertClean(self.scan("--staged"))
        self.commit("reads")
        self.assertClean(self.push())
        self.assertClean(self.scan("--history"))

    def test_a_real_password_beside_a_fake_word_fails(self):
        write(
            self.repo / "tests" / "a.rs", f'url = "postgresql://owner:npg_{token(14)}@{NEON_HOST}/fake-db"\n'
        )
        self.git("add", "tests")
        self.assertCaught(self.scan("--staged"))

    def test_an_allow_comment_does_not_silence_a_finding(self):
        write(self.repo / "src" / "a.rs", f'token = "ghp_{token(36)}" // {ALLOW_COMMENT}\n')
        self.git("add", "src")
        self.assertCaught(self.scan("--staged"))

    def test_output_never_shows_the_secret(self):
        value = token(36)
        write(self.repo / "src" / "a.rs", f'token = "ghp_{value}"\n')
        self.git("add", "src")
        result = self.scan("--staged")
        self.assertCaught(result)
        self.assertNotIn(value, result.stdout + result.stderr)

    def test_history_finds_a_secret_a_later_commit_removed(self):
        write(self.repo / "src" / "a.rs", leaks()["github token"] + "\n")
        self.commit("add")
        (self.repo / "src" / "a.rs").unlink()
        self.commit("remove")
        self.assertCaught(self.scan("--history"))

    def merge_with_a_secret(self):
        self.git("checkout", "-q", "-b", "side")
        write(self.repo / "side.txt", "side\n")
        self.commit("side")
        self.git("checkout", "-q", "-")
        write(self.repo / "main.txt", "main\n")
        self.commit("main")
        self.git("merge", "-q", "--no-commit", "side")
        write(self.repo / "src" / "a.rs", leaks()["github token"] + "\n")
        self.commit("merge")

    def test_a_secret_added_while_resolving_a_merge_fails_at_push_and_in_history(self):
        self.merge_with_a_secret()
        self.assertCaught(self.push())
        self.assertCaught(self.scan("--history"))

    def test_a_secret_in_a_commit_message_fails_at_push_and_in_history(self):
        write(self.repo / "a.txt", "a\n")
        self.git("add", "a.txt")
        self.git("commit", "-q", "-m", f"fix: thing\n\n{leaks()['session secret']}")
        self.assertCaught(self.push())
        self.assertCaught(self.scan("--history"))

    def test_a_secret_in_a_tag_message_fails_at_push_and_in_history(self):
        self.git("tag", "-a", "v1", "-m", f"release\n\n{leaks()['PGPASSWORD']}")
        line = f"refs/tags/v1 {self.git('rev-parse', 'v1')} refs/tags/v1 {'0' * 40}\n"
        self.assertCaught(self.scan("--push", "origin", stdin=line))
        self.assertCaught(self.scan("--history"))

    def test_an_ignored_message_finding_stays_ignored_as_history_grows(self):
        write(self.repo / "a.txt", "a\n")
        self.git("add", "a.txt")
        self.git("commit", "-q", "-m", f"fix: thing\n\n{leaks()['github token']}")
        leak = self.git("rev-parse", "HEAD")
        write(self.repo / ".betterleaksignore", f"# rotated\n{leak}:github-pat:3\n")
        self.assertClean(self.scan("--history"))
        self.git("commit", "-q", "--allow-empty", "-m", "later")
        self.assertClean(self.scan("--history"))

    def test_a_clean_history_passes(self):
        write(self.repo / "a.txt", "a\n")
        self.commit("clean")
        self.assertClean(self.scan("--history"))

    def test_push_passes_a_clean_commit_and_fails_one_with_a_secret(self):
        write(self.repo / "a.txt", "a\n")
        self.commit("clean")
        self.assertClean(self.push())
        write(self.repo / "src" / "a.rs", leaks()["render api key"] + "\n")
        self.commit("leak")
        self.assertCaught(self.push())

    def test_push_passes_when_the_remote_has_commits_this_clone_lacks(self):
        other = self.tmp / "other"
        run(["git", "clone", "-q", "-b", "main", self.remote, other], self.tmp, check=True)
        self.configure(other)
        write(other / "b.txt", "b\n")
        self.git("add", "b.txt", repo=other)
        self.git("commit", "-q", "-m", "elsewhere", repo=other)
        self.git("push", "-q", "origin", "HEAD:main", repo=other)
        write(self.repo / "a.txt", "a\n")
        self.commit("local")
        remote_head = self.git("rev-parse", "HEAD", repo=other)
        line = f"refs/heads/main {self.git('rev-parse', 'HEAD')} refs/heads/main {remote_head}\n"
        self.assertClean(self.scan("--push", "origin", stdin=line))

    def test_push_scans_commits_another_remote_already_has(self):
        """Only the remote being pushed to counts as already scanned."""
        mirror = self.tmp / "mirror.git"
        run(["git", "init", "-q", "--bare", mirror], self.tmp, check=True)
        self.git("remote", "add", "mirror", str(mirror))
        write(self.repo / "src" / "a.rs", leaks()["github token"] + "\n")
        self.commit("leak")
        self.git("push", "-q", "mirror", "HEAD:main")
        self.assertCaught(self.push())

    def test_push_scans_every_ref_pushed_at_once(self):
        write(self.repo / "a.txt", "a\n")
        self.commit("clean")
        clean = self.git("rev-parse", "HEAD")
        self.git("checkout", "-q", "-b", "other")
        write(self.repo / "src" / "a.rs", leaks()["github token"] + "\n")
        self.commit("leak")
        leaky = self.git("rev-parse", "HEAD")
        zero = "0" * 40
        lines = (
            f"refs/heads/main {clean} refs/heads/main {zero}\n"
            f"refs/heads/other {leaky} refs/heads/other {zero}\n"
        )
        self.assertCaught(self.scan("--push", "origin", stdin=lines))

    def test_a_secret_added_and_removed_on_a_merged_branch_fails(self):
        self.git("checkout", "-q", "-b", "side")
        write(self.repo / "src" / "a.rs", leaks()["github token"] + "\n")
        self.commit("leak")
        (self.repo / "src" / "a.rs").unlink()
        self.commit("remove")
        self.git("checkout", "-q", "-")
        self.git("merge", "-q", "--no-ff", "-m", "merge", "side")
        self.assertCaught(self.push())
        self.assertCaught(self.scan("--history"))

    def test_a_real_push_through_a_hook_is_stopped(self):
        hooks = self.repo / "hooks"
        hook = '#!/bin/sh\nexec "$(git rev-parse --show-toplevel)/scripts/scan_secrets.sh" --push "$1"\n'
        write(hooks / "pre-push", hook)
        (hooks / "pre-push").chmod(0o755)
        self.git("config", "core.hooksPath", str(hooks))
        write(self.repo / "src" / "a.rs", leaks()["render api key"] + "\n")
        self.commit("leak")
        result = run(["git", "push", "-q", "origin", "HEAD:main"], self.repo)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(
            self.git("rev-parse", "HEAD"), run(["git", "ls-remote", self.remote], self.tmp).stdout
        )

    def test_a_finding_names_where_it_is_but_not_what_it_is(self):
        value = token(36)
        write(self.repo / "src" / "a.rs", f'token = "ghp_{value}"\n')
        self.commit("leak")
        result = self.scan("--history")
        self.assertCaught(result)
        self.assertIn(f"{self.git('rev-parse', 'HEAD')} src/a.rs:1 (github-pat)", result.stderr)
        self.assertNotIn(value, result.stdout + result.stderr)
        write(self.repo / "src" / "b.rs", f'token = "ghp_{value}"\n')
        self.git("add", "src/b.rs")
        self.assertIn("staged src/b.rs:1 (github-pat)", self.scan("--staged").stderr)
        self.git("rm", "-q", "--cached", "src/b.rs")
        self.git("commit", "-q", "--allow-empty", "-m", f"note\n\ntoken ghp_{value}")
        message = self.git("rev-parse", "HEAD")
        result = self.scan("--history")
        self.assertIn(f"message of {message}:3 (github-pat)", result.stderr)
        self.assertNotIn(value, result.stdout + result.stderr)

    def test_an_ignore_entry_without_a_commit_is_refused(self):
        write(self.repo / ".betterleaksignore", "src/a.rs:github-pat:1\n")
        result = self.scan("--staged")
        self.assertEqual(result.returncode, 1)
        self.assertIn("must start with the commit", result.stderr)

    def test_push_skips_a_deleted_ref(self):
        line = f"(delete) {'0' * 40} refs/heads/old {self.git('rev-parse', 'HEAD')}\n"
        self.assertClean(self.scan("--push", "origin", stdin=line))

    def test_push_needs_the_remote_name(self):
        self.assertNotEqual(self.scan("--push", stdin="").returncode, 0)

    def test_errors_fail_rather_than_pass(self):
        with self.subTest(case="unknown pushed object"):
            line = f"refs/heads/main {'1' * 40} refs/heads/main {'0' * 40}\n"
            self.assertNotEqual(self.scan("--push", "origin", stdin=line).returncode, 0)
        with self.subTest(case="missing config"):
            (self.repo / ".betterleaks.toml").rename(self.repo / "moved.toml")
            self.assertNotEqual(self.scan("--history").returncode, 0)
            (self.repo / "moved.toml").rename(self.repo / ".betterleaks.toml")
        with self.subTest(case="not a repository"):
            plain = self.tmp / "plain"
            shutil.copytree(self.repo, plain, ignore=shutil.ignore_patterns(".git"), symlinks=True)
            self.assertNotEqual(
                run([plain / "scripts" / "scan_secrets.sh", "--history"], plain).returncode, 0
            )

    def test_the_scan_reads_rules_and_ignores_only_from_its_own_files(self):
        with self.subTest(case="symlinked ignore list"):
            write(self.tmp / "elsewhere", "# anything\n")
            (self.repo / ".betterleaksignore").unlink()
            (self.repo / ".betterleaksignore").symlink_to(self.tmp / "elsewhere")
            result = self.scan("--staged")
            self.assertEqual(result.returncode, 1)
            self.assertIn("must be a regular file", result.stderr)
        write_ignore = self.repo / ".betterleaksignore"
        write_ignore.unlink()
        write(write_ignore, "# none\n")
        with self.subTest(case="symlinked config"):
            config = self.repo / ".betterleaks.toml"
            (self.tmp / "loose.toml").write_text(config.read_text())
            config.rename(self.tmp / "real.toml")
            config.symlink_to(self.tmp / "loose.toml")
            self.assertIn("must be a regular file", self.scan("--staged").stderr)
            config.unlink()
            (self.tmp / "real.toml").rename(config)
        ignore = self.repo / ".betterleaksignore"
        for case in ("missing", "folder"):
            with self.subTest(case=f"{case} ignore list"):
                ignore.unlink()
                if case == "folder":
                    ignore.mkdir()
                result = self.scan("--staged")
                self.assertEqual(result.returncode, 1)
                self.assertIn("must be a regular file", result.stderr)
                if case == "folder":
                    ignore.rmdir()
                write(ignore, "# none\n")
        config = self.repo / ".betterleaks.toml"
        original = config.read_text()
        dotted = original.replace("[extend]\nuseDefault = true", 'extend.path = "x.toml"')
        for form in (
            'path = "x.toml"',
            '"path" = "x.toml"',
            "extend = { path = 'x.toml' }",
            "\\u0070ath = 1",
        ):
            with self.subTest(case="config naming another file", form=form):
                config.write_text(original.replace("useDefault = true", f"useDefault = true\n{form}"))
                result = self.scan("--staged")
                self.assertEqual(result.returncode, 1)
                self.assertIn("names a path", result.stderr)
        with self.subTest(case="dotted extend path"):
            config.write_text(dotted)
            self.assertIn("names a path", self.scan("--staged").stderr)
        config.write_text(original)

    def test_the_staged_scan_refuses_attributes_that_hide_files(self):
        for line in ("*.rs -diff", "*.rs binary", "*.env diff=nothing", "*.rs !diff"):
            with self.subTest(line=line):
                write(self.repo / "src" / ".gitattributes", line + "\n")
                write(self.repo / "src" / "a.rs", leaks()["github token"] + "\n")
                self.git("add", "src/a.rs")
                result = self.scan("--staged")
                self.git("rm", "-q", "--cached", "src/a.rs")
                self.assertEqual(result.returncode, 1)
                self.assertIn("turns diffs off", result.stderr)
        (self.repo / "src" / ".gitattributes").unlink()

    def test_each_rule_of_ours_catches_its_form_on_its_own(self):
        """The default rules also catch most of these, so each of ours runs alone to show it does its part."""
        forms = {
            "postgres-password": ["PGPASSWORD", "keyword connection string", "spaced keyword password"],
            "postgres-password-first": ["password first", "password before another option"],
            "provider-key": ["bare render api key", "long render api key", "bare neon password"],
            "postgres-uri-password": ["postgres uri query password"],
            "app-secret": ["session secret", "base64 session key"],
            "render-deploy-hook": ["render deploy hook", "render deploy hook, key last"],
        }
        planted = leaks()
        for rule, names in forms.items():
            for name in names:
                with self.subTest(rule=rule, form=name):
                    self.assertEqual(self.alone(rule, planted[name]), 1)
            with self.subTest(rule=rule, form="reads"):
                self.assertEqual(self.alone(rule, "\n".join(READS)), 0)

    def alone(self, rule, text):
        config = ["-c", ".betterleaks.toml", "--isolate-rule", rule, "--no-banner", "--redact"]
        return run([BETTERLEAKS, "stdin", *config], self.repo, input=text + "\n").returncode

    def test_an_unknown_mode_is_a_usage_error(self):
        self.assertEqual(self.scan("--everything").returncode, 2)


if __name__ == "__main__":
    unittest.main()
